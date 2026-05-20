use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct EmailOutboxItem {
    pub id: Uuid,
    pub to_email: String,
    pub subject: String,
    pub text_body: String,
    pub html_body: Option<String>,
    pub status: String,
    pub attempts: i32,
    pub last_error: Option<String>,
    pub scheduled_at: DateTime<Utc>,
    pub sent_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
