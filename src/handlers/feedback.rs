use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};

use crate::{
    models::feedback::{CorrectionRequest, Feedback, OutcomeRequest},
    routes::AppState,
    utils::errors::{AppError, AppResult},
};

/// POST /api/v1/feedback/correction — doctor overrides AI prediction
pub async fn record_correction(
    State(state): State<AppState>,
    Json(req): Json<CorrectionRequest>,
) -> AppResult<(StatusCode, Json<Feedback>)> {
    let feedback = state
        .feedback_repo
        .record_correction(&req)
        .await
        .map_err(AppError::Database)?;
    Ok((StatusCode::CREATED, Json(feedback)))
}

/// POST /api/v1/feedback/outcome — record patient discharge outcome
pub async fn record_outcome(
    State(state): State<AppState>,
    Json(req): Json<OutcomeRequest>,
) -> AppResult<(StatusCode, Json<Feedback>)> {
    let feedback = state
        .feedback_repo
        .record_outcome(&req)
        .await
        .map_err(AppError::Database)?;
    Ok((StatusCode::CREATED, Json(feedback)))
}

/// POST /api/v1/feedback/re-assess/:patient_id
/// Re-run Silver→Gold→ML for an existing patient (e.g. after doctor updates notes).
/// Returns immediately; result arrives via SSE /api/v1/pipeline/events?patient_id=...
pub async fn re_assess(
    State(state): State<AppState>,
    Path(patient_id): Path<String>,
) -> AppResult<StatusCode> {
    state
        .pipeline_service
        .re_assess(&patient_id)
        .await?;
    Ok(StatusCode::ACCEPTED)
}
