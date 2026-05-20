use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use uuid::Uuid;
use validator::Validate;
use utoipa::ToSchema;

use crate::{
    models::shift::{
        CreateShiftRequest, Shift, ShiftApplication, ShiftApplicationRequest,
        ShiftApplicationsQuery, ShiftAssignRequest, ShiftCancelRequest,
        ShiftInterestRequest, ShiftListQuery, ShiftRescheduleRequest,
    },
    routes::AppState,
    services::shift_service::{self, ShiftServiceError},
    utils::{errors::{AppError, AppResult}, extract_claims},
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

#[derive(Debug, serde::Serialize, serde::Deserialize, ToSchema)]
pub struct PaginationMetadata {
    pub current_page: i64,
    pub page_size: i64,
    pub total_items: i64,
    pub total_pages: i64,
    pub has_next: bool,
    pub has_previous: bool,
}

#[derive(Debug, serde::Serialize, serde::Deserialize, ToSchema)]
pub struct ShiftListResponse {
    pub shifts: Vec<Shift>,
    pub pagination: PaginationMetadata,
}

#[derive(Debug, serde::Serialize, serde::Deserialize, ToSchema)]
pub struct ShiftApplicationsResponse {
    pub applications: Vec<ShiftApplication>,
    pub pagination: PaginationMetadata,
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
    headers: HeaderMap,
    Json(payload): Json<CreateShiftRequest>,
) -> AppResult<(StatusCode, Json<Shift>)> {
    payload.validate().map_err(|e| AppError::Validation(e.to_string()))?;

    let claims = extract_claims(&headers)?;
    let created_by = Uuid::parse_str(&claims.sub)
        .map_err(|_| AppError::Unauthorized("Invalid user ID in token".to_string()))?;
    let hospital_id = claims.hospital_id
        .as_deref()
        .and_then(|s| Uuid::parse_str(s).ok())
        .ok_or_else(|| AppError::Forbidden("No hospital associated with this account".to_string()))?;

    match state.shift_service.create_shift(hospital_id, created_by, payload).await {
        Ok(shift) => Ok((StatusCode::CREATED, Json(shift))),
        Err(e) => Err(map_shift_error(e)),
    }
}

/// GET /api/v1/shifts
#[utoipa::path(
    get,
    path = "/api/v1/shifts",
    params(
        ("status" = Option<crate::models::shift::ShiftStatus>, Query, description = "Optional status filter"),
        ("page" = Option<i64>, Query, description = "Page number"),
        ("page_size" = Option<i64>, Query, description = "Page size"),
    ),
    responses(
        (status = 200, description = "Shifts retrieved", body = ShiftListResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    tag = "shifts",
    summary = "List shifts",
    description = "List shifts with optional status filter and pagination"
)]
pub async fn list_shifts(
    State(state): State<AppState>,
    Query(query): Query<ShiftListQuery>,
) -> AppResult<Json<ShiftListResponse>> {
    let page = query.page.unwrap_or(1).max(1);
    let page_size = query.page_size.unwrap_or(20).clamp(1, 100);

    let (shifts, total) = state
        .shift_service
        .list_shifts(query.status, page, page_size)
        .await
        .map_err(map_shift_error)?;

    let total_pages = (total as f64 / page_size as f64).ceil() as i64;

    Ok(Json(ShiftListResponse {
        shifts,
        pagination: PaginationMetadata {
            current_page: page,
            page_size,
            total_items: total,
            total_pages,
            has_next: page < total_pages,
            has_previous: page > 1,
        },
    }))
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

/// POST /api/v1/shifts/{shift_id}/interest
#[utoipa::path(
    post,
    path = "/api/v1/shifts/{shift_id}/interest",
    request_body = ShiftInterestRequest,
    params(
        ("shift_id" = Uuid, Path, description = "Shift unique identifier")
    ),
    responses(
        (status = 201, description = "Interest recorded"),
        (status = 404, description = "Shift not found", body = ErrorResponse),
        (status = 409, description = "Interest already exists", body = ErrorResponse),
        (status = 422, description = "Validation error", body = ErrorResponse)
    ),
    tag = "shifts",
    summary = "Express interest in a shift",
    description = "Clinician expresses interest in an open shift"
)]
pub async fn express_interest(
    State(state): State<AppState>,
    Path(shift_id): Path<Uuid>,
    Json(payload): Json<ShiftInterestRequest>,
) -> AppResult<StatusCode> {
    payload.validate().map_err(|e| AppError::Validation(e.to_string()))?;

    state
        .shift_service
        .express_interest(shift_id, payload.clinician_id)
        .await
        .map(|_| StatusCode::CREATED)
        .map_err(map_shift_error)
}

/// POST /api/v1/shifts/{shift_id}/apply
#[utoipa::path(
    post,
    path = "/api/v1/shifts/{shift_id}/apply",
    request_body = ShiftApplicationRequest,
    params(
        ("shift_id" = Uuid, Path, description = "Shift unique identifier")
    ),
    responses(
        (status = 201, description = "Application submitted"),
        (status = 403, description = "Profile incomplete or not allowed", body = ErrorResponse),
        (status = 404, description = "Shift not found", body = ErrorResponse),
        (status = 409, description = "Already applied or busy", body = ErrorResponse),
        (status = 422, description = "Validation error", body = ErrorResponse)
    ),
    tag = "shifts",
    summary = "Apply for a shift",
    description = "Submit a shift application with profile details and experience"
)]
pub async fn apply_for_shift(
    State(state): State<AppState>,
    Path(shift_id): Path<Uuid>,
    Json(payload): Json<ShiftApplicationRequest>,
) -> AppResult<StatusCode> {
    payload.validate().map_err(|e| AppError::Validation(e.to_string()))?;

    state
        .shift_service
        .apply_for_shift(shift_id, payload)
        .await
        .map(|_| StatusCode::CREATED)
        .map_err(map_shift_error)
}

/// GET /api/v1/shifts/{shift_id}/applications
#[utoipa::path(
    get,
    path = "/api/v1/shifts/{shift_id}/applications",
    params(
        ("shift_id" = Uuid, Path, description = "Shift unique identifier"),
        ("requester_user_id" = Uuid, Query, description = "User ID of the requester"),
        ("page" = Option<i64>, Query, description = "Page number"),
        ("page_size" = Option<i64>, Query, description = "Page size"),
    ),
    responses(
        (status = 200, description = "Applications retrieved", body = ShiftApplicationsResponse),
        (status = 403, description = "Not authorized", body = ErrorResponse),
        (status = 404, description = "Shift not found", body = ErrorResponse)
    ),
    tag = "shifts",
    summary = "List shift applications",
    description = "List applications for a shift (only shift creator can view)"
)]
pub async fn list_shift_applications(
    State(state): State<AppState>,
    Path(shift_id): Path<Uuid>,
    Query(query): Query<ShiftApplicationsQuery>,
) -> AppResult<Json<ShiftApplicationsResponse>> {
    let page = query.page.unwrap_or(1).max(1);
    let page_size = query.page_size.unwrap_or(20).clamp(1, 100);

    let (applications, total) = state
        .shift_service
        .list_applications_for_shift(shift_id, query.requester_user_id, page, page_size)
        .await
        .map_err(map_shift_error)?;

    let total_pages = (total as f64 / page_size as f64).ceil() as i64;

    Ok(Json(ShiftApplicationsResponse {
        applications,
        pagination: PaginationMetadata {
            current_page: page,
            page_size,
            total_items: total,
            total_pages,
            has_next: page < total_pages,
            has_previous: page > 1,
        },
    }))
}

/// POST /api/v1/shifts/{shift_id}/assign
#[utoipa::path(
    post,
    path = "/api/v1/shifts/{shift_id}/assign",
    request_body = ShiftAssignRequest,
    params(
        ("shift_id" = Uuid, Path, description = "Shift unique identifier")
    ),
    responses(
        (status = 204, description = "Shift assigned"),
        (status = 404, description = "Shift not found", body = ErrorResponse),
        (status = 409, description = "Shift already assigned", body = ErrorResponse),
        (status = 422, description = "Validation error", body = ErrorResponse)
    ),
    tag = "shifts",
    summary = "Assign a clinician to a shift",
    description = "Hospital assigns a clinician to an open shift"
)]
pub async fn assign_shift(
    State(state): State<AppState>,
    Path(shift_id): Path<Uuid>,
    Json(payload): Json<ShiftAssignRequest>,
) -> AppResult<StatusCode> {
    payload.validate().map_err(|e| AppError::Validation(e.to_string()))?;

    state
        .shift_service
        .assign_shift(shift_id, payload.clinician_id)
        .await
        .map(|_| StatusCode::NO_CONTENT)
        .map_err(map_shift_error)
}

/// POST /api/v1/shifts/{shift_id}/cancel
#[utoipa::path(
    post,
    path = "/api/v1/shifts/{shift_id}/cancel",
    request_body = ShiftCancelRequest,
    params(
        ("shift_id" = Uuid, Path, description = "Shift unique identifier")
    ),
    responses(
        (status = 204, description = "Shift cancelled"),
        (status = 404, description = "Shift not found", body = ErrorResponse),
        (status = 409, description = "Invalid shift status", body = ErrorResponse),
        (status = 422, description = "Validation error", body = ErrorResponse)
    ),
    tag = "shifts",
    summary = "Cancel a shift",
    description = "Cancel an open or upcoming shift"
)]
pub async fn cancel_shift(
    State(state): State<AppState>,
    Path(shift_id): Path<Uuid>,
    Json(payload): Json<ShiftCancelRequest>,
) -> AppResult<StatusCode> {
    payload.validate().map_err(|e| AppError::Validation(e.to_string()))?;

    state
        .shift_service
        .cancel_shift(shift_id, &payload.reason)
        .await
        .map(|_| StatusCode::NO_CONTENT)
        .map_err(map_shift_error)
}

/// POST /api/v1/shifts/{shift_id}/reschedule
#[utoipa::path(
    post,
    path = "/api/v1/shifts/{shift_id}/reschedule",
    request_body = ShiftRescheduleRequest,
    params(
        ("shift_id" = Uuid, Path, description = "Shift unique identifier")
    ),
    responses(
        (status = 204, description = "Shift rescheduled"),
        (status = 404, description = "Shift not found", body = ErrorResponse),
        (status = 409, description = "Invalid shift status", body = ErrorResponse),
        (status = 422, description = "Validation error", body = ErrorResponse)
    ),
    tag = "shifts",
    summary = "Reschedule a shift",
    description = "Update the start time and duration for a shift"
)]
pub async fn reschedule_shift(
    State(state): State<AppState>,
    Path(shift_id): Path<Uuid>,
    Json(payload): Json<ShiftRescheduleRequest>,
) -> AppResult<StatusCode> {
    payload.validate().map_err(|e| AppError::Validation(e.to_string()))?;

    state
        .shift_service
        .reschedule_shift(shift_id, payload.scheduled_start, payload.duration_hours)
        .await
        .map(|_| StatusCode::NO_CONTENT)
        .map_err(map_shift_error)
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
        ShiftServiceError::DuplicateInterest => {
            AppError::Conflict("Shift interest already exists".to_string())
        }
        ShiftServiceError::DuplicateApplication => {
            AppError::Conflict("Shift application already exists".to_string())
        }
        ShiftServiceError::ProfileIncomplete => {
            AppError::Forbidden("Clinician profile is incomplete".to_string())
        }
        ShiftServiceError::ClinicianBusy => {
            AppError::Conflict("Clinician already assigned to an active shift".to_string())
        }
        ShiftServiceError::NotAuthorized => {
            AppError::Forbidden("Not authorized to view applications".to_string())
        }
        ShiftServiceError::AlreadyAssigned => {
            AppError::Conflict("Shift already assigned".to_string())
        }
        ShiftServiceError::InvalidStatus(msg) => AppError::Conflict(msg),
    }
}
