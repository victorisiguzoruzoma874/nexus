use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    models::patient::{Patient, PatientListQuery, PipelineStats},
    routes::AppState,
    services::ml_service::MlAssessment,
    utils::errors::{AppError, AppResult},
};

#[derive(Debug, Serialize, Deserialize)]
pub struct PatientListResponse {
    pub patients: Vec<Patient>,
    pub total: i64,
    pub page: i64,
    pub page_size: i64,
}

/// GET /api/v1/patients?risk=High&state=Lagos
pub async fn list_patients(
    State(state): State<AppState>,
    Query(query): Query<PatientListQuery>,
) -> AppResult<Json<PatientListResponse>> {
    let page = query.page.unwrap_or(1).max(1);
    let page_size = query.page_size.unwrap_or(20).clamp(1, 100);

    let (patients, total) = state
        .patient_repo
        .list(
            query.risk.as_deref(),
            query.state.as_deref(),
            page,
            page_size,
        )
        .await
        .map_err(AppError::Database)?;

    Ok(Json(PatientListResponse { patients, total, page, page_size }))
}

/// GET /api/v1/patients/:id
pub async fn get_patient(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<Patient>> {
    state
        .patient_repo
        .find_by_uuid(id)
        .await
        .map_err(AppError::Database)?
        .map(Json)
        .ok_or_else(|| AppError::NotFound(format!("Patient {} not found", id)))
}

/// GET /api/v1/patients/:id/assessment — run full ML inference
pub async fn get_assessment(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<MlAssessment>> {
    let patient = state
        .patient_repo
        .find_by_uuid(id)
        .await
        .map_err(AppError::Database)?
        .ok_or_else(|| AppError::NotFound(format!("Patient {} not found", id)))?;

    let assessment = state.ml_service.run_full_inference(&patient).await;
    Ok(Json(assessment))
}

/// GET /api/v1/patients/queue/high-risk — nurse triage queue
pub async fn high_risk_queue(
    State(state): State<AppState>,
) -> AppResult<Json<Vec<Patient>>> {
    let patients = state
        .patient_repo
        .high_risk_queue()
        .await
        .map_err(AppError::Database)?;
    Ok(Json(patients))
}

/// GET /api/v1/patients/alerts/outbreak
pub async fn outbreak_alerts(
    State(state): State<AppState>,
) -> AppResult<Json<Vec<Patient>>> {
    let patients = state
        .patient_repo
        .outbreak_alerts()
        .await
        .map_err(AppError::Database)?;
    Ok(Json(patients))
}

/// GET /api/v1/patients/admin/stats
pub async fn pipeline_stats(
    State(state): State<AppState>,
) -> AppResult<Json<PipelineStats>> {
    let stats = state
        .patient_repo
        .stats()
        .await
        .map_err(AppError::Database)?;
    Ok(Json(stats))
}
