use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;
use utoipa::ToSchema;
use validator::Validate;

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, sqlx::Type, ToSchema)]
#[sqlx(type_name = "clinician_role", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum ClinicianRole {
    Doctor,
    Nurse,
    LabTechnician,
    Pharmacist,
    Radiographer,
    Physiotherapist,
    Other,
}

// ---------------------------------------------------------------------------
// DB row types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, FromRow)]
pub struct OtpCode {
    pub id: Uuid,
    pub phone: String,
    pub code: String,
    pub expires_at: DateTime<Utc>,
    pub used: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow)]
pub struct ClinicianBankAccount {
    pub id: Uuid,
    pub clinician_id: Uuid,
    pub account_number: String, // encrypted at rest
    pub bank_code: String,
    pub account_name: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// Request / response types
// ---------------------------------------------------------------------------

/// AC-01: Send OTP to phone number
#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct SendOtpRequest {
    #[validate(length(min = 7, max = 20, message = "Phone must be in E.164 format"))]
    #[schema(example = "+2348012345678")]
    pub phone: String,
}

/// AC-02: Verify OTP and create account
#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct VerifyOtpRequest {
    #[validate(length(min = 7, max = 20))]
    pub phone: String,
    #[validate(length(equal = 6, message = "OTP must be 6 digits"))]
    pub otp: String,
}

/// AC-03: Complete clinician profile
#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct CompleteProfileRequest {
    #[validate(length(min = 1, max = 100))]
    pub first_name: String,
    #[validate(length(min = 1, max = 100))]
    pub last_name: String,
    pub role: ClinicianRole,
    #[validate(length(min = 2, max = 100))]
    pub license_number: String,
    pub specialty: crate::models::clinician::ClinicalSpecialty,
}

/// AC-04: Add bank account
#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct AddBankAccountRequest {
    #[validate(length(equal = 10, message = "Account number must be 10 digits"))]
    pub account_number: String,
    #[validate(length(min = 3, max = 10))]
    pub bank_code: String,
}

// ---------------------------------------------------------------------------
// Responses
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, ToSchema)]
pub struct SendOtpResponse {
    pub message: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct VerifyOtpResponse {
    pub clinician_id: Uuid,
    pub access_token: String,
    pub message: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ProfileResponse {
    pub clinician_id: Uuid,
    pub first_name: String,
    pub last_name: String,
    pub role: ClinicianRole,
    pub license_number: String,
    pub phone: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct BankAccountResponse {
    pub account_name: String,
    pub account_number_masked: String,
    pub bank_code: String,
}
