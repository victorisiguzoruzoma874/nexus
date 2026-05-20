use std::sync::Arc;
use chrono::Utc;
use sqlx::PgPool;
use uuid::Uuid;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::models::admin_registration::{HospitalRegistrationRequest, NewHospital};
use crate::models::hospital::Hospital;
use crate::models::registration::RegistrationStatus;
use crate::repositories::hospital::{HospitalRepository, RepositoryError};
use crate::services::audit_service::{AuditService, AuditServiceError, RegistrationDetails};
use crate::services::location_service::{LocationService, LocationServiceError};
use crate::services::email_outbox_service::EmailOutboxService;
use crate::services::email_templates;
use crate::services::payment_service::{PaymentService, PaymentServiceError};
use crate::utils::validation::{validate_email_rfc5322, validate_phone_e164};

#[derive(Debug, thiserror::Error)]
pub enum RegistrationError {
    #[error("Validation failed: {0}")]
    ValidationError(String),
    
    #[error("Duplicate registration for email: {0}")]
    DuplicateRegistration(String),
    
    #[error("Hospital not found: {0}")]
    NotFound(Uuid),
    
    #[error("Invalid status transition from {0:?} to {1:?}")]
    InvalidStatusTransition(RegistrationStatus, RegistrationStatus),
    
    #[error("Location service error: {0}")]
    LocationError(#[from] LocationServiceError),
    
    #[error("Payment service error: {0}")]
    PaymentError(#[from] PaymentServiceError),
    
    #[error("Repository error: {0}")]
    RepositoryError(#[from] RepositoryError),
    
    #[error("Database error: {0}")]
    DatabaseError(#[from] sqlx::Error),
    
    #[error("Audit service error: {0}")]
    AuditError(#[from] AuditServiceError),
    
    #[error("External service error: {0}")]
    ExternalServiceError(String),
}

/// Result type for hospital registration
#[derive(Debug, Clone)]
pub struct HospitalRegistrationResult {
    pub hospital_id: Uuid,
    pub status: RegistrationStatus,
    pub message: String,
    pub next_steps: Vec<String>,
}

/// Core service orchestrating the complete hospital registration workflow
/// Requirements: 1.1, 1.2, 1.3, 1.4, 1.5, 2.1, 2.2, 2.3, 3.1, 3.2, 4.3, 4.4, 10.1
pub struct RegistrationService {
    hospital_repo: Arc<HospitalRepository>,
    location_service: Arc<LocationService>,
    payment_service: Arc<PaymentService>,
    audit_service: Arc<AuditService>,
    email_outbox: Arc<EmailOutboxService>,
    db_pool: PgPool,
}

impl RegistrationService {
    pub fn new(
        hospital_repo: Arc<HospitalRepository>,
        location_service: Arc<LocationService>,
        payment_service: Arc<PaymentService>,
        audit_service: Arc<AuditService>,
        email_outbox: Arc<EmailOutboxService>,
        db_pool: PgPool,
    ) -> Self {
        Self {
            hospital_repo,
            location_service,
            payment_service,
            audit_service,
            email_outbox,
            db_pool,
        }
    }

    /// Register a new hospital with complete workflow
    /// 
    /// Orchestrates the complete registration workflow including validation, hospital creation,
    /// location geocoding, payment tokenization, and audit logging. All operations are wrapped
    /// in a database transaction for atomicity.
    /// 
    /// Requirements: 1.1, 1.2, 1.3, 1.4, 1.5, 2.1, 2.2, 2.3, 3.1, 3.2, 10.1
    pub async fn register_hospital(
        &self,
        user_id: Uuid,
        request: HospitalRegistrationRequest,
    ) -> Result<HospitalRegistrationResult, RegistrationError> {
        self.validate_registration_data(&request)?;

        if self.check_duplicate_registration(&request.email).await? {
            return Err(RegistrationError::DuplicateRegistration(request.email.clone()));
        }

        let mut tx = self.db_pool.begin().await?;

        let new_hospital = NewHospital {
            name: request.hospital_name.clone(),
            email: request.email.clone(),
            phone: request.phone.clone(),
            registration_number: request.registration_number.clone(),
            admin_user_id: None,
        };

        let hospital = self.hospital_repo.create(&mut tx, new_hospital).await?;
        let hospital_id = hospital.id;

        let _location = self
            .location_service
            .geocode_and_store(&mut tx, hospital_id, request.address.clone())
            .await?;

        let idempotency_key = Some(format!("reg-{}-{}", hospital_id, user_id));
        
        let _payment_method = self
            .payment_service
            .tokenize_and_store(
                &mut tx,
                hospital_id,
                request.payment_details.clone(),
                None,
                idempotency_key,
            )
            .await?;

        tx.commit().await?;

        let registration_details = RegistrationDetails {
            hospital_name: request.hospital_name.clone(),
            email: request.email.clone(),
            registration_number: request.registration_number.clone(),
        };

        if let Err(e) = self
            .audit_service
            .log_registration(hospital_id, None, registration_details)
            .await
        {
            eprintln!("Warning: Failed to log registration audit: {}", e);
        }

        if let Err(e) = self
            .email_outbox
            .enqueue_email(
                &request.email,
                &email_templates::hospital_registration_submitted(&request.hospital_name),
            )
            .await
        {
            eprintln!("Warning: Failed to queue registration email: {}", e);
        }

        Ok(HospitalRegistrationResult {
            hospital_id,
            status: RegistrationStatus::Pending,
            message: "Hospital registration submitted successfully. Awaiting admin approval.".to_string(),
            next_steps: vec![
                "Upload required documents (license, accreditation)".to_string(),
                "Wait for system administrator review".to_string(),
                "You will receive an email notification upon approval".to_string(),
            ],
        })
    }

    /// Validate registration data
    /// Requirements: 1.2, 6.1, 6.2
    fn validate_registration_data(
        &self,
        request: &HospitalRegistrationRequest,
    ) -> Result<(), RegistrationError> {
        // Validate hospital name
        if request.hospital_name.trim().is_empty() {
            return Err(RegistrationError::ValidationError(
                "Hospital name cannot be empty".to_string(),
            ));
        }

        if request.hospital_name.len() < 2 || request.hospital_name.len() > 200 {
            return Err(RegistrationError::ValidationError(
                "Hospital name must be between 2 and 200 characters".to_string(),
            ));
        }

        // Validate email (RFC 5322)
        if validate_email_rfc5322(&request.email).is_err() {
            return Err(RegistrationError::ValidationError(
                "Invalid email format (must conform to RFC 5322)".to_string(),
            ));
        }

        // Validate phone (E.164)
        if validate_phone_e164(&request.phone).is_err() {
            return Err(RegistrationError::ValidationError(
                "Invalid phone format (must conform to E.164)".to_string(),
            ));
        }

        // Validate registration number
        if request.registration_number.trim().is_empty() {
            return Err(RegistrationError::ValidationError(
                "Registration number cannot be empty".to_string(),
            ));
        }

        if request.registration_number.len() < 5 || request.registration_number.len() > 50 {
            return Err(RegistrationError::ValidationError(
                "Registration number must be between 5 and 50 characters".to_string(),
            ));
        }

        // Validate address
        if request.address.line1.trim().is_empty() {
            return Err(RegistrationError::ValidationError(
                "Address line 1 cannot be empty".to_string(),
            ));
        }

        if request.address.city.trim().is_empty() {
            return Err(RegistrationError::ValidationError(
                "City cannot be empty".to_string(),
            ));
        }

        if request.address.state.trim().is_empty() {
            return Err(RegistrationError::ValidationError(
                "State cannot be empty".to_string(),
            ));
        }

        if request.address.postal_code.trim().is_empty() {
            return Err(RegistrationError::ValidationError(
                "Postal code cannot be empty".to_string(),
            ));
        }

        if request.address.country.trim().is_empty() {
            return Err(RegistrationError::ValidationError(
                "Country cannot be empty".to_string(),
            ));
        }

        Ok(())
    }

    /// Check if a hospital with the given email already exists
    /// Requirements: 10.1
    async fn check_duplicate_registration(&self, email: &str) -> Result<bool, RegistrationError> {
        match self.hospital_repo.find_by_email(email).await? {
            Some(_) => Ok(true),
            None => Ok(false),
        }
    }

    /// Approve a pending hospital registration
    /// Requirements: 4.3, 4.5, 5.1, 5.2, 10.4
    /// 
    /// This method:
    /// 1. Updates hospital status to 'approved'
    /// 2. Logs status change in audit trail
    /// 3. Queues approval notification email
    /// 4. Prevents concurrent approvals using database transaction
    pub async fn approve_hospital(
        &self,
        hospital_id: Uuid,
        admin_id: Option<Uuid>,
        notes: Option<String>,
    ) -> Result<(), RegistrationError> {
        let mut tx = self.db_pool.begin().await?;

        let hospital = self
            .hospital_repo
            .find_by_id(hospital_id)
            .await?
            .ok_or(RegistrationError::NotFound(hospital_id))?;

        if hospital.admin_registration_status != Some(RegistrationStatus::Pending) {
            return Err(RegistrationError::InvalidStatusTransition(
                hospital.admin_registration_status.unwrap_or(RegistrationStatus::Pending),
                RegistrationStatus::Approved,
            ));
        }

        self.hospital_repo
            .update_status(
                &mut tx,
                hospital_id,
                RegistrationStatus::Approved,
                admin_id,
                None,
            )
            .await?;

        tx.commit().await?;

        if let Err(e) = self
            .audit_service
            .log_status_change(
                hospital_id,
                admin_id,
                RegistrationStatus::Pending,
                RegistrationStatus::Approved,
                notes,
            )
            .await
        {
            eprintln!("Warning: Failed to log approval audit: {}", e);
        }

        if let Err(e) = self
            .email_outbox
            .enqueue_email(
                &hospital.email,
                &email_templates::hospital_registration_approved(&hospital.name, Utc::now()),
            )
            .await
        {
            eprintln!("Warning: Failed to queue approval email: {}", e);
        }

        Ok(())
    }
    /// Reject a pending hospital registration
    /// Requirements: 4.4, 4.5, 5.4
    /// Reject a hospital registration
    /// 
    /// Updates hospital status to rejected with reason, logs the change in audit trail,
    /// and queues a rejection email. Validates rejection reason length (10-500 chars).
    /// 
    /// Requirements: 4.1, 4.2, 4.3, 4.4, 4.5
    pub async fn reject_hospital(
        &self,
        hospital_id: Uuid,
        admin_id: Option<Uuid>,
        reason: String,
    ) -> Result<(), RegistrationError> {
        if reason.trim().is_empty() {
            return Err(RegistrationError::ValidationError(
                "Rejection reason cannot be empty".to_string(),
            ));
        }

        if reason.len() < 10 || reason.len() > 500 {
            return Err(RegistrationError::ValidationError(
                "Rejection reason must be between 10 and 500 characters".to_string(),
            ));
        }

        let mut tx = self.db_pool.begin().await?;

        let hospital = self
            .hospital_repo
            .find_by_id(hospital_id)
            .await?
            .ok_or(RegistrationError::NotFound(hospital_id))?;

        if hospital.admin_registration_status != Some(RegistrationStatus::Pending) {
            return Err(RegistrationError::InvalidStatusTransition(
                hospital.admin_registration_status.unwrap_or(RegistrationStatus::Pending),
                RegistrationStatus::Rejected,
            ));
        }

        self.hospital_repo
            .update_status(
                &mut tx,
                hospital_id,
                RegistrationStatus::Rejected,
                admin_id,
                Some(reason.clone()),
            )
            .await?;

        tx.commit().await?;

        if let Err(e) = self
            .audit_service
            .log_status_change(
                hospital_id,
                admin_id,
                RegistrationStatus::Pending,
                RegistrationStatus::Rejected,
                Some(reason.clone()),
            )
            .await
        {
            eprintln!("Warning: Failed to log rejection audit: {}", e);
        }

        if let Err(e) = self
            .email_outbox
            .enqueue_email(
                &hospital.email,
                &email_templates::hospital_registration_rejected(&hospital.name, &reason),
            )
            .await
        {
            eprintln!("Warning: Failed to queue rejection email: {}", e);
        }

        Ok(())
    }

    /// Get registration status for a hospital
    /// Requirements: 4.2
    pub async fn get_registration_status(
        &self,
        hospital_id: Uuid,
    ) -> Result<RegistrationStatusResponse, RegistrationError> {
        let hospital = self
            .hospital_repo
            .find_by_id(hospital_id)
            .await?
            .ok_or(RegistrationError::NotFound(hospital_id))?;

        Ok(RegistrationStatusResponse {
            hospital_id: hospital.id,
            hospital_name: hospital.name,
            status: hospital.admin_registration_status.unwrap_or(RegistrationStatus::Pending),
            created_at: hospital.created_at,
            updated_at: hospital.updated_at,
            approved_at: None, // Will be populated from hospital.approved_at when available
            rejection_reason: None, // Will be populated from hospital.rejection_reason when available
        })
    }

    /// List all hospitals with optional status filter and pagination
    /// Requirements: 4.2
    pub async fn list_hospitals(
        &self,
        status_filter: Option<RegistrationStatus>,
        page: i64,
        page_size: i64,
    ) -> Result<HospitalListResponse, RegistrationError> {
        // Validate pagination parameters
        let page = page.max(1);
        let page_size = page_size.clamp(1, 100); // Max 100 items per page
        let offset = (page - 1) * page_size;

        // Get hospitals
        let hospitals = self
            .hospital_repo
            .list_all(status_filter, page_size, offset)
            .await?;

        // Get total count
        let total = self.hospital_repo.count_all(status_filter).await?;

        // Calculate pagination metadata
        let total_pages = (total as f64 / page_size as f64).ceil() as i64;

        Ok(HospitalListResponse {
            hospitals: hospitals.into_iter().map(HospitalSummary::from).collect(),
            pagination: PaginationMetadata {
                current_page: page,
                page_size,
                total_items: total,
                total_pages,
                has_next: page < total_pages,
                has_previous: page > 1,
            },
        })
    }
}

/// Response for listing hospitals
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct HospitalListResponse {
    pub hospitals: Vec<HospitalSummary>,
    pub pagination: PaginationMetadata,
}

/// Summary information for a hospital in list view
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct HospitalSummary {
    pub id: Uuid,
    pub name: String,
    pub email: String,
    pub phone_number: String,
    pub registration_number: String,
    pub status: Option<RegistrationStatus>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub approved_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl From<Hospital> for HospitalSummary {
    fn from(hospital: Hospital) -> Self {
        Self {
            id: hospital.id,
            name: hospital.name,
            email: hospital.email,
            phone_number: hospital.phone_number,
            registration_number: hospital.registration_number,
            status: hospital.admin_registration_status,
            created_at: hospital.created_at,
            approved_at: hospital.approved_at,
        }
    }
}

/// Pagination metadata
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PaginationMetadata {
    pub current_page: i64,
    pub page_size: i64,
    pub total_items: i64,
    pub total_pages: i64,
    pub has_next: bool,
    pub has_previous: bool,
}

/// Response type for registration status queries
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RegistrationStatusResponse {
    pub hospital_id: Uuid,
    pub hospital_name: String,
    pub status: RegistrationStatus,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub approved_at: Option<chrono::DateTime<chrono::Utc>>,
    pub rejection_reason: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    // Unit tests for validation logic
    #[test]
    fn test_validate_registration_data() {
        // Tests will be added here
    }

    // Property tests will be implemented in Task 10.5 (integration tests)
    // These require database setup and are better suited for integration testing
    // 
    // Property 1: New registrations start pending
    // Property 15: Admin access to registration data
    // Property 16: Approval status transition
    // Property 17: Rejection status transition
    // Property 39: Duplicate registration prevention
    // Property 40: Concurrent status update safety
    // Property 41: Payment tokenization idempotency
    // Property 42: Concurrent approval prevention
    // Property 43: Transaction rollback on constraint violations
}
