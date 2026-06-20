use axum::{extract::State, Json};
use serde_json::Value;

use crate::{routes::AppState, utils::errors::{AppError, AppResult}};

/// GET /api/v1/ml/health — proxy to Python ML service /health
pub async fn ml_health(State(state): State<AppState>) -> AppResult<Json<Value>> {
    let ml_url = std::env::var("ML_SERVICE_URL").unwrap_or_else(|_| "http://localhost:8001".into());

    let resp = state
        .ml_service
        .client()
        .get(format!("{}/health", ml_url))
        .send()
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("ML service unreachable: {}", e)))?;

    let body: Value = resp
        .json()
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("ML health parse error: {}", e)))?;

    Ok(Json(body))
}
