use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use serde_json::Value;
use validator::Validate;

use crate::{
    models::patient::{IngestPatientDto, IngestResponse},
    routes::AppState,
    utils::errors::{AppError, AppResult},
};

/// POST /api/v1/ingest/patient
pub async fn ingest_patient(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(dto): Json<IngestPatientDto>,
) -> AppResult<(StatusCode, Json<IngestResponse>)> {
    dto.validate().map_err(|e| AppError::Validation(e.to_string()))?;

    let ip = headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown");

    let response = state
        .pipeline_service
        .ingest_and_process(dto, ip)
        .await?;

    Ok((StatusCode::ACCEPTED, Json(response)))
}

/// POST /api/v1/ingest/process-pending — manually trigger Silver
pub async fn process_pending(
    State(state): State<AppState>,
) -> AppResult<Json<serde_json::Value>> {
    let count = state
        .pipeline_service
        .silver
        .process_pending()
        .await
        .map_err(AppError::Database)?;

    Ok(Json(serde_json::json!({ "processed": count })))
}

/// POST /api/v1/ingest/enrich-all — manually trigger Gold
pub async fn enrich_all(
    State(state): State<AppState>,
) -> AppResult<Json<serde_json::Value>> {
    let count = state
        .pipeline_service
        .gold
        .enrich_all()
        .await
        .map_err(AppError::Database)?;

    Ok(Json(serde_json::json!({ "enriched": count })))
}

/// GET /api/v1/ingest/audit/:id — retrieve Bronze raw blob
pub async fn audit_blob(
    State(state): State<AppState>,
    Path(patient_id): Path<String>,
) -> AppResult<Json<Value>> {
    let blob = state.pipeline_service.bronze.read_blob(&patient_id)?;
    Ok(Json(blob))
}
