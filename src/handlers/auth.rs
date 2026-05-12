use axum::{extract::State, http::StatusCode, Json};
use validator::Validate;

use crate::{
    models::user::{
        CreateUserRequest, ForgotPasswordRequest, LoginRequest, LoginResponse, LogoutRequest,
        OtpVerifyRequest, PhoneLoginRequest, RefreshTokenRequest, ResetPasswordRequest, UserResponse,
    },
    routes::AppState,
    services::auth_service::AuthError,
    utils::errors::{AppError, AppResult},
};

// ---------------------------------------------------------------------------
// Existing: register + email/password login (updated to use auth_service)
// ---------------------------------------------------------------------------

/// POST /api/v1/auth/register
pub async fn register(
    State(state): State<AppState>,
    Json(payload): Json<CreateUserRequest>,
) -> AppResult<(StatusCode, Json<UserResponse>)> {
    payload.validate().map_err(|e| AppError::Validation(e.to_string()))?;

    let existing: Option<(uuid::Uuid,)> =
        sqlx::query_as("SELECT id FROM users WHERE email = $1")
            .bind(&payload.email)
            .fetch_optional(&state.pool)
            .await?;

    if existing.is_some() {
        return Err(AppError::Conflict("Email is already registered".to_string()));
    }

    let password_hash = crate::services::auth_service::hash_password(&payload.password)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Password hashing failed: {}", e)))?;

    let user: crate::models::user::User = sqlx::query_as(
        r#"
        INSERT INTO users (id, hospital_id, first_name, last_name, email, phone, password_hash, role)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        RETURNING id, hospital_id, first_name, last_name, email, phone, password_hash,
                  role, role_label, avatar_url, is_active, last_login_at, created_at, updated_at
        "#,
    )
    .bind(uuid::Uuid::new_v4())
    .bind(payload.hospital_id)
    .bind(&payload.first_name)
    .bind(&payload.last_name)
    .bind(&payload.email)
    .bind(&payload.phone)
    .bind(&password_hash)
    .bind(&payload.role)
    .fetch_one(&state.pool)
    .await?;

    Ok((StatusCode::CREATED, Json(UserResponse::from(user))))
}

/// POST /api/v1/auth/login  (AC-02: email + password)
pub async fn login(
    State(state): State<AppState>,
    Json(payload): Json<LoginRequest>,
) -> AppResult<Json<LoginResponse>> {
    payload.validate().map_err(|e| AppError::Validation(e.to_string()))?;

    state
        .auth_service
        .login_with_password(&payload.email, &payload.password)
        .await
        .map(Json)
        .map_err(auth_err_to_app)
}

// ---------------------------------------------------------------------------
// AC-01: Phone OTP login
// ---------------------------------------------------------------------------

/// POST /api/v1/auth/otp/send
pub async fn phone_otp_send(
    State(state): State<AppState>,
    Json(payload): Json<PhoneLoginRequest>,
) -> AppResult<StatusCode> {
    payload.validate().map_err(|e| AppError::Validation(e.to_string()))?;

    state
        .auth_service
        .send_login_otp(&payload.phone)
        .await
        .map(|_| StatusCode::NO_CONTENT)
        .map_err(auth_err_to_app)
}

/// POST /api/v1/auth/otp/verify
pub async fn phone_otp_verify(
    State(state): State<AppState>,
    Json(payload): Json<OtpVerifyRequest>,
) -> AppResult<Json<LoginResponse>> {
    payload.validate().map_err(|e| AppError::Validation(e.to_string()))?;

    state
        .auth_service
        .verify_login_otp(&payload.phone, &payload.code)
        .await
        .map(Json)
        .map_err(auth_err_to_app)
}

// ---------------------------------------------------------------------------
// AC-03: Password reset
// ---------------------------------------------------------------------------

/// POST /api/v1/auth/forgot-password
pub async fn forgot_password(
    State(state): State<AppState>,
    Json(payload): Json<ForgotPasswordRequest>,
) -> AppResult<StatusCode> {
    payload.validate().map_err(|e| AppError::Validation(e.to_string()))?;

    state
        .auth_service
        .forgot_password(&payload.email)
        .await
        .map(|_| StatusCode::NO_CONTENT)
        .map_err(auth_err_to_app)
}

/// POST /api/v1/auth/reset-password
pub async fn reset_password(
    State(state): State<AppState>,
    Json(payload): Json<ResetPasswordRequest>,
) -> AppResult<StatusCode> {
    payload.validate().map_err(|e| AppError::Validation(e.to_string()))?;

    state
        .auth_service
        .reset_password(&payload.token, &payload.new_password)
        .await
        .map(|_| StatusCode::NO_CONTENT)
        .map_err(auth_err_to_app)
}

// ---------------------------------------------------------------------------
// AC-04: Token refresh
// ---------------------------------------------------------------------------

/// POST /api/v1/auth/refresh
pub async fn refresh_token(
    State(state): State<AppState>,
    Json(payload): Json<RefreshTokenRequest>,
) -> AppResult<Json<LoginResponse>> {
    payload.validate().map_err(|e| AppError::Validation(e.to_string()))?;

    state
        .auth_service
        .refresh_token(&payload.refresh_token)
        .await
        .map(Json)
        .map_err(auth_err_to_app)
}

// ---------------------------------------------------------------------------
// AC-05: Logout
// ---------------------------------------------------------------------------

/// POST /api/v1/auth/logout
pub async fn logout(
    State(state): State<AppState>,
    Json(payload): Json<LogoutRequest>,
) -> AppResult<StatusCode> {
    payload.validate().map_err(|e| AppError::Validation(e.to_string()))?;

    state
        .auth_service
        .logout(&payload.refresh_token)
        .await
        .map(|_| StatusCode::NO_CONTENT)
        .map_err(auth_err_to_app)
}

// ---------------------------------------------------------------------------
// Error mapping
// ---------------------------------------------------------------------------

fn auth_err_to_app(e: AuthError) -> AppError {
    match e {
        AuthError::NotFound => AppError::NotFound("User not found".to_string()),
        AuthError::InvalidOtp => AppError::Unauthorized("Invalid or expired OTP".to_string()),
        AuthError::InvalidToken => AppError::Unauthorized("Invalid or expired token".to_string()),
        AuthError::InvalidCredentials => {
            AppError::Unauthorized("Invalid email or password".to_string())
        }
        AuthError::Deactivated => AppError::Forbidden("Account is deactivated".to_string()),
        AuthError::Sms(e) => AppError::Internal(anyhow::anyhow!("SMS error: {}", e)),
        AuthError::Database(e) => AppError::Database(e),
        AuthError::Internal(msg) => AppError::Internal(anyhow::anyhow!(msg)),
    }
}
