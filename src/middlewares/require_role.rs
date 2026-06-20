// ! Role-based authorization middleware.

use axum::{
    extract::Request,
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;

use crate::models::user::UserRole;
use crate::utils::extract_claims;

/// Build an Axum middleware that admits only callers whose JWT `role` claim is
pub fn require_role(
    allowed: &'static [UserRole],
) -> impl Fn(Request, Next) -> std::pin::Pin<Box<dyn std::future::Future<Output = Response> + Send>>
       + Clone
       + Send
       + Sync
       + 'static {
    move |req: Request, next: Next| {
        Box::pin(async move {
            let claims = match extract_claims(req.headers()) {
                Ok(c) => c,
                Err(_) => return reject(StatusCode::UNAUTHORIZED, "Missing or invalid token"),
            };
            if !allowed.iter().any(|r| r == &claims.role) {
                return reject(StatusCode::FORBIDDEN, "Insufficient role for this endpoint");
            }
            next.run(req).await
        })
    }
}

fn reject(status: StatusCode, msg: &str) -> Response {
    (status, Json(json!({ "error": msg }))).into_response()
}
