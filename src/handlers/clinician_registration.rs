use axum::{extract::State, http::StatusCode, Json};
use uuid::Uuid;
use validator::Validate;

use crate::models::clinician_registration::{
    AddBankAccountRequest, BankAccountResponse, CompleteProfileRequest,
    ProfileResponse, SendOtpRequest, SendOtpResponse, VerifyOtpRequest, VerifyOtpResponse,
};
use crate::routes::AppState;
use crate::services::clinician_registration_service::ClinicianRegistrationError;
use crate::utils::errors::{AppError, AppResult};

/// POST /api/v1/clinicians/otp/send
/// AC-01: Send OTP to phone number
pub async fn send_otp(
    State(state): State<AppState>,
    Json(req): Json<SendOtpRequest>,
) -> AppResult<(StatusCode, Json<SendOtpResponse>)> {
    req.validate().map_err(|e| AppError::Validation(e.to_string()))?;

    state
        .clinician_registration_service
        .send_otp(&req.phone)
        .await
        .map(|r| (StatusCode::OK, Json(r)))
        .map_err(map_err)
}

/// POST /api/v1/clinicians/otp/verify
/// AC-02: Verify OTP and create account
pub async fn verify_otp(
    State(state): State<AppState>,
    Json(req): Json<VerifyOtpRequest>,
) -> AppResult<(StatusCode, Json<VerifyOtpResponse>)> {
    req.validate().map_err(|e| AppError::Validation(e.to_string()))?;

    state
        .clinician_registration_service
        .verify_otp(&req.phone, &req.otp)
        .await
        .map(|r| (StatusCode::CREATED, Json(r)))
        .map_err(map_err)
}

/// PUT /api/v1/clinicians/{clinician_id}/profile
/// AC-03: Complete profile
pub async fn complete_profile(
    State(state): State<AppState>,
    axum::extract::Path(clinician_id): axum::extract::Path<Uuid>,
    Json(req): Json<CompleteProfileRequest>,
) -> AppResult<Json<ProfileResponse>> {
    req.validate().map_err(|e| AppError::Validation(e.to_string()))?;

    state
        .clinician_registration_service
        .complete_profile(clinician_id, req)
        .await
        .map(Json)
        .map_err(map_err)
}

/// POST /api/v1/clinicians/{clinician_id}/bank-account
/// AC-04: Add and validate bank account
pub async fn add_bank_account(
    State(state): State<AppState>,
    axum::extract::Path(clinician_id): axum::extract::Path<Uuid>,
    Json(req): Json<AddBankAccountRequest>,
) -> AppResult<Json<BankAccountResponse>> {
    req.validate().map_err(|e| AppError::Validation(e.to_string()))?;

    state
        .clinician_registration_service
        .add_bank_account(clinician_id, req)
        .await
        .map(Json)
        .map_err(map_err)
}

fn map_err(e: ClinicianRegistrationError) -> AppError {
    match e {
        ClinicianRegistrationError::DuplicatePhone => {
            AppError::Conflict("Phone number already registered".to_string())
        }
        ClinicianRegistrationError::InvalidOtp => {
            AppError::Validation("Invalid or expired OTP".to_string())
        }
        ClinicianRegistrationError::Validation(msg) => AppError::Validation(msg),
        ClinicianRegistrationError::NotFound => {
            AppError::NotFound("Clinician not found".to_string())
        }
        ClinicianRegistrationError::Payment(e) => {
            AppError::Validation(format!("Bank account validation failed: {}", e))
        }
        e => AppError::Internal(anyhow::anyhow!("{}", e)),
    }
}
