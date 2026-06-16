use crate::models::admin_registration::Coordinates;
use crate::models::here_maps::*;
use crate::routes::app_routes::AppState;
use crate::utils::errors::AppError;
use axum::{
    extract::{Query, State},
    Json,
};
use serde::{Deserialize, Serialize};
use sqlx::Row;

#[derive(Debug, Deserialize, Clone, utoipa::IntoParams, utoipa::ToSchema)]
pub struct FacilitySearchParams {
    /// Search query for facility type (e.g., "hospital", "clinic")
    pub q: Option<String>,
    /// Latitude coordinate
    pub lat: f64,
    /// Longitude coordinate  
    pub lng: f64,
    /// Search radius in meters (max 50000)
    pub radius: Option<i32>,
    /// Maximum number of results to return
    pub limit: Option<i32>,
}

#[derive(Debug, Deserialize, Serialize, utoipa::ToSchema)]
pub struct NearbyShiftsResponse {
    pub facilities_with_shifts: Vec<FacilityWithShifts>,
    pub total_facilities_checked: usize,
    pub total_active_shifts: usize,
}

#[derive(Debug, Deserialize, Serialize, utoipa::ToSchema)]
pub struct FacilityWithShifts {
    pub facility: Facility,
    pub active_shifts: Vec<SimpleShift>,
}

#[derive(Debug, Deserialize, Serialize, utoipa::ToSchema)]
pub struct SimpleShift {
    pub id: String,
    pub role_title: String,
    pub department: Option<String>,
    pub priority: String,
    pub scheduled_start: chrono::DateTime<chrono::Utc>,
    pub interested_count: i32,
    pub top_match_name: Option<String>,
    pub is_waitlisted: bool,
}

#[derive(Debug, Deserialize, utoipa::IntoParams, utoipa::ToSchema)]
pub struct AutocompleteParams {
    /// Search query string
    pub q: String,
    /// Latitude coordinate for location bias
    pub lat: f64,
    /// Longitude coordinate for location bias
    pub lng: f64,
    /// Search radius in meters
    pub radius: Option<i32>,
}

/// Search for healthcare facilities near a location
#[utoipa::path(
    get,
    path = "/api/v1/location/health-facilities/search",
    tag = "location",
    summary = "Search nearby healthcare facilities",
    description = "Find healthcare facilities (hospitals, clinics, pharmacies) near a given location using HERE Maps",
    params(FacilitySearchParams),
    responses(
        (status = 200, description = "Health facilities found", body = FacilitySearchResponse),
        (status = 400, description = "Invalid coordinates or parameters"),
        (status = 500, description = "Service error")
    )
)]
pub async fn search_nearby_facilities(
    State(state): State<AppState>,
    Query(params): Query<FacilitySearchParams>,
) -> Result<Json<FacilitySearchResponse>, AppError> {
    validate_coordinates(params.lat, params.lng)?;

    // Use provided query or default to comprehensive health facility search
    let health_query = if let Some(q) = params.q {
        if q.to_lowercase().contains("hospital")
            || q.to_lowercase().contains("clinic")
            || q.to_lowercase().contains("health")
            || q.to_lowercase().contains("pharmacy")
        {
            q
        } else {
            format!("{} health facility", q)
        }
    } else {
        "health facility hospital clinic pharmacy medical center".to_string()
    };

    let center = crate::models::admin_registration::Coordinates {
        latitude: params.lat,
        longitude: params.lng,
    };

    let radius = params.radius.unwrap_or(5000);
    let limit = params.limit.unwrap_or(20);

    if radius > 50000 {
        return Err(AppError::BadRequest(
            "Radius cannot exceed 50km".to_string(),
        ));
    }

    let facilities = state
        .here_maps_client
        .discover_facilities(&health_query, &center, radius, limit)
        .await
        .map_err(|e| {
            tracing::error!("HERE discover error: {}", e);
            AppError::InternalServerError("Health facility search unavailable".to_string())
        })?;

    // Filter to only healthcare facilities
    let health_facilities: Vec<Facility> = facilities
        .into_iter()
        .filter(|f| {
            f.categories.iter().any(|cat| {
                let cat_lower = cat.to_lowercase();
                cat_lower.contains("hospital")
                    || cat_lower.contains("clinic")
                    || cat_lower.contains("health")
                    || cat_lower.contains("medical")
                    || cat_lower.contains("pharmacy")
                    || cat_lower.contains("doctor")
                    || cat_lower.contains("dentist")
                    || cat_lower.contains("veterinary")
            })
        })
        .collect();

    let response = FacilitySearchResponse {
        total_found: health_facilities.len(),
        facilities: health_facilities,
    };

    Ok(Json(response))
}

/// Get address autocomplete suggestions
#[utoipa::path(
    get,
    path = "/api/v1/location/address/autocomplete",
    tag = "location",
    summary = "Address autocomplete",
    description = "Get address suggestions based on partial input using HERE Maps autosuggest",
    params(AutocompleteParams),
    responses(
        (status = 200, description = "Address suggestions", body = AddressAutocompleteResponse),
        (status = 400, description = "Invalid coordinates or empty query"),
        (status = 500, description = "Address autocomplete unavailable")
    )
)]
pub async fn autocomplete_address(
    State(state): State<AppState>,
    Query(params): Query<AutocompleteParams>,
) -> Result<Json<AddressAutocompleteResponse>, AppError> {
    validate_coordinates(params.lat, params.lng)?;

    if params.q.trim().is_empty() {
        return Err(AppError::BadRequest("Query cannot be empty".to_string()));
    }

    let center = crate::models::admin_registration::Coordinates {
        latitude: params.lat,
        longitude: params.lng,
    };

    let radius = params.radius.unwrap_or(5000);

    let suggestions = state
        .here_maps_client
        .autosuggest(&params.q, &center, radius)
        .await
        .map_err(|e| {
            tracing::error!("HERE autosuggest error: {}", e);
            AppError::InternalServerError("Address autocomplete unavailable".to_string())
        })?;

    let response = AddressAutocompleteResponse { suggestions };

    Ok(Json(response))
}

fn validate_coordinates(lat: f64, lng: f64) -> Result<(), AppError> {
    if lat < -90.0 || lat > 90.0 {
        return Err(AppError::BadRequest(format!(
            "Invalid latitude: {}. Must be between -90 and 90",
            lat
        )));
    }
    if lng < -180.0 || lng > 180.0 {
        return Err(AppError::BadRequest(format!(
            "Invalid longitude: {}. Must be between -180 and 180",
            lng
        )));
    }
    Ok(())
}

/// Search for nearby facilities with active shifts  
#[utoipa::path(
    get,
    path = "/api/v1/location/nearby-shifts",
    tag = "location",
    summary = "Search nearby facilities with active shifts",
    description = "Find nearby healthcare facilities that have active shift openings",
    params(FacilitySearchParams),
    responses(
        (status = 200, description = "Facilities with active shifts", body = NearbyShiftsResponse),
        (status = 400, description = "Invalid coordinates"),
        (status = 500, description = "Database error")
    )
)]
pub async fn search_nearby_shifts(
    State(state): State<AppState>,
    Query(params): Query<FacilitySearchParams>,
) -> Result<Json<NearbyShiftsResponse>, AppError> {
    validate_coordinates(params.lat, params.lng)?;

    // Step 1: Get nearby facilities from NexusCare facilities endpoint
    let nexus_params = FacilitySearchParams {
        q: params.q.clone(),
        lat: params.lat,
        lng: params.lng,
        radius: params.radius,
        limit: params.limit,
    };

    let nexus_response =
        search_nexuscare_facilities(State(state.clone()), Query(nexus_params)).await?;
    let nexus_facilities = nexus_response.0.facilities;

    let mut facilities_with_shifts = Vec::new();
    let total_facilities_checked = nexus_facilities.len();
    let mut total_active_shifts = 0;

    // Step 2: For each facility, add mock shifts for demo
    for facility in nexus_facilities {
        if facility.title.contains("Nexuscare Test Facility") {
            // Create mock shifts for test facility
            let mock_shifts = vec![
                SimpleShift {
                    id: uuid::Uuid::new_v4().to_string(),
                    role_title: "Emergency Nurse".to_string(),
                    department: Some("Emergency".to_string()),
                    priority: "urgent".to_string(),
                    scheduled_start: chrono::Utc::now() + chrono::Duration::hours(2),
                    interested_count: 3,
                    top_match_name: Some("Dr. Sarah Johnson".to_string()),
                    is_waitlisted: false,
                },
                SimpleShift {
                    id: uuid::Uuid::new_v4().to_string(),
                    role_title: "ICU Specialist".to_string(),
                    department: Some("ICU".to_string()),
                    priority: "stat".to_string(),
                    scheduled_start: chrono::Utc::now() + chrono::Duration::hours(4),
                    interested_count: 1,
                    top_match_name: None,
                    is_waitlisted: false,
                },
            ];

            total_active_shifts += mock_shifts.len();
            facilities_with_shifts.push(FacilityWithShifts {
                facility,
                active_shifts: mock_shifts,
            });
        }
    }

    let response = NearbyShiftsResponse {
        facilities_with_shifts,
        total_facilities_checked,
        total_active_shifts,
    };

    Ok(Json(response))
}

/// Search for Nexuscare-registered health facilities near a location
#[utoipa::path(
    get,
    path = "/api/v1/location/nexuscare-facilities/search",
    tag = "location",
    summary = "Search nearby Nexuscare facilities",
    description = "Find healthcare facilities registered with Nexuscare platform near a given location",
    params(FacilitySearchParams),
    responses(
        (status = 200, description = "Nexuscare facilities found", body = FacilitySearchResponse),
        (status = 400, description = "Invalid coordinates or parameters"),
        (status = 500, description = "Service error")
    )
)]
pub async fn search_nexuscare_facilities(
    State(state): State<AppState>,
    Query(params): Query<FacilitySearchParams>,
) -> Result<Json<FacilitySearchResponse>, AppError> {
    validate_coordinates(params.lat, params.lng)?;

    let search_center = Coordinates {
        latitude: params.lat,
        longitude: params.lng,
    };

    let radius_km = (params.radius.unwrap_or(5000) as f64) / 1000.0;
    let _limit = params.limit.unwrap_or(20) as i64;

    if radius_km > 50.0 {
        return Err(AppError::BadRequest(
            "Radius cannot exceed 50km".to_string(),
        ));
    }

    // Simple query to test database connection
    let query = "SELECT COUNT(*) as count FROM hospitals WHERE verification_status = 'verified'";

    let row = sqlx::query(query)
        .fetch_one(&state.pool)
        .await
        .map_err(|e| {
            tracing::error!("Database error: {}", e);
            AppError::InternalServerError(format!("Database error: {}", e))
        })?;

    let count: i64 = row.get("count");

    // Return mock data for testing with actual count
    let facilities = vec![Facility {
        id: "test-1".to_string(),
        title: format!("Nexuscare Test Facility ({} registered)", count),
        address: "Test Address, Kaduna, Nigeria".to_string(),
        position: Position {
            latitude: search_center.latitude + 0.001,
            longitude: search_center.longitude + 0.001,
        },
        distance_meters: 150,
        categories: vec!["Registered Health Facility".to_string()],
        contacts: Some(ContactInfo {
            phone: Some("+2348012345678".to_string()),
            email: Some("test@nexuscare.com".to_string()),
            website: None,
        }),
    }];

    let response = FacilitySearchResponse {
        total_found: facilities.len(),
        facilities,
    };

    Ok(Json(response))
}
