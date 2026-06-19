use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;
use validator::Validate;

// Hospital location

/// The confirmed GPS location of a hospital's entrance, set via the
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct HospitalLocation {
    pub id: Uuid,
    pub hospital_id: Uuid,

    /// Latitude of the hospital entrance pin (e.g. 6.4965)
    pub latitude: f64,
    /// Longitude of the hospital entrance pin (e.g. 3.3764)
    pub longitude: f64,

    /// Human-readable label from the map search, e.g. "Idi-Araba, Surulere, Lagos"
    pub place_label: Option<String>,

    // --- Geofencing settings ---
    pub clock_in_radius_meters: i32,
    /// Whether GPS clock-in fencing is active (auto-enabled on location confirm)
    pub gps_fencing_enabled: bool,

    // --- Shift broadcasting settings ---
    pub shift_broadcast_radius_km: f64,
    /// Whether shift distance prioritisation is active
    pub shift_distance_active: bool,

    /// Whether the hospital admin has confirmed this pin placement
    pub location_confirmed: bool,
    /// When the location was last confirmed
    pub confirmed_at: Option<DateTime<Utc>>,
    /// The user who confirmed the location
    pub confirmed_by: Option<Uuid>,

    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// Request / response types

/// Payload for setting or updating the hospital's map pin.
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct SetHospitalLocationRequest {
    #[validate(range(min = -90.0, max = 90.0, message = "Latitude must be between -90 and 90"))]
    pub latitude: f64,

    #[validate(range(min = -180.0, max = 180.0, message = "Longitude must be between -180 and 180"))]
    pub longitude: f64,

    pub place_label: Option<String>,

    /// Defaults to 100 if not provided
    #[validate(range(
        min = 50,
        max = 5000,
        message = "Clock-in radius must be between 50m and 5000m"
    ))]
    pub clock_in_radius_meters: Option<i32>,

    /// Defaults to 5.0 if not provided
    #[validate(range(
        min = 1.0,
        max = 100.0,
        message = "Broadcast radius must be between 1km and 100km"
    ))]
    pub shift_broadcast_radius_km: Option<f64>,
}

/// Payload for confirming the current pin placement.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfirmLocationRequest {
    /// Optionally override geofencing toggles at confirmation time
    pub gps_fencing_enabled: Option<bool>,
    pub shift_distance_active: Option<bool>,
}

/// Response returned for location queries.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HospitalLocationResponse {
    pub id: Uuid,
    pub hospital_id: Uuid,
    pub latitude: f64,
    pub longitude: f64,
    pub place_label: Option<String>,
    pub clock_in_radius_meters: i32,
    pub gps_fencing_enabled: bool,
    pub shift_broadcast_radius_km: f64,
    pub shift_distance_active: bool,
    pub location_confirmed: bool,
    pub confirmed_at: Option<DateTime<Utc>>,
    pub updated_at: DateTime<Utc>,
}

impl From<HospitalLocation> for HospitalLocationResponse {
    fn from(l: HospitalLocation) -> Self {
        Self {
            id: l.id,
            hospital_id: l.hospital_id,
            latitude: l.latitude,
            longitude: l.longitude,
            place_label: l.place_label,
            clock_in_radius_meters: l.clock_in_radius_meters,
            gps_fencing_enabled: l.gps_fencing_enabled,
            shift_broadcast_radius_km: l.shift_broadcast_radius_km,
            shift_distance_active: l.shift_distance_active,
            location_confirmed: l.location_confirmed,
            confirmed_at: l.confirmed_at,
            updated_at: l.updated_at,
        }
    }
}
