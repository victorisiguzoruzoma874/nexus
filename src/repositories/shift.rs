use sqlx::{FromRow, PgPool, Postgres, Transaction};
use uuid::Uuid;
use chrono::{Duration, Utc};

use crate::models::shift::{
    Shift, ShiftStatus, CreateShiftRequest, PayType, ShiftApplication,
    ShiftApplicationStatus, ShiftPriority, ShiftType,
};
use crate::models::registration::RegistrationStatus;

/// Raw row backing the Tier 2.3 ranking query. Internal to the repo —
/// the service layer turns these into `RankedInterestedClinician`.
#[derive(Debug, Clone, FromRow)]
pub struct InterestedClinicianRow {
    pub clinician_id: Uuid,
    pub first_name: String,
    pub last_name: String,
    pub rating: f32,
    pub rating_count: i32,
    pub completed_shifts: i64,
    pub accepts: i64,
    pub declines: i64,
    pub expires: i64,
    pub clinician_lat: Option<f64>,
    pub clinician_lng: Option<f64>,
}

/// Raw row backing the Tier 2.1 "Shifts Near You" query. Distance and final
/// sort happen in the service layer.
#[derive(Debug, Clone, FromRow)]
pub struct NearbyShiftRow {
    pub shift_id: Uuid,
    pub hospital_id: Uuid,
    pub hospital_name: Option<String>,
    pub role_title: String,
    pub specialty: Option<String>,
    pub shift_type: ShiftType,
    pub priority: ShiftPriority,
    pub scheduled_start: chrono::DateTime<chrono::Utc>,
    pub duration_hours: f32,
    pub pay_type: PayType,
    pub rate_kobo_per_hour: Option<i64>,
    pub fixed_rate_kobo: Option<i64>,
    pub stat_bonus_kobo: Option<i64>,
    pub hospital_lat: Option<f64>,
    pub hospital_lng: Option<f64>,
    pub clinician_lat: Option<f64>,
    pub clinician_lng: Option<f64>,
    pub interest_expressed: bool,
}

pub struct ShiftRepository {
    pool: PgPool,
}

impl ShiftRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Check if a hospital is approved to create shifts
    pub async fn check_hospital_approved(&self, hospital_id: Uuid) -> Result<bool, sqlx::Error> {
        let result = sqlx::query_scalar::<_, Option<RegistrationStatus>>(
            r#"
            SELECT admin_registration_status
            FROM hospitals
            WHERE id = $1
            "#,
        )
        .bind(hospital_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(matches!(result, Some(Some(RegistrationStatus::Approved))))
    }

    /// Get hospital name by ID
    pub async fn get_hospital_name(&self, hospital_id: Uuid) -> Result<Option<String>, sqlx::Error> {
        sqlx::query_scalar::<_, String>(
            r#"
            SELECT name
            FROM hospitals
            WHERE id = $1
            "#,
        )
        .bind(hospital_id)
        .fetch_optional(&self.pool)
        .await
    }

    pub async fn get_hospital_contact(
        &self,
        hospital_id: Uuid,
    ) -> Result<Option<(String, String)>, sqlx::Error> {
        sqlx::query_as::<_, (String, String)>(
            r#"
            SELECT name, email
            FROM hospitals
            WHERE id = $1
            "#,
        )
        .bind(hospital_id)
        .fetch_optional(&self.pool)
        .await
    }

    pub async fn get_clinician_contact(
        &self,
        clinician_id: Uuid,
    ) -> Result<Option<(String, String, String)>, sqlx::Error> {
        sqlx::query_as::<_, (String, String, String)>(
            r#"
            SELECT c.first_name, c.last_name, u.email
            FROM clinicians c
            JOIN users u ON c.user_id = u.id
            WHERE c.id = $1
            "#,
        )
        .bind(clinician_id)
        .fetch_optional(&self.pool)
        .await
    }

    /// Resolve `clinicians.id` from the authenticated user's `users.id`.
    /// Returns `None` if the caller has no clinician profile.
    pub async fn find_clinician_id_for_user(
        &self,
        user_id: Uuid,
    ) -> Result<Option<Uuid>, sqlx::Error> {
        sqlx::query_scalar::<_, Uuid>(
            r#"
            SELECT id FROM clinicians WHERE user_id = $1
            "#,
        )
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await
    }

    /// Get the hospital's first registered location coordinates, for distance
    /// calculations in the ranking algorithm (§3.4.3) and clock-in geofencing (§3.6).
    pub async fn get_hospital_coordinates(
        &self,
        hospital_id: Uuid,
    ) -> Result<Option<(f64, f64)>, sqlx::Error> {
        sqlx::query_as::<_, (f64, f64)>(
            r#"
            SELECT latitude, longitude
            FROM hospital_locations
            WHERE hospital_id = $1
            ORDER BY created_at ASC
            LIMIT 1
            "#,
        )
        .bind(hospital_id)
        .fetch_optional(&self.pool)
        .await
    }

    /// Tier 2.3 — fetch interested clinicians for a shift along with the data
    /// needed to score them (rating, completed_shifts, acceptance_rate,
    /// location). The caller computes haversine distance and the weighted
    /// score in Rust. Returns rows in `expressed_at ASC` order — the caller
    /// sorts by score.
    ///
    /// Tuple layout:
    /// `(clinician_id, first_name, last_name, rating, rating_count,
    ///   completed_shifts, accepts, declines, expires, clinician_lat,
    ///   clinician_lng)`
    pub async fn list_interested_with_stats(
        &self,
        shift_id: Uuid,
    ) -> Result<Vec<InterestedClinicianRow>, sqlx::Error> {
        sqlx::query_as::<_, InterestedClinicianRow>(
            r#"
            SELECT
                c.id                                                  AS clinician_id,
                c.first_name,
                c.last_name,
                c.rating,
                c.rating_count,
                (SELECT COUNT(*) FROM shifts s2
                    WHERE s2.assigned_clinician_id = c.id AND s2.status = 'completed') AS completed_shifts,
                (SELECT COUNT(*) FROM shift_assignments a
                    WHERE a.clinician_id = c.id AND a.status = 'accepted') AS accepts,
                (SELECT COUNT(*) FROM shift_assignments a
                    WHERE a.clinician_id = c.id AND a.status = 'declined') AS declines,
                (SELECT COUNT(*) FROM shift_assignments a
                    WHERE a.clinician_id = c.id AND a.status = 'expired')  AS expires,
                cl.latitude  AS clinician_lat,
                cl.longitude AS clinician_lng
            FROM shift_interests si
            JOIN clinicians c          ON c.id = si.clinician_id
            LEFT JOIN clinician_locations cl ON cl.clinician_id = c.id
            WHERE si.shift_id = $1
            ORDER BY si.expressed_at ASC
            "#,
        )
        .bind(shift_id)
        .fetch_all(&self.pool)
        .await
    }

    /// Tier 2.4 — Fetch a pending offer for `(shift, clinician)` in `offered`
    /// status. Returns `(assignment_id, expires_at)` so the service can detect
    /// expired offers without a separate round-trip.
    pub async fn get_pending_offer(
        &self,
        shift_id: Uuid,
        clinician_id: Uuid,
    ) -> Result<Option<(Uuid, chrono::DateTime<chrono::Utc>)>, sqlx::Error> {
        sqlx::query_as::<_, (Uuid, chrono::DateTime<chrono::Utc>)>(
            r#"
            SELECT id, expires_at
            FROM shift_assignments
            WHERE shift_id = $1 AND clinician_id = $2 AND status = 'offered'
            LIMIT 1
            "#,
        )
        .bind(shift_id)
        .bind(clinician_id)
        .fetch_optional(&self.pool)
        .await
    }

    /// Tier 2.4 — Accept an offer inside a transaction.
    pub async fn accept_offer_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        assignment_id: Uuid,
        ndpr_consent: &serde_json::Value,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            UPDATE shift_assignments
               SET status        = 'accepted',
                   responded_at  = NOW(),
                   ndpr_consent  = $2,
                   updated_at    = NOW()
             WHERE id = $1 AND status = 'offered'
            "#,
        )
        .bind(assignment_id)
        .bind(ndpr_consent)
        .execute(&mut **tx)
        .await?;
        Ok(())
    }

    /// Tier 2.4 — Cancel sibling offers when one is accepted. Marks every
    /// other `offered` row for the same shift as `expired` so the hospital
    /// sees the offer was superseded.
    pub async fn cancel_sibling_offers_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        shift_id: Uuid,
        keep_assignment_id: Uuid,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            UPDATE shift_assignments
               SET status       = 'expired',
                   responded_at = NOW(),
                   updated_at   = NOW()
             WHERE shift_id = $1
               AND id      <> $2
               AND status   = 'offered'
            "#,
        )
        .bind(shift_id)
        .bind(keep_assignment_id)
        .execute(&mut **tx)
        .await?;
        Ok(())
    }

    /// Tier 2.4 — Mark the shift assigned and stamp the chosen clinician.
    pub async fn assign_shift_to_clinician_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        shift_id: Uuid,
        clinician_id: Uuid,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            UPDATE shifts
               SET status                = 'assigned',
                   assigned_clinician_id = $2,
                   updated_at            = NOW()
             WHERE id = $1
            "#,
        )
        .bind(shift_id)
        .bind(clinician_id)
        .execute(&mut **tx)
        .await?;
        Ok(())
    }

    /// Tier 2.4 — Decline an offer.
    pub async fn decline_offer(
        &self,
        assignment_id: Uuid,
        reason: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            UPDATE shift_assignments
               SET status         = 'declined',
                   responded_at   = NOW(),
                   decline_reason = $2,
                   updated_at     = NOW()
             WHERE id = $1 AND status = 'offered'
            "#,
        )
        .bind(assignment_id)
        .bind(reason)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Tier 2.4 — BR-F1-26 conflict check. Returns true if the clinician has
    /// another accepted/assigned shift whose window overlaps the candidate
    /// window. Compares against shifts in statuses that block double-booking.
    pub async fn has_conflicting_shift(
        &self,
        clinician_id: Uuid,
        candidate_start: chrono::DateTime<chrono::Utc>,
        candidate_end: chrono::DateTime<chrono::Utc>,
    ) -> Result<bool, sqlx::Error> {
        let count: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*)
            FROM shifts
            WHERE assigned_clinician_id = $1
              AND status IN ('assigned', 'upcoming', 'in_progress')
              AND scheduled_start < $3
              AND scheduled_end   > $2
            "#,
        )
        .bind(clinician_id)
        .bind(candidate_start)
        .bind(candidate_end)
        .fetch_one(&self.pool)
        .await?;
        Ok(count > 0)
    }

    /// Tier 2.3 — Create a `shift_assignments` row marking an offer to a
    /// specific clinician. Expires at `expires_at` (30 minutes by spec).
    /// Returns 409-style `unique_violation` if the same clinician has
    /// already been offered this shift (BR-F1-24).
    pub async fn create_assignment_offer(
        &self,
        shift_id: Uuid,
        clinician_id: Uuid,
        expires_at: chrono::DateTime<chrono::Utc>,
    ) -> Result<Uuid, sqlx::Error> {
        let id: Uuid = sqlx::query_scalar(
            r#"
            INSERT INTO shift_assignments (shift_id, clinician_id, status, expires_at)
            VALUES ($1, $2, 'offered', $3)
            RETURNING id
            "#,
        )
        .bind(shift_id)
        .bind(clinician_id)
        .bind(expires_at)
        .fetch_one(&self.pool)
        .await?;
        Ok(id)
    }

    /// Tier 2.5 — clock_in_radius_meters for this hospital's primary location
    /// (defaults to 100m per the migration).
    pub async fn get_clock_in_radius_meters(
        &self,
        hospital_id: Uuid,
    ) -> Result<Option<i32>, sqlx::Error> {
        sqlx::query_scalar::<_, i32>(
            r#"
            SELECT clock_in_radius_meters
            FROM hospital_locations
            WHERE hospital_id = $1
            ORDER BY created_at ASC
            LIMIT 1
            "#,
        )
        .bind(hospital_id)
        .fetch_optional(&self.pool)
        .await
    }

    /// Tier 2.5 — Record a clock-in inside the create-shift-in-progress
    /// transaction. Inserts a `shift_attendance` row and flips the shift to
    /// `in_progress`. Caller must have validated all preconditions.
    pub async fn record_clockin_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        shift_id: Uuid,
        clinician_id: Uuid,
        method: &crate::models::shift::ClockinMethod,
        latitude: Option<f64>,
        longitude: Option<f64>,
        distance_meters: Option<f32>,
        late_minutes: i32,
        late_penalty_applied: bool,
    ) -> Result<Uuid, sqlx::Error> {
        let id: Uuid = sqlx::query_scalar(
            r#"
            INSERT INTO shift_attendance (
                shift_id, clinician_id, clockin_at, clockin_method,
                clockin_latitude, clockin_longitude, clockin_distance_meters,
                late_minutes, late_penalty_applied
            )
            VALUES ($1, $2, NOW(), $3, $4, $5, $6, $7, $8)
            ON CONFLICT (shift_id) DO UPDATE
              SET clockin_at               = EXCLUDED.clockin_at,
                  clockin_method           = EXCLUDED.clockin_method,
                  clockin_latitude         = EXCLUDED.clockin_latitude,
                  clockin_longitude        = EXCLUDED.clockin_longitude,
                  clockin_distance_meters  = EXCLUDED.clockin_distance_meters,
                  late_minutes             = EXCLUDED.late_minutes,
                  late_penalty_applied     = EXCLUDED.late_penalty_applied,
                  updated_at               = NOW()
            RETURNING id
            "#,
        )
        .bind(shift_id)
        .bind(clinician_id)
        .bind(method)
        .bind(latitude)
        .bind(longitude)
        .bind(distance_meters)
        .bind(late_minutes)
        .bind(late_penalty_applied)
        .fetch_one(&mut **tx)
        .await?;

        sqlx::query(
            r#"
            UPDATE shifts
               SET status     = 'in_progress',
                   updated_at = NOW()
             WHERE id = $1
            "#,
        )
        .bind(shift_id)
        .execute(&mut **tx)
        .await?;

        Ok(id)
    }

    /// Tier 2.6 — Upsert a handover row for the given shift.
    /// On first submission, creates the row with `submitted_at = NOW()`,
    /// `editable_until = NOW() + 1h`, `auto_approve_after = NOW() + 48h`.
    /// On resubmission within `editable_until`, updates the content fields
    /// while keeping the original `submitted_at` (so the 48h auto-approve
    /// clock is not reset).
    pub async fn upsert_handover(
        &self,
        shift_id: Uuid,
        patients_seen: i32,
        critical_patients: &serde_json::Value,
        pending_tasks: &serde_json::Value,
        instructions: &str,
        equipment_status: Option<&str>,
    ) -> Result<crate::models::shift::HandoverResponse, sqlx::Error> {
        sqlx::query_as::<_, crate::models::shift::HandoverResponse>(
            r#"
            INSERT INTO shift_handovers (
                shift_id, patients_seen, critical_patients, pending_tasks,
                instructions, equipment_status,
                submitted_at, editable_until, auto_approve_after
            )
            VALUES (
                $1, $2, $3, $4, $5, $6,
                NOW(), NOW() + INTERVAL '1 hour', NOW() + INTERVAL '48 hours'
            )
            ON CONFLICT (shift_id) DO UPDATE
              SET patients_seen     = EXCLUDED.patients_seen,
                  critical_patients = EXCLUDED.critical_patients,
                  pending_tasks     = EXCLUDED.pending_tasks,
                  instructions      = EXCLUDED.instructions,
                  equipment_status  = EXCLUDED.equipment_status,
                  updated_at        = NOW()
            RETURNING
                id, shift_id, patients_seen, critical_patients, pending_tasks,
                instructions, equipment_status,
                submitted_at, editable_until, auto_approve_after,
                hospital_approved_at, revision_requested_at, revision_notes
            "#,
        )
        .bind(shift_id)
        .bind(patients_seen)
        .bind(critical_patients)
        .bind(pending_tasks)
        .bind(instructions)
        .bind(equipment_status)
        .fetch_one(&self.pool)
        .await
    }

    /// Tier 2.6 — Fetch the existing handover row, if any.
    pub async fn get_handover(
        &self,
        shift_id: Uuid,
    ) -> Result<Option<crate::models::shift::HandoverResponse>, sqlx::Error> {
        sqlx::query_as::<_, crate::models::shift::HandoverResponse>(
            r#"
            SELECT id, shift_id, patients_seen, critical_patients, pending_tasks,
                   instructions, equipment_status,
                   submitted_at, editable_until, auto_approve_after,
                   hospital_approved_at, revision_requested_at, revision_notes
            FROM shift_handovers
            WHERE shift_id = $1
            "#,
        )
        .bind(shift_id)
        .fetch_optional(&self.pool)
        .await
    }

    /// Tier 2.6 — Read the current clock-in time for a shift, used to compute
    /// worked_minutes and the 24h revision deadline.
    pub async fn get_attendance_clockin(
        &self,
        shift_id: Uuid,
    ) -> Result<Option<chrono::DateTime<chrono::Utc>>, sqlx::Error> {
        sqlx::query_scalar::<_, Option<chrono::DateTime<chrono::Utc>>>(
            r#"
            SELECT clockin_at FROM shift_attendance WHERE shift_id = $1
            "#,
        )
        .bind(shift_id)
        .fetch_optional(&self.pool)
        .await
        .map(|opt| opt.flatten())
    }

    /// Tier 2.6 — Read clockout_at to enforce the 24-hour revision window.
    pub async fn get_attendance_clockout(
        &self,
        shift_id: Uuid,
    ) -> Result<Option<chrono::DateTime<chrono::Utc>>, sqlx::Error> {
        sqlx::query_scalar::<_, Option<chrono::DateTime<chrono::Utc>>>(
            r#"
            SELECT clockout_at FROM shift_attendance WHERE shift_id = $1
            "#,
        )
        .bind(shift_id)
        .fetch_optional(&self.pool)
        .await
        .map(|opt| opt.flatten())
    }

    /// Tier 2.6 — Record clockout inside a transaction and flip the shift
    /// to 'completed'. Returns `(attendance_id, worked_minutes)`.
    pub async fn record_clockout_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        shift_id: Uuid,
        worked_minutes: i32,
    ) -> Result<Uuid, sqlx::Error> {
        let id: Uuid = sqlx::query_scalar(
            r#"
            UPDATE shift_attendance
               SET clockout_at    = NOW(),
                   worked_minutes = $2,
                   updated_at     = NOW()
             WHERE shift_id = $1
             RETURNING id
            "#,
        )
        .bind(shift_id)
        .bind(worked_minutes)
        .fetch_one(&mut **tx)
        .await?;

        sqlx::query(
            r#"
            UPDATE shifts
               SET status = 'completed', updated_at = NOW()
             WHERE id = $1
            "#,
        )
        .bind(shift_id)
        .execute(&mut **tx)
        .await?;

        Ok(id)
    }

    /// Tier 2.6 — Request a handover revision (hospital-side, within 24h of
    /// clock-out per BR-F1-37). Caller validates the time window.
    pub async fn request_handover_revision(
        &self,
        shift_id: Uuid,
        notes: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            UPDATE shift_handovers
               SET revision_requested_at = NOW(),
                   revision_notes        = $2,
                   updated_at            = NOW()
             WHERE shift_id = $1
            "#,
        )
        .bind(shift_id)
        .bind(notes)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Tier 2.7 — Insert a rating row. `window_closes_at` is the 7-day cap
    /// (BR-F1-46); `editable_until` is the 48-hour edit window (BR-F1-50).
    /// Caller validates time windows and uniqueness before calling.
    #[allow(clippy::too_many_arguments)]
    pub async fn insert_rating(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        shift_id: Uuid,
        rater_user_id: Uuid,
        ratee_id: Uuid,
        ratee_kind: &str,
        score: i16,
        dimensions: Option<&serde_json::Value>,
        comment: Option<&str>,
        window_closes_at: chrono::DateTime<chrono::Utc>,
    ) -> Result<crate::models::shift::RatingResponse, sqlx::Error> {
        sqlx::query_as::<_, crate::models::shift::RatingResponse>(
            r#"
            INSERT INTO shift_ratings (
                shift_id, rater_user_id, ratee_id, ratee_kind,
                score, dimensions, comment, is_anonymous,
                editable_until, window_closes_at
            )
            VALUES (
                $1, $2, $3, $4::rating_ratee_kind,
                $5, $6, $7, TRUE,
                NOW() + INTERVAL '48 hours', $8
            )
            RETURNING
                id, shift_id, ratee_id, ratee_kind::text AS ratee_kind,
                score, dimensions, comment, is_anonymous,
                editable_until, window_closes_at, created_at
            "#,
        )
        .bind(shift_id)
        .bind(rater_user_id)
        .bind(ratee_id)
        .bind(ratee_kind)
        .bind(score)
        .bind(dimensions)
        .bind(comment)
        .bind(window_closes_at)
        .fetch_one(&mut **tx)
        .await
    }

    /// Tier 2.7 — Edit a rating within the 48h window. Caller validates the
    /// window. `dimensions` is fully replaced when provided.
    pub async fn update_rating(
        &self,
        rating_id: Uuid,
        score: Option<i16>,
        dimensions: Option<&serde_json::Value>,
        comment: Option<&str>,
    ) -> Result<crate::models::shift::RatingResponse, sqlx::Error> {
        sqlx::query_as::<_, crate::models::shift::RatingResponse>(
            r#"
            UPDATE shift_ratings
               SET score      = COALESCE($2, score),
                   dimensions = COALESCE($3, dimensions),
                   comment    = COALESCE($4, comment),
                   updated_at = NOW()
             WHERE id = $1
             RETURNING
                id, shift_id, ratee_id, ratee_kind::text AS ratee_kind,
                score, dimensions, comment, is_anonymous,
                editable_until, window_closes_at, created_at
            "#,
        )
        .bind(rating_id)
        .bind(score)
        .bind(dimensions)
        .bind(comment)
        .fetch_one(&self.pool)
        .await
    }

    /// Tier 2.7 — Fetch a rating by id (for the edit handler's auth check).
    /// Returns `(rating, rater_user_id)` so we can verify the caller owns it.
    pub async fn get_rating_for_edit(
        &self,
        rating_id: Uuid,
    ) -> Result<Option<(crate::models::shift::RatingResponse, Uuid)>, sqlx::Error> {
        let row = sqlx::query_as::<_, (
            Uuid, Uuid, Uuid, String, i16, Option<serde_json::Value>,
            Option<String>, bool, chrono::DateTime<chrono::Utc>,
            chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>, Uuid,
        )>(
            r#"
            SELECT id, shift_id, ratee_id, ratee_kind::text,
                   score, dimensions, comment, is_anonymous,
                   editable_until, window_closes_at, created_at, rater_user_id
            FROM shift_ratings
            WHERE id = $1
            "#,
        )
        .bind(rating_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|t| (
            crate::models::shift::RatingResponse {
                id: t.0,
                shift_id: t.1,
                ratee_id: t.2,
                ratee_kind: t.3,
                score: t.4,
                dimensions: t.5,
                comment: t.6,
                is_anonymous: t.7,
                editable_until: t.8,
                window_closes_at: t.9,
                created_at: t.10,
            },
            t.11,
        )))
    }

    /// Tier 2.7 — Recompute the clinician's cached average rating after a
    /// rating insert/edit. Runs inside the same tx as the rating change.
    pub async fn recompute_clinician_rating_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        clinician_id: Uuid,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            UPDATE clinicians c
               SET rating = COALESCE((
                       SELECT AVG(score)::REAL
                       FROM shift_ratings r
                       WHERE r.ratee_kind = 'clinician'
                         AND r.ratee_id   = c.id
                   ), 0.0),
                   rating_count = (
                       SELECT COUNT(*)::INTEGER
                       FROM shift_ratings r
                       WHERE r.ratee_kind = 'clinician'
                         AND r.ratee_id   = c.id
                   ),
                   updated_at = NOW()
             WHERE c.id = $1
            "#,
        )
        .bind(clinician_id)
        .execute(&mut **tx)
        .await?;
        Ok(())
    }

    /// Tier 2.1 — Discover open shifts for a worker.
    ///
    /// Returns one row per open shift along with the hospital coords, the
    /// clinician's own coords (if any), and whether the worker has already
    /// expressed interest. Distance + sort happen in the service.
    pub async fn list_open_shifts_for_worker(
        &self,
        clinician_id: Uuid,
    ) -> Result<Vec<NearbyShiftRow>, sqlx::Error> {
        sqlx::query_as::<_, NearbyShiftRow>(
            r#"
            SELECT
                s.id              AS shift_id,
                s.hospital_id,
                h.name            AS hospital_name,
                s.role_title,
                s.specialty,
                s.shift_type      AS "shift_type: _",
                s.priority        AS "priority: _",
                s.scheduled_start,
                s.duration_hours,
                s.pay_type        AS "pay_type: _",
                s.rate_kobo_per_hour,
                s.fixed_rate_kobo,
                s.stat_bonus_kobo,
                hl.latitude       AS hospital_lat,
                hl.longitude      AS hospital_lng,
                cl.latitude       AS clinician_lat,
                cl.longitude      AS clinician_lng,
                EXISTS (
                    SELECT 1 FROM shift_interests si
                    WHERE si.shift_id = s.id AND si.clinician_id = $1
                ) AS interest_expressed
            FROM shifts s
            JOIN hospitals h               ON h.id = s.hospital_id
            LEFT JOIN hospital_locations hl ON hl.hospital_id = h.id
            LEFT JOIN clinician_locations cl ON cl.clinician_id = $1
            WHERE s.status = 'open'
              AND NOT EXISTS (
                  SELECT 1 FROM shift_dismissals sd
                  WHERE sd.shift_id = s.id AND sd.clinician_id = $1
              )
            ORDER BY s.scheduled_start ASC
            "#,
        )
        .bind(clinician_id)
        .fetch_all(&self.pool)
        .await
    }

    /// Tier 2.1 — List a clinician's expressed interests + formal applications
    /// across all shifts (the "My Applications" tab, §3.3.6).
    pub async fn list_my_applications(
        &self,
        clinician_id: Uuid,
    ) -> Result<Vec<crate::models::shift::MyApplicationEntry>, sqlx::Error> {
        let interests = sqlx::query_as::<_, (Uuid, Uuid, String, chrono::DateTime<chrono::Utc>, ShiftStatus, chrono::DateTime<chrono::Utc>)>(
            r#"
            SELECT s.id, s.hospital_id, s.role_title, s.scheduled_start,
                   s.status AS "status: _", si.expressed_at AS created_at
            FROM shift_interests si
            JOIN shifts s ON s.id = si.shift_id
            WHERE si.clinician_id = $1
            "#,
        )
        .bind(clinician_id)
        .fetch_all(&self.pool)
        .await?;

        let applications = sqlx::query_as::<_, (Uuid, Uuid, String, chrono::DateTime<chrono::Utc>, ShiftStatus, ShiftApplicationStatus, chrono::DateTime<chrono::Utc>)>(
            r#"
            SELECT s.id, s.hospital_id, s.role_title, s.scheduled_start,
                   s.status AS "status: _",
                   a.status AS "app_status: _",
                   a.created_at
            FROM shift_applications a
            JOIN shifts s ON s.id = a.shift_id
            WHERE a.clinician_id = $1
            "#,
        )
        .bind(clinician_id)
        .fetch_all(&self.pool)
        .await?;

        let mut rows: Vec<crate::models::shift::MyApplicationEntry> = interests
            .into_iter()
            .map(|(sid, hid, title, start, status, created)| crate::models::shift::MyApplicationEntry {
                shift_id: sid,
                hospital_id: hid,
                role_title: title,
                scheduled_start: start,
                shift_status: status,
                kind: "interest".to_string(),
                application_status: None,
                created_at: created,
            })
            .collect();

        rows.extend(applications.into_iter().map(
            |(sid, hid, title, start, status, app_status, created)| crate::models::shift::MyApplicationEntry {
                shift_id: sid,
                hospital_id: hid,
                role_title: title,
                scheduled_start: start,
                shift_status: status,
                kind: "application".to_string(),
                application_status: Some(app_status),
                created_at: created,
            },
        ));

        // Newest first
        rows.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        Ok(rows)
    }

    /// Tier 2.2 — Withdraw a previously expressed interest. Returns the
    /// number of rows deleted so the caller can detect "no such interest".
    pub async fn withdraw_interest(
        &self,
        shift_id: Uuid,
        clinician_id: Uuid,
    ) -> Result<u64, sqlx::Error> {
        let result = sqlx::query(
            r#"
            DELETE FROM shift_interests
            WHERE shift_id = $1 AND clinician_id = $2
            "#,
        )
        .bind(shift_id)
        .bind(clinician_id)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected())
    }

    /// Tier 2.2 — Insert a bookmark. Idempotent via the unique constraint.
    pub async fn bookmark_shift(
        &self,
        shift_id: Uuid,
        clinician_id: Uuid,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            INSERT INTO shift_bookmarks (shift_id, clinician_id)
            VALUES ($1, $2)
            ON CONFLICT (shift_id, clinician_id) DO NOTHING
            "#,
        )
        .bind(shift_id)
        .bind(clinician_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Tier 2.2 — Remove a bookmark. Returns rows-affected so the caller can
    /// report "not bookmarked" if needed.
    pub async fn unbookmark_shift(
        &self,
        shift_id: Uuid,
        clinician_id: Uuid,
    ) -> Result<u64, sqlx::Error> {
        let result = sqlx::query(
            r#"
            DELETE FROM shift_bookmarks
            WHERE shift_id = $1 AND clinician_id = $2
            "#,
        )
        .bind(shift_id)
        .bind(clinician_id)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected())
    }

    /// Tier 2.2 — Record a dismissal so the shift no longer appears in this
    /// clinician's nearby feed. Idempotent.
    pub async fn dismiss_shift(
        &self,
        shift_id: Uuid,
        clinician_id: Uuid,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            INSERT INTO shift_dismissals (shift_id, clinician_id)
            VALUES ($1, $2)
            ON CONFLICT (shift_id, clinician_id) DO NOTHING
            "#,
        )
        .bind(shift_id)
        .bind(clinician_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// BR-F1-06: count shifts the hospital currently has in "active, unfilled" states.
    /// Used to enforce the 10-shift cap before creating another one.
    pub async fn count_active_unfilled_shifts(
        &self,
        hospital_id: Uuid,
    ) -> Result<i64, sqlx::Error> {
        sqlx::query_scalar::<_, i64>(
            r#"
            SELECT COUNT(*)
            FROM shifts
            WHERE hospital_id = $1
              AND status IN ('open', 'upcoming')
            "#,
        )
        .bind(hospital_id)
        .fetch_one(&self.pool)
        .await
    }

    pub async fn create(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        hospital_id: Uuid,
        created_by: Uuid,
        request: CreateShiftRequest,
    ) -> Result<Shift, sqlx::Error> {
        let id = Uuid::new_v4();
        let scheduled_end = request.scheduled_start + Duration::hours(request.duration_hours as i64);
        
        // Calculate effective rate and grand total
        let (effective_rate, grand_total) = self.calculate_compensation(&request);

        sqlx::query(
            r#"
            INSERT INTO shifts (
                id, hospital_id, role_category, role_title, specialty, department,
                shift_type, status, priority, urgency_bonus_pct,
                scheduled_start, duration_hours, scheduled_end,
                pay_type, rate_kobo_per_hour, fixed_rate_kobo, stat_bonus_kobo,
                effective_rate_kobo_per_hour, grand_total_kobo,
                shift_label, job_description, notes, created_by, broadcast_consent_confirmed,
                created_at, updated_at
            )
            VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8, $9, $10,
                $11, $12, $13, $14, $15, $16, $17, $18, $19,
                $20, $21, $22, $23, $24, NOW(), NOW()
            )
            "#,
        )
        .bind(id)
        .bind(hospital_id)
        .bind(&request.role_category)
        .bind(&request.role_title)
        .bind(&request.specialty)
        .bind(&request.department)
        .bind(&request.shift_type)
        .bind(ShiftStatus::Open)
        .bind(&request.priority)
        .bind(request.urgency_bonus_pct)
        .bind(request.scheduled_start)
        .bind(request.duration_hours)
        .bind(scheduled_end)
        .bind(&request.pay_type)
        .bind(request.rate_kobo_per_hour)
        .bind(request.fixed_rate_kobo)
        .bind(request.stat_bonus_kobo)
        .bind(effective_rate)
        .bind(grand_total)
        .bind(&request.shift_label)
        .bind(&request.job_description)
        .bind(&request.notes)
        .bind(created_by)
        .bind(request.broadcast_consent_confirmed)
        .execute(&mut **tx)
        .await?;

        let shift = sqlx::query_as::<_, Shift>(
            r#"
            SELECT s.*, h.name as hospital_name
            FROM shifts s
            LEFT JOIN hospitals h ON s.hospital_id = h.id
            WHERE s.id = $1
            "#,
        )
        .bind(id)
        .fetch_one(&mut **tx)
        .await?;

        Ok(shift)
    }

    pub async fn broadcast_shift(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        shift_id: Uuid,
        matched_count: i32,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            UPDATE shifts
            SET broadcast_at = NOW(),
                matched_clinicians_at_publish = $2,
                updated_at = NOW()
            WHERE id = $1
            "#,
        )
        .bind(shift_id)
        .bind(matched_count)
        .execute(&mut **tx)
        .await?;

        Ok(())
    }

    pub async fn get_by_id(&self, shift_id: Uuid) -> Result<Option<Shift>, sqlx::Error> {
        sqlx::query_as::<_, Shift>(
            r#"
            SELECT
                s.id, s.hospital_id, h.name as hospital_name,
                s.role_category, s.role_title, s.specialty, s.department,
                s.shift_type, s.status, s.priority, s.urgency_bonus_pct,
                s.scheduled_start, s.duration_hours, s.scheduled_end,
                s.actual_start, s.actual_end, s.assigned_clinician_id,
                s.rate_kobo_per_hour, s.fixed_rate_kobo, s.pay_type, s.stat_bonus_kobo,
                s.effective_rate_kobo_per_hour, s.grand_total_kobo,
                s.shift_label, s.job_description, s.draft_quality_score, s.notes,
                s.created_by, s.broadcast_consent_confirmed, s.matched_clinicians_at_publish,
                s.broadcast_at, s.billing_triggered_at, s.created_at, s.updated_at
            FROM shifts s
            LEFT JOIN hospitals h ON s.hospital_id = h.id
            WHERE s.id = $1
            "#,
        )
        .bind(shift_id)
        .fetch_optional(&self.pool)
        .await
    }

    pub async fn list_shifts(
        &self,
        status_filter: Option<ShiftStatus>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Shift>, sqlx::Error> {
        if let Some(status) = status_filter {
            sqlx::query_as::<_, Shift>(
                r#"
                SELECT
                    s.id, s.hospital_id, h.name as hospital_name,
                    s.role_category, s.role_title, s.specialty, s.department,
                    s.shift_type, s.status, s.priority, s.urgency_bonus_pct,
                    s.scheduled_start, s.duration_hours, s.scheduled_end,
                    s.actual_start, s.actual_end, s.assigned_clinician_id,
                    s.rate_kobo_per_hour, s.fixed_rate_kobo, s.pay_type, s.stat_bonus_kobo,
                    s.effective_rate_kobo_per_hour, s.grand_total_kobo,
                    s.shift_label, s.job_description, s.draft_quality_score, s.notes,
                    s.created_by, s.broadcast_consent_confirmed, s.matched_clinicians_at_publish,
                    s.broadcast_at, s.billing_triggered_at, s.created_at, s.updated_at
                FROM shifts s
                LEFT JOIN hospitals h ON s.hospital_id = h.id
                WHERE s.status = $1
                ORDER BY s.created_at DESC
                LIMIT $2 OFFSET $3
                "#,
            )
            .bind(status)
            .bind(limit)
            .bind(offset)
            .fetch_all(&self.pool)
            .await
        } else {
            sqlx::query_as::<_, Shift>(
                r#"
                SELECT
                    s.id, s.hospital_id, h.name as hospital_name,
                    s.role_category, s.role_title, s.specialty, s.department,
                    s.shift_type, s.status, s.priority, s.urgency_bonus_pct,
                    s.scheduled_start, s.duration_hours, s.scheduled_end,
                    s.actual_start, s.actual_end, s.assigned_clinician_id,
                    s.rate_kobo_per_hour, s.fixed_rate_kobo, s.pay_type, s.stat_bonus_kobo,
                    s.effective_rate_kobo_per_hour, s.grand_total_kobo,
                    s.shift_label, s.job_description, s.draft_quality_score, s.notes,
                    s.created_by, s.broadcast_consent_confirmed, s.matched_clinicians_at_publish,
                    s.broadcast_at, s.billing_triggered_at, s.created_at, s.updated_at
                FROM shifts s
                LEFT JOIN hospitals h ON s.hospital_id = h.id
                ORDER BY s.created_at DESC
                LIMIT $1 OFFSET $2
                "#,
            )
            .bind(limit)
            .bind(offset)
            .fetch_all(&self.pool)
            .await
        }
    }

    pub async fn count_shifts(
        &self,
        status_filter: Option<ShiftStatus>,
    ) -> Result<i64, sqlx::Error> {
        if let Some(status) = status_filter {
            sqlx::query_scalar::<_, i64>(
                r#"
                SELECT COUNT(*)
                FROM shifts
                WHERE status = $1
                "#,
            )
            .bind(status)
            .fetch_one(&self.pool)
            .await
        } else {
            sqlx::query_scalar::<_, i64>(
                r#"
                SELECT COUNT(*) FROM shifts
                "#,
            )
            .fetch_one(&self.pool)
            .await
        }
    }

    pub async fn clinician_has_active_assignment(
        &self,
        clinician_id: Uuid,
    ) -> Result<bool, sqlx::Error> {
        let row: (i64,) = sqlx::query_as(
            r#"
            SELECT COUNT(*)
            FROM shifts
            WHERE assigned_clinician_id = $1
              AND status IN ('upcoming', 'in_progress')
            "#,
        )
        .bind(clinician_id)
        .fetch_one(&self.pool)
        .await?;

        Ok(row.0 > 0)
    }

    pub async fn get_clinician_profile_snapshot(
        &self,
        clinician_id: Uuid,
    ) -> Result<Option<(String, String, Option<String>, Option<String>)>, sqlx::Error> {
        sqlx::query_as::<_, (String, String, Option<String>, Option<String>)>(
            r#"
            SELECT first_name, last_name, license_number, clinician_role::text
            FROM clinicians
            WHERE id = $1
            "#,
        )
        .bind(clinician_id)
        .fetch_optional(&self.pool)
        .await
    }

    pub async fn create_application(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        shift_id: Uuid,
        clinician_id: Uuid,
        applicant_name: &str,
        license_number: &str,
        role: &str,
        years_experience: i32,
        experience_summary: Option<&str>,
    ) -> Result<ShiftApplication, sqlx::Error> {
        sqlx::query_as::<_, ShiftApplication>(
            r#"
            INSERT INTO shift_applications (
                shift_id, clinician_id, applicant_name, license_number,
                role, years_experience, experience_summary
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            RETURNING
                id, shift_id, clinician_id, applicant_name, license_number,
                role, years_experience, experience_summary, status, created_at, updated_at
            "#,
        )
        .bind(shift_id)
        .bind(clinician_id)
        .bind(applicant_name)
        .bind(license_number)
        .bind(role)
        .bind(years_experience)
        .bind(experience_summary)
        .fetch_one(&mut **tx)
        .await
    }

    pub async fn list_applications_for_shift(
        &self,
        shift_id: Uuid,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<ShiftApplication>, sqlx::Error> {
        sqlx::query_as::<_, ShiftApplication>(
            r#"
            SELECT id, shift_id, clinician_id, applicant_name, license_number,
                   role, years_experience, experience_summary, status, created_at, updated_at
            FROM shift_applications
            WHERE shift_id = $1
            ORDER BY created_at DESC
            LIMIT $2 OFFSET $3
            "#,
        )
        .bind(shift_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
    }

    pub async fn count_applications_for_shift(
        &self,
        shift_id: Uuid,
    ) -> Result<i64, sqlx::Error> {
        sqlx::query_scalar::<_, i64>(
            r#"
            SELECT COUNT(*)
            FROM shift_applications
            WHERE shift_id = $1
            "#,
        )
        .bind(shift_id)
        .fetch_one(&self.pool)
        .await
    }

    pub async fn update_application_status(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        shift_id: Uuid,
        clinician_id: Uuid,
        status: ShiftApplicationStatus,
    ) -> Result<u64, sqlx::Error> {
        let result = sqlx::query(
            r#"
            UPDATE shift_applications
            SET status = $3,
                updated_at = NOW()
            WHERE shift_id = $1 AND clinician_id = $2
            "#,
        )
        .bind(shift_id)
        .bind(clinician_id)
        .bind(status)
        .execute(&mut **tx)
        .await?;

        Ok(result.rows_affected())
    }

    pub async fn add_interest(
        &self,
        shift_id: Uuid,
        clinician_id: Uuid,
        is_top_match: bool,
        is_waitlisted: bool,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            INSERT INTO shift_interests (shift_id, clinician_id, is_top_match, is_waitlisted)
            VALUES ($1, $2, $3, $4)
            "#,
        )
        .bind(shift_id)
        .bind(clinician_id)
        .bind(is_top_match)
        .bind(is_waitlisted)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn assign_clinician(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        shift_id: Uuid,
        clinician_id: Uuid,
        new_status: ShiftStatus,
    ) -> Result<u64, sqlx::Error> {
        let result = sqlx::query(
            r#"
            UPDATE shifts
            SET assigned_clinician_id = $2,
                status = $3,
                updated_at = NOW()
            WHERE id = $1 AND status = 'open'
            "#,
        )
        .bind(shift_id)
        .bind(clinician_id)
        .bind(new_status)
        .execute(&mut **tx)
        .await?;

        Ok(result.rows_affected())
    }

    pub async fn cancel_shift(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        shift_id: Uuid,
    ) -> Result<u64, sqlx::Error> {
        let result = sqlx::query(
            r#"
            UPDATE shifts
            SET status = 'cancelled',
                updated_at = NOW()
            WHERE id = $1 AND status IN ('open', 'upcoming')
            "#,
        )
        .bind(shift_id)
        .execute(&mut **tx)
        .await?;

        Ok(result.rows_affected())
    }

    pub async fn reschedule_shift(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        shift_id: Uuid,
        scheduled_start: chrono::DateTime<Utc>,
        duration_hours: f32,
        scheduled_end: chrono::DateTime<Utc>,
    ) -> Result<u64, sqlx::Error> {
        let result = sqlx::query(
            r#"
            UPDATE shifts
            SET scheduled_start = $2,
                duration_hours = $3,
                scheduled_end = $4,
                updated_at = NOW()
            WHERE id = $1 AND status IN ('open', 'upcoming')
            "#,
        )
        .bind(shift_id)
        .bind(scheduled_start)
        .bind(duration_hours)
        .bind(scheduled_end)
        .execute(&mut **tx)
        .await?;

        Ok(result.rows_affected())
    }

    /// AC-08: Find similar shift within time window
    pub async fn find_similar_shift(
        &self,
        hospital_id: Uuid,
        role_title: &str,
        scheduled_start: chrono::DateTime<Utc>,
        created_after: chrono::DateTime<Utc>,
    ) -> Result<Option<Shift>, sqlx::Error> {
        sqlx::query_as::<_, Shift>(
            r#"
            SELECT
                s.id, s.hospital_id, h.name as hospital_name,
                s.role_category, s.role_title, s.specialty, s.department,
                s.shift_type, s.status, s.priority, s.urgency_bonus_pct,
                s.scheduled_start, s.duration_hours, s.scheduled_end,
                s.actual_start, s.actual_end, s.assigned_clinician_id,
                s.rate_kobo_per_hour, s.fixed_rate_kobo, s.pay_type, s.stat_bonus_kobo,
                s.effective_rate_kobo_per_hour, s.grand_total_kobo,
                s.shift_label, s.job_description, s.draft_quality_score, s.notes,
                s.created_by, s.broadcast_consent_confirmed, s.matched_clinicians_at_publish,
                s.broadcast_at, s.billing_triggered_at, s.created_at, s.updated_at
            FROM shifts s
            LEFT JOIN hospitals h ON s.hospital_id = h.id
            WHERE s.hospital_id = $1
              AND s.role_title = $2
              AND s.scheduled_start = $3
              AND s.created_at > $4
              AND s.status = 'open'
            ORDER BY s.created_at DESC
            LIMIT 1
            "#,
        )
        .bind(hospital_id)
        .bind(role_title)
        .bind(scheduled_start)
        .bind(created_after)
        .fetch_optional(&self.pool)
        .await
    }

    /// AC-04 / F1-F15: Store the auto-generated virtual consultation link
    /// in the explicit `virtual_link` column on `shifts`.
    pub async fn update_virtual_link(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        shift_id: Uuid,
        virtual_link: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            UPDATE shifts
            SET virtual_link = $2,
                updated_at   = NOW()
            WHERE id = $1
            "#,
        )
        .bind(shift_id)
        .bind(virtual_link)
        .execute(&mut **tx)
        .await?;

        Ok(())
    }

    /// F1-F12 / F1-F13 / F1-F14 — atomically persist the shift's
    /// tasks (description items, category=task), equipment (category=equipment),
    /// and requirements (qualifications) inside the create-shift transaction.
    pub async fn insert_shift_description_and_requirements(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        shift_id: Uuid,
        tasks: &[String],
        equipment: &[String],
        requirements: &[String],
    ) -> Result<(), sqlx::Error> {
        for (idx, label) in tasks.iter().enumerate() {
            sqlx::query(
                r#"
                INSERT INTO shift_description_items (shift_id, category, label, sort_order)
                VALUES ($1, 'task', $2, $3)
                "#,
            )
            .bind(shift_id)
            .bind(label)
            .bind(idx as i16)
            .execute(&mut **tx)
            .await?;
        }

        for (idx, label) in equipment.iter().enumerate() {
            sqlx::query(
                r#"
                INSERT INTO shift_description_items (shift_id, category, label, sort_order)
                VALUES ($1, 'equipment', $2, $3)
                "#,
            )
            .bind(shift_id)
            .bind(label)
            .bind(idx as i16)
            .execute(&mut **tx)
            .await?;
        }

        for (idx, qualification) in requirements.iter().enumerate() {
            sqlx::query(
                r#"
                INSERT INTO shift_requirements (shift_id, qualification, sort_order)
                VALUES ($1, $2, $3)
                "#,
            )
            .bind(shift_id)
            .bind(qualification)
            .bind(idx as i16)
            .execute(&mut **tx)
            .await?;
        }

        Ok(())
    }

    fn calculate_compensation(&self, request: &CreateShiftRequest) -> (Option<i64>, Option<i64>) {
        let base_amount = match request.pay_type {
            PayType::HourlyRate => {
                request.rate_kobo_per_hour.map(|rate| (rate as f64 * request.duration_hours as f64) as i64)
            }
            PayType::FixedRate => request.fixed_rate_kobo,
        };

        let effective_rate = match request.pay_type {
            PayType::HourlyRate => {
                request.rate_kobo_per_hour.map(|rate| {
                    if let Some(bonus_pct) = request.urgency_bonus_pct {
                        rate + (rate * bonus_pct as i64 / 100)
                    } else {
                        rate
                    }
                })
            }
            PayType::FixedRate => None,
        };

        let grand_total = base_amount.map(|base| {
            let stat_bonus = request.stat_bonus_kobo.unwrap_or(0);
            base + stat_bonus
        });

        (effective_rate, grand_total)
    }
}
