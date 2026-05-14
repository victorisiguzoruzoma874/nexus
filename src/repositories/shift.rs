use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;
use chrono::Duration;

use crate::models::shift::{
    Shift, ShiftStatus, CreateShiftRequest, PayType,
};

pub struct ShiftRepository {
    pool: PgPool,
}

impl ShiftRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
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

        let shift = sqlx::query_as::<_, Shift>(
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
            RETURNING
                id, hospital_id, role_category, role_title, specialty, department,
                shift_type, status, priority, urgency_bonus_pct,
                scheduled_start, duration_hours, scheduled_end,
                actual_start, actual_end, assigned_clinician_id,
                rate_kobo_per_hour, fixed_rate_kobo, pay_type, stat_bonus_kobo,
                effective_rate_kobo_per_hour, grand_total_kobo,
                shift_label, job_description, draft_quality_score, notes,
                created_by, broadcast_consent_confirmed, matched_clinicians_at_publish,
                broadcast_at, billing_triggered_at, created_at, updated_at
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
                id, hospital_id, role_category, role_title, specialty, department,
                shift_type, status, priority, urgency_bonus_pct,
                scheduled_start, duration_hours, scheduled_end,
                actual_start, actual_end, assigned_clinician_id,
                rate_kobo_per_hour, fixed_rate_kobo, pay_type, stat_bonus_kobo,
                effective_rate_kobo_per_hour, grand_total_kobo,
                shift_label, job_description, draft_quality_score, notes,
                created_by, broadcast_consent_confirmed, matched_clinicians_at_publish,
                broadcast_at, billing_triggered_at, created_at, updated_at
            FROM shifts
            WHERE id = $1
            "#,
        )
        .bind(shift_id)
        .fetch_optional(&self.pool)
        .await
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
