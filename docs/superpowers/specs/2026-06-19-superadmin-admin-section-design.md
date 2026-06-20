# SuperAdmin Admin Section — Design Spec

- **Date:** 2026-06-19
- **Status:** Approved (brainstorming) → pending implementation plan
- **Author:** Victor
- **Branch:** `feat/super-admin-section`
- **Component:** NexusCare backend (Rust / axum / sqlx / Postgres)

---

## 1. Context & Problem

NexusCare has three roles in `UserRole` (`models/user.rs`): `HospitalAdmin`,
`HealthWorker`, `SuperAdmin`. Today `SuperAdmin` is a **placeholder**:

- It is **never a sole authority** — every gated route lists it bundled with
  `HospitalAdmin` (`[HospitalAdmin, SuperAdmin]`), so it has no exclusive power.
- The platform-admin endpoints that *should* belong to it
  (`/api/v1/admin/hospitals/*`, approve/reject hospital) are **ungated** and
  hardcode `admin_id = None` — anyone reachable can approve/reject a hospital,
  and there is no record of who did it.
- **No account is ever created with the role** — signup only assigns
  `HospitalAdmin` / `HealthWorker`, and there is no seed. A `SuperAdmin` also has
  `hospital_id = NULL`, which currently *breaks* hospital-scoped calls such as
  `create_shift` (it does `claims.hospital_id.ok_or(...)?`).

We are building a real **platform-level admin** ("app admin") that has
exclusive, cross-hospital control distinct from the hospital-scoped
`HospitalAdmin`.

## 2. Goals / Non-Goals

**Goals**
- Give `SuperAdmin` exclusive, cross-hospital powers behind a single authz
  choke-point.
- Close the existing authz/audit gap on the hospital-admin endpoints.
- Provide a way to create the first super admin.

**Non-Goals**
- No second platform tier (no separate "ops admin" vs "owner"). Single
  `SuperAdmin` tier only.
- No frontend/dashboard work (backend API only).
- No change to `HospitalAdmin` / `HealthWorker` behavior beyond moving the
  mis-placed platform-admin endpoints under the new guard.

## 3. Locked Decisions

| Decision | Choice |
| --- | --- |
| Role | Reuse existing `UserRole::SuperAdmin` (do **not** add a new role) |
| Tiering | Single tier |
| Scope | `SuperAdmin` is **unscoped** (all hospitals); `HospitalAdmin` stays scoped to its `hospital_id` |
| Authz | **Approach C** — one nested router with a single blanket `require_role(&[SuperAdmin])` layer |
| Bootstrap | **CLI subcommand** (`create-superadmin`), no new web attack surface |
| v1 scope | All four capability areas, delivered in four phases |

## 4. Architecture (Approach C — guarded nested router)

A dedicated nested router carries the entire admin surface with **one** authz
layer applied to the whole thing, so no individual route can be left unguarded:

```
/api/v1/admin/*  ─►  admin_router = Router::new()
                       .route("/hospitals", get(admin::hospitals::list))
                       .route("/hospitals/{id}/approve", post(...))
                       ... (all admin routes) ...
                       .route_layer(from_fn(require_role(&[UserRole::SuperAdmin])))
                       .with_state(state)
```

`admin_router` is merged into the existing `api_router` inside
`routes::create_router(...)` (`routes/app_routes.rs`). All admin routes are
`SuperAdmin`-only **by construction**.

### Module layout (small, cohesive files — per coding-style)

```
src/routes/admin_routes.rs                 # builds the guarded nested router
src/handlers/admin/mod.rs
src/handlers/admin/hospitals.rs            # Phase 1
src/handlers/admin/users.rs                # Phase 2
src/handlers/admin/oversight.rs            # Phase 3
src/handlers/admin/disputes.rs             # Phase 4
src/services/admin_service.rs              # admin-only business logic
src/repositories/admin.rs                  # cross-hospital (unscoped) queries
src/models/dispute.rs                      # Phase 4
src/cli/mod.rs + src/cli/bootstrap.rs      # create-superadmin (invoked from main.rs, shares config/pool)
migrations/20240037_create_disputes.sql    # Phase 4
```

Existing `handlers/admin.rs` (currently `list_hospitals_admin`,
`list_clinicians_admin`) is folded into the new `handlers/admin/` module and put
behind the guard.

## 5. The scope mechanism

A small helper makes "unscoped vs own-hospital" explicit and DRY rather than
branching on the role in every handler:

```rust
pub enum HospitalScope {
    All,                 // SuperAdmin
    Single(Uuid),        // HospitalAdmin — their hospital
}

pub fn resolve_scope(claims: &Claims) -> Result<HospitalScope, AppError> {
    match claims.role {
        UserRole::SuperAdmin => Ok(HospitalScope::All),
        _ => claims.hospital_id
            .as_deref()
            .and_then(|s| Uuid::parse_str(s).ok())
            .map(HospitalScope::Single)
            .ok_or_else(|| AppError::Forbidden("no hospital scope".into())),
    }
}
```

Cross-hospital repo methods accept `HospitalScope` and omit the
`WHERE hospital_id = $1` filter when `All`. (Admin handlers always resolve to
`All` since the router guarantees `SuperAdmin`, but the helper keeps the scoping
rule in one place and is reusable by non-admin handlers later.)

## 6. Bootstrap — `create-superadmin` CLI subcommand

`main.rs` inspects `std::env::args()` **before** booting the server. If invoked
as a subcommand, it runs the bootstrap path and exits; otherwise it serves
normally. No new dependency — minimal std-arg parsing.

```
$ cargo run -- create-superadmin --email admin@nexus.io --password '<pw>' \
    --first-name Ada --last-name Lovelace
```

Behavior:
- Loads config + DB pool, runs migrations (same as server boot).
- Validates email/password; hashes password with the existing bcrypt path.
- Inserts a `users` row: `role = super_admin`, `hospital_id = NULL`,
  `is_active = true`.
- **Idempotent on email:** if the email already exists, exits non-zero with a
  clear message (no duplicate).
- Prints the new user id on success.

## 7. Endpoints by phase

All paths below are under the guarded `/api/v1/admin` router unless noted.
Authz for every admin route = `SuperAdmin` only (blanket layer).

### Phase 1 — Hospital lifecycle + auth-gap fix
| Method | Path | Behavior |
| --- | --- | --- |
| GET | `/hospitals` | List all hospitals (filter by status) |
| POST | `/hospitals/{id}/approve` | Approve; `admin_id = claims.sub`; writes audit |
| POST | `/hospitals/{id}/reject` | Reject; reason; `admin_id = claims.sub`; audit |
| POST | `/hospitals/{id}/suspend` | Suspend an approved hospital; reason; audit |
| POST | `/hospitals/{id}/reactivate` | Reverse a suspension; audit |

The existing `approve_hospital` / `reject_hospital` handlers are moved under the
guard and changed to take `admin_id` from `claims.sub` (kills the hardcoded
`None`). Reuses `RegistrationService` + `AuditService::log_status_change`.

### Phase 2 — User management + bootstrap
| Method | Path | Behavior |
| --- | --- | --- |
| GET | `/users` | List/search users; filters: `search`, `role`, `status`, `hospital_id`; paginated |
| GET | `/users/{id}` | User detail |
| POST | `/users/{id}/suspend` | Set `is_active = false`; reason; audit |
| POST | `/users/{id}/reactivate` | Set `is_active = true`; audit |
| PATCH | `/users/{id}/role` | Change a user's role; audit |

`is_active = false` already blocks login (`AuthError::Deactivated`), so suspend
is enforced for free. Bootstrap (CLI) ships in this phase.

### Phase 3 — Platform oversight (read-only)
| Method | Path | Behavior |
| --- | --- | --- |
| GET | `/stats` | Aggregate counts: hospitals by status, users by role, shifts by status, wallet/payout totals, pending identity verifications |
| GET | `/shifts` | All shifts across hospitals (filter/paginate) |
| GET | `/wallets` | All hospital wallets |
| GET | `/payouts` | All payouts |
| GET | `/identity-verifications` | All identity verifications (filter by status) |

Read-only. Adds unscoped query methods to `repositories/admin.rs`, reusing the
wallet/payout/identity/shift tables already present post-merge.

### Phase 4 — Dispute resolution (new subsystem)
| Method | Path | Behavior |
| --- | --- | --- |
| POST | `/api/v1/disputes` | **Any authenticated user** raises a dispute (NOT under the admin guard) |
| GET | `/disputes` | List/filter disputes (admin) |
| GET | `/disputes/{id}` | Dispute detail (admin) |
| POST | `/disputes/{id}/resolve` | Resolve with notes; `resolved_by = claims.sub`; audit |
| POST | `/disputes/{id}/reject` | Reject with notes; audit |

Largest, genuinely new phase. Will get its own mini-spec at build time.

## 8. Data model changes

**New table (Phase 4)** — `migrations/20240037_create_disputes.sql`:

```sql
CREATE TYPE dispute_subject AS ENUM ('shift', 'payment');
CREATE TYPE dispute_status  AS ENUM ('open', 'under_review', 'resolved', 'rejected');

CREATE TABLE disputes (
    id               UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    raised_by        UUID NOT NULL REFERENCES users(id),
    subject_type     dispute_subject NOT NULL,
    subject_id       UUID NOT NULL,                 -- shift_id or payout/payment id
    reason           TEXT NOT NULL,
    status           dispute_status NOT NULL DEFAULT 'open',
    resolution_notes TEXT,
    resolved_by      UUID REFERENCES users(id),
    created_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at       TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_disputes_status ON disputes(status);
CREATE INDEX idx_disputes_subject ON disputes(subject_type, subject_id);
```

**Phases 1–3 add no new tables.** Suspension uses the existing `is_active`
column; hospital suspend/reactivate uses the existing registration-status
machinery. If a hospital "suspended" state is not representable in the current
status enum, a follow-up migration adds it (confirmed during planning).

## 9. Audit & safety

- **Audit every mutating admin action** via the existing `AuditService`
  (`actor_id = claims.sub`, `actor_type = Admin`). `log_status_change` already
  takes `admin_id: Option<Uuid>`; user-management and dispute actions may need a
  new `AuditEventType` variant (and possibly a generic `log_admin_action`).
- **Lock-out guards (must-have):**
  - A super admin cannot suspend or demote **themselves**.
  - The system refuses to suspend/demote the **last active super admin**.

## 10. Error handling

- Reuse `AppError` / `AppResult` and the existing `ErrorResponse` envelope.
- Generic client-facing messages; detailed server-side `tracing` (no leakage of
  internal/DB errors), per project security rules.
- Standard mappings: missing token → 401, wrong role → 403 (handled by the
  blanket layer), not found → 404, invalid transition → 409, validation → 400.

## 11. Testing strategy (TDD, 80%+ target)

**Unit**
- `resolve_scope` (SuperAdmin → All; HospitalAdmin → Single; missing scope →
  error).
- Lock-out guards (self-suspend rejected; last-super-admin rejected).
- Bootstrap insert (creates row; idempotent/duplicate rejected).
- Admin services with mocked repositories.

**Integration (`tests/`)**
- **Authz matrix per route:** no token → 401; `HospitalAdmin` → 403;
  `HealthWorker` → 403; `SuperAdmin` → 200.
- Cross-hospital visibility: `SuperAdmin` sees data from multiple hospitals.
- Suspend → subsequent login blocked (`Deactivated`).
- Approve hospital → audit row written with the acting admin's id.
- Self-suspend / last-super-admin → rejected.

## 12. Integration points (exact, on merged tree)

- Router assembly: `routes::create_router(pool, notification_service, email_outbox_service) -> (Router, AppState)` in `routes/app_routes.rs`; nest `admin_router` here.
- `AppState` (in `routes/app_routes.rs`) gains `admin_service: Arc<AdminService>`; admin repo built from `pool`.
- Auth identity: `extract_claims(&HeaderMap) -> Result<Claims, AppError>` (`utils/jwt.rs`); `Claims { sub: String, email, role: UserRole, hospital_id: Option<String> }` (`models/user.rs`). `admin_id = Uuid::parse_str(&claims.sub)`.
- Role gate: `middlewares::require_role(&[UserRole::SuperAdmin])`.
- Audit: `services::audit_service::AuditService`.
- Bootstrap: invoked from `main.rs` before `axum::serve`.

## 13. Precondition

Local must be synced to `origin/main` before implementation. **DONE
(2026-06-19):** merged `origin/main` into local; tree compiles (`cargo check`
green); snapshot at branch `backup/pre-sync-main`. This spec targets the merged
tree (wallets/payouts/identity present).

## 14. Delivery order

1. Phase 1 — Hospital lifecycle + auth-gap fix (highest value; fixes live hole)
2. Phase 2 — User management + super-admin bootstrap
3. Phase 3 — Platform oversight (read-only)
4. Phase 4 — Dispute resolution (own mini-spec)

Each phase: TDD → code review → gated commit, per project workflow.

## 15. Open questions (resolve during planning)

- Does the current hospital registration-status enum already express a
  "suspended" state, or is a new migration needed?
- Exact pagination convention to match existing `admin::PaginationMetadata`.
- Whether dispute `subject_id` needs a soft FK/validation per `subject_type`.
