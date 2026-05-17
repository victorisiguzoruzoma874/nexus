use axum::http::HeaderMap;
use jsonwebtoken::{decode, DecodingKey, Validation};
use crate::models::user::Claims;
use crate::utils::errors::AppError;

/// Extract and decode JWT claims from the `Authorization: Bearer <token>` header.
pub fn extract_claims(headers: &HeaderMap) -> Result<Claims, AppError> {
    let token = headers
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or_else(|| AppError::Unauthorized("Missing or invalid Authorization header".to_string()))?;

    let secret = std::env::var("JWT_SECRET").unwrap_or_default();
    decode::<Claims>(token, &DecodingKey::from_secret(secret.as_bytes()), &Validation::default())
        .map(|data| data.claims)
        .map_err(|_| AppError::Unauthorized("Invalid or expired token".to_string()))
}
