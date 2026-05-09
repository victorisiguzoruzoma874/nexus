use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, thiserror::Error)]
pub enum SmsError {
    #[error("SMS delivery failed: {0}")]
    DeliveryFailed(String),
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
}

#[derive(Serialize)]
struct TermiiSendRequest<'a> {
    to: &'a str,
    from: &'a str,
    sms: String,
    r#type: &'a str,
    channel: &'a str,
    api_key: &'a str,
}

#[derive(Deserialize)]
struct TermiiResponse {
    code: Option<String>,
    message: Option<String>,
}

pub struct SmsService {
    client: Client,
    api_key: String,
    base_url: String,
    mock: bool,
}

impl SmsService {
    pub fn new(api_key: String, base_url: Option<String>) -> Self {
        let mock = api_key.is_empty() || api_key == "mock";
        Self {
            client: Client::builder()
                .timeout(Duration::from_secs(15))
                .build()
                .expect("HTTP client"),
            api_key,
            base_url: base_url.unwrap_or_else(|| "https://api.ng.termii.com".to_string()),
            mock,
        }
    }

    /// Send a 6-digit OTP via SMS. In mock mode, logs the code instead.
    pub async fn send_otp(&self, phone: &str, code: &str) -> Result<(), SmsError> {
        let message = format!("Your NexusCare verification code is {}. Valid for 10 minutes.", code);

        if self.mock {
            tracing::info!("[MOCK SMS] To: {} | OTP: {}", phone, code);
            return Ok(());
        }

        let url = format!("{}/api/sms/send", self.base_url);
        let body = TermiiSendRequest {
            to: phone,
            from: "NexusCare",
            sms: message,
            r#type: "plain",
            channel: "generic",
            api_key: &self.api_key,
        };

        let resp = self.client.post(&url).json(&body).send().await?;

        if !resp.status().is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(SmsError::DeliveryFailed(text));
        }

        let parsed: TermiiResponse = resp.json().await.unwrap_or(TermiiResponse {
            code: None,
            message: None,
        });

        if parsed.code.as_deref() == Some("ok") || parsed.message.is_some() {
            Ok(())
        } else {
            Err(SmsError::DeliveryFailed("Unexpected Termii response".to_string()))
        }
    }
}
