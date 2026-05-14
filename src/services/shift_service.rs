use std::sync::Arc;
use sqlx::PgPool;
use uuid::Uuid;

use crate::models::shift::{Shift, CreateShiftRequest, ShiftType};
use crate::repositories::shift::ShiftRepository;

#[derive(Debug, thiserror::Error)]
pub enum ShiftServiceError {
    #[error("Validation failed: {0}")]
    ValidationError(String),
    
    #[error("Database error: {0}")]
    DatabaseError(#[from] sqlx::Error),
    
    #[error("Shift not found: {0}")]
    NotFound(Uuid),
}

pub struct ShiftService {
    shift_repo: Arc<ShiftRepository>,
    pool: PgPool,
}

impl ShiftService {
    pub fn new(shift_repo: Arc<ShiftRepository>, pool: PgPool) -> Self {
        Self { shift_repo, pool }
    }

    pub async fn create_shift(
        &self,
        hospital_id: Uuid,
        created_by: Uuid,
        request: CreateShiftRequest,
    ) -> Result<Shift, ShiftServiceError> {
        // Validate required fields based on pay type
        self.validate_request(&request)?;

        let mut tx = self.pool.begin().await?;

        // Create shift
        let shift = self.shift_repo.create(&mut tx, hospital_id, created_by, request).await?;

        // Generate virtual link for virtual shifts
        let _virtual_link = if shift.shift_type == ShiftType::Virtual {
            Some(self.generate_virtual_link(shift.id))
        } else {
            None
        };

        // Broadcast shift (simulate matching clinicians)
        let matched_count = self.calculate_matched_clinicians(&shift).await;
        self.shift_repo.broadcast_shift(&mut tx, shift.id, matched_count).await?;

        tx.commit().await?;

        Ok(shift)
    }

    pub async fn get_shift(&self, shift_id: Uuid) -> Result<Shift, ShiftServiceError> {
        self.shift_repo
            .get_by_id(shift_id)
            .await?
            .ok_or(ShiftServiceError::NotFound(shift_id))
    }

    fn validate_request(&self, request: &CreateShiftRequest) -> Result<(), ShiftServiceError> {
        // Validate pay type requirements
        match request.pay_type {
            crate::models::shift::PayType::HourlyRate => {
                if request.rate_kobo_per_hour.is_none() {
                    return Err(ShiftServiceError::ValidationError(
                        "Hourly rate is required for hourly pay type".to_string(),
                    ));
                }
            }
            crate::models::shift::PayType::FixedRate => {
                if request.fixed_rate_kobo.is_none() {
                    return Err(ShiftServiceError::ValidationError(
                        "Fixed rate is required for fixed pay type".to_string(),
                    ));
                }
            }
        }

        // Validate STAT/Urgent logic
        match request.priority {
            crate::models::shift::ShiftPriority::Stat | crate::models::shift::ShiftPriority::Urgent => {
                if request.urgency_bonus_pct.is_none() && request.stat_bonus_kobo.is_none() {
                    return Err(ShiftServiceError::ValidationError(
                        "STAT/Urgent shifts require urgency bonus or stat bonus".to_string(),
                    ));
                }
            }
            _ => {}
        }

        // Validate broadcast consent
        if !request.broadcast_consent_confirmed {
            return Err(ShiftServiceError::ValidationError(
                "Broadcast consent must be confirmed".to_string(),
            ));
        }

        Ok(())
    }

    fn generate_virtual_link(&self, shift_id: Uuid) -> String {
        format!("https://meet.nexuscare.com/shift/{}", shift_id)
    }

    async fn calculate_matched_clinicians(&self, _shift: &Shift) -> i32 {
        // Simplified: return a mock count
        // In production, this would query clinicians based on specialty, location, availability
        48
    }
}
