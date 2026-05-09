use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::models::clinician_registration::{ClinicianBankAccount, ClinicianRole};
use crate::models::clinician::ClinicalSpecialty;

#[derive(Debug, thiserror::Error)]
pub enum ClinicianRepoError {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("Phone already registered")]
    DuplicatePhone,
    #[error("Clinician not found")]
    NotFound,
}

pub struct ClinicianRepository {
    pool: PgPool,
}

impl ClinicianRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// AC-05: Check if phone is already registered
    pub async fn phone_exists(&self, phone: &str) -> Result<bool, ClinicianRepoError> {
        let row: Option<(i64,)> =
            sqlx::query_as("SELECT COUNT(*) FROM clinicians WHERE phone = $1")
                .bind(phone)
                .fetch_optional(&self.pool)
                .await?;
        Ok(row.map(|(c,)| c > 0).unwrap_or(false))
    }

    /// AC-02: Create user + clinician row atomically
    pub async fn create_clinician(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        phone: &str,
    ) -> Result<Uuid, ClinicianRepoError> {
        // Create a minimal user row (no email/password — phone-only auth)
        let user_id: Uuid = sqlx::query_scalar(
            r#"
            INSERT INTO users (id, first_name, last_name, email, password_hash, role)
            VALUES (gen_random_uuid(), '', '', $1, '', 'staff')
            RETURNING id
            "#,
        )
        .bind(phone) // use phone as placeholder email (unique)
        .fetch_one(&mut **tx)
        .await?;

        // Create clinician row linked to user
        let clinician_id: Uuid = sqlx::query_scalar(
            r#"
            INSERT INTO clinicians (id, user_id, first_name, last_name, specialty, role_title, phone)
            VALUES (gen_random_uuid(), $1, '', '', 'other', '', $2)
            RETURNING id
            "#,
        )
        .bind(user_id)
        .bind(phone)
        .fetch_one(&mut **tx)
        .await?;

        Ok(clinician_id)
    }

    /// AC-03: Save completed profile fields
    pub async fn update_profile(
        &self,
        clinician_id: Uuid,
        first_name: &str,
        last_name: &str,
        role: &ClinicianRole,
        license_number: &str,
        specialty: &ClinicalSpecialty,
    ) -> Result<(), ClinicianRepoError> {
        // Also update the linked user row
        sqlx::query(
            r#"
            UPDATE users u
            SET first_name = $2, last_name = $3
            FROM clinicians c
            WHERE c.id = $1 AND c.user_id = u.id
            "#,
        )
        .bind(clinician_id)
        .bind(first_name)
        .bind(last_name)
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            UPDATE clinicians
            SET first_name = $2, last_name = $3, clinician_role = $4,
                license_number = $5, specialty = $6, role_title = $7
            WHERE id = $1
            "#,
        )
        .bind(clinician_id)
        .bind(first_name)
        .bind(last_name)
        .bind(role)
        .bind(license_number)
        .bind(specialty)
        .bind(format!("{:?}", role))
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// AC-04: Upsert bank account (encrypted account number)
    pub async fn upsert_bank_account(
        &self,
        clinician_id: Uuid,
        account_number_encrypted: &str,
        bank_code: &str,
        account_name: &str,
    ) -> Result<(), ClinicianRepoError> {
        sqlx::query(
            r#"
            INSERT INTO clinician_bank_accounts
                (clinician_id, account_number, bank_code, account_name)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (clinician_id)
            DO UPDATE SET
                account_number = EXCLUDED.account_number,
                bank_code      = EXCLUDED.bank_code,
                account_name   = EXCLUDED.account_name,
                updated_at     = NOW()
            "#,
        )
        .bind(clinician_id)
        .bind(account_number_encrypted)
        .bind(bank_code)
        .bind(account_name)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Fetch bank account for a clinician
    pub async fn get_bank_account(
        &self,
        clinician_id: Uuid,
    ) -> Result<Option<ClinicianBankAccount>, ClinicianRepoError> {
        let row = sqlx::query_as::<_, ClinicianBankAccount>(
            "SELECT * FROM clinician_bank_accounts WHERE clinician_id = $1",
        )
        .bind(clinician_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    /// Get clinician_id for a given phone (used after OTP verify)
    pub async fn find_clinician_id_by_phone(
        &self,
        phone: &str,
    ) -> Result<Option<Uuid>, ClinicianRepoError> {
        let row: Option<(Uuid,)> =
            sqlx::query_as("SELECT id FROM clinicians WHERE phone = $1")
                .bind(phone)
                .fetch_optional(&self.pool)
                .await?;
        Ok(row.map(|(id,)| id))
    }

    /// Get phone for a given clinician_id
    pub async fn find_phone_by_clinician_id(
        &self,
        clinician_id: Uuid,
    ) -> Result<Option<String>, ClinicianRepoError> {
        let row: Option<(String,)> =
            sqlx::query_as("SELECT COALESCE(phone, '') FROM clinicians WHERE id = $1")
                .bind(clinician_id)
                .fetch_optional(&self.pool)
                .await?;
        Ok(row.map(|(p,)| p))
    }
}
