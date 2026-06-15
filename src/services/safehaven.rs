// ! SafeHaven microfinance bank API client. Mock mode kicks in when

use std::sync::Arc;
use std::time::Duration;

use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::sync::RwLock;
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum SafeHavenError {
    #[error("HTTP request to SafeHaven failed: {0}")]
    Request(#[from] reqwest::Error),

    #[error("SafeHaven authentication failed: {0}")]
    Auth(String),

    /// HTTP 200 with `responseCode != "00"` / `statusCode == 400` in the body.
    #[error("SafeHaven rejected the request: {0}")]
    Rejected(String),

    #[error("SafeHaven response was missing expected field: {0}")]
    MalformedResponse(String),

    #[error("SafeHaven service is unavailable")]
    Unavailable,
}

#[derive(Debug, Clone)]
struct TokenCache {
    access_token: String,
    expires_at: i64,
}

#[derive(Debug, Serialize)]
struct OAuthRequest<'a> {
    grant_type: &'a str,
    client_id: &'a str,
    client_assertion: &'a str,
    client_assertion_type: &'a str,
}

#[derive(Debug, Deserialize)]
struct OAuthResponse {
    access_token: String,
    #[serde(default = "default_expiry")]
    expires_in: i64,
}

fn default_expiry() -> i64 {
    3600
}

/// Result of `POST /transfers/name-enquiry`. The `session_id` must be threaded
#[derive(Debug, Clone)]
pub struct ResolvedBankAccount {
    pub account_name: String,
    pub account_number: String,
    pub session_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SubAccount {
    pub id: String,
    pub account_number: String,
    pub bank_code: Option<String>,
    pub account_name: Option<String>,
    pub raw: Value,
}

/// Dynamic virtual account created via `POST /virtual-accounts`.
#[derive(Debug, Clone)]
pub struct VirtualAccount {
    pub id: Option<String>,
    pub account_number: String,
    pub bank_code: Option<String>,
    pub account_name: Option<String>,
    pub raw: Value,
}

/// Result of a successful `POST /transfers`.
#[derive(Debug, Clone)]
pub struct TransferReceipt {
    pub session_id: Option<String>,
    pub payment_reference: String,
    pub raw: Value,
}

/// SafeHaven transfer lifecycle state (from `POST /transfers/status`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransferStatus {
    Created,
    Initiated,
    Processing,
    Completed,
    Cancelled,
    Failed,
    Unknown(String),
}

impl TransferStatus {
    fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "created" => TransferStatus::Created,
            "initiated" => TransferStatus::Initiated,
            "processing" | "pending" => TransferStatus::Processing,
            "completed" | "success" | "successful" => TransferStatus::Completed,
            "cancelled" | "canceled" => TransferStatus::Cancelled,
            "failed" | "reversed" => TransferStatus::Failed,
            other => TransferStatus::Unknown(other.to_string()),
        }
    }
}

#[derive(Debug, Clone)]
pub struct SafeHavenClient {
    http: Client,
    base_url: String,
    client_id: String,
    ibs_client_id: String,
    debit_account_number: String,
    bank_code: String,
    token: Arc<RwLock<Option<TokenCache>>>,
}

impl SafeHavenClient {
    /// An empty `base_url` flips the client into mock mode

    pub fn new(
        base_url: String,
        client_id: String,
        ibs_client_id: String,
        debit_account_number: String,
        bank_code: String,
    ) -> Self {
        let http = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .expect("Failed to build SafeHaven HTTP client");
        Self {
            http,
            base_url,
            client_id,
            ibs_client_id,
            debit_account_number,
            bank_code,
            token: Arc::new(RwLock::new(None)),
        }
    }

    pub fn from_env() -> Self {
        Self::new(
            std::env::var("SAFEHAVEN_BASE_URL").unwrap_or_default(), std::env::var("SAFEHAVEN_CLIENT_ID").unwrap_or_default(), std::env::var("SAFEHAVEN_IBS_CLIENT_ID").unwrap_or_default(), std::env::var("SAFEHAVEN_DEBIT_ACCOUNT_NUMBER").unwrap_or_default(), std::env::var("SAFEHAVEN_BANK_CODE").unwrap_or_default(), )
    }

    pub fn is_mock(&self) -> bool {
        self.base_url.trim(). is_empty()
    }

    pub fn debit_account_number(&self) -> &str {
        &self.debit_account_number
    }

    pub fn bank_code(&self) -> &str {
        &self.bank_code
    }

    fn now_secs() -> i64 {
        chrono::Utc::now().timestamp()
    }

    /// Returns a cached or fresh OAuth2 access token. Refreshes 60s before expiry


    pub async fn get_access_token(&self) -> Result<String, SafeHavenError> {
        if self.is_mock() {
            return Ok("mock-access-token".to_string());
        }

        {
            let cache = self.token.read(). await;
            if let Some(t) = cache.as_ref() {
                if Self::now_secs() < t.expires_at - 60 {
                    return Ok(t.access_token.clone());
                }
            }
        }

        let url = format!("{}/oauth2/token", self.base_url);
        let body = OAuthRequest {
            grant_type: "client_credentials",
            client_id: &self.client_id,
            client_assertion: &self.ibs_client_id,
            client_assertion_type:
                "urn:ietf:params:oauth:client-assertion-type:jwt-bearer",
        };

        let resp = self
            .http
            .post(&url)
            .header("Accept", "application/json")
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await?;

        if !resp.status(). is_success() {
            let status = resp.status(); let text = resp.text(). await.unwrap_or_default(); return Err(SafeHavenError::Auth(format!(
                "HTTP {} from /oauth2/token: {}",
                status, text
            )));
        }

        let parsed: OAuthResponse = resp.json(). await.map_err(SafeHavenError::Request)?;
        let expires_at = Self::now_secs() + parsed.expires_in;
        let token = parsed.access_token.clone(); let mut cache = self.token.write(). await;
        *cache = Some(TokenCache {
            access_token: token.clone(), expires_at,
        });
        Ok(token)
    }

    async fn post_authed(&self, path: &str, body: Value) -> Result<Value, SafeHavenError> {
        let token = self.get_access_token(). await?;
        let url = format!("{}{}", self.base_url, path);
        let resp = self
            .http
            .post(&url)
            .header("Authorization", format!("Bearer {token}"))
            .header("ClientID", &self.ibs_client_id)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await?;

        let status = resp.status(); let value: Value = resp.json(). await.unwrap_or(Value::Null);

        // Detect business errors embedded in a 2xx response.
        if let Some(code) = value.get("statusCode").and_then(|v| v.as_i64()) {
            if code >= 400 {
                let msg = value
                    .get("message")
                    .and_then(|v| v.as_str())
                    .unwrap_or("rejected by SafeHaven")
                    .to_string(); return Err(SafeHavenError::Rejected(msg));
            }
        }
        if let Some(rc) = value.get("responseCode").and_then(|v| v.as_str()) {
            if rc != "00" {
                let msg = value
                    .get("message")
                    .or_else(|| value.get("responseMessage"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("rejected by SafeHaven")
                    .to_string(); return Err(SafeHavenError::Rejected(format!("{msg} (rc={rc})")));
            }
        }

        if !status.is_success() {
            let msg = value
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("upstream error")
                .to_string(); return Err(SafeHavenError::Rejected(format!("HTTP {status}: {msg}")));
        }

        Ok(value)
    }

    async fn get_authed(&self, path: &str) -> Result<Value, SafeHavenError> {
        let token = self.get_access_token(). await?;
        let url = format!("{}{}", self.base_url, path);
        let resp = self
            .http
            .get(&url)
            .header("Authorization", format!("Bearer {token}"))
            .header("ClientID", &self.ibs_client_id)
            .send()
            .await?;
        if !resp.status(). is_success() {
            return Err(SafeHavenError::Rejected(format!("HTTP {}", resp.status())));
        }
        Ok(resp.json(). await.unwrap_or(Value::Null))
    }

    /// `POST /transfers/name-enquiry` — validates the account and returns the

    pub async fn name_enquiry(
        &self,
        bank_code: &str,
        account_number: &str,
    ) -> Result<ResolvedBankAccount, SafeHavenError> {
        if self.is_mock() {
            tracing::info!(
                "[SAFEHAVEN MOCK] name_enquiry bank={bank_code} acct={account_number}"
            );
            return Ok(ResolvedBankAccount {
                account_name: "MOCK ACCOUNT HOLDER".to_string(), account_number: account_number.to_string(), session_id: Some(format!("mock-session-{}", Uuid::new_v4())),
            });
        }

        let body = self
            .post_authed(
                "/transfers/name-enquiry",
                json!({ "bankCode": bank_code, "accountNumber": account_number }),
            )
            .await?;

        let data = body.get("data").unwrap_or(&body);
        let account_name = data
            .get("accountName")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                SafeHavenError::MalformedResponse("missing accountName".to_string())
            })?
            .to_string(); let resolved_number = data
            .get("accountNumber")
            .and_then(|v| v.as_str())
            .unwrap_or(account_number)
            .to_string(); let session_id = data
            .get("sessionId")
            .and_then(|v| v.as_str())
            .map(str::to_string);

        Ok(ResolvedBankAccount {
            account_name,
            account_number: resolved_number,
            session_id,
        })
    }

    /// `POST /accounts/v2/subaccount` — per-hospital sub-account. Pass the
    #[allow(clippy::too_many_arguments)]
    pub async fn create_sub_account(
        &self,
        phone_number: &str,
        email: &str,
        external_reference: &str,
        identity_type: &str,
        identity_number: Option<&str>,
        callback_url: Option<&str>,
    ) -> Result<SubAccount, SafeHavenError> {
        if self.is_mock() {
            let id = format!("mock-sub-{}", Uuid::new_v4());
            let acct = format!("9{:09}", rand_digits9());
            tracing::info!(
                "[SAFEHAVEN MOCK] create_sub_account hospital_ref={external_reference} acct={acct}"
            );
            return Ok(SubAccount {
                id: id.clone(), account_number: acct.clone(), bank_code: Some(self.bank_code.clone()),
                account_name: Some(email.to_string()),
                raw: json!({ "_id": id, "accountNumber": acct }),
            });
        }

        let body = json!({
            "phoneNumber": phone_number,
            "emailAddress": email,
            "externalReference": external_reference,
            "identityType": identity_type,
            "identityNumber": identity_number,
            "callbackUrl": callback_url,
            "autoSweep": false,
        });
        let value = self.post_authed("/accounts/v2/subaccount", body).await?;
        let data = value.get("data").unwrap_or(&value).clone(); Ok(SubAccount {
            id: data
                .get("_id")
                .or_else(|| data.get("id"))
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    SafeHavenError::MalformedResponse("missing sub-account id".to_string())
                })?
                .to_string(), account_number: data
                .get("accountNumber")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(), bank_code: data
                .get("bankCode")
                .and_then(|v| v.as_str())
                .map(str::to_string),
            account_name: data
                .get("accountName")
                .and_then(|v| v.as_str())
                .map(str::to_string),
            raw: data,
        })
    }

    /// `POST /virtual-accounts` — one-shot deposit collector. SafeHaven posts

    pub async fn create_virtual_account(
        &self,
        amount_naira: i64,
        valid_for_secs: i64,
        callback_url: &str,
        settlement_bank_code: Option<&str>,
        settlement_account_number: Option<&str>,
        external_reference: &str,
    ) -> Result<VirtualAccount, SafeHavenError> {
        if self.is_mock() {
            let acct = format!("8{:09}", rand_digits9());
            tracing::info!(
                "[SAFEHAVEN MOCK] create_virtual_account ref={external_reference} amount=NGN{amount_naira} acct={acct}"
            );
            return Ok(VirtualAccount {
                id: Some(format!("mock-va-{}", Uuid::new_v4())),
                account_number: acct.clone(), bank_code: Some(self.bank_code.clone()),
                account_name: Some("NEXUSCARE DEPOSIT".to_string()),
                raw: json!({ "accountNumber": acct, "externalReference": external_reference }),
            });
        }

        let body = json!({
            "validFor": valid_for_secs,
            "callbackUrl": callback_url,
            "settlementAccount": {
                "bankCode": settlement_bank_code.unwrap_or(&self.bank_code),
                "accountNumber": settlement_account_number.unwrap_or(&self.debit_account_number),
            },
            "amountControl": "OverPayment",
            "amount": amount_naira,
            "externalReference": external_reference,
        });
        let value = self.post_authed("/virtual-accounts", body).await?;
        let data = value.get("data").unwrap_or(&value).clone(); Ok(VirtualAccount {
            id: data
                .get("_id")
                .or_else(|| data.get("id"))
                .and_then(|v| v.as_str())
                .map(str::to_string),
            account_number: data
                .get("accountNumber")
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    SafeHavenError::MalformedResponse("missing virtual accountNumber".to_string())
                })?
                .to_string(), bank_code: data
                .get("bankCode")
                .and_then(|v| v.as_str())
                .map(str::to_string),
            account_name: data
                .get("accountName")
                .and_then(|v| v.as_str())
                .map(str::to_string),
            raw: data,
        })
    }

    /// `POST /transfers` — NIBSS payout. Does the name-enquiry handshake for `sessionId`.
    #[allow(clippy::too_many_arguments)]
    pub async fn transfer(
        &self,
        beneficiary_bank_code: &str,
        beneficiary_account_number: &str,
        amount_naira: i64,
        narration: &str,
        payment_reference: &str,
        debit_account_number: Option<&str>,
    ) -> Result<TransferReceipt, SafeHavenError> {
        if self.is_mock() {
            tracing::info!(
                "[SAFEHAVEN MOCK] transfer NGN{amount_naira} -> {beneficiary_bank_code}/{beneficiary_account_number} ref={payment_reference}"
            );
            return Ok(TransferReceipt {
                session_id: Some(format!("mock-tx-{}", Uuid::new_v4())),
                payment_reference: payment_reference.to_string(), raw: json!({
                    "status": "Completed",
                    "amount": amount_naira,
                    "paymentReference": payment_reference,
                }),
            });
        }

        let enquiry = self
            .name_enquiry(beneficiary_bank_code, beneficiary_account_number)
            .await?;
        let session_id = enquiry.session_id.ok_or_else(|| {
            SafeHavenError::MalformedResponse(
                "name-enquiry did not return sessionId".to_string(), )
        })?;

        let body = json!({
            "nameEnquiryReference": session_id,
            "beneficiaryBankCode": beneficiary_bank_code,
            "beneficiaryAccountNumber": beneficiary_account_number,
            "amount": amount_naira,
            "saveBeneficiary": false,
            "narration": narration,
            "debitAccountNumber": debit_account_number.unwrap_or(&self.debit_account_number),
            "paymentReference": payment_reference,
        });

        let value = self.post_authed("/transfers", body).await?;
        let data = value.get("data").unwrap_or(&value).clone(); Ok(TransferReceipt {
            session_id: data
                .get("sessionId")
                .and_then(|v| v.as_str())
                .map(str::to_string),
            payment_reference: data
                .get("paymentReference")
                .and_then(|v| v.as_str())
                .unwrap_or(payment_reference)
                .to_string(), raw: data,
        })
    }

    /// `POST /transfers/status` — looks up a transfer's current state by the
    /// paymentReference we set when initiating it.
    pub async fn transfer_status(
        &self,
        payment_reference: &str,
    ) -> Result<TransferStatus, SafeHavenError> {
        if self.is_mock() {
            tracing::info!("[SAFEHAVEN MOCK] transfer_status ref={payment_reference}");
            return Ok(TransferStatus::Completed);
        }

        let body = self
            .post_authed(
                "/transfers/status",
                json!({ "paymentReference": payment_reference }),
            )
            .await?;
        let data = body.get("data").unwrap_or(&body);
        let status = data
            .get("status")
            .and_then(|v| v.as_str())
            .ok_or_else(|| SafeHavenError::MalformedResponse("missing transfer status".to_string()))?;
        Ok(TransferStatus::parse(status))
    }

    /// `GET /transfers` — transfer history for an account (for reconciliation /
    /// statement). Passes through the raw SafeHaven JSON.
    pub async fn list_transfers(
        &self,
        account_id: &str,
        page: i64,
        limit: i64,
        status: Option<&str>,
    ) -> Result<Value, SafeHavenError> {
        if self.is_mock() {
            return Ok(json!({ "data": [
                { "paymentReference": "mock-ref-1", "amount": 5000, "status": "Completed", "type": "Outwards" }
            ]}));
        }

        let mut path = format!("/transfers?accountId={account_id}&page={page}&limit={limit}");
        if let Some(s) = status {
            path.push_str(&format!("&status={s}"));
        }
        self.get_authed(&path).await
    }

    /// `POST /identity/v2` — initiates BVN/NIN verification. SafeHaven debits
    /// the platform account and sends an OTP to the registered phone. Returns
    /// the `data._id` to thread into validate.
    pub async fn initiate_identity_verification(
        &self,
        id_type: &str,
        number: &str,
    ) -> Result<String, SafeHavenError> {
        if self.is_mock() {
            tracing::info!("[SAFEHAVEN MOCK] initiate_identity_verification type={id_type}");
            return Ok(format!("mock-identity-{}", Uuid::new_v4()));
        }

        let body = self
            .post_authed(
                "/identity/v2",
                json!({
                    "type": id_type,
                    "number": number,
                    "debitAccountNumber": self.debit_account_number,
                    "async": false,
                }),
            )
            .await?;

        let data = body.get("data").unwrap_or(&body);
        data.get("_id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| SafeHavenError::MalformedResponse("missing identity _id".to_string()))
    }

    /// `POST /identity/v2/validate` — confirms the OTP and returns the verified
    /// provider data on success.
    pub async fn validate_identity_verification(
        &self,
        identity_id: &str,
        id_type: &str,
        otp: &str,
    ) -> Result<Value, SafeHavenError> {
        if self.is_mock() {
            tracing::info!("[SAFEHAVEN MOCK] validate_identity_verification type={id_type}");
            if otp == "123456" {
                return Ok(json!({
                    "fullName": "MOCK VERIFIED USER",
                    "type": id_type,
                    "verified": true,
                }));
            }
            return Err(SafeHavenError::Rejected("invalid OTP".to_string()));
        }

        let body = self
            .post_authed(
                "/identity/v2/validate",
                json!({ "identityId": identity_id, "type": id_type, "otp": otp }),
            )
            .await?;

        Ok(body.get("data").cloned().unwrap_or(body))
    }

    pub async fn get_bank_list(&self) -> Result<Value, SafeHavenError> {
        if self.is_mock() {
            return Ok(json!({ "data": [
                { "bankCode": "044", "name": "Access Bank" },
                { "bankCode": "058", "name": "GTBank" },
                { "bankCode": "057", "name": "Zenith Bank" }
            ]}));
        }
        self.get_authed("/transfers/banks").await
    }
}

fn rand_digits9() -> u64 {
    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(Uuid::new_v4().as_bytes());
    let n = u128::from_be_bytes(bytes);
    (n % 1_000_000_000) as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mock_client() -> SafeHavenClient {
        SafeHavenClient::new(
            String::new(), // empty base_url => mock mode
            "test-client".to_string(), "test-ibs".to_string(), "0000000000".to_string(), "090286".to_string(), )
    }

    #[tokio::test]
    async fn is_mock_when_base_url_empty() {
        let c = mock_client();
        assert!(c.is_mock());
        // Tokens should be returned trivially without an HTTP call.
        let t = c.get_access_token().await.unwrap();
        assert_eq!(t, "mock-access-token");
    }

    #[tokio::test]
    async fn mock_name_enquiry_returns_session_id() {
        let c = mock_client();
        let r = c.name_enquiry("058", "0123456789").await.unwrap();
        assert_eq!(r.account_number, "0123456789");
        assert!(r.account_name.contains("MOCK"));
        assert!(r.session_id.is_some());
    }

    #[tokio::test]
    async fn mock_initiate_identity_returns_id() {
        let c = mock_client();
        let id = c.initiate_identity_verification("BVN", "12345678901").await.unwrap();
        assert!(id.starts_with("mock-identity-"));
    }

    #[tokio::test]
    async fn mock_validate_identity_accepts_correct_otp() {
        let c = mock_client();
        let data = c
            .validate_identity_verification("mock-identity-x", "NIN", "123456")
            .await
            .unwrap();
        assert_eq!(data.get("verified").and_then(|v| v.as_bool()), Some(true));
    }

    #[tokio::test]
    async fn mock_validate_identity_rejects_wrong_otp() {
        let c = mock_client();
        let res = c
            .validate_identity_verification("mock-identity-x", "BVN", "000000")
            .await;
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn mock_transfer_returns_receipt() {
        let c = mock_client();
        let receipt = c
            .transfer("058", "0123456789", 50_000, "test", "ref-123", None)
            .await
            .unwrap();
        assert_eq!(receipt.payment_reference, "ref-123");
        assert!(receipt.session_id.is_some());
    }

    #[tokio::test]
    async fn mock_transfer_status_is_completed() {
        let c = mock_client();
        let st = c.transfer_status("mock-ref").await.unwrap();
        assert_eq!(st, TransferStatus::Completed);
    }

    #[tokio::test]
    async fn mock_list_transfers_returns_data() {
        let c = mock_client();
        let v = c.list_transfers("acct-1", 0, 100, None).await.unwrap();
        assert!(v.get("data").and_then(|d| d.as_array()).is_some());
    }

    #[test]
    fn transfer_status_parsing() {
        assert_eq!(TransferStatus::parse("Completed"), TransferStatus::Completed);
        assert_eq!(TransferStatus::parse("Processing"), TransferStatus::Processing);
        assert_eq!(TransferStatus::parse("Failed"), TransferStatus::Failed);
        assert_eq!(TransferStatus::parse("Canceled"), TransferStatus::Cancelled);
        assert!(matches!(TransferStatus::parse("weird"), TransferStatus::Unknown(_)));
    }

    #[tokio::test]
    async fn mock_create_sub_account_returns_account_number() {
        let c = mock_client();
        let sub = c
            .create_sub_account(
                "08012345678",
                "hospital@example.com",
                "hospital-uuid-here",
                "BVN",
                Some("22222222222"),
                None,
            )
            .await
            .unwrap();
        assert!(sub.account_number.starts_with('9'));
        assert_eq!(sub.account_number.len(), 10);
        assert_eq!(sub.bank_code.as_deref(), Some("090286"));
    }

    #[tokio::test]
    async fn mock_create_virtual_account_uses_external_reference() {
        let c = mock_client();
        let va = c
            .create_virtual_account(
                10_000,
                3600,
                "https://example.test/webhook",
                None,
                None,
                "dep_test",
            )
            .await
            .unwrap();
        assert!(va.account_number.starts_with('8'));
        assert!(va.id.is_some());
    }
}
