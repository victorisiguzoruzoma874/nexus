use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use uuid::Uuid;
use validator::Validate;
use utoipa::ToSchema;

use crate::{
    models::shift::{CreateShiftRequest, Shift},
    routes::AppState,
    services::shift_service::{self, ShiftServiceError},
    utils::errors::{AppError, AppResult},
};

/// Response for shift preview
#[derive(Debug, serde::Serialize, serde::Deserialize, ToSchema)]
pub struct ShiftPreviewResponse {
    pub role_title: String,
    pub specialty: Option<String>,
    pub department: Option<String>,
    pub shift_type: String,
    pub priority: String,
    pub scheduled_start: String,
    pub duration_hours: f32,
    pub base_amount_kobo: i64,
    pub stat_bonus_kobo: i64,
    pub grand_total_kobo: i64,
    pub virtual_link: Option<String>,
    pub estimated_matches: i32,
}

impl From<shift_service::ShiftPreview> for ShiftPreviewResponse {
    fn from(preview: shift_service::ShiftPreview) -> Self {
        Self {
            role_title: preview.role_title,
            specialty: preview.specialty,
            department: preview.department,
            shift_type: format!("{:?}", preview.shift_type),
            priority: format!("{:?}", preview.priority),
            scheduled_start: preview.scheduled_start.to_rfc3339(),
            duration_hours: preview.duration_hours,
            base_amount_kobo: preview.base_amount_kobo,
            stat_bonus_kobo: preview.stat_bonus_kobo,
            grand_total_kobo: preview.grand_total_kobo,
            virtual_link: preview.virtual_link,
            estimated_matches: preview.estimated_matches,
        }
    }
}

/// POST /api/v1/shifts
/// Create a new shift posting
#[utoipa::path(
    post,
    path = "/api/v1/shifts",
    request_body = CreateShiftRequest,
    responses(
        (status = 201, description = "Shift created successfully", body = Shift),
        (status = 422, description = "Validation error", body = ErrorResponse),
        (status = 409, description = "Duplicate shift exists", body = ErrorResponse),
        (status = 403, description = "Hospital not approved to create shifts", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    tag = "shifts",
    summary = "Create a new shift",
    description = "Create a new shift posting for hospital staff. Only approved hospitals can create shifts. Validates all required fields, checks for duplicates, generates virtual links for virtual shifts, and broadcasts notifications to eligible workers."
)]
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

/// POST /api/v1/shifts/preview
/// AC-06: Preview shift before publishing
#[utoipa::path(
    post,
    path = "/api/v1/shifts/preview",
    request_body = CreateShiftRequest,
    responses(
        (status = 200, description = "Shift preview generated successfully", body = ShiftPreviewResponse),
        (status = 422, description = "Validation error", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    tag = "shifts",
    summary = "Preview a shift before creation",
    description = "Preview how a shift will appear to workers before actually creating it. Shows compensation breakdown, estimated matched workers, and virtual meeting link if applicable."
)]
pub async fn preview_shift(
    State(state): State<AppState>,
    Json(payload): Json<CreateShiftRequest>,
) -> AppResult<Json<ShiftPreviewResponse>> {
    payload.validate().map_err(|e| AppError::Validation(e.to_string()))?;

    match state.shift_service.preview_shift(&payload).await {
        Ok(preview) => Ok(Json(preview.into())),
        Err(e) => Err(map_shift_error(e)),
    }
}

/// GET /api/v1/shifts/{shift_id}
/// Get shift details
#[utoipa::path(
    get,
    path = "/api/v1/shifts/{shift_id}",
    params(
        ("shift_id" = Uuid, Path, description = "Shift unique identifier")
    ),
    responses(
        (status = 200, description = "Shift details retrieved successfully", body = Shift),
        (status = 404, description = "Shift not found", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    tag = "shifts",
    summary = "Get shift details",
    description = "Retrieve detailed information about a specific shift by its ID."
)]
pub async fn get_shift(
    State(state): State<AppState>,
    Path(shift_id): Path<Uuid>,
) -> AppResult<Json<Shift>> {
    match state.shift_service.get_shift(shift_id).await {
        Ok(shift) => Ok(Json(shift)),
        Err(e) => Err(map_shift_error(e)),
    }
}

/// Error response for API documentation
#[derive(Debug, serde::Serialize, serde::Deserialize, ToSchema)]
pub struct ErrorResponse {
    pub error: ErrorDetail,
}

#[derive(Debug, serde::Serialize, serde::Deserialize, ToSchema)]
pub struct ErrorDetail {
    pub message: String,
    pub status: u16,
}

fn map_shift_error(e: ShiftServiceError) -> AppError {
    match e {
        ShiftServiceError::ValidationError(msg) => AppError::Validation(msg),
        ShiftServiceError::NotFound(id) => AppError::NotFound(format!("Shift {} not found", id)),
        ShiftServiceError::DatabaseError(e) => AppError::Database(e),
        ShiftServiceError::DuplicateShift(msg) => AppError::Conflict(msg),
        ShiftServiceError::HospitalNotApproved(msg) => AppError::Forbidden(msg),
    }
}
