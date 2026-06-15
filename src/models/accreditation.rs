use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

// Hospital accreditation record

/// The accreditation record created when a hospital reaches Step 4 (Access Granted).
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct HospitalAccreditation {
    pub id: Uuid,
    pub hospital_id: Uuid,
    /// The super_admin who granted accreditation
    pub granted_by: Uuid,
    /// When accreditation was granted (drives the "Access Granted" step transition)
    pub granted_at: DateTime<Utc>,
    /// Accreditation reference / certificate number, e.g. "NXC-2024-LUTH-001"
    pub certificate_number: String,
    /// URL to the downloadable accreditation certificate PDF
    pub certificate_url: Option<String>,
    /// Accreditation expires and must be renewed (NULL = no expiry)
    pub expires_at: Option<DateTime<Utc>>,
    /// Whether accreditation is currently active
    pub is_active: bool,
    /// Reason if accreditation was later revoked
    pub revocation_reason: Option<String>,
    pub revoked_at: Option<DateTime<Utc>>,
    pub revoked_by: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// Platform features

/// A platform feature/capability unlocked for verified hospitals.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "platform_feature", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum PlatformFeature {
    /// Reach the entire network of vetted clinicians instantly
    UnlimitedShiftBroadcasting,
    /// Invite top-rated specialists directly to departments
    DirectClinicianOutreach,
    /// Automated billing and seamless clinician compensation
    VerifiedPayrollIntegration,
    /// Deep insights into staffing efficiency and costs
    PerformanceAnalytics,
}

// Response types

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HospitalAccreditationResponse {
    pub id: Uuid,
    pub hospital_id: Uuid,
    pub certificate_number: String,
    pub certificate_url: Option<String>,
    pub granted_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
    pub is_active: bool,
}

impl From<HospitalAccreditation> for HospitalAccreditationResponse {
    fn from(a: HospitalAccreditation) -> Self {
        Self {
            id: a.id,
            hospital_id: a.hospital_id,
            certificate_number: a.certificate_number,
            certificate_url: a.certificate_url,
            granted_at: a.granted_at,
            expires_at: a.expires_at,
            is_active: a.is_active,
        }
    }
}

/// Summary of all unlocked features for a hospital — returned alongside
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HospitalFeaturesResponse {
    pub hospital_id: Uuid,
    pub features: Vec<PlatformFeature>,
}
