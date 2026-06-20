use std::sync::Arc;

use crate::{
    models::patient::{DiseaseType, MortalityRisk, Patient, SeverityLevel},
    repositories::patient::PatientRepository,
};

pub struct GoldService {
    patient_repo: Arc<PatientRepository>,
}

fn compute_risk_score(patient: &Patient) -> f64 {
    let mut score = 0.0f64;

    if let Some(age) = patient.age {
        if age >= 65 { score += 0.25; }
        else if age >= 50 { score += 0.15; }
        else if age < 5 { score += 0.20; }
    }

    if let Some(ref conds) = patient.existing_conditions {
        let lower = conds.to_lowercase();
        if lower.contains("hypertension") { score += 0.10; }
        if lower.contains("diabetes") { score += 0.10; }
        if lower.contains("cardiovascular") { score += 0.15; }
    }

    if patient.smoking_status.unwrap_or(false) { score += 0.08; }
    if patient.alcohol_consumption.unwrap_or(false) { score += 0.05; }

    if let Some(ref genotype) = patient.genotype {
        if genotype.to_uppercase() == "SS" { score += 0.15; }
    }

    match &patient.severity_level {
        Some(SeverityLevel::Critical) => score += 0.20,
        Some(SeverityLevel::Severe)   => score += 0.12,
        _ => {}
    }

    if matches!(&patient.weather_condition, Some(w) if w.to_lowercase().contains("rain"))
        && matches!(&patient.disease_type, Some(DiseaseType::Infectious))
    {
        score += 0.05;
    }

    score.min(1.0)
}

fn derive_mortality_risk(score: f64) -> MortalityRisk {
    if score >= 0.7 { MortalityRisk::High }
    else if score >= 0.4 { MortalityRisk::Medium }
    else { MortalityRisk::Low }
}

fn derive_readmission(score: f64, disease_type: Option<&DiseaseType>) -> String {
    let base = score >= 0.5 || matches!(disease_type, Some(DiseaseType::Chronic));
    if base { "High".to_string() } else { "Low".to_string() }
}

fn derive_drug(patient: &Patient) -> String {
    let symptoms = patient.symptoms.as_deref().unwrap_or("").to_lowercase();
    let conditions = patient.existing_conditions.as_deref().unwrap_or("").to_lowercase();

    match &patient.disease_type {
        Some(DiseaseType::Infectious) if symptoms.contains("pyrexia") || symptoms.contains("fever") => {
            "Artemether-Lumefantrine".to_string()
        }
        Some(DiseaseType::Chronic) if conditions.contains("hypertension") => {
            "Amlodipine 5mg".to_string()
        }
        Some(DiseaseType::Chronic) if conditions.contains("diabetes") => {
            "Metformin 500mg".to_string()
        }
        _ => "Consult specialist — no automated recommendation".to_string(),
    }
}

impl GoldService {
    pub fn new(patient_repo: Arc<PatientRepository>) -> Self {
        Self { patient_repo }
    }

    pub async fn enrich_patient(&self, patient: &Patient) -> Result<(), sqlx::Error> {
        let risk_score = compute_risk_score(patient);
        let mortality_risk = derive_mortality_risk(risk_score);
        let readmission = derive_readmission(risk_score, patient.disease_type.as_ref());
        let drug = derive_drug(patient);
        let pattern = risk_score > 0.6;

        let outbreak = if let Some(ref state) = patient.state {
            if matches!(&patient.disease_type, Some(DiseaseType::Infectious)) {
                self.patient_repo
                    .count_infectious_in_state_recent(state)
                    .await
                    .unwrap_or(0) >= 5
            } else {
                false
            }
        } else {
            false
        };

        self.patient_repo.mark_gold_complete(
            &patient.patient_id,
            risk_score,
            &mortality_risk,
            &readmission,
            &drug,
            outbreak,
            pattern,
        ).await
    }

    pub async fn enrich_all(&self) -> Result<usize, sqlx::Error> {
        let pending = self.patient_repo.find_unprocessed_gold().await?;
        let count = pending.len();
        for patient in pending {
            self.enrich_patient(&patient).await.ok();
        }
        Ok(count)
    }
}
