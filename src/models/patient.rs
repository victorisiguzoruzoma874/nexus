use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;
use validator::Validate;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "disease_type", rename_all = "PascalCase")]
pub enum DiseaseType {
    Infectious,
    Chronic,
    Genetic,
    MentalHealth,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "severity_level", rename_all = "PascalCase")]
pub enum SeverityLevel {
    Mild,
    Moderate,
    Severe,
    Critical,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "mortality_risk", rename_all = "PascalCase")]
pub enum MortalityRisk {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "patient_category", rename_all = "PascalCase")]
pub enum PatientCategory {
    Child,
    Teenager,
    Adult,
    Elderly,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "pipeline_source", rename_all = "snake_case")]
pub enum PipelineSource {
    Clinical,
    PatientApp,
    EmrExport,
    Sensor,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Patient {
    pub id: Uuid,
    pub patient_id: String,
    pub full_name: String,
    pub gender: Option<String>,
    pub date_of_birth: Option<NaiveDate>,
    pub age: Option<i32>,
    pub marital_status: Option<String>,
    pub nationality: Option<String>,
    pub state: Option<String>,
    pub city: Option<String>,
    pub address: Option<String>,
    pub phone_number: Option<String>,
    pub emergency_contact: Option<String>,
    pub blood_group: Option<String>,
    pub genotype: Option<String>,
    pub height_cm: Option<f64>,
    pub weight_kg: Option<f64>,
    pub allergies: Option<String>,
    pub existing_conditions: Option<String>,
    pub disability_status: Option<String>,
    pub pregnancy_status: Option<bool>,
    pub vaccination_history: Option<String>,
    pub current_medications: Option<String>,
    pub disease_type: Option<DiseaseType>,
    pub symptoms: Option<String>,
    pub severity_level: Option<SeverityLevel>,
    pub symptom_start_date: Option<NaiveDate>,
    pub previous_medical_history: Option<String>,
    pub family_medical_history: Option<String>,
    pub weather_condition: Option<String>,
    pub temperature: Option<f64>,
    pub humidity: Option<f64>,
    pub occupation: Option<String>,
    pub smoking_status: Option<bool>,
    pub alcohol_consumption: Option<bool>,
    pub exercise_habits: Option<String>,
    pub diet_type: Option<String>,
    pub water_source: Option<String>,
    pub patient_category: Option<PatientCategory>,
    pub disease_trends: Option<String>,
    pub outbreak_detection: Option<bool>,
    pub predictive_risk_score: Option<f64>,
    pub readmission_prediction: Option<String>,
    pub mortality_risk: Option<MortalityRisk>,
    pub drug_recommendation: Option<String>,
    pub pattern_recognition: Option<bool>,
    pub source: Option<PipelineSource>,
    pub raw_blob_path: Option<String>,
    pub pipeline_processed: Option<bool>,
    pub silver_processed: Option<bool>,
    pub gold_processed: Option<bool>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct IngestPatientDto {
    #[validate(length(min = 2, max = 255))]
    pub full_name: String,
    pub gender: Option<String>,
    pub date_of_birth: Option<NaiveDate>,
    pub age: Option<i32>,
    pub marital_status: Option<String>,
    pub nationality: Option<String>,
    pub state: Option<String>,
    pub city: Option<String>,
    pub address: Option<String>,
    pub phone_number: Option<String>,
    pub emergency_contact: Option<String>,
    pub blood_group: Option<String>,
    pub genotype: Option<String>,
    pub height_cm: Option<f64>,
    pub weight_kg: Option<f64>,
    pub allergies: Option<String>,
    pub existing_conditions: Option<String>,
    pub disability_status: Option<String>,
    pub pregnancy_status: Option<bool>,
    pub vaccination_history: Option<String>,
    pub current_medications: Option<String>,
    pub disease_type: Option<DiseaseType>,
    pub symptoms: Option<String>,
    pub severity_level: Option<SeverityLevel>,
    pub symptom_start_date: Option<NaiveDate>,
    pub previous_medical_history: Option<String>,
    pub family_medical_history: Option<String>,
    pub weather_condition: Option<String>,
    pub temperature: Option<f64>,
    pub humidity: Option<f64>,
    pub occupation: Option<String>,
    pub smoking_status: Option<bool>,
    pub alcohol_consumption: Option<bool>,
    pub exercise_habits: Option<String>,
    pub diet_type: Option<String>,
    pub water_source: Option<String>,
    pub patient_category: Option<PatientCategory>,
    pub source: Option<PipelineSource>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngestResponse {
    pub patient_id: String,
    pub message: &'static str,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineStats {
    pub total: i64,
    pub pending_silver: i64,
    pub pending_gold: i64,
    pub high_risk: i64,
    pub outbreak_active: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PatientListQuery {
    pub risk: Option<String>,
    pub state: Option<String>,
    pub page: Option<i64>,
    pub page_size: Option<i64>,
}
