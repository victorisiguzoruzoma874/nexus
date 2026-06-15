// ! Hospital wallet — sub-account provisioning, deposits, escrow, webhooks.

use std::sync::Arc;

use chrono::{Duration, Utc};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::models::wallet::{Wallet, WalletDepositRequest, WalletLedgerEntry};
use crate::repositories::wallet::{WalletRepoError, WalletRepository};
use crate::services::safehaven::{SafeHavenClient, SafeHavenError};

#[derive(Debug, thiserror::Error)]
pub enum WalletServiceError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("wallet repository error: {0}")]
    Repo(#[from] WalletRepoError),

    #[error("SafeHaven error: {0}")]
    SafeHaven(#[from] SafeHavenError),

    #[error("wallet not found for hospital {0}")]
    WalletNotFound(Uuid),

    #[error("validation error: {0}")]
    Validation(String),
}

pub struct WalletService {
    repo: Arc<WalletRepository>,
    safehaven: Arc<SafeHavenClient>,
    pool: PgPool,
    callback_url: String,
    deposit_validity: Duration,
}

impl WalletService {
    pub fn new(
        repo: Arc<WalletRepository>,
        safehaven: Arc<SafeHavenClient>,
        pool: PgPool,
    ) -> Self {
        let callback_url = std::env::var("SAFEHAVEN_CALLBACK_URL").unwrap_or_default(); Self {
            repo,
            safehaven,
            pool,
            callback_url,
            deposit_validity: Duration::hours(24),
        }
    }

    /// Idempotently provision a SafeHaven sub-account for the hospital


    pub async fn ensure_sub_account(
        &self,
        hospital_id: Uuid,
        phone_number: &str,
        email: &str,
        identity_type: &str,
        identity_number: Option<&str>,
    ) -> Result<(), WalletServiceError> {
        self.repo.ensure_wallet_row(hospital_id).await?;

        if let Some(w) = self.repo.find_wallet(hospital_id).await? {
            if w.safehaven_account_id.is_some() {
                return Ok(());
            }
        }

        let callback = (!self.callback_url.trim(). is_empty()).then_some(self.callback_url.as_str());

        let sub = self
            .safehaven
            .create_sub_account(
                phone_number,
                email,
                &hospital_id.to_string(), identity_type,
                identity_number,
                callback,
            )
            .await?;

        self.repo
            .save_sub_account(
                hospital_id,
                &sub.id,
                &sub.account_number,
                sub.bank_code.as_deref(), sub.account_name.as_deref(), )
            .await?;

        tracing::info!(
            "Provisioned SafeHaven sub-account for hospital {}: {} ({})",
            hospital_id,
            sub.account_number,
            sub.id
        );
        Ok(())
    }

    pub async fn get_wallet(&self, hospital_id: Uuid) -> Result<Wallet, WalletServiceError> {
        self.repo.ensure_wallet_row(hospital_id).await?;
        self.repo
            .find_wallet(hospital_id)
            .await?
            .ok_or(WalletServiceError::WalletNotFound(hospital_id))
    }

    pub async fn list_ledger(
        &self,
        hospital_id: Uuid,
        page: i64,
        page_size: i64,
    ) -> Result<(Vec<WalletLedgerEntry>, i64), WalletServiceError> {
        Ok(self.repo.list_ledger(hospital_id, page, page_size).await?)
    }

    pub async fn list_deposits(
        &self,
        hospital_id: Uuid,
        limit: i64,
    ) -> Result<Vec<WalletDepositRequest>, WalletServiceError> {
        Ok(self.repo.list_deposit_requests(hospital_id, limit).await?)
    }

    /// Mint a one-shot virtual account at SafeHaven and record a pending deposit


    pub async fn request_deposit(
        &self,
        hospital_id: Uuid,
        amount_kobo: i64,
    ) -> Result<WalletDepositRequest, WalletServiceError> {
        if amount_kobo < 100_000 {
            return Err(WalletServiceError::Validation(
                "Minimum deposit is ₦1,000".to_string(), ));
        }

        self.repo.ensure_wallet_row(hospital_id).await?;

        let amount_naira = amount_kobo / 100;
        let external_reference = format!("dep_{}", Uuid::new_v4());

        let wallet = self.repo.find_wallet(hospital_id).await?;
        let (settlement_bank, settlement_acct) = wallet
            .as_ref()
            .and_then(|w| match (&w.safehaven_bank_code, &w.safehaven_account_number) {
                (Some(b), Some(a)) => Some((b.clone(), a.clone())),
                _ => None,
            })
            .map(|(b, a)| (Some(b), Some(a)))
            .unwrap_or((None, None));

        let callback = if self.callback_url.trim(). is_empty() {
            // Mock mode tolerates a placeholder URL; real SafeHaven rejects it.
            if self.safehaven.is_mock() {
                "https://mock.invalid/webhook".to_string()
            } else {
                return Err(WalletServiceError::Validation(
                    "SAFEHAVEN_CALLBACK_URL is not configured".to_string(), ));
            }
        } else {
            self.callback_url.clone()
        };

        let va = self
            .safehaven
            .create_virtual_account(
                amount_naira,
                self.deposit_validity.num_seconds(), &callback,
                settlement_bank.as_deref(), settlement_acct.as_deref(), &external_reference,
            )
            .await?;

        let valid_until = Utc::now() + self.deposit_validity;
        let row = self
            .repo
            .insert_deposit_request(
                hospital_id,
                amount_kobo,
                &va.account_number,
                va.bank_code.as_deref(), va.account_name.as_deref(), valid_until,
                &external_reference,
            )
            .await?;

        Ok(row)
    }

    pub async fn try_hold_in_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        hospital_id: Uuid,
        shift_id: Option<Uuid>,
        amount_kobo: i64,
    ) -> Result<(), WalletServiceError> {
        self.repo
            .try_hold_in_tx(tx, hospital_id, shift_id, amount_kobo)
            .await?;
        Ok(())
    }

    pub async fn release_hold_in_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        hospital_id: Uuid,
        shift_id: Option<Uuid>,
        amount_kobo: i64,
    ) -> Result<(), WalletServiceError> {
        self.repo
            .release_hold_in_tx(tx, hospital_id, shift_id, amount_kobo)
            .await?;
        Ok(())
    }

    /// Process a SafeHaven webhook. Idempotent via `webhook_events.provider_event_id`


    pub async fn process_webhook(
        &self,
        payload: &serde_json::Value,
    ) -> Result<WebhookOutcome, WalletServiceError> {
        let event_id = payload
            .get("data")
            .and_then(|d| d.get("_id"))
            .or_else(|| payload.get("data").and_then(|d| d.get("sessionId")))
            .and_then(|v| v.as_str());
        let event_type = payload
            .get("type")
            .or_else(|| payload.get("event"))
            .and_then(|v| v.as_str());

        let inserted = self
            .repo
            .insert_webhook_event_if_new(event_id, event_type, payload)
            .await?;
        let webhook_event_id = match inserted {
            Some(id) => id,
            None => return Ok(WebhookOutcome::AlreadySeen),
        };

        let result = match event_type {
            Some("virtualAccount.transfer") | Some("transfer.inflow") => {
                self.handle_virtual_account_transfer(payload).await
            }
            Some("subaccount.inflow") => self.handle_subaccount_inflow(payload).await,
            _ => {
                tracing::info!(
                    "SafeHaven webhook ignored (event_type={:?}): {}",
                    event_type,
                    payload
                );
                Ok(WebhookOutcome::Ignored)
            }
        };

        match &result {
            Ok(_) => {
                self.repo.mark_webhook_processed(webhook_event_id, None).await?;
            }
            Err(e) => {
                let _ = self
                    .repo
                    .mark_webhook_processed(webhook_event_id, Some(&e.to_string()))
                    .await;
            }
        }
        result
    }

    async fn handle_virtual_account_transfer(
        &self,
        payload: &serde_json::Value,
    ) -> Result<WebhookOutcome, WalletServiceError> {
        let data = payload.get("data").cloned(). unwrap_or(serde_json::Value::Null);
        let status = data.get("status").and_then(|v| v.as_str()).unwrap_or("");
        let success = matches!(status.to_ascii_lowercase().as_str(), "completed" | "success");
        if !success {
            return Ok(WebhookOutcome::Ignored);
        }

        let external_reference = data
            .get("externalReference")
            .and_then(|v| v.as_str())
            .map(str::to_string);
        let credit_account_number = data
            .get("creditAccountNumber")
            .or_else(|| data.get("destinationAccountNumber"))
            .and_then(|v| v.as_str())
            .map(str::to_string);

        let deposit = match &external_reference {
            Some(ext) => self.repo.find_deposit_by_external_ref(ext).await?,
            None => None,
        };
        let deposit = match deposit {
            Some(d) => Some(d),
            None => match &credit_account_number {
                Some(acct) => self.repo.find_pending_deposit_by_account_number(acct).await?,
                None => None,
            },
        };
        let deposit = match deposit {
            Some(d) => d,
            None => return Ok(WebhookOutcome::Ignored),
        };

        // SafeHaven returns amounts in NGN; convert to kobo.
        let received_amount_kobo = data
            .get("amount")
            .and_then(|v| v.as_i64())
            .map(|n| n * 100)
            .unwrap_or(deposit.amount_kobo);

        let provider_reference = data
            .get("paymentReference")
            .or_else(|| data.get("reference"))
            .and_then(|v| v.as_str());

        let mut tx = self.pool.begin(). await?;
        let (hospital_id, _ledger_id) = self
            .repo
            .complete_deposit_in_tx(
                &mut tx,
                deposit.id,
                received_amount_kobo,
                provider_reference,
                payload,
            )
            .await?;
        tx.commit(). await?;

        tracing::info!(
            "Wallet credited: hospital {} <- ₦{} (deposit {})",
            hospital_id,
            received_amount_kobo / 100,
            deposit.id
        );
        Ok(WebhookOutcome::DepositCredited {
            deposit_id: deposit.id,
            hospital_id,
            amount_kobo: received_amount_kobo,
        })
    }

    /// Hospital wired straight to its sub-account, bypassing the virtual-account flow


    async fn handle_subaccount_inflow(
        &self,
        payload: &serde_json::Value,
    ) -> Result<WebhookOutcome, WalletServiceError> {
        let data = payload.get("data").cloned(). unwrap_or(serde_json::Value::Null);
        let dest_account = data
            .get("creditAccountNumber")
            .or_else(|| data.get("destinationAccountNumber"))
            .and_then(|v| v.as_str())
            .map(str::to_string);
        let amount_naira = data.get("amount").and_then(|v| v.as_i64()).unwrap_or(0);
        if amount_naira <= 0 || dest_account.is_none() {
            return Ok(WebhookOutcome::Ignored);
        }
        let dest_account = dest_account.unwrap(); let hospital_id: Option<Uuid> = sqlx::query_scalar(
            r#"SELECT hospital_id FROM hospital_wallets
               WHERE safehaven_account_number = $1 LIMIT 1"#,
        )
        .bind(&dest_account)
        .fetch_optional(&self.pool)
        .await?;

        let hospital_id = match hospital_id {
            Some(h) => h,
            None => return Ok(WebhookOutcome::Ignored),
        };

        let amount_kobo = amount_naira * 100;
        let provider_reference = data
            .get("paymentReference")
            .or_else(|| data.get("reference"))
            .and_then(|v| v.as_str());

        let mut tx = self.pool.begin(). await?;
        sqlx::query(
            r#"
            INSERT INTO hospital_wallets (hospital_id, balance_kobo)
            VALUES ($1, $2)
            ON CONFLICT (hospital_id) DO UPDATE
              SET balance_kobo = hospital_wallets.balance_kobo + EXCLUDED.balance_kobo,
                  updated_at   = NOW()
            "#,
        )
        .bind(hospital_id)
        .bind(amount_kobo)
        .execute(&mut *tx)
        .await?;
        self.repo
            .insert_ledger_entry_in_tx(
                &mut tx,
                hospital_id,
                "deposit_credit",
                amount_kobo,
                0,
                None,
                provider_reference,
                Some("direct sub-account inflow"),
            )
            .await?;
        tx.commit(). await?;

        Ok(WebhookOutcome::DepositCredited {
            deposit_id: Uuid::nil(),
            hospital_id,
            amount_kobo,
        })
    }
}

#[derive(Debug, Clone)]
pub enum WebhookOutcome {
    AlreadySeen,
    Ignored,
    DepositCredited {
        deposit_id: Uuid,
        hospital_id: Uuid,
        amount_kobo: i64,
    },
}
