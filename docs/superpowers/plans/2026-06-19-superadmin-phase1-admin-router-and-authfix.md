# SuperAdmin Phase 1 — Guarded Admin Router + Auth-Gap Fix — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Put the platform-admin endpoints behind a single `SuperAdmin`-only guard and record the acting admin's id on hospital approve/reject (closing a live authz + audit hole).

**Architecture:** Approach C from the spec — a dedicated nested router (`routes/admin_routes.rs`) registers all `/api/v1/admin/*` routes and applies ONE blanket `require_role(&[UserRole::SuperAdmin])` layer, then is merged into the main router before shared state is applied. The existing `approve_hospital`/`reject_hospital` handlers gain a `HeaderMap` extractor and pass `admin_id = Uuid::parse_str(claims.sub)` into the (already-`admin_id`-aware) `RegistrationService`.

**Tech Stack:** Rust, axum, sqlx/Postgres, jsonwebtoken, tower (`oneshot` for tests).

**Spec:** `docs/superpowers/specs/2026-06-19-superadmin-admin-section-design.md`
**Branch:** `feat/super-admin-section`

---

## Scope of THIS plan

In scope (Phase 1 core):
- A `SuperAdmin`-only nested router for `/api/v1/admin/*`.
- Move the four existing admin endpoints under it: `GET /admin/hospitals`, `GET /admin/clinicians`, `POST /admin/hospitals/{id}/approve`, `POST /admin/hospitals/{id}/reject`.
- Remove the old ungated route definitions.
- Fix the auth gap: approve/reject pass the real `admin_id` (from JWT) instead of `None`.
- DB-free authz-matrix integration tests proving the guard.

Deferred (NOT in this plan — see "Deferred" at the bottom):
- `suspend` / `reactivate` hospital (needs a new `Suspended` state — own plan, "Phase 1b").
- DB-backed behavioral tests (need a test-Postgres harness + SafeHaven mock — own task).
- Phases 2–4 (user management, oversight, disputes).

## Pre-flight facts (verified on the merged tree)

- `RegistrationStatus` (`src/models/registration.rs:14`) is the Postgres enum `registration_status` with ONLY `Pending`, `Approved`, `Rejected`. There is **no** `Suspended` — this is why suspend/reactivate is deferred.
- `RegistrationService::approve_hospital(&self, hospital_id: Uuid, admin_id: Option<Uuid>, notes: Option<String>)` and `reject_hospital(&self, hospital_id: Uuid, admin_id: Option<Uuid>, reason: String)` (`src/services/registration_service.rs:263,371`) ALREADY accept `admin_id` and ALREADY write the audit log. The bug is purely that the handlers pass `None`.
- Handlers `approve_hospital` / `reject_hospital` (`src/handlers/registration.rs`) currently take `(State, Path, Json)` and set `let admin_id = None;`.
- `Claims { sub: String, email: String, role: UserRole, hospital_id: Option<String>, exp: usize, iat: usize }` (`src/models/user.rs:114`).
- `extract_claims(&HeaderMap) -> Result<Claims, AppError>` re-exported at `crate::utils::extract_claims` (`src/utils/jwt.rs:9`); reads `JWT_SECRET` from env, validates with `Validation::default()` (HS256).
- `require_role(allowed: &'static [UserRole]) -> impl Fn(Request, Next) -> ...` (`src/middlewares/require_role.rs`); returns 401 (no/invalid token) or 403 (wrong role) BEFORE the handler runs.
- `create_router(pool: PgPool, notification_service: Arc<NotificationService>, email_outbox_service: Arc<EmailOutboxService>) -> (axum::Router, AppState)` (`src/routes/app_routes.rs:300`).
- `routes/mod.rs` currently: `pub mod app_routes; pub use app_routes::{create_router, AppState};`.
- `ErrorResponse { code: String, message: String }` (`src/handlers/registration.rs`).
- dev-dependencies today: `tokio-test`, `proptest` (no HTTP test tooling).

---

## Task 1: Guarded admin router (closes the authz hole)

**Files:**
- Modify: `Cargo.toml` (`[dev-dependencies]`)
- Create: `tests/admin_authz_tests.rs`
- Create: `src/routes/admin_routes.rs`
- Modify: `src/routes/mod.rs`
- Modify: `src/routes/app_routes.rs` (remove old admin route block ~lines 412-428; add merge + import)

- [ ] **Step 1: Add the `tower` dev-dependency**

In `Cargo.toml`, under `[dev-dependencies]`, add the `tower` util feature (provides `ServiceExt::oneshot`). Result:

```toml
[dev-dependencies]
tokio-test = "0.4.4"
proptest = "1.6.0"
tower = { version = "0.5", features = ["util"] }
```

(If `cargo tree -p tower` shows the workspace already resolves a different tower major, match that major to avoid a duplicate.)

- [ ] **Step 2: Write the failing authz-matrix test**

Create `tests/admin_authz_tests.rs`:

```rust
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

fn mint_token(role: UserRole) -> String {
    let now = Utc::now().timestamp() as usize;
    let claims = Claims {
        sub: "11111111-1111-1111-1111-111111111111".to_string(),
        email: "admin@example.test".to_string(),
        role,
        hospital_id: None,
        exp: now + 3600,
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
    let (router, _state) = create_router(pool, notification, outbox);
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
    std::env::set_var("JWT_SECRET", TEST_SECRET);
    assert_eq!(admin_hospitals_status(None).await, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn health_worker_is_forbidden() {
    std::env::set_var("JWT_SECRET", TEST_SECRET);
    let token = mint_token(UserRole::HealthWorker);
    assert_eq!(
        admin_hospitals_status(Some(token)).await,
        StatusCode::FORBIDDEN
    );
}

#[tokio::test]
async fn hospital_admin_is_forbidden() {
    std::env::set_var("JWT_SECRET", TEST_SECRET);
    let token = mint_token(UserRole::HospitalAdmin);
    assert_eq!(
        admin_hospitals_status(Some(token)).await,
        StatusCode::FORBIDDEN
    );
}

#[tokio::test]
async fn super_admin_passes_the_guard() {
    std::env::set_var("JWT_SECRET", TEST_SECRET);
    let token = mint_token(UserRole::SuperAdmin);
    let status = admin_hospitals_status(Some(token)).await;
    // Guard passed -> handler ran (and hit the unreachable DB -> 5xx, or
    // succeeded if a local test DB happens to exist). Either way it is NOT
    // blocked by authz.
    assert_ne!(status, StatusCode::UNAUTHORIZED);
    assert_ne!(status, StatusCode::FORBIDDEN);
}
```

- [ ] **Step 3: Run the test — verify it FAILS**

Run: `cargo test --test admin_authz_tests`
Expected: `hospital_admin_is_forbidden` and `no_token_is_unauthorized` FAIL — the admin routes are currently ungated, so a HospitalAdmin/no-token request reaches the handler instead of being rejected. (Compilation must succeed first; if `create_router`/exports don't resolve, fix imports before proceeding.)

- [ ] **Step 4: Create the guarded admin router**

Create `src/routes/admin_routes.rs`:

```rust
//! SuperAdmin-only admin surface.
//!
//! Every route registered here is gated by a SINGLE blanket
//! `require_role(&[SuperAdmin])` layer, so no individual admin route can be
//! left unguarded (Approach C in the design spec).

use axum::{
    middleware::from_fn,
    routing::{get, post},
    Router,
};

use crate::handlers::{admin, registration};
use crate::middlewares::require_role;
use crate::models::user::UserRole;
use crate::routes::AppState;

/// Build the `/api/v1/admin` router. The caller merges this into the main API
/// router BEFORE applying shared state with `.with_state(...)`.
pub fn admin_routes() -> Router<AppState> {
    Router::new()
        .route("/api/v1/admin/hospitals", get(admin::list_hospitals_admin))
        .route("/api/v1/admin/clinicians", get(admin::list_clinicians_admin))
        .route(
            "/api/v1/admin/hospitals/{hospital_id}/approve",
            post(registration::approve_hospital),
        )
        .route(
            "/api/v1/admin/hospitals/{hospital_id}/reject",
            post(registration::reject_hospital),
        )
        // ONE guard for the whole admin surface.
        .route_layer(from_fn(require_role(&[UserRole::SuperAdmin])))
}
```

- [ ] **Step 5: Register the module**

In `src/routes/mod.rs`, add the module declaration. Result:

```rust
pub mod app_routes;
pub mod admin_routes;

pub use app_routes::{create_router, AppState};
```

- [ ] **Step 6: Merge the admin router and delete the old ungated routes**

In `src/routes/app_routes.rs`, add this import near the other `use crate::...` imports at the top of the file:

```rust
use crate::routes::admin_routes::admin_routes;
```

Then replace the old admin route block (the `// Admin endpoints` comment through the clinicians route — currently around lines 412-428):

```rust
        // Admin endpoints
        .route(
            "/api/v1/admin/hospitals/{hospital_id}/approve",
            post(registration::approve_hospital),
        )
        .route(
            "/api/v1/admin/hospitals/{hospital_id}/reject",
            post(registration::reject_hospital),
        )
        .route(
            "/api/v1/admin/hospitals",
            get(admin::list_hospitals_admin),
        )
        .route(
            "/api/v1/admin/clinicians",
            get(admin::list_clinicians_admin),
        )
```

with a single merge:

```rust
        // Admin surface — SuperAdmin-only, one blanket guard (see admin_routes).
        .merge(admin_routes())
```

- [ ] **Step 7: Remove the now-unused `admin` import alias (if flagged)**

The `admin` alias in `app_routes.rs` (`use crate::handlers::{admin, auth, health, hospitals, registration, clinician_registration, shifts};`) may now be unused there (the `ApiDoc` `paths(...)` macro uses the full path `crate::handlers::admin::...`, not the alias). If the compiler/clippy flags it, drop `admin` from that `use` line:

```rust
use crate::handlers::{auth, health, hospitals, registration, clinician_registration, shifts};
```

(Leave `registration` — it is still used by the registration routes.)

- [ ] **Step 8: Run the test — verify it PASSES**

Run: `cargo test --test admin_authz_tests`
Expected: all four tests PASS (`no_token` → 401, `health_worker`/`hospital_admin` → 403, `super_admin` → not-401/not-403).

- [ ] **Step 9: Verify the whole crate still builds**

Run: `cargo check`
Expected: exit 0 (warnings ok; no errors).

- [ ] **Step 10: Commit**

```bash
git add Cargo.toml Cargo.lock tests/admin_authz_tests.rs src/routes/admin_routes.rs src/routes/mod.rs src/routes/app_routes.rs
git commit -m "feat(admin): gate /api/v1/admin/* behind a single SuperAdmin guard"
```

---

## Task 2: Auth-gap fix — record the acting admin on approve/reject

**Files:**
- Modify: `src/handlers/registration.rs` (add helper + `#[cfg(test)]` tests; wire approve/reject)

- [ ] **Step 1: Write the failing unit test for `admin_id_from_claims`**

In `src/handlers/registration.rs`, add at the end of the file:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::user::{Claims, UserRole};

    fn claims_with_sub(sub: &str) -> Claims {
        Claims {
            sub: sub.to_string(),
            email: "a@b.test".to_string(),
            role: UserRole::SuperAdmin,
            hospital_id: None,
            exp: 0,
            iat: 0,
        }
    }

    #[test]
    fn admin_id_parses_valid_uuid_subject() {
        let id = "11111111-1111-1111-1111-111111111111";
        let claims = claims_with_sub(id);
        assert_eq!(
            admin_id_from_claims(&claims),
            Some(Uuid::parse_str(id).unwrap())
        );
    }

    #[test]
    fn admin_id_is_none_for_non_uuid_subject() {
        let claims = claims_with_sub("not-a-uuid");
        assert_eq!(admin_id_from_claims(&claims), None);
    }
}
```

- [ ] **Step 2: Run the test — verify it FAILS to compile**

Run: `cargo test --lib registration::tests`
Expected: FAIL — `admin_id_from_claims` does not exist yet (compile error).

- [ ] **Step 3: Implement the helper and the new imports**

In `src/handlers/registration.rs`, add to the imports at the top:

```rust
use axum::http::HeaderMap;
use crate::models::user::Claims;
use crate::utils::extract_claims;
```

(`uuid::Uuid` is already imported.) Then add the helper (place it above `approve_hospital`):

```rust
/// Parse the acting admin's user id from JWT claims. Returns `None` when the
/// subject is not a valid UUID, which the audit log treats as "unknown actor".
pub(crate) fn admin_id_from_claims(claims: &Claims) -> Option<Uuid> {
    Uuid::parse_str(&claims.sub).ok()
}
```

- [ ] **Step 4: Run the unit test — verify it PASSES**

Run: `cargo test --lib registration::tests`
Expected: PASS (both cases).

- [ ] **Step 5: Wire `approve_hospital` to use the real admin id**

In `src/handlers/registration.rs`, change the `approve_hospital` signature to add a `HeaderMap` extractor (after `State`, before `Path`/`Json` — `Json` must stay last) and replace `let admin_id = None;`:

```rust
pub async fn approve_hospital(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(hospital_id): Path<Uuid>,
    Json(request): Json<ApprovalRequest>,
) -> Result<Json<StatusChangeResponse>, (StatusCode, Json<ErrorResponse>)> {
    let claims = extract_claims(&headers).map_err(|_| {
        (
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse {
                code: "UNAUTHORIZED".to_string(),
                message: "Missing or invalid token".to_string(),
            }),
        )
    })?;
    let admin_id = admin_id_from_claims(&claims);

    match state
        .registration_service
        .approve_hospital(hospital_id, admin_id, request.notes)
        .await
    {
        // ... existing match arms unchanged ...
```

Leave the existing `match` arms exactly as they are.

- [ ] **Step 6: Wire `reject_hospital` the same way**

In `src/handlers/registration.rs`, change `reject_hospital` identically — add `headers: HeaderMap`, extract claims, replace `let admin_id = None;`:

```rust
pub async fn reject_hospital(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(hospital_id): Path<Uuid>,
    Json(request): Json<RejectionRequest>,
) -> Result<Json<StatusChangeResponse>, (StatusCode, Json<ErrorResponse>)> {
    let claims = extract_claims(&headers).map_err(|_| {
        (
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse {
                code: "UNAUTHORIZED".to_string(),
                message: "Missing or invalid token".to_string(),
            }),
        )
    })?;
    let admin_id = admin_id_from_claims(&claims);

    match state
        .registration_service
        .reject_hospital(hospital_id, admin_id, request.reason)
        .await
    {
        // ... existing match arms unchanged ...
```

(Confirm the exact field passed to `reject_hospital` matches the current code — it takes the rejection `reason: String`. Keep whatever the current handler already passes; only the `admin_id` source changes.)

- [ ] **Step 7: Verify build + full test suite**

Run: `cargo test`
Expected: all tests pass, including `--test admin_authz_tests` and `registration::tests`.

- [ ] **Step 8: Commit**

```bash
git add src/handlers/registration.rs
git commit -m "fix(admin): record acting admin id on hospital approve/reject (was hardcoded None)"
```

---

## Task 3: Verify & polish

**Files:** none (verification only; commit fixups if any)

- [ ] **Step 1: Format**

Run: `cargo fmt`

- [ ] **Step 2: Lint (warnings as errors)**

Run: `cargo clippy --all-targets -- -D warnings`
Expected: no errors. If clippy flags the unused `admin` import (Task 1 Step 7) or anything else, fix it now.

- [ ] **Step 3: Full test run**

Run: `cargo test`
Expected: green.

- [ ] **Step 4: Commit any fixups**

```bash
git add -A
git commit -m "chore(admin): fmt + clippy clean for phase 1" || echo "nothing to commit"
```

---

## Manual verification (optional, needs a real DB)

With a local Postgres + migrations applied and a real `SuperAdmin` row (created later via the Phase 2 CLI), a SuperAdmin bearer token on `POST /api/v1/admin/hospitals/{id}/approve` should approve the hospital AND write an audit row whose `actor_id` equals the admin's user id (previously `NULL`). This is covered behaviorally once the test-DB harness exists.

## Deferred (explicit — not silently dropped)

- **Phase 1b — suspend/reactivate hospital:** needs a `Suspended` value added to the `registration_status` Postgres enum (or a separate `is_suspended` column), plus transition rules (`Approved ⇄ Suspended`) and an audit path. `ALTER TYPE ... ADD VALUE` cannot run inside a transaction on some Postgres versions, so the migration strategy needs its own design. Will be its own plan.
- **DB-backed behavioral tests:** a test-Postgres harness (spin up DB, run migrations, seed users, mock SafeHaven sub-account provisioning in `approve_hospital`) — its own infrastructure task. This plan's authz-matrix tests are DB-free by design.
- **Phases 2–4:** user management + super-admin CLI bootstrap, platform oversight (read), disputes — separate plans per the spec.
