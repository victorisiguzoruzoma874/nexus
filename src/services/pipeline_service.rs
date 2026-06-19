use std::sync::Arc;
use tracing::info;

use crate::{
    models::patient::{IngestPatientDto, MortalityRisk},
    repositories::patient::PatientRepository,
    services::{
        bronze_service::BronzeService,
        gold_service::GoldService,
        ml_service::MlService,
        pipeline_event_service::{PipelineEvent, PipelineEventService},
        silver_service::SilverService,
    },
    utils::errors::AppError,
};

pub struct PipelineService {
    pub bronze: Arc<BronzeService>,
    pub silver: Arc<SilverService>,
    pub gold: Arc<GoldService>,
    pub ml: Arc<MlService>,
    pub events: Arc<PipelineEventService>,
    pub patient_repo: Arc<PatientRepository>,
}

impl PipelineService {
    pub fn new(
        bronze: Arc<BronzeService>,
        silver: Arc<SilverService>,
        gold: Arc<GoldService>,
        ml: Arc<MlService>,
        events: Arc<PipelineEventService>,
        patient_repo: Arc<PatientRepository>,
    ) -> Self {
        Self { bronze, silver, gold, ml, events, patient_repo }
    }

    /// Returns immediately (<100ms). Full Bronze→Silver→Gold→ML chain fires in background.
    pub async fn ingest_and_process(
        &self,
        dto: IngestPatientDto,
        source_ip: &str,
    ) -> Result<crate::models::patient::IngestResponse, AppError> {
        let (patient, response) = self.bronze.ingest(dto, source_ip).await?;

        self.events.publish(PipelineEvent::Ingested {
            patient_id: patient.patient_id.clone(),
            message: "Bronze complete — data received".to_string(),
        });

        let silver = self.silver.clone();
        let gold = self.gold.clone();
        let ml = self.ml.clone();
        let events = self.events.clone();
        let patient_repo = self.patient_repo.clone();
        let patient_id = patient.patient_id.clone();

        tokio::spawn(async move {
            // --- Silver ---
            if let Err(e) = silver.process_patient(&patient).await {
                events.publish(PipelineEvent::PipelineError {
                    patient_id: patient_id.clone(),
                    stage: "silver".into(),
                    error: e.to_string(),
                });
                return;
            }
            info!("Silver complete for {}", patient_id);
            events.publish(PipelineEvent::SilverComplete { patient_id: patient_id.clone() });

            // --- Gold ---
            if let Err(e) = gold.enrich_patient(&patient).await {
                events.publish(PipelineEvent::PipelineError {
                    patient_id: patient_id.clone(),
                    stage: "gold".into(),
                    error: e.to_string(),
                });
                return;
            }
            info!("Gold complete for {}", patient_id);

            // Re-fetch enriched patient for ML + events
            let enriched = match patient_repo.find_by_patient_id(&patient_id).await {
                Ok(Some(p)) => p,
                _ => {
                    events.publish(PipelineEvent::PipelineError {
                        patient_id: patient_id.clone(),
                        stage: "gold_refetch".into(),
                        error: "Failed to re-fetch patient after gold".into(),
                    });
                    return;
                }
            };

            let risk_score = enriched.predictive_risk_score.unwrap_or(0.0);
            let mortality = enriched.mortality_risk.as_ref()
                .map(|r| format!("{:?}", r))
                .unwrap_or_else(|| "Low".to_string());

            events.publish(PipelineEvent::GoldComplete {
                patient_id: patient_id.clone(),
                risk_score,
                mortality_risk: mortality.clone(),
            });

            // Outbreak alert
            if enriched.outbreak_detection.unwrap_or(false) {
                events.publish(PipelineEvent::OutbreakAlert {
                    patient_id: patient_id.clone(),
                    state: enriched.state.clone().unwrap_or_default(),
                    disease_type: enriched.disease_type.as_ref()
                        .map(|d| format!("{:?}", d))
                        .unwrap_or_default(),
                });
            }

            // --- ML Inference ---
            let assessment = ml.run_full_inference(&enriched).await;
            info!("ML assessment complete for {}", patient_id);

            // Persist ML output back to DB
            patient_repo.save_ml_assessment(
                &patient_id,
                &assessment.diagnosis.probable_condition,
                &assessment.risk.risk_level,
                &assessment.recommendations.drug_recommendation,
                &assessment.routing.route_to,
                &assessment.routing.department,
            ).await.ok();

            // High-risk alert for nurses
            if enriched.mortality_risk == Some(MortalityRisk::High) {
                events.publish(PipelineEvent::HighRiskAlert {
                    patient_id: patient_id.clone(),
                    patient_name: enriched.full_name.clone(),
                    department: assessment.routing.department.clone(),
                    alert_priority: assessment.routing.alert_priority,
                });
            }

            events.publish(PipelineEvent::AssessmentReady {
                patient_id: patient_id.clone(),
                risk_score,
                mortality_risk: mortality,
                probable_condition: assessment.diagnosis.probable_condition.clone(),
                route_to: assessment.routing.route_to.clone(),
                department: assessment.routing.department.clone(),
                alert_priority: assessment.routing.alert_priority,
            });
        });

        Ok(response)
    }

    /// Re-run the full pipeline for an existing patient (e.g. after doctor update)
    pub async fn re_assess(&self, patient_id: &str) -> Result<(), AppError> {
        let patient = self.patient_repo
            .find_by_patient_id(patient_id)
            .await
            .map_err(AppError::Database)?
            .ok_or_else(|| AppError::NotFound(format!("Patient {} not found", patient_id)))?;

        let silver = self.silver.clone();
        let gold = self.gold.clone();
        let ml = self.ml.clone();
        let events = self.events.clone();
        let patient_repo = self.patient_repo.clone();
        let pid = patient_id.to_string();

        tokio::spawn(async move {
            silver.process_patient(&patient).await.ok();
            events.publish(PipelineEvent::SilverComplete { patient_id: pid.clone() });

            gold.enrich_patient(&patient).await.ok();

            if let Ok(Some(enriched)) = patient_repo.find_by_patient_id(&pid).await {
                let risk_score = enriched.predictive_risk_score.unwrap_or(0.0);
                let mortality = enriched.mortality_risk.as_ref()
                    .map(|r| format!("{:?}", r))
                    .unwrap_or_else(|| "Low".to_string());

                events.publish(PipelineEvent::GoldComplete {
                    patient_id: pid.clone(),
                    risk_score,
                    mortality_risk: mortality.clone(),
                });

                let assessment = ml.run_full_inference(&enriched).await;

                // Persist ML output back to DB
                patient_repo.save_ml_assessment(
                    &pid,
                    &assessment.diagnosis.probable_condition,
                    &assessment.risk.risk_level,
                    &assessment.recommendations.drug_recommendation,
                    &assessment.routing.route_to,
                    &assessment.routing.department,
                ).await.ok();

                events.publish(PipelineEvent::AssessmentReady {
                    patient_id: pid.clone(),
                    risk_score,
                    mortality_risk: mortality,
                    probable_condition: assessment.diagnosis.probable_condition,
                    route_to: assessment.routing.route_to,
                    department: assessment.routing.department,
                    alert_priority: assessment.routing.alert_priority,
                });
            }
        });

        Ok(())
    }
}
