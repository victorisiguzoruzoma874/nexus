use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct FacilitySearchRequest {
    pub query: String,
    pub latitude: f64,
    pub longitude: f64,
    #[serde(default = "default_radius")]
    pub radius_meters: i32,
    #[serde(default = "default_limit")]
    pub limit: i32,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct AddressAutocompleteRequest {
    pub query: String,
    pub latitude: f64,
    pub longitude: f64,
    #[serde(default = "default_radius")]
    pub radius_meters: i32,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct FacilitySearchResponse {
    pub facilities: Vec<Facility>,
    pub total_found: usize,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct AddressAutocompleteResponse {
    pub suggestions: Vec<AddressSuggestion>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct Facility {
    pub id: String,
    pub title: String,
    pub address: String,
    pub position: Position,
    pub distance_meters: i32,
    pub categories: Vec<String>,
    pub contacts: Option<ContactInfo>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct AddressSuggestion {
    pub title: String,
    pub address: String,
    pub position: Option<Position>,
    pub result_type: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct Position {
    pub latitude: f64,
    pub longitude: f64,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ContactInfo {
    pub phone: Option<String>,
    pub email: Option<String>,
    pub website: Option<String>,
}

fn default_radius() -> i32 {
    5000 // 5km default
}

fn default_limit() -> i32 {
    20
}
