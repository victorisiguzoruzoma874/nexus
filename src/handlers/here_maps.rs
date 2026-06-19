use crate::routes::app_routes::AppState;
use crate::utils::errors::AppError;
use axum::{
    extract::{Query, State},
    Json,
};
use serde::Deserialize;

#[derive(Debug, serde::Serialize, utoipa::ToSchema)]
pub struct GeocodeResponse {
    pub items: Vec<GeocodeItem>,
}

#[derive(Debug, serde::Serialize, utoipa::ToSchema)]
pub struct GeocodeItem {
    pub position: GeocodePosition,
    pub address: AddressDetails,
    pub title: String,
}

#[derive(Debug, serde::Serialize, utoipa::ToSchema)]
pub struct GeocodePosition {
    pub lat: f64,
    pub lng: f64,
}

#[derive(Debug, serde::Serialize, utoipa::ToSchema)]
pub struct AddressDetails {
    pub label: String,
    pub country_code: Option<String>,
    pub country_name: Option<String>,
    pub state: Option<String>,
    pub city: Option<String>,
}

#[derive(Debug, serde::Serialize, utoipa::ToSchema)]
pub struct ReverseGeocodeResponse {
    pub items: Vec<ReverseGeocodeItem>,
}

#[derive(Debug, serde::Serialize, utoipa::ToSchema)]
pub struct ReverseGeocodeItem {
    pub title: String,
    pub address: AddressDetails,
    pub position: GeocodePosition,
}

#[derive(Debug, Deserialize)]
pub struct GeocodeParams {
    pub q: String,
    #[serde(rename = "in")]
    pub in_area: Option<String>,
    pub limit: Option<i32>,
}

#[derive(Debug, Deserialize)]
pub struct ReverseGeocodeParams {
    pub at: String, // lat,lng format
    pub limit: Option<i32>,
}

/// Geocode an address to coordinates
#[utoipa::path(
    get,
    path = "/api/v1/here/geocode",
    tag = "location",
    summary = "Geocode address to coordinates",
    description = "Convert address string to latitude/longitude coordinates using HERE Maps API",
    params(
        ("q" = String, Query, description = "Address query string", example = "Lagos, Nigeria"),
        ("limit" = Option<i32>, Query, description = "Maximum results", example = 5),
        ("in_area" = Option<String>, Query, description = "Area filter for results")
    ),
    responses(
        (status = 200, description = "Geocoding results", body = GeocodeResponse),
        (status = 400, description = "Invalid query parameters"),
        (status = 500, description = "HERE API error")
    )
)]
pub async fn geocode_address(
    State(_state): State<AppState>,
    Query(params): Query<GeocodeParams>,
) -> Result<Json<GeocodeResponse>, AppError> {
    if params.q.trim().is_empty() {
        return Err(AppError::BadRequest(
            "Query parameter 'q' cannot be empty".to_string(),
        ));
    }

    // Use HERE Maps client
    let url = "https://geocode.search.hereapi.com/v1/geocode";
    let api_key = std::env::var("HERE_API_KEY")
        .map_err(|_| AppError::InternalServerError("HERE_API_KEY not configured".to_string()))?;
    let limit_str = params.limit.unwrap_or(5).to_string();

    let mut query_params = vec![
        ("q", params.q.as_str()),
        ("apiKey", api_key.as_str()),
        ("limit", limit_str.as_str()),
    ];

    if let Some(area_filter) = &params.in_area {
        query_params.push(("in", area_filter));
    }

    let client = reqwest::Client::new();
    let response = client
        .get(url)
        .query(&query_params)
        .send()
        .await
        .map_err(|e| AppError::InternalServerError(format!("HERE API error: {}", e)))?;

    if !response.status().is_success() {
        return Err(AppError::InternalServerError(format!(
            "HERE API error: {}",
            response.status()
        )));
    }

    let here_response: serde_json::Value = response
        .json()
        .await
        .map_err(|e| AppError::InternalServerError(format!("Failed to parse response: {}", e)))?;

    // Convert HERE response to our format
    let items = here_response["items"]
        .as_array()
        .unwrap_or(&vec![])
        .iter()
        .map(|item| {
            let pos = &item["position"];
            let addr = &item["address"];
            GeocodeItem {
                position: GeocodePosition {
                    lat: pos["lat"].as_f64().unwrap_or(0.0),
                    lng: pos["lng"].as_f64().unwrap_or(0.0),
                },
                address: AddressDetails {
                    label: addr["label"].as_str().unwrap_or("").to_string(),
                    country_code: addr["countryCode"].as_str().map(|s| s.to_string()),
                    country_name: addr["countryName"].as_str().map(|s| s.to_string()),
                    state: addr["state"].as_str().map(|s| s.to_string()),
                    city: addr["city"].as_str().map(|s| s.to_string()),
                },
                title: item["title"].as_str().unwrap_or("").to_string(),
            }
        })
        .collect();

    Ok(Json(GeocodeResponse { items }))
}

/// Reverse geocode coordinates to address
#[utoipa::path(
    get,
    path = "/api/v1/here/reverse-geocode",
    tag = "location",
    summary = "Reverse geocode coordinates to address", 
    description = "Convert latitude/longitude coordinates to human-readable address using HERE Maps API",
    params(
        ("at" = String, Query, description = "Coordinates in 'lat,lng' format", example = "6.5244,3.3792"),
        ("limit" = Option<i32>, Query, description = "Maximum results", example = 5)
    ),
    responses(
        (status = 200, description = "Reverse geocoding results", body = ReverseGeocodeResponse),
        (status = 400, description = "Invalid coordinate format"),
        (status = 500, description = "HERE API error")
    )
)]
pub async fn reverse_geocode(
    State(_state): State<AppState>,
    Query(params): Query<ReverseGeocodeParams>,
) -> Result<Json<ReverseGeocodeResponse>, AppError> {
    // Validate coordinate format
    let coords: Vec<&str> = params.at.split(',').collect();
    if coords.len() != 2 {
        return Err(AppError::BadRequest(
            "Invalid coordinate format. Use 'lat,lng'".to_string(),
        ));
    }

    let lat: f64 = coords[0]
        .parse()
        .map_err(|_| AppError::BadRequest("Invalid latitude".to_string()))?;
    let lng: f64 = coords[1]
        .parse()
        .map_err(|_| AppError::BadRequest("Invalid longitude".to_string()))?;

    if lat < -90.0 || lat > 90.0 || lng < -180.0 || lng > 180.0 {
        return Err(AppError::BadRequest("Coordinates out of range".to_string()));
    }

    let url = "https://revgeocode.search.hereapi.com/v1/revgeocode";
    let api_key = std::env::var("HERE_API_KEY")
        .map_err(|_| AppError::InternalServerError("HERE_API_KEY not configured".to_string()))?;
    let limit_str = params.limit.unwrap_or(5).to_string();

    let query_params = vec![
        ("at", params.at.as_str()),
        ("apiKey", api_key.as_str()),
        ("limit", limit_str.as_str()),
    ];

    let client = reqwest::Client::new();
    let response = client
        .get(url)
        .query(&query_params)
        .send()
        .await
        .map_err(|e| AppError::InternalServerError(format!("HERE API error: {}", e)))?;

    if !response.status().is_success() {
        return Err(AppError::InternalServerError(format!(
            "HERE API error: {}",
            response.status()
        )));
    }

    let here_response: serde_json::Value = response
        .json()
        .await
        .map_err(|e| AppError::InternalServerError(format!("Failed to parse response: {}", e)))?;

    let items = here_response["items"]
        .as_array()
        .unwrap_or(&vec![])
        .iter()
        .map(|item| {
            let pos = &item["position"];
            let addr = &item["address"];
            ReverseGeocodeItem {
                title: item["title"].as_str().unwrap_or("").to_string(),
                position: GeocodePosition {
                    lat: pos["lat"].as_f64().unwrap_or(lat),
                    lng: pos["lng"].as_f64().unwrap_or(lng),
                },
                address: AddressDetails {
                    label: addr["label"].as_str().unwrap_or("").to_string(),
                    country_code: addr["countryCode"].as_str().map(|s| s.to_string()),
                    country_name: addr["countryName"].as_str().map(|s| s.to_string()),
                    state: addr["state"].as_str().map(|s| s.to_string()),
                    city: addr["city"].as_str().map(|s| s.to_string()),
                },
            }
        })
        .collect();

    Ok(Json(ReverseGeocodeResponse { items }))
}
