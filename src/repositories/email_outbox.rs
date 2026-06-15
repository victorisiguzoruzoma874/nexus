use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::models::email_outbox::EmailOutboxItem;

pub struct EmailOutboxRepository {
    pool: PgPool,
}

impl EmailOutboxRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn enqueue(
        &self,
        to_email: &str,
        subject: &str,
        text_body: &str,
        html_body: Option<&str>,
    ) -> Result<Uuid, sqlx::Error> {
        sqlx::query_scalar(
            r#"
            INSERT INTO email_outbox (to_email, subject, text_body, html_body)
            VALUES ($1, $2, $3, $4)
            RETURNING id
            "#,
        )
        .bind(to_email)
        .bind(subject)
        .bind(text_body)
        .bind(html_body)
        .fetch_one(&self.pool)
        .await
    }

    pub async fn fetch_pending(&self, limit: i64) -> Result<Vec<EmailOutboxItem>, sqlx::Error> {
        sqlx::query_as::<_, EmailOutboxItem>(
            r#"
            SELECT id, to_email, subject, text_body, html_body, status, attempts, last_error,
                   scheduled_at, sent_at, created_at, updated_at
            FROM email_outbox
            WHERE status = 'pending'
              AND scheduled_at <= NOW()
            ORDER BY created_at ASC
            LIMIT $1
            "#,
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await
    }

    pub async fn mark_processing(&self, id: Uuid) -> Result<i32, sqlx::Error> {
        sqlx::query_scalar(
            r#"
            UPDATE email_outbox
            SET status = 'processing',
                attempts = attempts + 1,
                updated_at = NOW()
            WHERE id = $1
            RETURNING attempts
            "#,
        )
        .bind(id)
        .fetch_one(&self.pool)
        .await
    }

    pub async fn mark_sent(&self, id: Uuid) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            UPDATE email_outbox
            SET status = 'sent',
                sent_at = NOW, updated_at = NOW()
            WHERE id = $1
            "#,
        )
        .bind(id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn reschedule(
        &self,
        id: Uuid,
        last_error: &str,
        scheduled_at: DateTime<Utc>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            UPDATE email_outbox
            SET status = 'pending',
                last_error = $2,
                scheduled_at = $3,
                updated_at = NOW()
            WHERE id = $1
            "#,
        )
        .bind(id)
        .bind(last_error)
        .bind(scheduled_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn mark_failed(&self, id: Uuid, last_error: &str) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            UPDATE email_outbox
            SET status = 'failed',
                last_error = $2,
                updated_at = NOW()
            WHERE id = $1
            "#,
        )
        .bind(id)
        .bind(last_error)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
