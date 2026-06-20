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
        .ok_or_else(|| {
            AppError::Unauthorized("Missing or invalid Authorization header".to_string())
        })?;

    let secret = std::env::var("JWT_SECRET").unwrap_or_default();
    decode_token(token, &secret)
}

/// Verify and decode a token against `secret`. Fails closed when `secret` is
/// empty, so a missing/empty `JWT_SECRET` can never validate forged tokens
/// (the startup check in `AppConfig::from_env` is the primary guard; this is
/// defense in depth for the request path).
fn decode_token(token: &str, secret: &str) -> Result<Claims, AppError> {
    if secret.trim().is_empty() {
        tracing::error!("JWT_SECRET is empty; refusing to validate tokens (failing closed)");
        return Err(AppError::Unauthorized(
            "Invalid or expired token".to_string(),
        ));
    }

    decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &Validation::default(),
    )
    .map(|data| data.claims)
    .map_err(|_| AppError::Unauthorized("Invalid or expired token".to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::user::{Claims, UserRole};
    use chrono::Utc;
    use jsonwebtoken::{encode, EncodingKey, Header};

    fn token_signed_with(secret: &str) -> String {
        let now = Utc::now().timestamp() as usize;
        let claims = Claims {
            sub: "11111111-1111-1111-1111-111111111111".to_string(),
            email: "a@b.test".to_string(),
            role: UserRole::SuperAdmin,
            hospital_id: None,
            exp: now + 3600,
            iat: now,
        };
        encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(secret.as_bytes()),
        )
        .unwrap()
    }

    #[test]
    fn fails_closed_when_secret_is_empty() {
        // The fail-open we are closing: a token forged with an empty key must
        // NOT be accepted when the server's secret is also empty.
        let forged = token_signed_with("");
        assert!(decode_token(&forged, "").is_err());
    }

    #[test]
    fn accepts_token_signed_with_matching_secret() {
        let secret = "a-sufficiently-long-test-secret-0123456789";
        let token = token_signed_with(secret);
        let claims = decode_token(&token, secret).expect("should decode");
        assert_eq!(claims.role, UserRole::SuperAdmin);
    }

    #[test]
    fn rejects_token_signed_with_a_different_secret() {
        let token = token_signed_with("secret-alpha-secret-alpha-secret-alpha");
        assert!(decode_token(&token, "secret-bravo-secret-bravo-secret-bravo").is_err());
    }

    #[test]
    fn rejects_expired_token() {
        let secret = "a-sufficiently-long-test-secret-0123456789";
        let now = Utc::now().timestamp() as usize;
        let claims = Claims {
            sub: "11111111-1111-1111-1111-111111111111".to_string(),
            email: "a@b.test".to_string(),
            role: UserRole::SuperAdmin,
            hospital_id: None,
            exp: now - 7200, // expired 2h ago, well beyond the default leeway
            iat: now - 10800,
        };
        let token = encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(secret.as_bytes()),
        )
        .unwrap();
        assert!(decode_token(&token, secret).is_err());
    }
}
