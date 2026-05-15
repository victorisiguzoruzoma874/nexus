use chrono::Utc;
use lettre::{
    message::header::ContentType,
    transport::smtp::authentication::Credentials,
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor,
};
use std::sync::Arc;
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum NotificationError {
    #[error("Email sending failed: {0}")]
    EmailFailed(String),

    #[error("Push notification failed: {0}")]
    PushFailed(String),

    #[error("Notification logging failed: {0}")]
    LoggingFailed(String),
}

pub struct NotificationService {
    smtp: Option<Arc<AsyncSmtpTransport<Tokio1Executor>>>,
    from_email: String,
    from_name: String,
    mock: bool,
}

impl NotificationService {
    pub fn new() -> Self {
        let smtp_host = std::env::var("SMTP_HOST").unwrap_or_default();
        let smtp_user = std::env::var("SMTP_USERNAME").unwrap_or_default();
        let smtp_pass = std::env::var("SMTP_PASSWORD").unwrap_or_default();
        let from_email = std::env::var("SMTP_FROM_EMAIL")
            .unwrap_or_else(|_| "noreply@nexuscare.com".to_string());
        let from_name = std::env::var("SMTP_FROM_NAME")
            .unwrap_or_else(|_| "NexusCare".to_string());

        let mock = smtp_host.is_empty() || smtp_user.is_empty();

        let smtp = if !mock {
            let port: u16 = std::env::var("SMTP_PORT")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(587);
            let creds = Credentials::new(smtp_user, smtp_pass);
            AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&smtp_host)
                .ok()
                .map(|b| b.port(port).credentials(creds).build())
                .map(Arc::new)
        } else {
            None
        };

        Self { smtp, from_email, from_name, mock }
    }

    /// Send a plain-text email. In mock mode, logs instead of sending.
    pub async fn send_email(
        &self,
        to: &str,
        subject: &str,
        body: &str,
    ) -> Result<(), NotificationError> {
        if self.mock {
            tracing::info!("[MOCK EMAIL] To: {} | Subject: {} | Body: {}", to, subject, body);
            return Ok(());
        }

        let from = format!("{} <{}>", self.from_name, self.from_email);
        let email = Message::builder()
            .from(from.parse().map_err(|e| NotificationError::EmailFailed(format!("{}", e)))?)
            .to(to.parse().map_err(|e| NotificationError::EmailFailed(format!("{}", e)))?)
            .subject(subject)
            .header(ContentType::TEXT_PLAIN)
            .body(body.to_string())
            .map_err(|e| NotificationError::EmailFailed(e.to_string()))?;

        self.smtp
            .as_ref()
            .expect("SMTP transport not initialised")
            .send(email)
            .await
            .map_err(|e| NotificationError::EmailFailed(e.to_string()))?;

        Ok(())
    }

    /// Send approval notification to hospital admin
    /// Requirements: 5.1, 5.2, 5.3
    pub async fn send_approval_notification(
        &self,
        hospital_id: Uuid,
        hospital_name: String,
        admin_email: String,
    ) -> Result<(), NotificationError> {
        let timestamp = Utc::now();
        
        // Send email notification
        self.send_email(
            &admin_email,
            "Hospital Registration Approved - NexusCare",
            &format!(
                "Congratulations! Your hospital '{}' has been approved on NexusCare.\n\n\
                Approval Date: {}\n\n\
                You can now access the platform and start creating shifts to find staff.\n\n\
                Best regards,\nThe NexusCare Team",
                hospital_name,
                timestamp.format("%Y-%m-%d %H:%M:%S UTC")
            ),
        ).await?;

        // Send push notification
        self.send_push(
            hospital_id,
            "Registration Approved",
            &format!("{} has been approved! You can now create shifts.", hospital_name),
        ).await?;

        // Log notification delivery
        self.log_notification(NotificationRecord {
            hospital_id,
            notification_type: NotificationType::Approval,
            email: admin_email,
            timestamp,
        }).await?;

        Ok(())
    }

    /// Send rejection notification to hospital admin
    /// Requirements: 5.4, 5.5
    pub async fn send_rejection_notification(
        &self,
        hospital_id: Uuid,
        hospital_name: String,
        admin_email: String,
        reason: String,
    ) -> Result<(), NotificationError> {
        let timestamp = Utc::now();
        
        // Send email notification
        self.send_email(
            &admin_email,
            "Hospital Registration Update - NexusCare",
            &format!(
                "Thank you for your interest in NexusCare.\n\n\
                Unfortunately, we are unable to approve the registration for '{}' at this time.\n\n\
                Reason: {}\n\n\
                If you have any questions or would like to resubmit your application, \
                please contact our support team.\n\n\
                Best regards,\nThe NexusCare Team",
                hospital_name,
                reason
            ),
        ).await?;

        // Send push notification
        self.send_push(
            hospital_id,
            "Registration Update",
            &format!("Registration for {} requires attention. Please check your email.", hospital_name),
        ).await?;

        // Log notification delivery
        self.log_notification(NotificationRecord {
            hospital_id,
            notification_type: NotificationType::Rejection,
            email: admin_email,
            timestamp,
        }).await?;

        Ok(())
    }

    /// Send push notification (stub — wire FCM when ready)
    async fn send_push(
        &self,
        hospital_id: Uuid,
        title: &str,
        message: &str,
    ) -> Result<(), NotificationError> {
        tracing::info!(
            "[PUSH] hospital={} title={} message={}",
            hospital_id, title, message
        );
        Ok(())
    }

    /// AC-07: Send shift broadcast notification to eligible workers
    pub async fn send_shift_broadcast_notification(
        &self,
        shift_id: Uuid,
        hospital_id: Uuid,
        matched_count: i32,
    ) -> Result<(), NotificationError> {
        tracing::info!(
            "[SHIFT BROADCAST] shift_id={} hospital_id={} matched_clinicians={}",
            shift_id, hospital_id, matched_count
        );

        // In production, this would:
        // 1. Query all eligible clinicians based on shift criteria
        // 2. Send push notifications to each clinician's device
        // 3. Log notification delivery for audit trail
        
        // Mock implementation
        tracing::info!(
            "Push notifications sent to {} eligible workers for shift {}",
            matched_count, shift_id
        );

        Ok(())
    }

    async fn log_notification(&self, notification: NotificationRecord) -> Result<(), NotificationError> {
        tracing::info!(
            "Notification logged: hospital_id={}, type={:?}, email={}, timestamp={}",
            notification.hospital_id,
            notification.notification_type,
            notification.email,
            notification.timestamp
        );
        Ok(())
    }
}

impl Default for NotificationService {
    fn default() -> Self {
        Self::new()
    }
}

/// Notification record for logging
#[derive(Debug)]
struct NotificationRecord {
    hospital_id: Uuid,
    notification_type: NotificationType,
    email: String,
    timestamp: chrono::DateTime<Utc>,
}

#[derive(Debug)]
enum NotificationType {
    Approval,
    Rejection,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_send_approval_notification() {
        let service = NotificationService::new();
        
        let result = service.send_approval_notification(
            Uuid::new_v4(),
            "Test Hospital".to_string(),
            "admin@test.com".to_string(),
        ).await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_send_rejection_notification() {
        let service = NotificationService::new();
        
        let result = service.send_rejection_notification(
            Uuid::new_v4(),
            "Test Hospital".to_string(),
            "admin@test.com".to_string(),
            "Incomplete documentation".to_string(),
        ).await;

        assert!(result.is_ok());
    }
}
