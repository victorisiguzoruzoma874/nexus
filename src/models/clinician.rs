use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;
use validator::Validate;

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

/// Clinical specialty of a clinician.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, sqlx::Type, utoipa::ToSchema)]
#[sqlx(type_name = "clinical_specialty", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum ClinicalSpecialty {
    EmergencyMedicine,
    Pediatrics,
    IcuSpecialist,
    GeneralNursing,
    Pharmacy,
    LabTechnician,
    Surgery,
    Radiology,
    Anesthesiology,
    Cardiology,
    Obstetrics,
    Psychiatry,
    Other,
}

/// Real-time availability status of a clinician shown in the Workforce Pool.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "clinician_availability", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum ClinicianAvailability {
    /// Available and not currently on a shift
    AvailableNow,
    /// Currently on an active shift at this hospital
    OnSite,
    /// Off duty — has a resume time
    OffDuty,
    /// Unavailable (on leave, blocked, etc.)
    Unavailable,
}

// ---------------------------------------------------------------------------
// Clinician profile
// ---------------------------------------------------------------------------

/// A clinician (doctor, nurse, technician) registered on the NexusCare platform.
/// Shown in the Workforce Pool panel on the Clinical Dashboard.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Clinician {
    pub id: Uuid,
    /// Links to the platform user account
    pub user_id: Uuid,

    pub first_name: String,
    pub last_name: String,
    pub specialty: ClinicalSpecialty,
    /// Professional title / role label, e.g. "Emergency Doctor", "ICU Specialist"
    pub role_title: String,

    /// NexusCare platform rating (0.0–5.0), e.g. 4.9
    pub rating: f32,
    /// Total number of ratings contributing to the average
    pub rating_count: i32,

    pub avatar_url: Option<String>,

    /// Current availability status shown in the Workforce Pool
    pub availability: ClinicianAvailability,
    /// When the clinician will next be available (shown as "Resumes 8 AM")
    pub available_from: Option<DateTime<Utc>>,

    /// Whether this clinician is verified / vetted on the platform
    pub is_verified: bool,
    pub is_active: bool,

    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// Clinician location (real-time proximity for Workforce Pool)
// ---------------------------------------------------------------------------

/// Last known GPS position of a clinician.
/// Used to calculate distance shown in the Workforce Pool (e.g. "2.4km", "0.8km").
/// Updated by the clinician's mobile app.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ClinicianLocation {
    pub id: Uuid,
    pub clinician_id: Uuid,
    pub latitude: f64,
    pub longitude: f64,
    /// Accuracy of the GPS fix in metres
    pub accuracy_meters: Option<f32>,
    pub recorded_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// Response types
// ---------------------------------------------------------------------------

/// Clinician card shown in the Workforce Pool panel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClinicianPoolCard {
    pub id: Uuid,
    pub first_name: String,
    pub last_name: String,
    pub specialty: ClinicalSpecialty,
    pub role_title: String,
    pub rating: f32,
    pub rating_count: i32,
    pub avatar_url: Option<String>,
    pub availability: ClinicianAvailability,
    pub available_from: Option<DateTime<Utc>>,
    /// Distance from the hospital in km, computed at query time
    pub distance_km: Option<f64>,
    pub is_verified: bool,
}

/// Payload for updating a clinician's availability status.
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct UpdateClinicianAvailabilityRequest {
    pub availability: ClinicianAvailability,
    pub available_from: Option<DateTime<Utc>>,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
}
