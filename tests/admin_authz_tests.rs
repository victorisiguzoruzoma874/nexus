//! Authorization-matrix tests for the SuperAdmin-only admin router.
//!
//! These prove the blanket `require_role(&[SuperAdmin])` guard on
//! `/api/v1/admin/*` WITHOUT a live database: the guard runs before any
//! handler, so 401/403 outcomes never touch Postgres. The pool is built
//! lazily (`connect_lazy`) only so the router can be constructed.

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use chrono::Utc;
use jsonwebtoken::{encode, EncodingKey, Header};
use sqlx::postgres::PgPoolOptions;
use tower::ServiceExt; // for `oneshot`

use nexuscare_backend::models::user::{Claims, UserRole};
use nexuscare_backend::repositories::EmailOutboxRepository;
use nexuscare_backend::routes::create_router;
use nexuscare_backend::services::{EmailOutboxService, NotificationService};

const TEST_SECRET: &str = "test-secret-for-admin-authz";
const TOKEN_TTL_SECS: usize = 3600;

fn ensure_jwt_secret() {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        std::env::set_var("JWT_SECRET", TEST_SECRET);
    });
}

fn mint_token(role: UserRole) -> String {
    let now = Utc::now().timestamp() as usize;
    let claims = Claims {
        sub: "11111111-1111-1111-1111-111111111111".to_string(),
        email: "admin@example.test".to_string(),
        role,
        hospital_id: None,
        exp: now + TOKEN_TTL_SECS,
        iat: now,
    };
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(TEST_SECRET.as_bytes()),
    )
    .expect("failed to mint test token")
}

fn build_app() -> axum::Router {
    // Lazy pool: never connects unless a handler issues a query. The guard
    // short-circuits 401/403 before any handler, so no DB is needed for them.
    let pool = PgPoolOptions::new()
        .connect_lazy("postgres://nexus:nexus@localhost:5432/nexus_test")
        .expect("failed to build lazy pool");
    let notification = Arc::new(NotificationService::new());
    let outbox_repo = Arc::new(EmailOutboxRepository::new(pool.clone()));
    let outbox = Arc::new(EmailOutboxService::new(outbox_repo, notification.clone()));
    let (router, _) = create_router(pool, notification, outbox);
    router
}

async fn admin_hospitals_status(auth: Option<String>) -> StatusCode {
    let app = build_app();
    let mut builder = Request::builder()
        .method("GET")
        .uri("/api/v1/admin/hospitals");
    if let Some(token) = auth {
        builder = builder.header("Authorization", format!("Bearer {token}"));
    }
    let req = builder.body(Body::empty()).unwrap();
    app.oneshot(req).await.unwrap().status()
}

#[tokio::test]
async fn no_token_is_unauthorized() {
    ensure_jwt_secret();
    assert_eq!(admin_hospitals_status(None).await, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn health_worker_is_forbidden() {
    ensure_jwt_secret();
    let token = mint_token(UserRole::HealthWorker);
    assert_eq!(
        admin_hospitals_status(Some(token)).await,
        StatusCode::FORBIDDEN
    );
}

#[tokio::test]
async fn hospital_admin_is_forbidden() {
    ensure_jwt_secret();
    let token = mint_token(UserRole::HospitalAdmin);
    assert_eq!(
        admin_hospitals_status(Some(token)).await,
        StatusCode::FORBIDDEN
    );
}

#[tokio::test]
async fn super_admin_passes_the_guard() {
    ensure_jwt_secret();
    let token = mint_token(UserRole::SuperAdmin);
    let status = admin_hospitals_status(Some(token)).await;
    // Guard passed -> request reached the handler. With the unreachable lazy
    // pool this is a 5xx; if a local test DB happens to exist it is 2xx. Either
    // way it must NOT be a client auth rejection (401/403).
    assert!(
        status.is_success() || status.is_server_error(),
        "expected SuperAdmin to pass the guard and reach the handler, got {status}"
    );
}
