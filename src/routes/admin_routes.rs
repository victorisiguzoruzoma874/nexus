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
