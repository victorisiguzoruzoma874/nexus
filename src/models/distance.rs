use crate::models::admin_registration::Coordinates;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct DistanceRequest {
    pub origin: LocationInput,
    pub destination: LocationInput,
    #[serde(default = "default_transport_mode")]
    pub transport_mode: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct LocationInput {
    #[serde(rename = "type")]
    pub location_type: LocationType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub coordinates: Option<Coordinates>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub address: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum LocationType {
    Coordinates,
    Address,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct DistanceResponse {
    pub origin: LocationDetails,
    pub destination: LocationDetails,
    pub distance: DistanceInfo,
    pub travel_time: TimeInfo,
    pub route_summary: RouteSummary,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct LocationDetails {
    pub coordinates: Coordinates,
    pub address: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct DistanceInfo {
    pub value: i32,
    pub unit: String,
    pub formatted: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct TimeInfo {
    pub value: i32,
    pub unit: String,
    pub formatted: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct RouteSummary {
    pub transport_mode: String,
    pub base_duration: i32,
    pub traffic_impact: String,
    pub route_id: String,
}

impl DistanceInfo {
    pub fn new(meters: i32) -> Self {
        if meters < 1000 {
            Self {
                value: meters,
                unit: "meters".to_string(),
                formatted: format!("{} meters", meters),
            }
        } else {
            let km = meters as f64 / 1000.0;
            Self {
                value: meters,
                unit: "kilometers".to_string(),
                formatted: format!("{:.1} km", km),
            }
        }
    }
}

impl TimeInfo {
    pub fn new(seconds: i32) -> Self {
        if seconds < 60 {
            Self {
                value: seconds,
                unit: "seconds".to_string(),
                formatted: format!("{} seconds", seconds),
            }
        } else if seconds < 3600 {
            let minutes = seconds / 60;
            let remaining_seconds = seconds % 60;
            let formatted = if remaining_seconds > 0 {
                format!("{} minutes {} seconds", minutes, remaining_seconds)
            } else {
                format!("{} minutes", minutes)
            };
            Self {
                value: seconds,
                unit: "minutes".to_string(),
                formatted,
            }
        } else {
            let hours = seconds / 3600;
            let minutes = (seconds % 3600) / 60;
            let formatted = if minutes > 0 {
                format!(
                    "{} hour{} {} minutes",
                    hours,
                    if hours == 1 { "" } else { "s" },
                    minutes
                )
            } else {
                format!("{} hour{}", hours, if hours == 1 { "" } else { "s" })
            };
            Self {
                value: seconds,
                unit: "hours".to_string(),
                formatted,
            }
        }
    }
}

fn default_transport_mode() -> String {
    "car".to_string()
}

impl RouteSummary {
    pub fn new(
        transport_mode: String,
        duration: i32,
        base_duration: i32,
        route_id: String,
    ) -> Self {
        let traffic_impact = if duration <= base_duration {
            "none".to_string()
        } else {
            let increase = ((duration - base_duration) as f64 / base_duration as f64) * 100.0;
            if increase < 10.0 {
                "light".to_string()
            } else if increase < 25.0 {
                "moderate".to_string()
            } else {
                "heavy".to_string()
            }
        };

        Self {
            transport_mode,
            base_duration,
            traffic_impact,
            route_id,
        }
    }
}
