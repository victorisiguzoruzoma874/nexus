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
const SAMPLE_ID: &str = "11111111-1111-1111-1111-111111111111";

/// Every route on the SuperAdmin-only admin surface. Keeping this list in sync
/// with `admin_routes()` is what protects the single-choke-point invariant:
/// each one must reject a missing token (401) and any non-SuperAdmin role (403).
fn admin_routes_table() -> Vec<(&'static str, String)> {
    vec![
        ("GET", "/api/v1/admin/hospitals".to_string()),
        ("GET", "/api/v1/admin/clinicians".to_string()),
        (
            "POST",
            format!("/api/v1/admin/hospitals/{SAMPLE_ID}/approve"),
        ),
        (
            "POST",
            format!("/api/v1/admin/hospitals/{SAMPLE_ID}/reject"),
        ),
        ("POST", format!("/api/v1/admin/payouts/{SAMPLE_ID}/retry")),
    ]
}

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
        sub: SAMPLE_ID.to_string(),
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

async fn route_status(method: &str, path: &str, auth: Option<String>) -> StatusCode {
    let app = build_app();
    let mut builder = Request::builder().method(method).uri(path);
    if let Some(token) = auth {
        builder = builder.header("Authorization", format!("Bearer {token}"));
    }
    let req = builder.body(Body::empty()).unwrap();
    app.oneshot(req).await.unwrap().status()
}

#[tokio::test]
async fn every_admin_route_requires_a_token() {
    ensure_jwt_secret();
    for (method, path) in admin_routes_table() {
        assert_eq!(
            route_status(method, &path, None).await,
            StatusCode::UNAUTHORIZED,
            "{method} {path} must reject a missing token with 401"
        );
    }
}

#[tokio::test]
async fn every_admin_route_forbids_non_super_admin_roles() {
    ensure_jwt_secret();
    for role in [UserRole::HospitalAdmin, UserRole::HealthWorker] {
        for (method, path) in admin_routes_table() {
            let token = mint_token(role.clone());
            assert_eq!(
                route_status(method, &path, Some(token)).await,
                StatusCode::FORBIDDEN,
                "{method} {path} must reject {role:?} with 403"
            );
        }
    }
}

#[tokio::test]
async fn super_admin_passes_the_guard() {
    ensure_jwt_secret();
    let token = mint_token(UserRole::SuperAdmin);
    // Use a read route so a passing guard reaches the handler (then the
    // unreachable lazy pool -> 5xx, or 2xx if a local test DB exists). Either
    // way it must NOT be a client auth rejection (401/403).
    let status = route_status("GET", "/api/v1/admin/hospitals", Some(token)).await;
    assert!(
        status.is_success() || status.is_server_error(),
        "expected SuperAdmin to pass the guard and reach the handler, got {status}"
    );
}
