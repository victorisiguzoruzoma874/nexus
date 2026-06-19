use axum::extract::State;
use axum::{extract::Query, http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::handlers::registration::{
    ErrorResponse as RegistrationErrorResponse, ListHospitalsQuery,
};
use crate::models::clinician::ClinicianAdminSummary;
use crate::models::registration::RegistrationStatus;
use crate::routes::AppState;
use crate::services::registration_service::HospitalListResponse;

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ClinicianListResponse {
    pub clinicians: Vec<ClinicianAdminSummary>,
    pub pagination: PaginationMetadata,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct PaginationMetadata {
    pub current_page: i64,
    pub page_size: i64,
    pub total_items: i64,
    pub total_pages: i64,
    pub has_next: bool,
    pub has_previous: bool,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct ListCliniciansQuery {
    pub page: Option<i64>,
    pub page_size: Option<i64>,
}

/// GET /api/v1/admin/hospitals
#[utoipa::path(
    get,
    path = "/api/v1/admin/hospitals",
    tag = "admin",
    params(
        ("status" = Option<String>, Query, description = "Filter by status: pending, approved, rejected"),
        ("page" = Option<i64>, Query, description = "Page number (default: 1)"),
        ("page_size" = Option<i64>, Query, description = "Items per page (default: 20, max: 100)")
    ),
    responses(
        (status = 200, description = "Hospitals retrieved successfully", body = HospitalListResponse),
        (status = 400, description = "Invalid status", body = RegistrationErrorResponse),
        (status = 500, description = "Internal server error", body = RegistrationErrorResponse)
    )
)]
pub async fn list_hospitals_admin(
    State(state): State<AppState>,
    Query(params): Query<ListHospitalsQuery>,
) -> Result<Json<HospitalListResponse>, (StatusCode, Json<RegistrationErrorResponse>)> {
    let status_filter = if let Some(status_str) = params.status {
        match status_str.to_lowercase().as_str() {
            "pending" => Some(RegistrationStatus::Pending),
            "approved" => Some(RegistrationStatus::Approved),
            "rejected" => Some(RegistrationStatus::Rejected),
            _ => {
                return Err((
                    StatusCode::BAD_REQUEST,
                    Json(RegistrationErrorResponse {
                        code: "INVALID_STATUS".to_string(),
                        message: "Status must be one of: pending, approved, rejected".to_string(),
                    }),
                ));
            }
        }
    } else {
        None
    };

    let page = params.page.unwrap_or(1);
    let page_size = params.page_size.unwrap_or(20);

    match state
        .registration_service
        .list_hospitals(status_filter, page, page_size)
        .await
    {
        Ok(response) => Ok(Json(response)),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(RegistrationErrorResponse {
                code: "ERROR".to_string(),
                message: e.to_string(),
            }),
        )),
    }
}

/// GET /api/v1/admin/clinicians
#[utoipa::path(
    get,
    path = "/api/v1/admin/clinicians",
    tag = "admin",
    params(
        ("page" = Option<i64>, Query, description = "Page number (default: 1)"),
        ("page_size" = Option<i64>, Query, description = "Items per page (default: 20, max: 100)")
    ),
    responses(
        (status = 200, description = "Clinicians retrieved successfully", body = ClinicianListResponse),
        (status = 500, description = "Internal server error", body = RegistrationErrorResponse)
    )
)]
pub async fn list_clinicians_admin(
    State(state): State<AppState>,
    Query(params): Query<ListCliniciansQuery>,
) -> Result<Json<ClinicianListResponse>, (StatusCode, Json<RegistrationErrorResponse>)> {
    let page = params.page.unwrap_or(1).max(1);
    let page_size = params.page_size.unwrap_or(20).clamp(1, 100);
    let offset = (page - 1) * page_size;

    let clinicians = state
        .clinician_repo
        .list_completed_clinicians(page_size, offset)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(RegistrationErrorResponse {
                    code: "ERROR".to_string(),
                    message: e.to_string(),
                }),
            )
        })?;

    let total = state
        .clinician_repo
        .count_completed_clinicians()
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(RegistrationErrorResponse {
                    code: "ERROR".to_string(),
                    message: e.to_string(),
                }),
            )
        })?;

    let total_pages = (total as f64 / page_size as f64).ceil() as i64;

    Ok(Json(ClinicianListResponse {
        clinicians,
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
