use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;
use chrono::{Duration, Utc};

use crate::models::shift::{
    Shift, ShiftStatus, CreateShiftRequest, PayType, ShiftApplication,
    ShiftApplicationStatus,
};
use crate::models::registration::RegistrationStatus;

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
                shift_label, notes, created_by, broadcast_consent_confirmed,
                created_at, updated_at
            )
            VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8, $9, $10,
                $11, $12, $13, $14, $15, $16, $17, $18, $19,
                $20, $21, $22, $23, NOW(), NOW()
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

    /// AC-04: Update virtual meeting link for virtual shifts
    pub async fn update_virtual_link(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        shift_id: Uuid,
        virtual_link: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            UPDATE shifts
            SET notes = COALESCE(notes || E'\n\n', '') || 'Virtual Meeting Link: ' || $2,
                updated_at = NOW()
            WHERE id = $1
            "#,
        )
        .bind(shift_id)
        .bind(virtual_link)
        .execute(&mut **tx)
        .await?;

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
