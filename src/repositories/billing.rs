use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum BillingError {
    #[error("Database error: {0}")]
    DatabaseError(#[from] sqlx::Error),

    #[error("Billing record not found: {0}")]
    NotFound(Uuid),
}

/// Persistence for `billing_transactions` and (in ) the wallet ledger.

pub struct BillingRepository {
    #[allow(dead_code)]
    pool: PgPool,
}

impl BillingRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}
