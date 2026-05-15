use std::sync::Arc;
use uuid::Uuid;

use crate::models::admin_registration::{NewBillingInfo, PaymentDetails};
use crate::models::billing::HospitalPaymentMethod;
use crate::repositories::billing::{BillingError, BillingRepository};
use crate::services::encryption::{EncryptionError, EncryptionService};
use crate::services::paystack::{PaystackClient, PaystackError};

#[derive(Debug, thiserror::Error)]
pub enum PaymentServiceError {
    #[error("Payment tokenization failed: {0}")]
    TokenizationFailed(#[from] PaystackError),
    
    #[error("Encryption failed: {0}")]
    EncryptionFailed(#[from] EncryptionError),
    
    #[error("Storage failed: {0}")]
    StorageFailed(#[from] BillingError),
}

/// Service for payment tokenization and secure storage (AC-03)
/// Requirements: 3.1, 3.2, 3.5
pub struct PaymentService {
    paystack_client: Arc<PaystackClient>,
    billing_repo: Arc<BillingRepository>,
    encryption_service: Arc<EncryptionService>,
}

impl PaymentService {
    pub fn new(
        paystack_client: Arc<PaystackClient>,
        billing_repo: Arc<BillingRepository>,
        encryption_service: Arc<EncryptionService>,
    ) -> Self {
        Self {
            paystack_client,
            billing_repo,
            encryption_service,
        }
    }

    /// Tokenize payment method and store encrypted token
    /// 
    /// Ensures raw payment data is NEVER stored. Flow: Raw data → Paystack (tokenize) 
    /// → Encrypt token → Store encrypted token.
    /// 
    /// Requirements: 3.1, 3.2, 3.5
    pub async fn tokenize_and_store(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        hospital_id: Uuid,
        payment_details: PaymentDetails,
        added_by: Option<Uuid>,
        idempotency_key: Option<String>,
    ) -> Result<HospitalPaymentMethod, PaymentServiceError> {
        let payment_token = self
            .paystack_client
            .tokenize_payment_method(&payment_details, idempotency_key)
            .await?;

        let encrypted_token = self.encryption_service.encrypt_token(&payment_token)?;

        let last_four = payment_details
            .card_number
            .as_ref()
            .and_then(|card| card.chars().rev().take(4).collect::<String>().chars().rev().collect::<String>().into());

        let new_billing = NewBillingInfo {
            hospital_id,
            payment_provider: "paystack".to_string(),
            encrypted_token,
            payment_method_type: payment_details.method_type,
            last_four,
        };

        let billing_info = self.billing_repo.create(tx, new_billing, added_by).await?;

        Ok(billing_info)
    }

    /// Validate that payment method is properly configured
    /// 
    /// Requirements: 3.5
    pub async fn validate_payment_method(
        &self,
        hospital_id: Uuid,
    ) -> Result<bool, PaymentServiceError> {
        match self.billing_repo.find_by_hospital_id(hospital_id).await? {
            Some(payment_method) => {
                Ok(payment_method.is_active && !payment_method.paystack_authorization_code.is_empty())
            }
            None => Ok(false),
        }
    }

    /// Mark payment setup as complete for a hospital
    /// 
    /// Requirements: 3.5
    pub async fn mark_payment_setup_complete(
        &self,
        hospital_id: Uuid,
    ) -> Result<bool, PaymentServiceError> {
        self.validate_payment_method(hospital_id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Integration tests will be added in Task 10.5
    // These require database setup
}
