use std::sync::Arc;
use chrono::{Duration, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::models::clinician_registration::{
    AddBankAccountRequest, BankAccountResponse, CompleteProfileRequest,
    ProfileResponse, SendOtpResponse, VerifyOtpResponse,
};
use crate::repositories::clinician::{ClinicianRepoError, ClinicianRepository};
use crate::services::paystack::{PaystackClient, PaystackError};
use crate::services::sms_service::{SmsError, SmsService};
use crate::services::encryption::EncryptionService;
use crate::utils::validation::validate_phone_e164;

#[derive(Debug, thiserror::Error)]
pub enum ClinicianRegistrationError {
    #[error("Phone number already registered")]
    DuplicatePhone,
    #[error("Invalid or expired OTP")]
    InvalidOtp,
    #[error("Validation error: {0}")]
    Validation(String),
    #[error("SMS error: {0}")]
    Sms(#[from] SmsError),
    #[error("Payment error: {0}")]
    Payment(#[from] PaystackError),
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("Repository error: {0}")]
    Repository(#[from] ClinicianRepoError),
    #[error("Encryption error: {0}")]
    Encryption(String),
    #[error("Clinician not found")]
    NotFound,
}

pub struct ClinicianRegistrationService {
    repo: Arc<ClinicianRepository>,
    sms: Arc<SmsService>,
    paystack: Arc<PaystackClient>,
    encryption: Arc<EncryptionService>,
    pool: PgPool,
}

impl ClinicianRegistrationService {
    pub fn new(
        repo: Arc<ClinicianRepository>,
        sms: Arc<SmsService>,
        paystack: Arc<PaystackClient>,
        encryption: Arc<EncryptionService>,
        pool: PgPool,
    ) -> Self {
        Self { repo, sms, paystack, encryption, pool }
    }

    // -----------------------------------------------------------------------
    // AC-01: Send OTP
    // -----------------------------------------------------------------------
    pub async fn send_otp(
        &self,
        phone: &str,
    ) -> Result<SendOtpResponse, ClinicianRegistrationError> {
        validate_phone_e164(phone)
            .map_err(|e| ClinicianRegistrationError::Validation(e.to_string()))?;

        // AC-05: Duplicate prevention
        if self.repo.phone_exists(phone).await? {
            return Err(ClinicianRegistrationError::DuplicatePhone);
        }

        let code = generate_otp();
        let expires_at = Utc::now() + Duration::minutes(10);

        // Persist OTP
        sqlx::query(
            "INSERT INTO otp_codes (phone, code, expires_at) VALUES ($1, $2, $3)",
        )
        .bind(phone)
        .bind(&code)
        .bind(expires_at)
        .execute(&self.pool)
        .await?;

        self.sms.send_otp(phone, &code).await?;

        Ok(SendOtpResponse {
            message: "OTP sent successfully".to_string(),
        })
    }

    // -----------------------------------------------------------------------
    // AC-02: Verify OTP → create account → return JWT
    // -----------------------------------------------------------------------
    pub async fn verify_otp(
        &self,
        phone: &str,
        otp: &str,
    ) -> Result<VerifyOtpResponse, ClinicianRegistrationError> {
        // Fetch the latest unused, non-expired OTP for this phone
        let row: Option<(Uuid,)> = sqlx::query_as(
            r#"
            SELECT id FROM otp_codes
            WHERE phone = $1 AND code = $2 AND used = FALSE AND expires_at > NOW()
            ORDER BY created_at DESC
            LIMIT 1
            "#,
        )
        .bind(phone)
        .bind(otp)
        .fetch_optional(&self.pool)
        .await?;

        let otp_id = row.map(|(id,)| id).ok_or(ClinicianRegistrationError::InvalidOtp)?;

        // Mark OTP as used
        sqlx::query("UPDATE otp_codes SET used = TRUE WHERE id = $1")
            .bind(otp_id)
            .execute(&self.pool)
            .await?;

        // Create user + clinician in a transaction
        let mut tx = self.pool.begin().await?;
        let clinician_id = self.repo.create_clinician(&mut tx, phone).await?;
        tx.commit().await?;

        let token = issue_jwt(clinician_id);

        Ok(VerifyOtpResponse {
            clinician_id,
            access_token: token,
            message: "Account created successfully".to_string(),
        })
    }

    // -----------------------------------------------------------------------
    // AC-03: Complete profile
    // -----------------------------------------------------------------------
    pub async fn complete_profile(
        &self,
        clinician_id: Uuid,
        req: CompleteProfileRequest,
    ) -> Result<ProfileResponse, ClinicianRegistrationError> {
        if req.first_name.trim().is_empty() || req.last_name.trim().is_empty() {
            return Err(ClinicianRegistrationError::Validation(
                "Full name is required".to_string(),
            ));
        }
        if req.license_number.trim().is_empty() {
            return Err(ClinicianRegistrationError::Validation(
                "License number is required".to_string(),
            ));
        }

        self.repo
            .update_profile(
                clinician_id,
                &req.first_name,
                &req.last_name,
                &req.role,
                &req.license_number,
                &req.specialty,
            )
            .await?;

        let phone = self
            .repo
            .find_phone_by_clinician_id(clinician_id)
            .await?
            .unwrap_or_default();

        Ok(ProfileResponse {
            clinician_id,
            first_name: req.first_name,
            last_name: req.last_name,
            role: req.role,
            license_number: req.license_number,
            phone,
        })
    }

    // -----------------------------------------------------------------------
    // AC-04: Add bank account (Paystack validation)
    // -----------------------------------------------------------------------
    pub async fn add_bank_account(
        &self,
        clinician_id: Uuid,
        req: AddBankAccountRequest,
    ) -> Result<BankAccountResponse, ClinicianRegistrationError> {
        // Validate with Paystack
        let resolved = self
            .paystack
            .resolve_bank_account(&req.account_number, &req.bank_code)
            .await?;

        // Encrypt account number before storage
        let encrypted = self
            .encryption
            .encrypt_token(&req.account_number)
            .map_err(|e| ClinicianRegistrationError::Encryption(e.to_string()))?;

        self.repo
            .upsert_bank_account(clinician_id, &encrypted, &req.bank_code, &resolved.account_name)
            .await?;

        let masked = mask_account(&req.account_number);

        Ok(BankAccountResponse {
            account_name: resolved.account_name,
            account_number_masked: masked,
            bank_code: req.bank_code,
        })
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn generate_otp() -> String {
    // Use 4 bytes from a random UUID to produce a 6-digit code
    let id = Uuid::new_v4();
    let bytes = id.as_bytes();
    let n = u32::from_be_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]) % 1_000_000;
    format!("{:06}", n)
}

fn mask_account(account: &str) -> String {
    if account.len() <= 4 {
        return "*".repeat(account.len());
    }
    format!("{}****{}", &account[..3], &account[account.len() - 3..])
}

/// Minimal JWT issuance — reuses the same secret as the rest of the app.
fn issue_jwt(clinician_id: Uuid) -> String {
    use chrono::Utc;
    use jsonwebtoken::{encode, EncodingKey, Header};
    use serde::{Deserialize, Serialize};

    #[derive(Serialize, Deserialize)]
    struct Claims {
        sub: String,
        role: String,
        exp: usize,
        iat: usize,
    }

    let secret = std::env::var("JWT_SECRET").unwrap_or_else(|_| "dev_secret".to_string());
    let now = Utc::now().timestamp() as usize;
    let claims = Claims {
        sub: clinician_id.to_string(),
        role: "staff".to_string(),
        exp: now + 86400,
        iat: now,
    };

    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .unwrap_or_else(|_| "token_error".to_string())
}

// ---------------------------------------------------------------------------
// Extend ClinicianRepository with a reverse lookup helper is in repositories/clinician.rs

#[cfg(test)]
mod tests {
    use super::*;

    // --- OTP generation ---

    #[test]
    fn otp_is_six_digits() {
        for _ in 0..20 {
            let code = generate_otp();
            assert_eq!(code.len(), 6, "OTP must be 6 chars: {}", code);
            assert!(code.chars().all(|c| c.is_ascii_digit()), "OTP must be numeric: {}", code);
        }
    }

    #[test]
    fn otp_values_in_range() {
        for _ in 0..50 {
            let code = generate_otp();
            let n: u32 = code.parse().expect("OTP must parse as u32");
            assert!(n < 1_000_000, "OTP out of range: {}", n);
        }
    }

    // --- Account masking ---

    #[test]
    fn mask_account_hides_middle_digits() {
        let masked = mask_account("0123456789");
        assert!(masked.starts_with("012"), "Should keep first 3: {}", masked);
        assert!(masked.ends_with("789"), "Should keep last 3: {}", masked);
        assert!(masked.contains("****"), "Should have mask: {}", masked);
    }

    #[test]
    fn mask_account_short_input() {
        let masked = mask_account("123");
        assert_eq!(masked, "***");
    }

    // --- Phone validation (via validate_phone_e164) ---

    #[test]
    fn valid_e164_phones_accepted() {
        use crate::utils::validation::validate_phone_e164;
        assert!(validate_phone_e164("+2348012345678").is_ok());
        assert!(validate_phone_e164("+14155552671").is_ok());
    }

    #[test]
    fn invalid_phones_rejected() {
        use crate::utils::validation::validate_phone_e164;
        assert!(validate_phone_e164("08012345678").is_err()); // no +
        assert!(validate_phone_e164("+0123456789").is_err()); // starts with 0
        assert!(validate_phone_e164("not-a-phone").is_err());
    }

    // --- Bank account validation (Paystack mock) ---

    #[tokio::test]
    async fn paystack_mock_resolves_bank_account() {
        use crate::services::paystack::PaystackClient;
        let client = PaystackClient::new("sk_test_dummy".to_string(), None);
        let result = client.resolve_bank_account("0123456789", "058").await;
        assert!(result.is_ok(), "Mock should succeed: {:?}", result);
        let resolved = result.unwrap();
        assert!(!resolved.account_name.is_empty());
    }

    #[tokio::test]
    async fn paystack_mock_returns_account_number() {
        use crate::services::paystack::PaystackClient;
        let client = PaystackClient::new("sk_test_dummy".to_string(), None);
        let result = client.resolve_bank_account("0123456789", "058").await.unwrap();
        assert_eq!(result.account_number, "0123456789");
    }
}
