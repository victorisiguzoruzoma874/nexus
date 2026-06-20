use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "feedback_type", rename_all = "snake_case")]
pub enum FeedbackType {
    Correction,
    Outcome,
    EmrDischarge,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Feedback {
    pub id: Uuid,
    pub patient_id: String,
    pub feedback_type: FeedbackType,
    pub field: Option<String>,
    pub predicted: Option<String>,
    pub corrected: Option<String>,
    pub doctor_id: Option<String>,
    pub was_readmitted: Option<bool>,
    pub treatment_worked: Option<bool>,
    pub notes: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CorrectionRequest {
    pub patient_id: String,
    pub doctor_id: String,
    pub field: String,
    pub predicted: String,
    pub corrected: String,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutcomeRequest {
    pub patient_id: String,
    pub actual_diagnosis: Option<String>,
    pub was_readmitted: bool,
    pub treatment_worked: bool,
    pub notes: Option<String>,
}
