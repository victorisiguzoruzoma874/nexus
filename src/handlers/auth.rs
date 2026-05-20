use axum::{extract::State, http::StatusCode, Json};
use validator::Validate;

use crate::{
    models::user::{
        CreateUserRequest, ForgotPasswordRequest, LoginRequest, LoginResponse, LogoutRequest,
        EmailOtpVerifyRequest, EmailLoginRequest, RefreshTokenRequest, ResetPasswordRequest, UserResponse,
    },
    routes::AppState,
    services::auth_service::AuthError,
    utils::errors::{AppError, AppResult},
};

// ---------------------------------------------------------------------------
// Existing: register + email/password login (updated to use auth_service)
// ---------------------------------------------------------------------------

/// POST /api/v1/auth/register
#[utoipa::path(
    post,
    path = "/api/v1/auth/register",
    request_body = CreateUserRequest,
    responses(
        (status = 201, description = "User registered successfully", body = UserResponse),
        (status = 409, description = "Email already registered"),
        (status = 422, description = "Validation error")
    ),
    tag = "auth",
    summary = "Register a new user",
    description = "Create a new user account with email and password"
)]
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
#[utoipa::path(
    post,
    path = "/api/v1/auth/login",
    request_body = LoginRequest,
    responses(
        (status = 200, description = "Login successful", body = LoginResponse),
        (status = 401, description = "Invalid credentials"),
        (status = 403, description = "Account deactivated"),
        (status = 422, description = "Validation error")
    ),
    tag = "auth",
    summary = "Login with email and password",
    description = "Authenticate user with email and password, returns JWT tokens"
)]
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
// AC-01: Email OTP login
// ---------------------------------------------------------------------------

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
    payload.validate().map_err(|e| AppError::Validation(e.to_string()))?;

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
    payload.validate().map_err(|e| AppError::Validation(e.to_string()))?;

    state
        .auth_service
        .verify_login_otp(&payload.email, &payload.code)
        .await
        .map(Json)
        .map_err(auth_err_to_app)
}

// ---------------------------------------------------------------------------
// AC-03: Password reset
// ---------------------------------------------------------------------------

/// POST /api/v1/auth/forgot-password
#[utoipa::path(
    post,
    path = "/api/v1/auth/forgot-password",
    request_body = ForgotPasswordRequest,
    responses(
        (status = 204, description = "Password reset email sent"),
        (status = 404, description = "Email not found"),
        (status = 422, description = "Validation error")
    ),
    tag = "auth",
    summary = "Request password reset",
    description = "Send a password reset link to the user's email address"
)]
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
#[utoipa::path(
    post,
    path = "/api/v1/auth/reset-password",
    request_body = ResetPasswordRequest,
    responses(
        (status = 204, description = "Password reset successful"),
        (status = 401, description = "Invalid or expired token"),
        (status = 422, description = "Validation error")
    ),
    tag = "auth",
    summary = "Reset password with token",
    description = "Reset user password using the token from the reset email"
)]
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
        AuthError::EmailQueue(e) => AppError::Internal(anyhow::anyhow!("Email queue error: {}", e)),
        AuthError::Database(e) => AppError::Database(e),
        AuthError::Internal(msg) => AppError::Internal(anyhow::anyhow!(msg)),
    }
}
