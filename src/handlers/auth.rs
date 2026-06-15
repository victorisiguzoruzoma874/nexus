use axum::{extract::State, http::StatusCode, Json};
use validator::Validate;

use crate::{
    models::user::{
        EmailLoginRequest, EmailOtpVerifyRequest, LoginResponse, LogoutRequest,
        RefreshTokenRequest,
    },
    routes::AppState,
    services::auth_service::AuthError,
    utils::errors::{AppError, AppResult},
};

// Email OTP login — the only authentication path.

/// POST /api/v1/auth/otp/send
#[utoipa::path(
    post,
    path = "/api/v1/auth/otp/send",
    request_body = EmailLoginRequest,
    responses(
        (status = 204, description = "OTP sent successfully"),
        (status = 404, description = "Email not found"),
        (status = 422, description = "Validation error")
    ),
    tag = "auth",
    summary = "Send OTP to email",
    description = "Send a 6-digit OTP code to the user's email address for authentication"
)]
pub async fn email_otp_send(
    State(state): State<AppState>,
    Json(payload): Json<EmailLoginRequest>,
) -> AppResult<StatusCode> {
    payload.validate(). map_err(|e| AppError::Validation(e.to_string()))?;

    state
        .auth_service
        .send_login_otp(&payload.email)
        .await
        .map(|_| StatusCode::NO_CONTENT)
        .map_err(auth_err_to_app)
}

/// POST /api/v1/auth/otp/verify
#[utoipa::path(
    post,
    path = "/api/v1/auth/otp/verify",
    request_body = EmailOtpVerifyRequest,
    responses(
        (status = 200, description = "OTP verified, login successful", body = LoginResponse),
        (status = 401, description = "Invalid or expired OTP"),
        (status = 422, description = "Validation error")
    ),
    tag = "auth",
    summary = "Verify OTP and login",
    description = "Verify the OTP code and complete email-based authentication"
)]
pub async fn email_otp_verify(
    State(state): State<AppState>,
    Json(payload): Json<EmailOtpVerifyRequest>,
) -> AppResult<Json<LoginResponse>> {
    payload.validate(). map_err(|e| AppError::Validation(e.to_string()))?;

    state
        .auth_service
        .verify_login_otp(&payload.email, &payload.code)
        .await
        .map(Json)
        .map_err(auth_err_to_app)
}

// Token refresh

/// POST /api/v1/auth/refresh
#[utoipa::path(
    post,
    path = "/api/v1/auth/refresh",
    request_body = RefreshTokenRequest,
    responses(
        (status = 200, description = "Token refreshed successfully", body = LoginResponse),
        (status = 401, description = "Invalid or expired refresh token"),
        (status = 422, description = "Validation error")
    ),
    tag = "auth",
    summary = "Refresh access token",
    description = "Get a new access token using a valid refresh token"
)]
pub async fn refresh_token(
    State(state): State<AppState>,
    Json(payload): Json<RefreshTokenRequest>,
) -> AppResult<Json<LoginResponse>> {
    payload.validate(). map_err(|e| AppError::Validation(e.to_string()))?;

    state
        .auth_service
        .refresh_token(&payload.refresh_token)
        .await
        .map(Json)
        .map_err(auth_err_to_app)
}

// Logout

/// POST /api/v1/auth/logout
#[utoipa::path(
    post,
    path = "/api/v1/auth/logout",
    request_body = LogoutRequest,
    responses(
        (status = 204, description = "Logout successful"),
        (status = 401, description = "Invalid refresh token"),
        (status = 422, description = "Validation error")
    ),
    tag = "auth",
    summary = "Logout user",
    description = "Revoke the refresh token and logout the user"
)]
pub async fn logout(
    State(state): State<AppState>,
    Json(payload): Json<LogoutRequest>,
) -> AppResult<StatusCode> {
    payload.validate(). map_err(|e| AppError::Validation(e.to_string()))?;

    state
        .auth_service
        .logout(&payload.refresh_token)
        .await
        .map(|_| StatusCode::NO_CONTENT)
        .map_err(auth_err_to_app)
}

// Error mapping

fn auth_err_to_app(e: AuthError) -> AppError {
    match e {
        AuthError::NotFound => AppError::NotFound("User not found".to_string()),
        AuthError::InvalidOtp => AppError::Unauthorized("Invalid or expired OTP".to_string()),
        AuthError::InvalidToken => AppError::Unauthorized("Invalid or expired token".to_string()),
        AuthError::InvalidCredentials => {
            AppError::Unauthorized("Invalid email or password".to_string())
        }
        AuthError::Deactivated => AppError::Forbidden("Account is deactivated".to_string()),
        AuthError::EmailQueue(e) => AppError::Internal(anyhow::anyhow!("Email queue error: {}", e)),
        AuthError::Database(e) => AppError::Database(e),
        AuthError::Internal(msg) => AppError::Internal(anyhow::anyhow!(msg)),
    }
}
