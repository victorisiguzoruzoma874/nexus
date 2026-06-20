use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::models::patient::Patient;

#[derive(Debug, Serialize, Deserialize)]
pub struct MlAssessment {
    pub patient_id: String,
    pub diagnosis: DiagnosisResult,
    pub risk: RiskResult,
    pub recommendations: RecommendationResult,
    pub routing: RoutingResult,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DiagnosisResult {
    pub probable_condition: String,
    pub confidence: f64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RiskResult {
    pub risk_level: String,
    pub risk_score: f64,
    pub deterioration_probability: f64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RecommendationResult {
    pub drug_recommendation: String,
    pub recommendations: Vec<String>,
    pub urgency: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RoutingResult {
    pub route_to: String,
    pub department: String,
    pub alert_priority: u8,
}

pub struct MlService {
    ml_base_url: String,
    client: reqwest::Client,
}

impl MlService {
    pub fn new() -> Self {
        Self {
            ml_base_url: std::env::var("ML_SERVICE_URL")
                .unwrap_or_else(|_| "http://localhost:8001".to_string()),
            client: reqwest::Client::new(),
        }
    }

    /// Expose the inner client for health proxying.
    pub fn client(&self) -> &reqwest::Client {
        &self.client
    }

    /// Run all 4 models via POST /predict/full on the Python ML service.
    /// Falls back to deterministic defaults if the service is unreachable.
    pub async fn run_full_inference(&self, patient: &Patient) -> MlAssessment {
        let payload = json!({
            "patient_id":           patient.patient_id,
            "symptoms":             patient.symptoms.as_deref().unwrap_or(""),
            "existing_conditions":  patient.existing_conditions.as_deref().unwrap_or("None"),
            "blood_group":          patient.blood_group.as_deref().unwrap_or("O+"),
            "genotype":             patient.genotype.as_deref().unwrap_or("AA"),
            "age":                  patient.age.unwrap_or(30),
            "gender":               patient.gender.as_deref().unwrap_or("Male"),
            "height_cm":            patient.height_cm.unwrap_or(170.0),
            "weight_kg":            patient.weight_kg.unwrap_or(70.0),
            "disease_type":         patient.disease_type.as_ref().map(|d| format!("{:?}", d)),
            "severity_level":       patient.severity_level.as_ref().map(|s| format!("{:?}", s)).unwrap_or_else(|| "Mild".into()),
            "weather_condition":    patient.weather_condition.as_deref().unwrap_or("Dry"),
            "smoking_status":       patient.smoking_status.unwrap_or(false),
            "alcohol_consumption":  patient.alcohol_consumption.unwrap_or(false),
            "exercise_habits":      patient.exercise_habits.as_deref().unwrap_or("Weekly"),
            "diet_type":            patient.diet_type.as_deref().unwrap_or("Mixed"),
            "water_source":         patient.water_source.as_deref().unwrap_or("Tap"),
            "patient_category":     patient.patient_category.as_ref().map(|c| format!("{:?}", c)).unwrap_or_else(|| "Adult".into()),
            "predictive_risk_score": patient.predictive_risk_score.unwrap_or(0.0),
        });

        let result = self.client
            .post(format!("{}/predict/full", self.ml_base_url))
            .json(&payload)
            .send()
            .await;

        match result {
            Ok(resp) if resp.status().is_success() => {
                if let Ok(assessment) = resp.json::<MlAssessment>().await {
                    return assessment;
                }
            }
            _ => tracing::warn!("ML service unreachable for {} — using fallback", patient.patient_id),
        }

        // Fallback: deterministic rules (mirrors routing_rules.json)
        self.fallback_inference(patient)
    }

    fn fallback_inference(&self, patient: &Patient) -> MlAssessment {
        use crate::models::patient::{DiseaseType, PatientCategory, SeverityLevel};

        let score = patient.predictive_risk_score.unwrap_or(0.0);
        let risk_level = if score >= 0.7 { "High" } else if score >= 0.4 { "Medium" } else { "Low" };

        let dept = match &patient.disease_type {
            Some(DiseaseType::MentalHealth) => "Psychiatry",
            Some(DiseaseType::Genetic)      => "Genetics",
            Some(DiseaseType::Infectious)   => "Infectious Disease",
            _ => match &patient.patient_category {
                Some(PatientCategory::Child)   => "Paediatrics",
                Some(PatientCategory::Elderly) => "Geriatrics",
                _                              => "General Medicine",
            },
        };

        let priority = match &patient.severity_level {
            Some(SeverityLevel::Critical) => 1,
            Some(SeverityLevel::Severe)   => 2,
            _                             => 3,
        };

        MlAssessment {
            patient_id: patient.patient_id.clone(),
            diagnosis: DiagnosisResult {
                probable_condition: patient.disease_type
                    .as_ref().map(|d| format!("{:?}", d))
                    .unwrap_or_else(|| "Unknown".into()),
                confidence: score,
            },
            risk: RiskResult {
                risk_level: risk_level.into(),
                risk_score: score,
                deterioration_probability: (score * 0.85).min(1.0),
            },
            recommendations: RecommendationResult {
                drug_recommendation: "Consult specialist".into(),
                recommendations: vec!["Consult attending physician".into()],
                urgency: if priority == 1 { "emergency" } else if priority == 2 { "urgent" } else { "routine" }.into(),
            },
            routing: RoutingResult {
                route_to: if priority == 1 { "emergency" } else if priority == 2 { "specialist" } else { "gp" }.into(),
                department: dept.into(),
                alert_priority: priority,
            },
        }
    }
}

impl Default for MlService {
    fn default() -> Self {
        Self::new()
    }
}
