use crate::models::distance::{DistanceRequest, DistanceResponse};
use crate::routes::app_routes::AppState;
use crate::services::distance_service::DistanceServiceError;
use crate::utils::errors::AppError;
use axum::{extract::State, Json};

/// Calculate distance and travel time between two locations
///
/// Accepts locations as either coordinates or addresses and returns
/// detailed distance, travel time, and route information with automatic
/// granularity formatting.
#[utoipa::path(
    post,
    path = "/api/v1/distance/calculate",
    tag = "location",
    summary = "Calculate distance between locations",
    description = "Calculate distance and travel time between two locations using either coordinates or addresses",
    request_body = DistanceRequest,
    responses(
        (status = 200, description = "Distance calculated successfully", body = DistanceResponse),
        (status = 400, description = "Invalid request"),
        (status = 404, description = "Location not found"),
        (status = 422, description = "No route available"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn calculate_distance(
    State(state): State<AppState>,
    Json(request): Json<DistanceRequest>,
) -> Result<Json<DistanceResponse>, AppError> {
    // Validate request
    validate_distance_request(&request)?;

    let distance_service = &state.distance_service;

    let response = distance_service
        .calculate_distance(request)
        .await
        .map_err(map_distance_error)?;

    Ok(Json(response))
}

fn validate_distance_request(request: &DistanceRequest) -> Result<(), AppError> {
    // Validate origin
    match &request.origin.location_type {
        crate::models::distance::LocationType::Coordinates => {
            if request.origin.coordinates.is_none() {
                return Err(AppError::BadRequest(
                    "Coordinates required when origin type is 'coordinates'".to_string(),
                ));
            }
        }
        crate::models::distance::LocationType::Address => {
            if request.origin.address.is_none() {
                return Err(AppError::BadRequest(
                    "Address required when origin type is 'address'".to_string(),
                ));
            }
        }
    }

    // Validate destination
    match &request.destination.location_type {
        crate::models::distance::LocationType::Coordinates => {
            if request.destination.coordinates.is_none() {
                return Err(AppError::BadRequest(
                    "Coordinates required when destination type is 'coordinates'".to_string(),
                ));
            }
        }
        crate::models::distance::LocationType::Address => {
            if request.destination.address.is_none() {
                return Err(AppError::BadRequest(
                    "Address required when destination type is 'address'".to_string(),
                ));
            }
        }
    }

    // Validate transport mode
    let valid_modes = ["car", "pedestrian", "truck"];
    if !valid_modes.contains(&request.transport_mode.as_str()) {
        return Err(AppError::BadRequest(format!(
            "Invalid transport mode. Allowed: {}",
            valid_modes.join(", ")
        )));
    }

    // Validate coordinates if provided
    if let Some(coords) = &request.origin.coordinates {
        validate_coordinates(coords)?;
    }
    if let Some(coords) = &request.destination.coordinates {
        validate_coordinates(coords)?;
    }

    Ok(())
}

fn validate_coordinates(
    coords: &crate::models::admin_registration::Coordinates,
) -> Result<(), AppError> {
    if coords.latitude < -90.0 || coords.latitude > 90.0 {
        return Err(AppError::BadRequest(format!(
            "Invalid latitude: {}. Must be between -90 and 90",
            coords.latitude
        )));
    }
    if coords.longitude < -180.0 || coords.longitude > 180.0 {
        return Err(AppError::BadRequest(format!(
            "Invalid longitude: {}. Must be between -180 and 180",
            coords.longitude
        )));
    }
    Ok(())
}

fn map_distance_error(error: DistanceServiceError) -> AppError {
    match error {
        DistanceServiceError::InvalidInput(msg) => AppError::BadRequest(msg),
        DistanceServiceError::LocationResolution(msg) => {
            AppError::NotFound(format!("Location not found: {}", msg))
        }
        DistanceServiceError::HereApiError(here_error) => match here_error {
            crate::services::here_maps::HereError::LocationNotFound(msg) => AppError::NotFound(msg),
            crate::services::here_maps::HereError::NoRouteFound => {
                AppError::UnprocessableEntity("No route available between locations".to_string())
            }
            crate::services::here_maps::HereError::InvalidCoordinates(msg) => {
                AppError::BadRequest(msg)
            }
            _ => {
                tracing::error!("HERE Maps API error: {}", here_error);
                AppError::InternalServerError(
                    "Distance calculation service unavailable".to_string(),
                )
            }
        },
    }
}
