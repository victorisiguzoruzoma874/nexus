use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

// Enums

/// A discrete action a hospital can attempt on the platform.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "hospital_action", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum HospitalAction {
    // --- Read-only actions (allowed in verification pending state) ---
    BrowseApp,
    ViewDoctorProfiles,
    ExploreSystemTools,
    // --- Restricted actions (blocked until verified) ---
    CreateShift,
    ApproveContract,
    InviteStaff,
    InitiatePayment,
    ExportData,
}

/// The access level granted for a specific action under a given verification status.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "access_level", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum AccessLevel {
    Allowed,
    Restricted,
    Hidden,
}

// Hospital access policy

/// Defines what a hospital is allowed to do at a given verification status.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct AccessPolicy {
    pub id: Uuid,
    pub verification_status: String,
    pub action: HospitalAction,
    pub access_level: AccessLevel,
    /// Human-readable reason shown to the user when access is restricted
    pub restriction_reason: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// Response types

/// The resolved access policy for a hospital — what the API returns
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HospitalAccessSummary {
    pub hospital_id: Uuid,
    pub verification_status: String,
    pub permissions: Vec<ActionPermission>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionPermission {
    pub action: HospitalAction,
    pub access_level: AccessLevel,
    pub restriction_reason: Option<String>,
}
