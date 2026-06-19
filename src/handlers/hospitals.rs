use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use uuid::Uuid;
use validator::Validate;

use crate::{
    routes::AppState,
    utils::errors::{AppError, AppResult},
    models::hospital::{
        CreateHospitalRequest, Hospital, HospitalResponse, RegistrationStep,
        UpdateHospitalRequest, VerificationStatus,
    },
};

/// POST /api/v1/hospitals

pub async fn create_hospital(
    State(state): State<AppState>,
    Json(payload): Json<CreateHospitalRequest>,
) -> AppResult<(StatusCode, Json<HospitalResponse>)> {
    payload.validate(). map_err(|e| AppError::Validation(e.to_string()))?;

    // Check for duplicate registration number or email
    let existing: Option<(Uuid,)> = sqlx::query_as(
        "SELECT id FROM hospitals WHERE registration_number = $1 OR email = $2",
    )
    .bind(&payload.registration_number)
    .bind(&payload.email)
    .fetch_optional(&state.pool)
    .await?;

    if existing.is_some() {
        return Err(AppError::Conflict(
            "A hospital with this registration number or email already exists".to_string(), ));
    }

    let hospital: Hospital = sqlx::query_as(
        r#"
        INSERT INTO hospitals (id, name, registration_number, email, address, phone_number,
                               verification_status, registration_step)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        RETURNING id, name, registration_number, email, address, phone_number,
                  verification_status, registration_step, logo_url, created_at, updated_at
        "#,
    )
    .bind(Uuid::new_v4())
    .bind(&payload.name)
    .bind(&payload.registration_number)
    .bind(&payload.email)
    .bind(&payload.address)
    .bind(&payload.phone_number)
    .bind(VerificationStatus::Pending)
    .bind(RegistrationStep::ProfileSetup)
    .fetch_one(&state.pool)
    .await?;

    Ok((StatusCode::CREATED, Json(HospitalResponse::from(hospital))))
}

/// GET /api/v1/hospitals/:id
pub async fn get_hospital(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<HospitalResponse>> {
    let hospital: Option<Hospital> = sqlx::query_as(
        r#"
        SELECT id, name, registration_number, email, address, phone_number,
               verification_status, registration_step, logo_url, created_at, updated_at
        FROM hospitals
        WHERE id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(&state.pool)
    .await?;

    let hospital = hospital.ok_or_else(|| AppError::NotFound(format!("Hospital {} not found", id)))?;
    Ok(Json(HospitalResponse::from(hospital)))
}

/// PATCH /api/v1/hospitals/:id
pub async fn update_hospital(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(payload): Json<UpdateHospitalRequest>,
) -> AppResult<Json<HospitalResponse>> {
    payload.validate(). map_err(|e| AppError::Validation(e.to_string()))?;

    let hospital: Option<Hospital> = sqlx::query_as(
        r#"
        UPDATE hospitals
        SET
            name         = COALESCE($2, name),
            email        = COALESCE($3, email),
            address      = COALESCE($4, address),
            phone_number = COALESCE($5, phone_number),
            logo_url     = COALESCE($6, logo_url),
            updated_at   = NOW()
        WHERE id = $1
        RETURNING id, name, registration_number, email, address, phone_number,
                  verification_status, registration_step, logo_url, created_at, updated_at
        "#,
    )
    .bind(id)
    .bind(&payload.name)
    .bind(&payload.email)
    .bind(&payload.address)
    .bind(&payload.phone_number)
    .bind(&payload.logo_url)
    .fetch_optional(&state.pool)
    .await?;

    let hospital = hospital.ok_or_else(|| AppError::NotFound(format!("Hospital {} not found", id)))?;
    Ok(Json(HospitalResponse::from(hospital)))
}

/// PATCH /api/v1/hospitals/:id/advance-step

pub async fn advance_registration_step(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<HospitalResponse>> {
    // Fetch current step
    let row: Option<(RegistrationStep,)> = sqlx::query_as(
        "SELECT registration_step FROM hospitals WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(&state.pool)
    .await?;

    let (current_step,) = row.ok_or_else(|| AppError::NotFound(format!("Hospital {} not found", id)))?;

    let next_step = match current_step {
        RegistrationStep::ProfileSetup => RegistrationStep::Credentials,
        RegistrationStep::Credentials => RegistrationStep::Verification,
        RegistrationStep::Verification => RegistrationStep::AccessGranted,
        RegistrationStep::AccessGranted => {
            return Err(AppError::Conflict("Registration is already complete".to_string()));
        }
    };

    let hospital: Hospital = sqlx::query_as(
        r#"
        UPDATE hospitals
        SET registration_step = $2, updated_at = NOW()
        WHERE id = $1
        RETURNING id, name, registration_number, email, address, phone_number,
                  verification_status, registration_step, logo_url, created_at, updated_at
        "#,
    )
    .bind(id)
    .bind(next_step)
    .fetch_one(&state.pool)
    .await?;

    Ok(Json(HospitalResponse::from(hospital)))
}

/// GET /api/v1/hospitals

pub async fn list_hospitals(
    State(state): State<AppState>,
) -> AppResult<Json<Vec<HospitalResponse>>> {
    let hospitals: Vec<Hospital> = sqlx::query_as(
        r#"
        SELECT id, name, registration_number, email, address, phone_number,
               verification_status, registration_step, logo_url, created_at, updated_at
        FROM hospitals
        ORDER BY created_at DESC
        "#,
    )
    .fetch_all(&state.pool)
    .await?;

    Ok(Json(hospitals.into_iter(). map(HospitalResponse::from).collect()))
}
