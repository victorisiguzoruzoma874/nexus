use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::models::admin_registration::NewBillingInfo;
use crate::models::billing::HospitalPaymentMethod;

#[derive(Debug, thiserror::Error)]
pub enum BillingError {
    #[error("Database error: {0}")]
    DatabaseError(#[from] sqlx::Error),
    
    #[error("Billing info not found for hospital: {0}")]
    NotFound(Uuid),
}

/// Repository for billing and payment method data persistence
/// Stores encrypted payment tokens (AC-03)
pub struct BillingRepository {
    pool: PgPool,
}

impl BillingRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Create a new billing info record within a transaction
    /// Requirements: 3.2, 3.3
    pub async fn create(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        billing: NewBillingInfo,
        added_by: Option<Uuid>,
    ) -> Result<HospitalPaymentMethod, BillingError> {
        let result = sqlx::query_as::<_, HospitalPaymentMethod>(
            r#"
            INSERT INTO hospital_payment_methods (
                hospital_id,
                paystack_authorization_code,
                encrypted_token,
                cardholder_name,
                card_last_four,
                card_type,
                card_expiry,
                payment_method_type,
                is_default,
                is_active,
                added_by
            )
            VALUES ($1, $2, $3, $4, $5, 'unknown'::card_type, $6, $7, TRUE, TRUE, $8)
            RETURNING 
                id, hospital_id, paystack_authorization_code, paystack_customer_code,
                cardholder_name, card_last_four, card_type, card_expiry, bank_name,
                is_default, is_active, added_by, created_at, updated_at
            "#,
        )
        .bind(billing.hospital_id)
        .bind(&billing.payment_provider)
        .bind(&billing.encrypted_token)
        .bind("Cardholder")
        .bind(billing.last_four.as_deref().unwrap_or("0000"))
        .bind("12/25")
        .bind(billing.payment_method_type)
        .bind(added_by)
        .fetch_one(&mut **tx)
        .await?;

        Ok(result)
    }

    /// Find billing info by hospital ID
    /// Requirements: 3.2
    pub async fn find_by_hospital_id(
        &self,
        hospital_id: Uuid,
    ) -> Result<Option<HospitalPaymentMethod>, BillingError> {
        let billing = sqlx::query_as::<_, HospitalPaymentMethod>(
            r#"
            SELECT 
                id, hospital_id, paystack_authorization_code, paystack_customer_code,
                cardholder_name, card_last_four, card_type, card_expiry, bank_name,
                is_default, is_active, added_by, created_at, updated_at
            FROM hospital_payment_methods
            WHERE hospital_id = $1 AND is_default = TRUE
            ORDER BY created_at DESC
            LIMIT 1
            "#,
        )
        .bind(hospital_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(billing)
    }

    /// Update payment token
    /// Requirements: 3.2
    pub async fn update_payment_token(
        &self,
        billing_id: Uuid,
        encrypted_token: String,
    ) -> Result<(), BillingError> {
        let result = sqlx::query(
            r#"
            UPDATE hospital_payment_methods
            SET 
                encrypted_token = $2,
                updated_at = NOW()
            WHERE id = $1
            "#,
        )
        .bind(billing_id)
        .bind(encrypted_token)
        .execute(&self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(BillingError::NotFound(billing_id));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Unit tests will be added here
    // Property tests will be in Task 2.5
}
 