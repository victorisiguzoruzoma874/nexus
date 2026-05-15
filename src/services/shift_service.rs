use std::sync::Arc;
use sqlx::PgPool;
use uuid::Uuid;
use chrono::{Utc, Duration};

use crate::models::shift::{Shift, CreateShiftRequest, ShiftType, ShiftPriority};
use crate::repositories::shift::ShiftRepository;
use crate::services::notification_service::NotificationService;

#[derive(Debug, thiserror::Error)]
pub enum ShiftServiceError {
    #[error("Validation failed: {0}")]
    ValidationError(String),
    
    #[error("Database error: {0}")]
    DatabaseError(#[from] sqlx::Error),
    
    #[error("Shift not found: {0}")]
    NotFound(Uuid),
    
    #[error("Duplicate shift: {0}")]
    DuplicateShift(String),
    
    #[error("Hospital not approved: {0}")]
    HospitalNotApproved(String),
}

pub struct ShiftService {
    shift_repo: Arc<ShiftRepository>,
    pool: PgPool,
    notification_service: Arc<NotificationService>,
}

impl ShiftService {
    pub fn new(shift_repo: Arc<ShiftRepository>, pool: PgPool, notification_service: Arc<NotificationService>) -> Self {
        Self { shift_repo, pool, notification_service }
    }

    pub async fn create_shift(
        &self,
        hospital_id: Uuid,
        created_by: Uuid,
        request: CreateShiftRequest,
    ) -> Result<Shift, ShiftServiceError> {
        // Check if hospital is approved
        let is_approved = self.shift_repo.check_hospital_approved(hospital_id).await?;
        if !is_approved {
            return Err(ShiftServiceError::HospitalNotApproved(
                "Only approved hospitals can create shifts. Please complete your registration and wait for approval.".to_string()
            ));
        }

        // Validate required fields based on pay type
        self.validate_request(&request)?;

        // AC-08: Check for duplicate shifts
        self.check_duplicate_shift(hospital_id, &request).await?;

        let mut tx = self.pool.begin().await?;

        // Create shift
        let shift = self.shift_repo.create(&mut tx, hospital_id, created_by, request).await?;

        // AC-04: Generate virtual link for virtual shifts
        if shift.shift_type == ShiftType::Virtual {
            let virtual_link = self.generate_virtual_link(shift.id);
            self.shift_repo.update_virtual_link(&mut tx, shift.id, &virtual_link).await?;
        }

        // Broadcast shift (calculate matching clinicians)
        let matched_count = self.calculate_matched_clinicians(&shift).await;
        self.shift_repo.broadcast_shift(&mut tx, shift.id, matched_count).await?;

        tx.commit().await?;

        // AC-07: Send push notifications to eligible workers
        self.broadcast_shift_notifications(shift.id, hospital_id, matched_count).await?;

        Ok(shift)
    }

    pub async fn get_shift(&self, shift_id: Uuid) -> Result<Shift, ShiftServiceError> {
        self.shift_repo
            .get_by_id(shift_id)
            .await?
            .ok_or(ShiftServiceError::NotFound(shift_id))
    }

    fn validate_request(&self, request: &CreateShiftRequest) -> Result<(), ShiftServiceError> {
        // AC-02: Validate required fields
        if request.role_title.trim().is_empty() {
            return Err(ShiftServiceError::ValidationError(
                "Role title is required".to_string(),
            ));
        }

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

        // AC-03: Validate STAT shift logic - start time must be within one hour
        if request.priority == ShiftPriority::Stat {
            let now = Utc::now();
            let time_until_start = request.scheduled_start.signed_duration_since(now);
            
            if time_until_start > Duration::hours(1) {
                return Err(ShiftServiceError::ValidationError(
                    "STAT shifts must start within one hour".to_string(),
                ));
            }

            // AC-03: STAT shifts must have bonus payment
            if request.urgency_bonus_pct.is_none() && request.stat_bonus_kobo.is_none() {
                return Err(ShiftServiceError::ValidationError(
                    "STAT shifts require urgency bonus or stat bonus".to_string(),
                ));
            }
        }

        // Validate Urgent logic
        if request.priority == ShiftPriority::Urgent {
            if request.urgency_bonus_pct.is_none() && request.stat_bonus_kobo.is_none() {
                return Err(ShiftServiceError::ValidationError(
                    "Urgent shifts require urgency bonus or stat bonus".to_string(),
                ));
            }
        }

        // Validate broadcast consent
        if !request.broadcast_consent_confirmed {
            return Err(ShiftServiceError::ValidationError(
                "Broadcast consent must be confirmed".to_string(),
            ));
        }

        Ok(())
    }

    /// AC-08: Check for duplicate shifts within the last hour
    async fn check_duplicate_shift(
        &self,
        hospital_id: Uuid,
        request: &CreateShiftRequest,
    ) -> Result<(), ShiftServiceError> {
        let one_hour_ago = Utc::now() - Duration::hours(1);
        
        let duplicate = self.shift_repo.find_similar_shift(
            hospital_id,
            &request.role_title,
            request.scheduled_start,
            one_hour_ago,
        ).await?;

        if duplicate.is_some() {
            return Err(ShiftServiceError::DuplicateShift(
                "Similar shift already exists.".to_string(),
            ));
        }

        Ok(())
    }

    /// AC-04: Generate virtual meeting link for virtual shifts
    fn generate_virtual_link(&self, shift_id: Uuid) -> String {
        format!("https://meet.nexuscare.com/shift/{}", shift_id)
    }

    /// AC-05: Calculate matched clinicians based on shift type and location
    async fn calculate_matched_clinicians(&self, shift: &Shift) -> i32 {
        // AC-05: For in-person shifts, apply 5km distance restriction
        // AC-04: For virtual shifts, no distance restriction
        let _distance_km = match shift.shift_type {
            ShiftType::InPerson => Some(5.0),
            ShiftType::Virtual => None,
        };

        // In production, this would query clinicians based on:
        // - specialty matching shift.specialty
        // - location within distance_km (for in-person)
        // - availability matching shift.scheduled_start
        // - verified status
        
        // Mock implementation
        match shift.shift_type {
            ShiftType::InPerson => 48, // Fewer matches due to distance restriction
            ShiftType::Virtual => 85,  // More matches, no distance restriction
        }
    }

    /// AC-07: Broadcast shift notifications to eligible workers
    async fn broadcast_shift_notifications(
        &self,
        shift_id: Uuid,
        hospital_id: Uuid,
        matched_count: i32,
    ) -> Result<(), ShiftServiceError> {
        // Send push notifications to all eligible clinicians
        self.notification_service
            .send_shift_broadcast_notification(shift_id, hospital_id, matched_count)
            .await
            .map_err(|e| ShiftServiceError::ValidationError(format!("Failed to send notifications: {}", e)))?;

        tracing::info!(
            "Broadcast notifications sent for shift {} to {} eligible workers",
            shift_id,
            matched_count
        );

        Ok(())
    }

    /// AC-06: Preview shift before publishing
    pub async fn preview_shift(&self, request: &CreateShiftRequest) -> Result<ShiftPreview, ShiftServiceError> {
        // Validate the request first
        self.validate_request(request)?;

        // Calculate compensation
        let (base_amount, stat_bonus, grand_total) = self.calculate_preview_compensation(request);

        // Generate preview
        Ok(ShiftPreview {
            role_title: request.role_title.clone(),
            specialty: request.specialty.clone(),
            department: request.department.clone(),
            shift_type: request.shift_type.clone(),
            priority: request.priority.clone(),
            scheduled_start: request.scheduled_start,
            duration_hours: request.duration_hours,
            base_amount_kobo: base_amount,
            stat_bonus_kobo: stat_bonus,
            grand_total_kobo: grand_total,
            virtual_link: if request.shift_type == ShiftType::Virtual {
                Some("https://meet.nexuscare.com/shift/preview".to_string())
            } else {
                None
            },
            estimated_matches: match request.shift_type {
                ShiftType::InPerson => 48,
                ShiftType::Virtual => 85,
            },
        })
    }

    fn calculate_preview_compensation(&self, request: &CreateShiftRequest) -> (i64, i64, i64) {
        use crate::models::shift::PayType;

        let base_amount = match request.pay_type {
            PayType::HourlyRate => {
                request.rate_kobo_per_hour
                    .map(|rate| (rate as f64 * request.duration_hours as f64) as i64)
                    .unwrap_or(0)
            }
            PayType::FixedRate => request.fixed_rate_kobo.unwrap_or(0),
        };

        let stat_bonus = request.stat_bonus_kobo.unwrap_or(0);
        let grand_total = base_amount + stat_bonus;

        (base_amount, stat_bonus, grand_total)
    }
}

/// AC-06: Shift preview response
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ShiftPreview {
    pub role_title: String,
    pub specialty: Option<String>,
    pub department: Option<String>,
    pub shift_type: ShiftType,
    pub priority: ShiftPriority,
    pub scheduled_start: chrono::DateTime<Utc>,
    pub duration_hours: f32,
    pub base_amount_kobo: i64,
    pub stat_bonus_kobo: i64,
    pub grand_total_kobo: i64,
    pub virtual_link: Option<String>,
    pub estimated_matches: i32,
}
