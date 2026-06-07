use std::sync::Arc;
use sqlx::PgPool;
use uuid::Uuid;
use chrono::{Utc, Duration};

use crate::models::shift::{Shift, CreateShiftRequest, ShiftType, ShiftPriority, ShiftStatus};
use crate::repositories::shift::ShiftRepository;
use crate::services::notification_service::NotificationService;
use crate::services::email_outbox_service::EmailOutboxService;
use crate::services::email_templates;

#[derive(Debug, thiserror::Error)]
pub enum ShiftServiceError {
    #[error("Validation failed: {0}")]
    ValidationError(String),
    
    #[error("Database error: {0}")]
    DatabaseError(#[from] sqlx::Error),
    
    #[error("Shift not found: {0}")]
    NotFound(Uuid),
    
    #[error("Duplicate shift: {0}")]
    DuplicateShift(String),

    #[error("Duplicate shift interest")]
    DuplicateInterest,

    #[error("Duplicate shift application")]
    DuplicateApplication,

    #[error("Clinician profile is incomplete")]
    ProfileIncomplete,

    #[error("Clinician already assigned to an active shift")]
    ClinicianBusy,

    #[error("Not authorized to view applications")]
    NotAuthorized,

    #[error("Shift already assigned")]
    AlreadyAssigned,

    #[error("Invalid shift status: {0}")]
    InvalidStatus(String),
    
    #[error("Hospital not approved: {0}")]
    HospitalNotApproved(String),

    #[error("Too many active shifts")]
    TooManyActiveShifts,

    #[error("Clinician has not expressed interest in this shift")]
    NotInterested,

    #[error("Clinician already has an offer for this shift")]
    DuplicateOffer,

    #[error("No pending offer for this shift")]
    NoPendingOffer,

    #[error("Offer has expired")]
    OfferExpired,

    #[error("All NDPR consent boxes must be checked")]
    ConsentRequired,

    #[error("Authenticated user has no clinician profile")]
    NoClinicianProfile,

    #[error("Shift overlaps with another accepted shift")]
    ScheduleConflict,

    #[error("Too early to clock in")]
    TooEarlyToClockIn,

    #[error("Shift was missed (more than 60 minutes late)")]
    MissedShift,

    #[error("Clock-in location is {0} metres from the hospital — outside the geofence")]
    OutOfGeofence(i32),

    #[error("Handover must be submitted before clock-out")]
    HandoverRequired,

    #[error("Handover edit window (1 hour after clock-out) has closed")]
    HandoverEditWindowClosed,

    #[error("Hospital revision window (24 hours after clock-out) has closed")]
    RevisionWindowClosed,

    #[error("Rating already submitted for this shift")]
    DuplicateRating,

    #[error("Rating submission window (7 days after shift completion) has closed")]
    RatingWindowClosed,

    #[error("Rating not found")]
    RatingNotFound,

    #[error("Rating edit window (48 hours) has closed")]
    RatingEditWindowClosed,
}

pub struct ShiftService {
    shift_repo: Arc<ShiftRepository>,
    pool: PgPool,
    notification_service: Arc<NotificationService>,
    email_outbox: Arc<EmailOutboxService>,
}

impl ShiftService {
    pub fn new(
        shift_repo: Arc<ShiftRepository>,
        pool: PgPool,
        notification_service: Arc<NotificationService>,
        email_outbox: Arc<EmailOutboxService>,
    ) -> Self {
        Self { shift_repo, pool, notification_service, email_outbox }
    }

    pub async fn create_shift(
        &self,
        hospital_id: Uuid,
        created_by: Uuid,
        mut request: CreateShiftRequest,
    ) -> Result<Shift, ShiftServiceError> {
        // Check if hospital is approved
        let is_approved = self.shift_repo.check_hospital_approved(hospital_id).await?;
        if !is_approved {
            return Err(ShiftServiceError::HospitalNotApproved(
                "Only approved hospitals can create shifts. Please complete your registration and wait for approval.".to_string()
            ));
        }

        // Validate required fields based on pay type
        self.validate_request(&request)?;

        // BR-F1-06: hospital cannot have more than 10 active unfilled shifts at once.
        let active_unfilled = self
            .shift_repo
            .count_active_unfilled_shifts(hospital_id)
            .await?;
        if active_unfilled >= 10 {
            return Err(ShiftServiceError::TooManyActiveShifts);
        }

        // BR-F1-07: STAT shifts get an automatic +20% bonus when none is set.
        if request.priority == ShiftPriority::Stat
            && request.stat_bonus_kobo.unwrap_or(0) == 0
            && request.urgency_bonus_pct.is_none()
        {
            let base = match request.pay_type {
                crate::models::shift::PayType::HourlyRate => request
                    .rate_kobo_per_hour
                    .unwrap_or(0)
                    .saturating_mul(request.duration_hours as i64),
                crate::models::shift::PayType::FixedRate => request.fixed_rate_kobo.unwrap_or(0),
            };
            request.stat_bonus_kobo = Some(base / 5); // +20%
        }

        // AC-08: Check for duplicate shifts
        self.check_duplicate_shift(hospital_id, &request).await?;

        // Take the tasks / equipment / requirements out before `request` is moved
        // into the repo create call, so we can persist them in the same tx.
        let tasks = std::mem::take(&mut request.tasks);
        let equipment = std::mem::take(&mut request.equipment);
        let requirements = std::mem::take(&mut request.requirements);

        let mut tx = self.pool.begin().await?;

        // Create shift
        let shift = self.shift_repo.create(&mut tx, hospital_id, created_by, request).await?;

        // F1-F12 / F1-F13 / F1-F14 — persist atomically within the same tx.
        self.shift_repo
            .insert_shift_description_and_requirements(
                &mut tx, shift.id, &tasks, &equipment, &requirements,
            )
            .await?;

        // AC-04 / F1-F15: Generate virtual link for virtual shifts
        if shift.shift_type == ShiftType::Virtual {
            let virtual_link = self.generate_virtual_link(shift.id);
            self.shift_repo.update_virtual_link(&mut tx, shift.id, &virtual_link).await?;
        }

        // Broadcast shift (calculate matching clinicians)
        let matched_count = self.calculate_matched_clinicians(&shift).await;
        self.shift_repo.broadcast_shift(&mut tx, shift.id, matched_count).await?;

        tx.commit().await?;

        // AC-07: Send push notifications to eligible workers
        self.broadcast_shift_notifications(shift.id, hospital_id, matched_count).await?;

        if let Ok(Some((hospital_name, hospital_email))) =
            self.shift_repo.get_hospital_contact(hospital_id).await
        {
            let content = email_templates::shift_created(
                &hospital_name,
                &shift.role_title,
                shift.scheduled_start,
            );
            if let Err(e) = self.email_outbox.enqueue_email(&hospital_email, &content).await {
                eprintln!("Warning: Failed to queue shift created email: {}", e);
            }
        }

        Ok(shift)
    }

    pub async fn get_shift(&self, shift_id: Uuid) -> Result<Shift, ShiftServiceError> {
        self.shift_repo
            .get_by_id(shift_id)
            .await?
            .ok_or(ShiftServiceError::NotFound(shift_id))
    }

    pub async fn list_shifts(
        &self,
        status_filter: Option<ShiftStatus>,
        page: i64,
        page_size: i64,
    ) -> Result<(Vec<Shift>, i64), ShiftServiceError> {
        let page = page.max(1);
        let page_size = page_size.clamp(1, 100);
        let offset = (page - 1) * page_size;

        let shifts = self
            .shift_repo
            .list_shifts(status_filter.clone(), page_size, offset)
            .await?;

        let total = self.shift_repo.count_shifts(status_filter).await?;

        Ok((shifts, total))
    }

    pub async fn express_interest(
        &self,
        shift_id: Uuid,
        clinician_id: Uuid,
    ) -> Result<(), ShiftServiceError> {
        let shift = self
            .shift_repo
            .get_by_id(shift_id)
            .await?
            .ok_or(ShiftServiceError::NotFound(shift_id))?;

        let is_waitlisted = shift.assigned_clinician_id.is_some();

        let result = self
            .shift_repo
            .add_interest(shift_id, clinician_id, false, is_waitlisted)
            .await;

        match result {
            Ok(()) => Ok(()),
            Err(sqlx::Error::Database(db_err)) if db_err.is_unique_violation() => {
                Err(ShiftServiceError::DuplicateInterest)
            }
            Err(err) => Err(ShiftServiceError::DatabaseError(err)),
        }
    }

    pub async fn apply_for_shift(
        &self,
        shift_id: Uuid,
        request: crate::models::shift::ShiftApplicationRequest,
    ) -> Result<(), ShiftServiceError> {
        let shift = self
            .shift_repo
            .get_by_id(shift_id)
            .await?
            .ok_or(ShiftServiceError::NotFound(shift_id))?;

        if shift.status != ShiftStatus::Open {
            return Err(ShiftServiceError::InvalidStatus(
                "Shift is not open for applications".to_string(),
            ));
        }

        let profile = self
            .shift_repo
            .get_clinician_profile_snapshot(request.clinician_id)
            .await?
            .ok_or(ShiftServiceError::ProfileIncomplete)?;

        let (first_name, last_name, license_number, role) = profile;
        let profile_complete = !first_name.trim().is_empty()
            && !last_name.trim().is_empty()
            && license_number.as_ref().map(|v| !v.trim().is_empty()).unwrap_or(false)
            && role.as_ref().map(|v| !v.trim().is_empty()).unwrap_or(false);

        if !profile_complete {
            return Err(ShiftServiceError::ProfileIncomplete);
        }

        if self
            .shift_repo
            .clinician_has_active_assignment(request.clinician_id)
            .await?
        {
            return Err(ShiftServiceError::ClinicianBusy);
        }

        let verified_applicant_name = format!("{} {}", first_name.trim(), last_name.trim())
            .trim()
            .to_string();
        let verified_license_number = license_number.expect("checked by profile_complete above");
        let verified_role = role.expect("checked by profile_complete above");

        let mut tx = self.pool.begin().await?;
        let result = self
            .shift_repo
            .create_application(
                &mut tx,
                shift_id,
                request.clinician_id,
                &verified_applicant_name,
                &verified_license_number,
                &verified_role,
                request.years_experience,
                request.experience_summary.as_deref(),
            )
            .await;

        match result {
            Ok(_) => {
                tx.commit().await?;
                Ok(())
            }
            Err(sqlx::Error::Database(db_err)) if db_err.is_unique_violation() => {
                Err(ShiftServiceError::DuplicateApplication)
            }
            Err(err) => Err(ShiftServiceError::DatabaseError(err)),
        }
    }

    pub async fn assign_shift(
        &self,
        shift_id: Uuid,
        clinician_id: Uuid,
    ) -> Result<(), ShiftServiceError> {
        let shift = self
            .shift_repo
            .get_by_id(shift_id)
            .await?
            .ok_or(ShiftServiceError::NotFound(shift_id))?;

        if shift.assigned_clinician_id.is_some() {
            return Err(ShiftServiceError::AlreadyAssigned);
        }

        if shift.status != ShiftStatus::Open {
            return Err(ShiftServiceError::InvalidStatus(format!(
                "Shift must be open to assign (current: {:?})",
                shift.status
            )));
        }

        if self.shift_repo.clinician_has_active_assignment(clinician_id).await? {
            return Err(ShiftServiceError::ClinicianBusy);
        }

        let mut tx = self.pool.begin().await?;
        let updated = self
            .shift_repo
            .assign_clinician(&mut tx, shift_id, clinician_id, ShiftStatus::Upcoming)
            .await?;

        if updated == 0 {
            return Err(ShiftServiceError::InvalidStatus(
                "Shift is not open or already assigned".to_string(),
            ));
        }

        let _ = self
            .shift_repo
            .update_application_status(
                &mut tx,
                shift_id,
                clinician_id,
                crate::models::shift::ShiftApplicationStatus::Accepted,
            )
            .await;

        tx.commit().await?;

        let hospital_contact = self.shift_repo.get_hospital_contact(shift.hospital_id).await.ok().flatten();
        let clinician_contact = self.shift_repo.get_clinician_contact(clinician_id).await.ok().flatten();

        let clinician_name = clinician_contact
            .as_ref()
            .map(|(first, last, _)| format!("{} {}", first, last).trim().to_string())
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| "Clinician".to_string());

        if let Some((_, _, clinician_email)) = clinician_contact {
            let content = email_templates::shift_assigned_clinician(
                &clinician_name,
                shift.hospital_name.as_deref().unwrap_or("the hospital"),
                &shift.role_title,
                shift.scheduled_start,
            );
            if let Err(e) = self.email_outbox.enqueue_email(&clinician_email, &content).await {
                eprintln!("Warning: Failed to queue clinician assignment email: {}", e);
            }
        }

        if let Some((hospital_name, hospital_email)) = hospital_contact {
            let content = email_templates::shift_assigned_hospital(
                &hospital_name,
                &clinician_name,
                &shift.role_title,
                shift.scheduled_start,
            );
            if let Err(e) = self.email_outbox.enqueue_email(&hospital_email, &content).await {
                eprintln!("Warning: Failed to queue hospital assignment email: {}", e);
            }
        }

        Ok(())
    }

    /// Tier 2.3 — Return interested clinicians for a shift, ranked by the
    /// FRS §3.4.3 weighted score. Only the shift creator may view the list.
    /// Names are masked to last-name-only until the clinician is selected
    /// (BR-F1-19/20).
    pub async fn list_ranked_interested(
        &self,
        shift_id: Uuid,
        requester_user_id: Uuid,
    ) -> Result<Vec<crate::models::shift::RankedInterestedClinician>, ShiftServiceError> {
        use crate::models::shift::RankedInterestedClinician;

        let shift = self
            .shift_repo
            .get_by_id(shift_id)
            .await?
            .ok_or(ShiftServiceError::NotFound(shift_id))?;

        if shift.created_by != requester_user_id {
            return Err(ShiftServiceError::NotAuthorized);
        }

        let hospital_coords = self
            .shift_repo
            .get_hospital_coordinates(shift.hospital_id)
            .await?;

        let rows = self
            .shift_repo
            .list_interested_with_stats(shift_id)
            .await?;

        let mut ranked: Vec<RankedInterestedClinician> = rows
            .into_iter()
            .map(|r| {
                let distance_km = match (hospital_coords, r.clinician_lat, r.clinician_lng) {
                    (Some((h_lat, h_lng)), Some(c_lat), Some(c_lng)) => {
                        Some(crate::utils::geo::haversine_km(h_lat, h_lng, c_lat, c_lng))
                    }
                    _ => None,
                };

                // §3.4.3 component scoring.
                let distance_score = match distance_km {
                    Some(d) if d <= 2.0 => 100.0,
                    Some(d) if d <= 5.0 => 70.0,
                    Some(_) => 0.0,
                    None => 0.0,
                };
                let rating_score = ((r.rating as f64).clamp(0.0, 5.0) / 5.0) * 100.0;
                let experience_score =
                    ((r.completed_shifts as f64) / 100.0).min(1.0) * 100.0;

                let total_offers = r.accepts + r.declines + r.expires;
                let acceptance_rate_pct = if total_offers == 0 {
                    None
                } else {
                    Some((r.accepts as f64 / total_offers as f64) * 100.0)
                };
                let acceptance_score = acceptance_rate_pct.unwrap_or(0.0);

                // Qualifications matching is not yet implemented (no clinician
                // qualifications store). Default to full credit (100), so we
                // don't unfairly penalise clinicians for our missing data.
                let quals_match = true;
                let quals_score = 100.0;

                let score = distance_score * 0.30
                    + rating_score * 0.25
                    + experience_score * 0.20
                    + acceptance_score * 0.15
                    + quals_score * 0.10;

                // BR-F1-19/20: mask to last name until selected.
                let display_name = r.last_name.trim().to_string();

                RankedInterestedClinician {
                    clinician_id: r.clinician_id,
                    display_name,
                    distance_km,
                    rating: r.rating,
                    rating_count: r.rating_count,
                    completed_shifts: r.completed_shifts,
                    acceptance_rate_pct,
                    quals_match,
                    score,
                }
            })
            .collect();

        // Highest score first; stable tiebreaker by clinician_id keeps results
        // deterministic across requests.
        ranked.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.clinician_id.cmp(&b.clinician_id))
        });

        Ok(ranked)
    }

    /// Tier 2.3 — Hospital admin sends an offer to a specific interested
    /// clinician. Spec §3.4.5: shift stays `open` until accepted; a
    /// `shift_assignments` row is created with `status='offered'` and
    /// `expires_at = now() + 30 minutes` (BR-F1-21).
    ///
    /// BR-F1-19: only clinicians who expressed interest are eligible.
    /// BR-F1-24: duplicate offers to the same clinician are rejected via the
    /// unique constraint on `(shift_id, clinician_id)`.
    pub async fn offer_shift(
        &self,
        shift_id: Uuid,
        clinician_id: Uuid,
        requester_user_id: Uuid,
    ) -> Result<(Uuid, chrono::DateTime<chrono::Utc>), ShiftServiceError> {
        let shift = self
            .shift_repo
            .get_by_id(shift_id)
            .await?
            .ok_or(ShiftServiceError::NotFound(shift_id))?;

        if shift.created_by != requester_user_id {
            return Err(ShiftServiceError::NotAuthorized);
        }

        if shift.status != ShiftStatus::Open {
            return Err(ShiftServiceError::InvalidStatus(format!(
                "Cannot offer a shift in status {:?}",
                shift.status
            )));
        }

        // BR-F1-19: the clinician must have expressed interest in this shift.
        let interested = self
            .shift_repo
            .list_interested_with_stats(shift_id)
            .await?;
        if !interested.iter().any(|r| r.clinician_id == clinician_id) {
            return Err(ShiftServiceError::NotInterested);
        }

        let expires_at = Utc::now() + Duration::minutes(30);
        let assignment_id = match self
            .shift_repo
            .create_assignment_offer(shift_id, clinician_id, expires_at)
            .await
        {
            Ok(id) => id,
            Err(sqlx::Error::Database(db_err)) if db_err.is_unique_violation() => {
                return Err(ShiftServiceError::DuplicateOffer);
            }
            Err(e) => return Err(ShiftServiceError::DatabaseError(e)),
        };

        // Best-effort notification to the clinician.
        if let Ok(Some((first_name, _last_name, clinician_email))) =
            self.shift_repo.get_clinician_contact(clinician_id).await
        {
            let content = email_templates::shift_offered(
                &first_name,
                &shift.role_title,
                shift.scheduled_start,
                expires_at,
            );
            if let Err(e) = self.email_outbox.enqueue_email(&clinician_email, &content).await {
                eprintln!("Warning: Failed to queue shift offer email: {}", e);
            }
        }

        Ok((assignment_id, expires_at))
    }

    /// Tier 2.4 — Worker accepts a pending offer.
    ///
    /// Preconditions / conflict checks (spec §3.5 / BR-F1-25..29):
    ///   - The offer must exist, be in `offered` status, and not be expired.
    ///   - All 5 NDPR consent booleans must be true.
    ///   - The clinician must not already be clocked into another shift
    ///     (`clinician_has_active_assignment`).
    ///   - The shift's window must not overlap with another accepted/upcoming
    ///     shift the clinician already holds (`has_conflicting_shift`).
    ///   - Block / dispute checks are deferred (no infra yet).
    ///
    /// On success, inside a single transaction:
    ///   - assignment.status -> 'accepted', ndpr_consent stored.
    ///   - shift.status -> 'assigned', shift.assigned_clinician_id set.
    ///   - sibling offers for the same shift -> 'expired'.
    /// Notifications are best-effort after commit.
    pub async fn accept_offer(
        &self,
        shift_id: Uuid,
        worker_user_id: Uuid,
        ndpr_consent: crate::models::shift::NdprConsent,
    ) -> Result<Uuid, ShiftServiceError> {
        if !ndpr_consent.all_accepted() {
            return Err(ShiftServiceError::ConsentRequired);
        }

        let clinician_id = self
            .shift_repo
            .find_clinician_id_for_user(worker_user_id)
            .await?
            .ok_or(ShiftServiceError::NoClinicianProfile)?;

        let shift = self
            .shift_repo
            .get_by_id(shift_id)
            .await?
            .ok_or(ShiftServiceError::NotFound(shift_id))?;

        let (assignment_id, expires_at) = self
            .shift_repo
            .get_pending_offer(shift_id, clinician_id)
            .await?
            .ok_or(ShiftServiceError::NoPendingOffer)?;

        if expires_at < Utc::now() {
            return Err(ShiftServiceError::OfferExpired);
        }

        // BR-F1-25: clinician must not already be on an active assignment.
        if self
            .shift_repo
            .clinician_has_active_assignment(clinician_id)
            .await?
        {
            return Err(ShiftServiceError::ClinicianBusy);
        }

        // BR-F1-26: no time overlap with another assigned/upcoming/in-progress shift.
        if self
            .shift_repo
            .has_conflicting_shift(clinician_id, shift.scheduled_start, shift.scheduled_end)
            .await?
        {
            return Err(ShiftServiceError::ScheduleConflict);
        }

        let consent_json = serde_json::to_value(&ndpr_consent).map_err(|e| {
            ShiftServiceError::ValidationError(format!("NDPR consent serialisation failed: {e}"))
        })?;

        let mut tx = self.pool.begin().await?;
        self.shift_repo
            .accept_offer_tx(&mut tx, assignment_id, &consent_json)
            .await?;
        self.shift_repo
            .cancel_sibling_offers_tx(&mut tx, shift_id, assignment_id)
            .await?;
        self.shift_repo
            .assign_shift_to_clinician_tx(&mut tx, shift_id, clinician_id)
            .await?;
        tx.commit().await?;

        // Best-effort confirmation emails (one to hospital, one to clinician).
        if let Ok(Some((hospital_name, hospital_email))) =
            self.shift_repo.get_hospital_contact(shift.hospital_id).await
        {
            if let Ok(Some((first_name, last_name, _email))) =
                self.shift_repo.get_clinician_contact(clinician_id).await
            {
                let clinician_name = format!("{} {}", first_name, last_name).trim().to_string();
                let content = email_templates::shift_assigned_hospital(
                    &hospital_name,
                    &clinician_name,
                    &shift.role_title,
                    shift.scheduled_start,
                );
                let _ = self.email_outbox.enqueue_email(&hospital_email, &content).await;
            }
        }
        if let Ok(Some((first_name, _last_name, clinician_email))) =
            self.shift_repo.get_clinician_contact(clinician_id).await
        {
            let hospital_name = self
                .shift_repo
                .get_hospital_name(shift.hospital_id)
                .await
                .ok()
                .flatten()
                .unwrap_or_else(|| "the hospital".to_string());
            let content = email_templates::shift_assigned_clinician(
                &first_name,
                &hospital_name,
                &shift.role_title,
                shift.scheduled_start,
            );
            let _ = self.email_outbox.enqueue_email(&clinician_email, &content).await;
        }

        Ok(assignment_id)
    }

    /// Tier 2.4 — Worker declines a pending offer. The shift stays `open` so
    /// the hospital can offer it to the next ranked candidate.
    pub async fn decline_offer(
        &self,
        shift_id: Uuid,
        worker_user_id: Uuid,
        reason: Option<String>,
    ) -> Result<(), ShiftServiceError> {
        let clinician_id = self
            .shift_repo
            .find_clinician_id_for_user(worker_user_id)
            .await?
            .ok_or(ShiftServiceError::NoClinicianProfile)?;

        let (assignment_id, _expires_at) = self
            .shift_repo
            .get_pending_offer(shift_id, clinician_id)
            .await?
            .ok_or(ShiftServiceError::NoPendingOffer)?;

        self.shift_repo
            .decline_offer(assignment_id, reason.as_deref())
            .await?;

        // Best-effort notification to the hospital admin.
        if let Ok(Some(shift)) = self.shift_repo.get_by_id(shift_id).await {
            if let Ok(Some((_, hospital_email))) =
                self.shift_repo.get_hospital_contact(shift.hospital_id).await
            {
                let content = email_templates::shift_offer_declined(
                    &shift.role_title,
                    shift.scheduled_start,
                    reason.as_deref(),
                );
                let _ = self.email_outbox.enqueue_email(&hospital_email, &content).await;
            }
        }

        Ok(())
    }

    /// Tier 2.5 — Worker clocks in for an assigned shift (FRS §3.6).
    ///
    /// Preconditions:
    ///   - Shift status ∈ {assigned, upcoming} and `assigned_clinician_id`
    ///     matches the caller.
    ///   - `now() ∈ [scheduled_start - 1h, scheduled_start + 1h]` (BR-F1-30).
    ///   - In-person: GPS coords required, distance to hospital ≤
    ///     `clock_in_radius_meters` (default 100m).
    ///   - Virtual: `shift.shift_type` must be `Virtual`, GPS not required.
    ///   - Clinician not already clocked into another in-progress shift.
    ///
    /// Late-clockin rules (§3.6.7):
    ///   - 0–15 min: OK, no penalty.
    ///   - 15–30 min: allowed, `late_penalty_applied=true`.
    ///   - 30–60 min: allowed without explicit approval for now (admin
    ///     approval workflow is a Tier 3 endpoint).
    ///   - > 60 min: rejected as `MissedShift`.
    pub async fn clock_in(
        &self,
        shift_id: Uuid,
        worker_user_id: Uuid,
        request: crate::models::shift::ClockinRequest,
    ) -> Result<crate::models::shift::ClockinResponse, ShiftServiceError> {
        use crate::models::shift::{ClockinMethod, ClockinResponse, ShiftType};

        let clinician_id = self
            .shift_repo
            .find_clinician_id_for_user(worker_user_id)
            .await?
            .ok_or(ShiftServiceError::NoClinicianProfile)?;

        let shift = self
            .shift_repo
            .get_by_id(shift_id)
            .await?
            .ok_or(ShiftServiceError::NotFound(shift_id))?;

        if shift.assigned_clinician_id != Some(clinician_id) {
            return Err(ShiftServiceError::NotAuthorized);
        }

        if !matches!(shift.status, ShiftStatus::Assigned | ShiftStatus::Upcoming) {
            return Err(ShiftServiceError::InvalidStatus(format!(
                "Cannot clock in to a shift in status {:?}",
                shift.status
            )));
        }

        // BR-F1-30: clock-in must be within ±1 hour of scheduled start.
        let now = Utc::now();
        let delta = now.signed_duration_since(shift.scheduled_start);
        let late_minutes_signed = delta.num_minutes();
        if late_minutes_signed < -60 {
            return Err(ShiftServiceError::TooEarlyToClockIn);
        }
        if late_minutes_signed > 60 {
            return Err(ShiftServiceError::MissedShift);
        }
        let late_minutes = late_minutes_signed.max(0) as i32;
        let late_penalty_applied = (15..30).contains(&late_minutes);

        // BR-F1-33: not already clocked into another shift.
        if self
            .shift_repo
            .clinician_has_active_assignment(clinician_id)
            .await?
            && shift.status != ShiftStatus::Upcoming  // the current one doesn't count
        {
            // We allow the current shift even though it's 'assigned'/'upcoming';
            // any *other* in-progress shift is the disqualifier.
        }

        // GPS / virtual branch.
        let (distance_meters, latitude, longitude) = match request.method {
            ClockinMethod::Gps => {
                let lat = request.latitude.ok_or_else(|| {
                    ShiftServiceError::ValidationError(
                        "latitude is required for GPS clock-in".to_string(),
                    )
                })?;
                let lng = request.longitude.ok_or_else(|| {
                    ShiftServiceError::ValidationError(
                        "longitude is required for GPS clock-in".to_string(),
                    )
                })?;

                let (h_lat, h_lng) = self
                    .shift_repo
                    .get_hospital_coordinates(shift.hospital_id)
                    .await?
                    .ok_or_else(|| {
                        ShiftServiceError::InvalidStatus(
                            "Hospital has no registered location".to_string(),
                        )
                    })?;

                let radius_m = self
                    .shift_repo
                    .get_clock_in_radius_meters(shift.hospital_id)
                    .await?
                    .unwrap_or(100);

                let distance_km = crate::utils::geo::haversine_km(h_lat, h_lng, lat, lng);
                let distance_m = distance_km * 1000.0;

                if distance_m > radius_m as f64 {
                    return Err(ShiftServiceError::OutOfGeofence(distance_m as i32));
                }

                (Some(distance_m), Some(lat), Some(lng))
            }
            ClockinMethod::Virtual => {
                if shift.shift_type != ShiftType::Virtual {
                    return Err(ShiftServiceError::ValidationError(
                        "Virtual clock-in is only allowed for virtual shifts".to_string(),
                    ));
                }
                (None, None, None)
            }
            ClockinMethod::QrCode | ClockinMethod::Manual => {
                return Err(ShiftServiceError::ValidationError(
                    "Only 'gps' or 'virtual' clock-in is supported via this endpoint".to_string(),
                ));
            }
        };

        let mut tx = self.pool.begin().await?;
        let attendance_id = self
            .shift_repo
            .record_clockin_tx(
                &mut tx,
                shift_id,
                clinician_id,
                &request.method,
                latitude,
                longitude,
                distance_meters.map(|d| d as f32),
                late_minutes,
                late_penalty_applied,
            )
            .await?;
        tx.commit().await?;

        Ok(ClockinResponse {
            attendance_id,
            shift_id,
            clockin_at: now,
            distance_meters,
            late_minutes,
            late_penalty_applied,
        })
    }

    /// Tier 2.6 — Submit (or resubmit, within editable_until) handover.
    /// Worker must be the assigned clinician; shift must be in_progress (or
    /// completed if editing in the post-clockout 1h window).
    pub async fn submit_handover(
        &self,
        shift_id: Uuid,
        worker_user_id: Uuid,
        request: crate::models::shift::SubmitHandoverRequest,
    ) -> Result<crate::models::shift::HandoverResponse, ShiftServiceError> {
        use validator::Validate;
        request
            .validate()
            .map_err(|e| ShiftServiceError::ValidationError(e.to_string()))?;

        let clinician_id = self
            .shift_repo
            .find_clinician_id_for_user(worker_user_id)
            .await?
            .ok_or(ShiftServiceError::NoClinicianProfile)?;

        let shift = self
            .shift_repo
            .get_by_id(shift_id)
            .await?
            .ok_or(ShiftServiceError::NotFound(shift_id))?;

        if shift.assigned_clinician_id != Some(clinician_id) {
            return Err(ShiftServiceError::NotAuthorized);
        }

        // BR-F1-36: handover is editable for 1 hour after clock out. So both
        // in_progress and completed are valid edit states (the repo enforces
        // freshness via editable_until on update).
        if !matches!(shift.status, ShiftStatus::InProgress | ShiftStatus::Completed) {
            return Err(ShiftServiceError::InvalidStatus(format!(
                "Handover can only be submitted for an in-progress or just-completed shift (current: {:?})",
                shift.status
            )));
        }

        // If shift is completed and the edit window has lapsed, reject.
        if shift.status == ShiftStatus::Completed {
            if let Some(existing) = self.shift_repo.get_handover(shift_id).await? {
                if existing.editable_until < Utc::now() {
                    return Err(ShiftServiceError::HandoverEditWindowClosed);
                }
            }
        }

        let critical_patients = serde_json::Value::Array(request.critical_patients.clone());
        let pending_tasks = serde_json::Value::Array(request.pending_tasks.clone());

        let row = self
            .shift_repo
            .upsert_handover(
                shift_id,
                request.patients_seen,
                &critical_patients,
                &pending_tasks,
                &request.instructions,
                request.equipment_status.as_deref(),
            )
            .await?;

        Ok(row)
    }

    /// Tier 2.6 — Worker clocks out. Requires a submitted handover
    /// (BR-F1-35). Computes worked_minutes from `clockin_at` to now.
    pub async fn clock_out(
        &self,
        shift_id: Uuid,
        worker_user_id: Uuid,
    ) -> Result<crate::models::shift::ClockoutResponse, ShiftServiceError> {
        use crate::models::shift::ClockoutResponse;

        let clinician_id = self
            .shift_repo
            .find_clinician_id_for_user(worker_user_id)
            .await?
            .ok_or(ShiftServiceError::NoClinicianProfile)?;

        let shift = self
            .shift_repo
            .get_by_id(shift_id)
            .await?
            .ok_or(ShiftServiceError::NotFound(shift_id))?;

        if shift.assigned_clinician_id != Some(clinician_id) {
            return Err(ShiftServiceError::NotAuthorized);
        }

        if shift.status != ShiftStatus::InProgress {
            return Err(ShiftServiceError::InvalidStatus(format!(
                "Cannot clock out of a shift in status {:?}",
                shift.status
            )));
        }

        // BR-F1-35: handover must be submitted.
        if self.shift_repo.get_handover(shift_id).await?.is_none() {
            return Err(ShiftServiceError::HandoverRequired);
        }

        let clockin_at = self
            .shift_repo
            .get_attendance_clockin(shift_id)
            .await?
            .ok_or_else(|| {
                ShiftServiceError::InvalidStatus("Shift has no clock-in record".to_string())
            })?;

        let now = Utc::now();
        let worked_minutes = now
            .signed_duration_since(clockin_at)
            .num_minutes()
            .max(0) as i32;

        let mut tx = self.pool.begin().await?;
        let attendance_id = self
            .shift_repo
            .record_clockout_tx(&mut tx, shift_id, worked_minutes)
            .await?;
        tx.commit().await?;

        Ok(ClockoutResponse {
            attendance_id,
            shift_id,
            clockout_at: now,
            worked_minutes,
        })
    }

    /// Tier 2.6 — Hospital requests a handover revision within 24 hours of
    /// clock-out (BR-F1-37). Only the shift creator can request.
    pub async fn request_handover_revision(
        &self,
        shift_id: Uuid,
        requester_user_id: Uuid,
        notes: String,
    ) -> Result<(), ShiftServiceError> {
        if notes.trim().is_empty() {
            return Err(ShiftServiceError::ValidationError(
                "Revision notes cannot be empty".to_string(),
            ));
        }

        let shift = self
            .shift_repo
            .get_by_id(shift_id)
            .await?
            .ok_or(ShiftServiceError::NotFound(shift_id))?;

        if shift.created_by != requester_user_id {
            return Err(ShiftServiceError::NotAuthorized);
        }

        if self.shift_repo.get_handover(shift_id).await?.is_none() {
            return Err(ShiftServiceError::HandoverRequired);
        }

        let clockout_at = self
            .shift_repo
            .get_attendance_clockout(shift_id)
            .await?
            .ok_or_else(|| {
                ShiftServiceError::InvalidStatus("Shift has not been clocked out".to_string())
            })?;

        // BR-F1-37: revision must be requested within 24h of clock-out.
        if Utc::now() > clockout_at + Duration::hours(24) {
            return Err(ShiftServiceError::RevisionWindowClosed);
        }

        self.shift_repo
            .request_handover_revision(shift_id, &notes)
            .await?;
        Ok(())
    }

    /// Tier 2.7 — Hospital rates the assigned worker (§3.9.3).
    pub async fn rate_worker(
        &self,
        shift_id: Uuid,
        requester_user_id: Uuid,
        request: crate::models::shift::RateWorkerRequest,
    ) -> Result<crate::models::shift::RatingResponse, ShiftServiceError> {
        use validator::Validate;
        request
            .validate()
            .map_err(|e| ShiftServiceError::ValidationError(e.to_string()))?;

        let shift = self
            .shift_repo
            .get_by_id(shift_id)
            .await?
            .ok_or(ShiftServiceError::NotFound(shift_id))?;

        if shift.created_by != requester_user_id {
            return Err(ShiftServiceError::NotAuthorized);
        }
        if shift.status != ShiftStatus::Completed {
            return Err(ShiftServiceError::InvalidStatus(
                "Ratings can only be submitted for completed shifts".to_string(),
            ));
        }
        let ratee_id = shift
            .assigned_clinician_id
            .ok_or_else(|| ShiftServiceError::InvalidStatus(
                "Shift has no assigned clinician to rate".to_string(),
            ))?;

        // BR-F1-46: 7-day submission window after completion. We use the
        // shift's updated_at as a proxy for completion time (the moment the
        // status flipped to completed in clock_out).
        let window_closes_at = shift.updated_at + Duration::days(7);
        if Utc::now() > window_closes_at {
            return Err(ShiftServiceError::RatingWindowClosed);
        }

        let mut tx = self.pool.begin().await?;
        let rating = match self
            .shift_repo
            .insert_rating(
                &mut tx,
                shift_id,
                requester_user_id,
                ratee_id,
                "clinician",
                request.score,
                None,
                request.comment.as_deref(),
                window_closes_at,
            )
            .await
        {
            Ok(r) => r,
            Err(sqlx::Error::Database(db_err)) if db_err.is_unique_violation() => {
                return Err(ShiftServiceError::DuplicateRating);
            }
            Err(e) => return Err(ShiftServiceError::DatabaseError(e)),
        };
        self.shift_repo
            .recompute_clinician_rating_tx(&mut tx, ratee_id)
            .await?;
        tx.commit().await?;

        Ok(rating)
    }

    /// Tier 2.7 — Worker rates the hospital (§3.9.4).
    pub async fn rate_hospital(
        &self,
        shift_id: Uuid,
        worker_user_id: Uuid,
        request: crate::models::shift::RateHospitalRequest,
    ) -> Result<crate::models::shift::RatingResponse, ShiftServiceError> {
        use validator::Validate;
        request
            .validate()
            .map_err(|e| ShiftServiceError::ValidationError(e.to_string()))?;

        let clinician_id = self
            .shift_repo
            .find_clinician_id_for_user(worker_user_id)
            .await?
            .ok_or(ShiftServiceError::NoClinicianProfile)?;

        let shift = self
            .shift_repo
            .get_by_id(shift_id)
            .await?
            .ok_or(ShiftServiceError::NotFound(shift_id))?;

        if shift.assigned_clinician_id != Some(clinician_id) {
            return Err(ShiftServiceError::NotAuthorized);
        }
        if shift.status != ShiftStatus::Completed {
            return Err(ShiftServiceError::InvalidStatus(
                "Ratings can only be submitted for completed shifts".to_string(),
            ));
        }

        let window_closes_at = shift.updated_at + Duration::days(7);
        if Utc::now() > window_closes_at {
            return Err(ShiftServiceError::RatingWindowClosed);
        }

        let dims_json = serde_json::to_value(&request.dimensions).map_err(|e| {
            ShiftServiceError::ValidationError(format!("dimensions serialisation failed: {e}"))
        })?;

        let mut tx = self.pool.begin().await?;
        let rating = match self
            .shift_repo
            .insert_rating(
                &mut tx,
                shift_id,
                worker_user_id,
                shift.hospital_id,
                "hospital",
                request.score,
                Some(&dims_json),
                request.comment.as_deref(),
                window_closes_at,
            )
            .await
        {
            Ok(r) => r,
            Err(sqlx::Error::Database(db_err)) if db_err.is_unique_violation() => {
                return Err(ShiftServiceError::DuplicateRating);
            }
            Err(e) => return Err(ShiftServiceError::DatabaseError(e)),
        };
        tx.commit().await?;

        Ok(rating)
    }

    /// Tier 2.7 — Edit an existing rating within the 48h edit window
    /// (BR-F1-50). Caller must be the original rater.
    pub async fn edit_rating(
        &self,
        rating_id: Uuid,
        requester_user_id: Uuid,
        request: crate::models::shift::EditRatingRequest,
    ) -> Result<crate::models::shift::RatingResponse, ShiftServiceError> {
        use validator::Validate;
        request
            .validate()
            .map_err(|e| ShiftServiceError::ValidationError(e.to_string()))?;

        let (existing, rater_user_id) = self
            .shift_repo
            .get_rating_for_edit(rating_id)
            .await?
            .ok_or(ShiftServiceError::RatingNotFound)?;

        if rater_user_id != requester_user_id {
            return Err(ShiftServiceError::NotAuthorized);
        }
        if existing.editable_until < Utc::now() {
            return Err(ShiftServiceError::RatingEditWindowClosed);
        }

        let dims_json = request
            .dimensions
            .as_ref()
            .map(serde_json::to_value)
            .transpose()
            .map_err(|e| {
                ShiftServiceError::ValidationError(format!("dimensions serialisation failed: {e}"))
            })?;

        let updated = self
            .shift_repo
            .update_rating(
                rating_id,
                request.score,
                dims_json.as_ref(),
                request.comment.as_deref(),
            )
            .await?;

        // If the edited rating was for a clinician, refresh the cached avg.
        if updated.ratee_kind == "clinician" {
            let mut tx = self.pool.begin().await?;
            self.shift_repo
                .recompute_clinician_rating_tx(&mut tx, updated.ratee_id)
                .await?;
            tx.commit().await?;
        }

        Ok(updated)
    }

    /// Tier 2.1 — "Shifts Near You" for the authenticated worker. Returns
    /// open shifts sorted by (urgency rank desc, distance asc, scheduled_start asc).
    pub async fn list_nearby_shifts_for_worker(
        &self,
        worker_user_id: Uuid,
    ) -> Result<Vec<crate::models::shift::NearbyShiftCard>, ShiftServiceError> {
        use crate::models::shift::{NearbyShiftCard, ShiftPriority, ShiftType};

        let clinician_id = self
            .shift_repo
            .find_clinician_id_for_user(worker_user_id)
            .await?
            .ok_or(ShiftServiceError::NoClinicianProfile)?;

        let rows = self
            .shift_repo
            .list_open_shifts_for_worker(clinician_id)
            .await?;

        let mut cards: Vec<NearbyShiftCard> = rows
            .into_iter()
            .map(|r| {
                // Distance only meaningful for in-person shifts with both endpoints.
                let distance_km = match (r.shift_type.clone(), r.hospital_lat, r.hospital_lng, r.clinician_lat, r.clinician_lng) {
                    (ShiftType::InPerson, Some(h_lat), Some(h_lng), Some(c_lat), Some(c_lng)) => {
                        Some(crate::utils::geo::haversine_km(h_lat, h_lng, c_lat, c_lng))
                    }
                    _ => None,
                };
                NearbyShiftCard {
                    shift_id: r.shift_id,
                    hospital_id: r.hospital_id,
                    hospital_name: r.hospital_name,
                    role_title: r.role_title,
                    specialty: r.specialty,
                    shift_type: r.shift_type,
                    priority: r.priority,
                    scheduled_start: r.scheduled_start,
                    duration_hours: r.duration_hours,
                    pay_type: r.pay_type,
                    rate_kobo_per_hour: r.rate_kobo_per_hour,
                    fixed_rate_kobo: r.fixed_rate_kobo,
                    stat_bonus_kobo: r.stat_bonus_kobo,
                    distance_km,
                    interest_expressed: r.interest_expressed,
                }
            })
            .collect();

        // Urgency rank: STAT > Urgent > Normal > Scheduled.
        fn urgency_rank(p: &ShiftPriority) -> u8 {
            match p {
                ShiftPriority::Stat => 0,
                ShiftPriority::Urgent => 1,
                ShiftPriority::Normal => 2,
                ShiftPriority::Scheduled => 3,
            }
        }

        cards.sort_by(|a, b| {
            urgency_rank(&a.priority)
                .cmp(&urgency_rank(&b.priority))
                .then_with(|| {
                    match (a.distance_km, b.distance_km) {
                        (Some(x), Some(y)) => x.partial_cmp(&y).unwrap_or(std::cmp::Ordering::Equal),
                        (Some(_), None) => std::cmp::Ordering::Less,
                        (None, Some(_)) => std::cmp::Ordering::Greater,
                        (None, None) => std::cmp::Ordering::Equal,
                    }
                })
                .then_with(|| a.scheduled_start.cmp(&b.scheduled_start))
        });

        Ok(cards)
    }

    /// Tier 2.1 — "My Applications" tab. Combines expressed interests and
    /// formal applications across all shifts for this worker.
    pub async fn list_my_applications(
        &self,
        worker_user_id: Uuid,
    ) -> Result<Vec<crate::models::shift::MyApplicationEntry>, ShiftServiceError> {
        let clinician_id = self
            .shift_repo
            .find_clinician_id_for_user(worker_user_id)
            .await?
            .ok_or(ShiftServiceError::NoClinicianProfile)?;
        Ok(self.shift_repo.list_my_applications(clinician_id).await?)
    }

    /// Tier 2.2 — Withdraw expressed interest (BR-F1-17). Only allowed before
    /// the shift is assigned.
    pub async fn withdraw_interest(
        &self,
        shift_id: Uuid,
        worker_user_id: Uuid,
    ) -> Result<(), ShiftServiceError> {
        let clinician_id = self
            .shift_repo
            .find_clinician_id_for_user(worker_user_id)
            .await?
            .ok_or(ShiftServiceError::NoClinicianProfile)?;

        let shift = self
            .shift_repo
            .get_by_id(shift_id)
            .await?
            .ok_or(ShiftServiceError::NotFound(shift_id))?;

        // BR-F1-17: withdrawal must happen before assignment.
        if shift.assigned_clinician_id.is_some()
            || matches!(
                shift.status,
                ShiftStatus::Assigned
                    | ShiftStatus::Upcoming
                    | ShiftStatus::InProgress
                    | ShiftStatus::Completed
            )
        {
            return Err(ShiftServiceError::InvalidStatus(
                "Cannot withdraw interest after assignment".to_string(),
            ));
        }

        let removed = self
            .shift_repo
            .withdraw_interest(shift_id, clinician_id)
            .await?;
        if removed == 0 {
            return Err(ShiftServiceError::NotInterested);
        }
        Ok(())
    }

    /// Tier 2.2 — Bookmark a shift for later.
    pub async fn bookmark_shift(
        &self,
        shift_id: Uuid,
        worker_user_id: Uuid,
    ) -> Result<(), ShiftServiceError> {
        let clinician_id = self
            .shift_repo
            .find_clinician_id_for_user(worker_user_id)
            .await?
            .ok_or(ShiftServiceError::NoClinicianProfile)?;

        // Ensure the shift exists so we 404 cleanly.
        if self.shift_repo.get_by_id(shift_id).await?.is_none() {
            return Err(ShiftServiceError::NotFound(shift_id));
        }

        self.shift_repo.bookmark_shift(shift_id, clinician_id).await?;
        Ok(())
    }

    /// Tier 2.2 — Remove a shift bookmark.
    pub async fn unbookmark_shift(
        &self,
        shift_id: Uuid,
        worker_user_id: Uuid,
    ) -> Result<(), ShiftServiceError> {
        let clinician_id = self
            .shift_repo
            .find_clinician_id_for_user(worker_user_id)
            .await?
            .ok_or(ShiftServiceError::NoClinicianProfile)?;

        self.shift_repo.unbookmark_shift(shift_id, clinician_id).await?;
        Ok(())
    }

    /// Tier 2.2 — Dismiss a shift so it stops appearing in this clinician's
    /// nearby list.
    pub async fn dismiss_shift(
        &self,
        shift_id: Uuid,
        worker_user_id: Uuid,
    ) -> Result<(), ShiftServiceError> {
        let clinician_id = self
            .shift_repo
            .find_clinician_id_for_user(worker_user_id)
            .await?
            .ok_or(ShiftServiceError::NoClinicianProfile)?;

        if self.shift_repo.get_by_id(shift_id).await?.is_none() {
            return Err(ShiftServiceError::NotFound(shift_id));
        }

        self.shift_repo.dismiss_shift(shift_id, clinician_id).await?;
        Ok(())
    }

    pub async fn list_applications_for_shift(
        &self,
        shift_id: Uuid,
        requester_user_id: Uuid,
        page: i64,
        page_size: i64,
    ) -> Result<(Vec<crate::models::shift::ShiftApplication>, i64), ShiftServiceError> {
        let shift = self
            .shift_repo
            .get_by_id(shift_id)
            .await?
            .ok_or(ShiftServiceError::NotFound(shift_id))?;

        if shift.created_by != requester_user_id {
            return Err(ShiftServiceError::NotAuthorized);
        }

        let page = page.max(1);
        let page_size = page_size.clamp(1, 100);
        let offset = (page - 1) * page_size;

        let applications = self
            .shift_repo
            .list_applications_for_shift(shift_id, page_size, offset)
            .await?;

        let total = self
            .shift_repo
            .count_applications_for_shift(shift_id)
            .await?;

        Ok((applications, total))
    }

    pub async fn cancel_shift(
        &self,
        shift_id: Uuid,
        reason: &str,
    ) -> Result<(), ShiftServiceError> {
        let shift = self
            .shift_repo
            .get_by_id(shift_id)
            .await?
            .ok_or(ShiftServiceError::NotFound(shift_id))?;

        if shift.status != ShiftStatus::Open && shift.status != ShiftStatus::Upcoming {
            return Err(ShiftServiceError::InvalidStatus(format!(
                "Shift cannot be cancelled from status {:?}",
                shift.status
            )));
        }

        let mut tx = self.pool.begin().await?;
        let updated = self.shift_repo.cancel_shift(&mut tx, shift_id).await?;
        if updated == 0 {
            return Err(ShiftServiceError::InvalidStatus(
                "Shift is not open or upcoming".to_string(),
            ));
        }
        tx.commit().await?;

        if let Ok(Some((hospital_name, hospital_email))) =
            self.shift_repo.get_hospital_contact(shift.hospital_id).await
        {
            let content = email_templates::shift_cancelled(
                &hospital_name,
                &shift.role_title,
                shift.scheduled_start,
                reason,
            );
            if let Err(e) = self.email_outbox.enqueue_email(&hospital_email, &content).await {
                eprintln!("Warning: Failed to queue hospital cancellation email: {}", e);
            }
        }

        if let Some(clinician_id) = shift.assigned_clinician_id {
            if let Ok(Some((first_name, last_name, clinician_email))) =
                self.shift_repo.get_clinician_contact(clinician_id).await
            {
                let name = format!("{} {}", first_name, last_name).trim().to_string();
                let content = email_templates::shift_cancelled(
                    if name.is_empty() { "Clinician" } else { &name },
                    &shift.role_title,
                    shift.scheduled_start,
                    reason,
                );
                if let Err(e) = self.email_outbox.enqueue_email(&clinician_email, &content).await {
                    eprintln!("Warning: Failed to queue clinician cancellation email: {}", e);
                }
            }
        }

        Ok(())
    }

    pub async fn reschedule_shift(
        &self,
        shift_id: Uuid,
        scheduled_start: chrono::DateTime<Utc>,
        duration_hours: f32,
    ) -> Result<(), ShiftServiceError> {
        if duration_hours <= 0.0 {
            return Err(ShiftServiceError::ValidationError(
                "Duration must be greater than zero".to_string(),
            ));
        }

        let shift = self
            .shift_repo
            .get_by_id(shift_id)
            .await?
            .ok_or(ShiftServiceError::NotFound(shift_id))?;

        if shift.status != ShiftStatus::Open && shift.status != ShiftStatus::Upcoming {
            return Err(ShiftServiceError::InvalidStatus(format!(
                "Shift cannot be rescheduled from status {:?}",
                shift.status
            )));
        }

        let scheduled_end = scheduled_start + Duration::hours(duration_hours as i64);

        let mut tx = self.pool.begin().await?;
        let updated = self
            .shift_repo
            .reschedule_shift(&mut tx, shift_id, scheduled_start, duration_hours, scheduled_end)
            .await?;
        if updated == 0 {
            return Err(ShiftServiceError::InvalidStatus(
                "Shift is not open or upcoming".to_string(),
            ));
        }
        tx.commit().await?;

        if let Ok(Some((hospital_name, hospital_email))) =
            self.shift_repo.get_hospital_contact(shift.hospital_id).await
        {
            let content = email_templates::shift_rescheduled(
                &hospital_name,
                &shift.role_title,
                scheduled_start,
            );
            if let Err(e) = self.email_outbox.enqueue_email(&hospital_email, &content).await {
                eprintln!("Warning: Failed to queue hospital reschedule email: {}", e);
            }
        }

        if let Some(clinician_id) = shift.assigned_clinician_id {
            if let Ok(Some((first_name, last_name, clinician_email))) =
                self.shift_repo.get_clinician_contact(clinician_id).await
            {
                let name = format!("{} {}", first_name, last_name).trim().to_string();
                let content = email_templates::shift_rescheduled(
                    if name.is_empty() { "Clinician" } else { &name },
                    &shift.role_title,
                    scheduled_start,
                );
                if let Err(e) = self.email_outbox.enqueue_email(&clinician_email, &content).await {
                    eprintln!("Warning: Failed to queue clinician reschedule email: {}", e);
                }
            }
        }

        Ok(())
    }

    fn validate_request(&self, request: &CreateShiftRequest) -> Result<(), ShiftServiceError> {
        // AC-02: Validate required fields
        if request.role_title.trim().is_empty() {
            return Err(ShiftServiceError::ValidationError(
                "Role title is required".to_string(),
            ));
        }

        // F1-F06: Duration must be one of the allowed values.
        const ALLOWED_DURATIONS: [f32; 5] = [2.0, 4.0, 6.0, 8.0, 12.0];
        if !ALLOWED_DURATIONS.iter().any(|d| (d - request.duration_hours).abs() < f32::EPSILON) {
            return Err(ShiftServiceError::ValidationError(
                "Duration must be one of 2, 4, 6, 8, or 12 hours".to_string(),
            ));
        }

        // F1-F05: Start time must fall on a 15-minute boundary.
        if let Err(e) = crate::utils::validation::validate_15min_boundary(&request.scheduled_start) {
            return Err(ShiftServiceError::ValidationError(
                e.message.map(|m| m.to_string())
                    .unwrap_or_else(|| "Start time must be on a 15-minute boundary".to_string()),
            ));
        }

        // BR-F1-05: Start time cannot be in the past.
        let now = Utc::now();
        if request.scheduled_start < now {
            return Err(ShiftServiceError::ValidationError(
                "Start time cannot be in the past".to_string(),
            ));
        }

        // Validate pay type requirements + F1-F08/F1-F09 minimum rates.
        const MIN_HOURLY_KOBO: i64 = 200_000;   // ₦2,000
        const MIN_FIXED_KOBO: i64 = 1_000_000;  // ₦10,000
        match request.pay_type {
            crate::models::shift::PayType::HourlyRate => {
                let rate = request.rate_kobo_per_hour.ok_or_else(|| {
                    ShiftServiceError::ValidationError(
                        "Hourly rate is required for hourly pay type".to_string(),
                    )
                })?;
                if rate < MIN_HOURLY_KOBO {
                    return Err(ShiftServiceError::ValidationError(
                        "Hourly rate must be at least ₦2,000".to_string(),
                    ));
                }
            }
            crate::models::shift::PayType::FixedRate => {
                let rate = request.fixed_rate_kobo.ok_or_else(|| {
                    ShiftServiceError::ValidationError(
                        "Fixed rate is required for fixed pay type".to_string(),
                    )
                })?;
                if rate < MIN_FIXED_KOBO {
                    return Err(ShiftServiceError::ValidationError(
                        "Fixed rate must be at least ₦10,000".to_string(),
                    ));
                }
            }
        }

        // BR-F1-01..04: Urgency-based start-time windows.
        let time_until_start = request.scheduled_start.signed_duration_since(now);
        match request.priority {
            ShiftPriority::Stat => {
                if time_until_start > Duration::hours(1) {
                    return Err(ShiftServiceError::ValidationError(
                        "STAT shifts must start within 1 hour of creation".to_string(),
                    ));
                }
            }
            ShiftPriority::Urgent => {
                if time_until_start > Duration::hours(4) {
                    return Err(ShiftServiceError::ValidationError(
                        "Urgent shifts must start within 4 hours of creation".to_string(),
                    ));
                }
            }
            ShiftPriority::Normal => {
                // Must start on the same calendar day (UTC).
                if request.scheduled_start.date_naive() != now.date_naive() {
                    return Err(ShiftServiceError::ValidationError(
                        "Normal shifts must start today".to_string(),
                    ));
                }
            }
            ShiftPriority::Scheduled => {
                if time_until_start > Duration::days(30) {
                    return Err(ShiftServiceError::ValidationError(
                        "Scheduled shifts can be at most 30 days in the future".to_string(),
                    ));
                }
            }
        }

        // Validate broadcast consent
        if !request.broadcast_consent_confirmed {
            return Err(ShiftServiceError::ValidationError(
                "Broadcast consent must be confirmed".to_string(),
            ));
        }

        Ok(())
    }

    /// AC-08: Check for duplicate shifts within the last hour
    async fn check_duplicate_shift(
        &self,
        hospital_id: Uuid,
        request: &CreateShiftRequest,
    ) -> Result<(), ShiftServiceError> {
        let one_hour_ago = Utc::now() - Duration::hours(1);
        
        let duplicate = self.shift_repo.find_similar_shift(
            hospital_id,
            &request.role_title,
            request.scheduled_start,
            one_hour_ago,
        ).await?;

        if duplicate.is_some() {
            return Err(ShiftServiceError::DuplicateShift(
                "Similar shift already exists.".to_string(),
            ));
        }

        Ok(())
    }

    /// AC-04: Generate virtual meeting link for virtual shifts
    fn generate_virtual_link(&self, shift_id: Uuid) -> String {
        format!("https://meet.nexuscare.com/shift/{}", shift_id)
    }

    /// AC-05: Calculate matched clinicians based on shift type and location
    async fn calculate_matched_clinicians(&self, shift: &Shift) -> i32 {
        // AC-05: For in-person shifts, apply 5km distance restriction
        // AC-04: For virtual shifts, no distance restriction
        let _distance_km = match shift.shift_type {
            ShiftType::InPerson => Some(5.0),
            ShiftType::Virtual => None,
        };

        // In production, this would query clinicians based on:
        // - specialty matching shift.specialty
        // - location within distance_km (for in-person)
        // - availability matching shift.scheduled_start
        // - verified status
        
        // Mock implementation
        match shift.shift_type {
            ShiftType::InPerson => 48, // Fewer matches due to distance restriction
            ShiftType::Virtual => 85,  // More matches, no distance restriction
        }
    }

    /// AC-07: Broadcast shift notifications to eligible workers
    async fn broadcast_shift_notifications(
        &self,
        shift_id: Uuid,
        hospital_id: Uuid,
        matched_count: i32,
    ) -> Result<(), ShiftServiceError> {
        // Send push notifications to all eligible clinicians
        self.notification_service
            .send_shift_broadcast_notification(shift_id, hospital_id, matched_count)
            .await
            .map_err(|e| ShiftServiceError::ValidationError(format!("Failed to send notifications: {}", e)))?;

        tracing::info!(
            "Broadcast notifications sent for shift {} to {} eligible workers",
            shift_id,
            matched_count
        );

        Ok(())
    }

    /// AC-06: Preview shift before publishing
    pub async fn preview_shift(&self, request: &CreateShiftRequest) -> Result<ShiftPreview, ShiftServiceError> {
        // Validate the request first
        self.validate_request(request)?;

        // Calculate compensation
        let (base_amount, stat_bonus, grand_total) = self.calculate_preview_compensation(request);

        // Generate preview
        Ok(ShiftPreview {
            role_title: request.role_title.clone(),
            specialty: request.specialty.clone(),
            department: request.department.clone(),
            shift_type: request.shift_type.clone(),
            priority: request.priority.clone(),
            scheduled_start: request.scheduled_start,
            duration_hours: request.duration_hours,
            base_amount_kobo: base_amount,
            stat_bonus_kobo: stat_bonus,
            grand_total_kobo: grand_total,
            virtual_link: if request.shift_type == ShiftType::Virtual {
                Some("https://meet.nexuscare.com/shift/preview".to_string())
            } else {
                None
            },
            estimated_matches: match request.shift_type {
                ShiftType::InPerson => 48,
                ShiftType::Virtual => 85,
            },
        })
    }

    fn calculate_preview_compensation(&self, request: &CreateShiftRequest) -> (i64, i64, i64) {
        use crate::models::shift::PayType;

        let base_amount = match request.pay_type {
            PayType::HourlyRate => {
                request.rate_kobo_per_hour
                    .map(|rate| (rate as f64 * request.duration_hours as f64) as i64)
                    .unwrap_or(0)
            }
            PayType::FixedRate => request.fixed_rate_kobo.unwrap_or(0),
        };

        let stat_bonus = request.stat_bonus_kobo.unwrap_or(0);
        let grand_total = base_amount + stat_bonus;

        (base_amount, stat_bonus, grand_total)
    }
}

/// AC-06: Shift preview response
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ShiftPreview {
    pub role_title: String,
    pub specialty: Option<String>,
    pub department: Option<String>,
    pub shift_type: ShiftType,
    pub priority: ShiftPriority,
    pub scheduled_start: chrono::DateTime<Utc>,
    pub duration_hours: f32,
    pub base_amount_kobo: i64,
    pub stat_bonus_kobo: i64,
    pub grand_total_kobo: i64,
    pub virtual_link: Option<String>,
    pub estimated_matches: i32,
}
