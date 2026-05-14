use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use uuid::Uuid;
use validator::Validate;

use crate::{
    models::shift::{CreateShiftRequest, Shift},
    routes::AppState,
    services::shift_service::ShiftServiceError,
    utils::errors::{AppError, AppResult},
};

/// POST /api/v1/shifts
/// Create a new shift posting
pub async fn create_shift(
    State(state): State<AppState>,
    Json(payload): Json<CreateShiftRequest>,
) -> AppResult<(StatusCode, Json<Shift>)> {
    payload.validate().map_err(|e| AppError::Validation(e.to_string()))?;

    // Extract hospital_id and user_id from state (mock for now)
    // In production, these would come from JWT token
    let hospital_id = Uuid::new_v4(); // TODO: Get from authenticated user
    let created_by = Uuid::new_v4(); // TODO: Get from authenticated user

    match state.shift_service.create_shift(hospital_id, created_by, payload).await {
        Ok(shift) => Ok((StatusCode::CREATED, Json(shift))),
        Err(e) => Err(map_shift_error(e)),
    }
}

/// GET /api/v1/shifts/{shift_id}
/// Get shift details
pub async fn get_shift(
    State(state): State<AppState>,
    Path(shift_id): Path<Uuid>,
) -> AppResult<Json<Shift>> {
    match state.shift_service.get_shift(shift_id).await {
        Ok(shift) => Ok(Json(shift)),
        Err(e) => Err(map_shift_error(e)),
    }
}

fn map_shift_error(e: ShiftServiceError) -> AppError {
    match e {
        ShiftServiceError::ValidationError(msg) => AppError::Validation(msg),
        ShiftServiceError::NotFound(id) => AppError::NotFound(format!("Shift {} not found", id)),
        ShiftServiceError::DatabaseError(e) => AppError::Database(e),
    }
}
