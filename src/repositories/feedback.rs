use sqlx::PgPool;

use crate::models::feedback::{CorrectionRequest, Feedback, OutcomeRequest};

pub struct FeedbackRepository {
    pool: PgPool,
}

impl FeedbackRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn record_correction(&self, req: &CorrectionRequest) -> Result<Feedback, sqlx::Error> {
        let row = sqlx::query!(
            r#"INSERT INTO feedback (patient_id, feedback_type, field, predicted, corrected, doctor_id, notes)
            VALUES ($1, 'correction', $2, $3, $4, $5, $6)
            RETURNING id, patient_id, feedback_type AS "feedback_type: FeedbackType",
                field, predicted, corrected, doctor_id,
                was_readmitted, treatment_worked, notes, created_at"#,
            req.patient_id,
            req.field,
            req.predicted,
            req.corrected,
            req.doctor_id,
            req.notes,
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(Feedback {
            id: row.id,
            patient_id: row.patient_id,
            feedback_type: row.r#feedback_type,
            field: row.field,
            predicted: row.predicted,
            corrected: row.corrected,
            doctor_id: row.doctor_id,
            was_readmitted: row.was_readmitted,
            treatment_worked: row.treatment_worked,
            notes: row.notes,
            created_at: row.created_at,
        })
    }

    pub async fn record_outcome(&self, req: &OutcomeRequest) -> Result<Feedback, sqlx::Error> {
        let row = sqlx::query!(
            r#"INSERT INTO feedback (patient_id, feedback_type, corrected, was_readmitted, treatment_worked, notes)
            VALUES ($1, 'outcome', $2, $3, $4, $5)
            RETURNING id, patient_id, feedback_type AS "feedback_type: FeedbackType",
                field, predicted, corrected, doctor_id,
                was_readmitted, treatment_worked, notes, created_at"#,
            req.patient_id,
            req.actual_diagnosis,
            req.was_readmitted,
            req.treatment_worked,
            req.notes,
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(Feedback {
            id: row.id,
            patient_id: row.patient_id,
            feedback_type: row.r#feedback_type,
            field: row.field,
            predicted: row.predicted,
            corrected: row.corrected,
            doctor_id: row.doctor_id,
            was_readmitted: row.was_readmitted,
            treatment_worked: row.treatment_worked,
            notes: row.notes,
            created_at: row.created_at,
        })
    }
}
