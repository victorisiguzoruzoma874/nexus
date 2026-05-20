use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;
use validator::Validate;
use utoipa::ToSchema;

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

/// Lifecycle status of a shift.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, sqlx::Type, ToSchema)]
#[sqlx(type_name = "shift_status", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum ShiftStatus {
    /// Posted, waiting for a clinician to be assigned
    Open,
    /// Clinician assigned, shift not yet started
    Upcoming,
    /// Shift is currently running
    InProgress,
    /// Shift completed successfully
    Completed,
    /// Shift was cancelled before it started
    Cancelled,
    /// Clinician did not show up
    NoShow,
}

/// Priority level of an open shift — shown as badge on the dashboard.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, sqlx::Type, ToSchema)]
#[sqlx(type_name = "shift_priority", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum ShiftPriority {
    /// Normal priority
    Normal,
    /// Elevated priority — shown as "STAT" badge (orange), +20% bonus rate
    Stat,
    /// Highest priority — shown as "URGENT" badge (red/yellow)
    Urgent,
}

/// Delivery mode of a shift — shown as radio toggle in the wizard.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, sqlx::Type, ToSchema)]
#[sqlx(type_name = "shift_type", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum ShiftType {
    /// On-site at the hospital (GPS clock-in enforced)
    InPerson,
    /// Remote / telemedicine shift
    Virtual,
}

/// Broad role category selected in Step 1 of the shift wizard.
/// Separate from the free-text `role_title` and `specialty`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, sqlx::Type, ToSchema)]
#[sqlx(type_name = "role_category", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum RoleCategory {
    Doctor,
    Nurse,
    Pharmacist,
    LabTechnician,
    Radiographer,
    Physiotherapist,
    Other,
}

/// Which step of the 5-step shift creation wizard the draft is on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "shift_wizard_step", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum ShiftWizardStep {
    /// Step 1 — Basic Information (role, specialty, type, date, duration, urgency)
    BasicInformation,
    /// Step 2 — Shift Compensation (pay type, hourly rate, bonuses, allowances)
    Compensation,
    /// Step 3 — Shift Description (job description, tasks, deliverables, equipment)
    ShiftDescription,
    /// Step 4 — Requirements (qualifications, institutional verification)
    Requirements,
    /// Step 5 — Preview (shift card preview + Broadcast Shift action)
    Preview,
}

/// Pay type for a shift — radio toggle in Step 2 (Shift Compensation).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, sqlx::Type, ToSchema)]
#[sqlx(type_name = "pay_type", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum PayType {
    /// Hourly rate × expected hours — "Best for standard rotations"
    HourlyRate,
    /// Fixed lump sum per shift — "Lump sum per shift"
    FixedRate,
}

/// How a clinician's clock-in was verified.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "clockin_method", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum ClockinMethod {
    /// GPS geofence verified (within clock_in_radius_meters of hospital entrance)
    Gps,
    /// QR code scanned on-site
    QrCode,
    /// Manually confirmed by a hospital admin
    Manual,
}

// ---------------------------------------------------------------------------
// Shift
// ---------------------------------------------------------------------------

/// A shift posting created by a hospital.
/// Shown in "Today's Active Shifts" and "Open Shifts Needing Staff" on the dashboard.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow, ToSchema)]
pub struct Shift {
    pub id: Uuid,
    pub hospital_id: Uuid,
    /// Name of the hospital that created this shift
    #[sqlx(default)]
    pub hospital_name: Option<String>,

    /// Broad role category, e.g. Doctor, Nurse (Step 1 dropdown)
    pub role_category: RoleCategory,
    /// Specific role title, e.g. "Emergency Doctor", "General Nurse"
    pub role_title: String,
    /// Clinical specialty, e.g. Emergency Medicine, Pediatrics
    pub specialty: Option<String>,
    /// Department or unit, e.g. "Hematology Unit", "Main Pharmacy"
    pub department: Option<String>,

    /// In-person (GPS enforced) or virtual
    pub shift_type: ShiftType,

    pub status: ShiftStatus,
    pub priority: ShiftPriority,
    /// Bonus percentage applied for STAT urgency (e.g. 20 for +20%)
    pub urgency_bonus_pct: Option<i16>,

    /// Scheduled start datetime, e.g. 2026-04-14 14:00 UTC
    pub scheduled_start: DateTime<Utc>,
    /// Explicit duration in hours as entered in the wizard (e.g. 8)
    pub duration_hours: f32,
    /// Derived: scheduled_start + duration_hours
    pub scheduled_end: DateTime<Utc>,

    /// Actual clock-in time (set when clinician clocks in)
    pub actual_start: Option<DateTime<Utc>>,
    /// Actual clock-out time
    pub actual_end: Option<DateTime<Utc>>,

    /// The clinician assigned to this shift (NULL while open)
    pub assigned_clinician_id: Option<Uuid>,

    /// Base hourly rate in kobo (e.g. 800000 = ₦8,000/hr)
    pub rate_kobo_per_hour: Option<i64>,
    /// Fixed lump-sum rate in kobo (used when pay_type = fixed_rate)
    pub fixed_rate_kobo: Option<i64>,
    /// Whether pay is hourly or a fixed lump sum per shift
    pub pay_type: PayType,
    /// STAT bonus fixed amount in kobo (e.g. 500000 = ₦5,000)
    pub stat_bonus_kobo: Option<i64>,
    /// Effective rate after urgency bonus applied (computed field)
    pub effective_rate_kobo_per_hour: Option<i64>,
    /// Pre-computed grand total for the shift in kobo (base + bonuses + allowances)
    pub grand_total_kobo: Option<i64>,

    /// Human-readable label for the shift, e.g. "Night Shift: General Ward A"
    /// Shown in the "Current Progress" card during wizard navigation
    pub shift_label: Option<String>,

    /// Free-text job description ("We need an experienced Emergency Doctor...")
    pub job_description: Option<String>,
    /// AI-generated draft quality score 0–100 shown during wizard (e.g. 85)
    pub draft_quality_score: Option<i16>,
    /// Notes or special requirements for the shift
    pub notes: Option<String>,

    /// The user who created this shift posting
    pub created_by: Uuid,
    /// Whether the hospital confirmed the institutional verification consent
    /// shown in Step 4: "By broadcasting this shift, you confirm that [hospital]
    /// is authorized to match this request with verified clinicians in the LUTH network."
    pub broadcast_consent_confirmed: bool,
    /// Number of matched clinicians the shift was visible to at publish time
    /// ("The shift will be visible to 48 matched clinicians immediately.")
    pub matched_clinicians_at_publish: Option<i32>,
    /// When the shift was broadcast to the clinician marketplace (Step 5 action)
    pub broadcast_at: Option<DateTime<Utc>>,
    /// Billing is triggered only when a clinician is successfully booked
    /// ("Charges will only apply once a clinician is successfully booked.")
    pub billing_triggered_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// Shift interest / applications
// ---------------------------------------------------------------------------

/// A clinician expressing interest in an open shift.
/// Shown as "3 Interested", "1 Interested" on the Open Shifts panel.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ShiftInterest {
    pub id: Uuid,
    pub shift_id: Uuid,
    pub clinician_id: Uuid,
    /// Whether this clinician is the top algorithmic match
    pub is_top_match: bool,
    /// Whether this clinician is on the waitlist (shown as "Waitlisting active")
    pub is_waitlisted: bool,
    pub expressed_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// Clock-in / clock-out record
// ---------------------------------------------------------------------------

/// Records a clinician's clock-in and clock-out for a shift.
/// Clock-in requires GPS verification within the hospital's geofence (100m default).
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ShiftAttendance {
    pub id: Uuid,
    pub shift_id: Uuid,
    pub clinician_id: Uuid,

    pub clockin_at: Option<DateTime<Utc>>,
    pub clockin_method: Option<ClockinMethod>,
    pub clockin_latitude: Option<f64>,
    pub clockin_longitude: Option<f64>,
    /// Distance from hospital entrance at clock-in time (metres)
    pub clockin_distance_meters: Option<f32>,

    pub clockout_at: Option<DateTime<Utc>>,
    pub clockout_method: Option<ClockinMethod>,
    pub clockout_latitude: Option<f64>,
    pub clockout_longitude: Option<f64>,

    /// Total worked duration in minutes (computed on clock-out)
    pub worked_minutes: Option<i32>,

    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// Dashboard KPI snapshot
// ---------------------------------------------------------------------------

/// Pre-computed KPI snapshot for the Clinical Dashboard top cards.
/// Recalculated periodically (e.g. hourly) and cached here.
///   - Shift Fill Rate: 84% (goal: 92%)
///   - Total Disbursements: ₦12.4M (+4.2% from last week)
///   - Average Fill Time: 14.2 hrs (-2h improvement)
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct DashboardKpiSnapshot {
    pub id: Uuid,
    pub hospital_id: Uuid,

    /// Percentage of shifts filled in the current week (0–100)
    pub shift_fill_rate_pct: f32,
    /// Institutional target fill rate (e.g. 92.0)
    pub fill_rate_goal_pct: f32,
    /// Delta vs previous week in percentage points
    pub fill_rate_delta_pct: f32,

    /// Total disbursements this week in kobo
    pub total_disbursements_kobo: i64,
    /// Week-over-week change as a percentage (e.g. +4.2)
    pub disbursements_delta_pct: f32,

    /// Average time from shift posting to clinician assignment, in hours
    pub avg_fill_time_hours: f32,
    /// Change vs previous period in hours (negative = improvement)
    pub fill_time_delta_hours: f32,

    /// When this snapshot was computed
    pub computed_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// Staffing insight
// ---------------------------------------------------------------------------

/// An AI/analytics-generated staffing insight shown in the dashboard panel.
/// e.g. "Nurse availability is 15% higher than average on Fridays.
///        Consider scheduling elective surgery coverage now."
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct StaffingInsight {
    pub id: Uuid,
    pub hospital_id: Uuid,
    pub insight_text: String,
    /// Optional CTA label, e.g. "Explore Trends"
    pub cta_label: Option<String>,
    pub is_active: bool,
    pub generated_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// Shift allowances
// ---------------------------------------------------------------------------

/// An additional allowance added to a shift's compensation.
/// Created via the "+ Add Additional Allowance" button in Step 2.
/// Examples: transport allowance, hazard pay, night differential.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ShiftAllowance {
    pub id: Uuid,
    /// Links to either a published shift or a wizard draft (one must be set)
    pub shift_id: Option<Uuid>,
    pub draft_id: Option<Uuid>,
    /// Label entered by the hospital admin, e.g. "Transport Allowance"
    pub label: String,
    /// Amount in kobo
    pub amount_kobo: i64,
    pub created_at: DateTime<Utc>,
}

/// Payload for adding an allowance.
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct AddAllowanceRequest {
    #[validate(length(min = 1, max = 100, message = "Allowance label is required"))]
    pub label: String,
    #[validate(range(min = 1, message = "Amount must be greater than zero"))]
    pub amount_kobo: i64,
}

/// The full compensation breakdown returned to the frontend for the summary card.
///   Base (8 hrs × ₦8,000)  ₦64,000
///   STAT Bonus              ₦5,000
///   GRAND TOTAL             ₦69,000
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompensationSummary {
    pub pay_type: PayType,
    /// Base amount in kobo (hourly_rate × duration_hours OR fixed_rate)
    pub base_amount_kobo: i64,
    /// STAT bonus fixed amount in kobo
    pub stat_bonus_kobo: i64,
    /// Sum of all additional allowances in kobo
    pub allowances_total_kobo: i64,
    /// Grand total in kobo
    pub grand_total_kobo: i64,
    /// Display label, e.g. "8 hrs × ₦8,000"
    pub base_calculation_label: String,
}

// ---------------------------------------------------------------------------
// Shift description items (Step 3)
// ---------------------------------------------------------------------------

/// Category of a shift description line item.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "shift_item_category", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum ShiftItemCategory {
    /// Clinical task the clinician must perform, e.g. "See 20-25 emergency patients"
    Task,
    /// Expected output / deliverable, e.g. "Documents — PDF Files"
    Deliverable,
    /// Resource provided to the clinician, e.g. "Workstation with EMR access"
    Equipment,
}

/// A single line item in the shift description (task, deliverable, or equipment).
/// Linked to either a published shift or a wizard draft.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ShiftDescriptionItem {
    pub id: Uuid,
    pub shift_id: Option<Uuid>,
    pub draft_id: Option<Uuid>,
    pub category: ShiftItemCategory,
    /// Primary label, e.g. "Workstation with EMR access"
    pub label: String,
    /// Optional sub-label / description, e.g. "Full privileges for the duration of shift"
    pub description: Option<String>,
    /// Display order within the category
    pub sort_order: i16,
    pub created_at: DateTime<Utc>,
}

/// Payload for adding a description item.
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct AddDescriptionItemRequest {
    pub category: ShiftItemCategory,
    #[validate(length(min = 1, max = 255, message = "Label is required"))]
    pub label: String,
    #[validate(length(max = 500))]
    pub description: Option<String>,
}

// ---------------------------------------------------------------------------
// Shift requirements / qualifications (Step 4)
// ---------------------------------------------------------------------------

/// A qualification tag required for a shift.
/// Shown as pill badges in Step 4: "2+ years emergency experience",
/// "ACLS certified", "Valid medical license".
/// Linked to either a published shift or a wizard draft.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ShiftRequirement {
    pub id: Uuid,
    pub shift_id: Option<Uuid>,
    pub draft_id: Option<Uuid>,
    /// Free-text qualification tag, e.g. "ACLS certified"
    pub qualification: String,
    /// Display order among the tags
    pub sort_order: i16,
    pub created_at: DateTime<Utc>,
}

/// Payload for adding a qualification tag.
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct AddRequirementRequest {
    #[validate(length(min = 1, max = 200, message = "Qualification text is required"))]
    pub qualification: String,
}

// ---------------------------------------------------------------------------
// Shift bookmark (Step 5 — Preview card)
// ---------------------------------------------------------------------------

/// A clinician bookmarking a shift from the preview card or marketplace.
/// Shown as the bookmark icon (🔖) in the top-right of the shift preview card.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ShiftBookmark {
    pub id: Uuid,
    pub shift_id: Uuid,
    pub clinician_id: Uuid,
    pub bookmarked_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// Shift broadcast record (Step 5 — Broadcast Shift action)
// ---------------------------------------------------------------------------

/// Audit record created when a hospital clicks "Broadcast Shift".
/// Captures the exact state at broadcast time for compliance and billing.
/// "By clicking broadcast, this position will be immediately published to
///  the clinician marketplace. Charges will only apply once a clinician
///  is successfully booked."
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ShiftBroadcastRecord {
    pub id: Uuid,
    pub shift_id: Uuid,
    /// The hospital admin who clicked Broadcast
    pub broadcast_by: Uuid,
    pub broadcast_at: DateTime<Utc>,
    /// Number of eligible clinicians the shift was sent to (e.g. 45)
    pub eligible_clinicians_count: i32,
    /// Distance from hospital used to filter nearby clinicians (km)
    pub broadcast_radius_km: f64,
    /// Location label shown on the card, e.g. "Idi-Araba, Lagos"
    pub location_label: Option<String>,
    pub created_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// Shift creation wizard draft
// ---------------------------------------------------------------------------

/// Persists partial state of the 4-step shift creation wizard so the user
/// can navigate back and forward without losing data.
/// Promoted to a full `Shift` record when Step 4 (Review & Publish) is submitted.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ShiftWizardDraft {
    pub id: Uuid,
    pub hospital_id: Uuid,
    pub created_by: Uuid,
    /// Which step the user is currently on
    pub current_step: ShiftWizardStep,

    // --- Step 1: Basic Information ---
    pub role_category: Option<RoleCategory>,
    pub role_title: Option<String>,
    pub specialty: Option<String>,
    pub shift_type: Option<ShiftType>,
    pub scheduled_start: Option<DateTime<Utc>>,
    pub duration_hours: Option<f32>,
    pub priority: Option<ShiftPriority>,
    pub urgency_bonus_pct: Option<i16>,

    // --- Step 2: Compensation ---
    pub pay_type: Option<PayType>,
    pub rate_kobo_per_hour: Option<i64>,
    pub fixed_rate_kobo: Option<i64>,
    pub stat_bonus_kobo: Option<i64>,
    /// Pre-computed grand total snapshot for the summary card
    pub grand_total_kobo: Option<i64>,

    // --- Step 3: Requirements / Shift Description ---
    pub department: Option<String>,
    /// Free-text job description (required field in Step 3)
    pub job_description: Option<String>,
    /// AI-generated draft quality score 0–100 (e.g. 85)
    pub draft_quality_score: Option<i16>,
    pub notes: Option<String>,

    /// Human-readable label shown in "Current Progress" card, e.g. "Night Shift: General Ward A"
    pub shift_label: Option<String>,

    /// Number of matched professionals shown during wizard ("14 Available Now")
    pub matched_professionals_count: Option<i32>,

    // --- Step 4: Requirements ---
    /// Whether the hospital confirmed the institutional verification consent
    pub broadcast_consent_confirmed: bool,
    /// Matched clinician count shown at publish time ("48 matched clinicians")
    pub matched_clinicians_at_publish: Option<i32>,

    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// Shift action requests
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize, Validate, ToSchema)]
pub struct ShiftInterestRequest {
    pub clinician_id: Uuid,
}

#[derive(Debug, Clone, Deserialize, Validate, ToSchema)]
pub struct ShiftAssignRequest {
    pub clinician_id: Uuid,
}

#[derive(Debug, Clone, Deserialize, Validate, ToSchema)]
pub struct ShiftCancelRequest {
    #[validate(length(min = 3, max = 500))]
    pub reason: String,
}

#[derive(Debug, Clone, Deserialize, Validate, ToSchema)]
pub struct ShiftRescheduleRequest {
    pub scheduled_start: DateTime<Utc>,
    #[validate(range(min = 0.1, max = 72.0))]
    pub duration_hours: f32,
}

// ---------------------------------------------------------------------------
// Shift applications
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, sqlx::Type, ToSchema)]
#[sqlx(type_name = "shift_application_status", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum ShiftApplicationStatus {
    Submitted,
    Withdrawn,
    Accepted,
    Rejected,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, ToSchema)]
pub struct ShiftApplication {
    pub id: Uuid,
    pub shift_id: Uuid,
    pub clinician_id: Uuid,
    pub applicant_name: String,
    pub license_number: String,
    pub role: String,
    pub years_experience: i32,
    pub experience_summary: Option<String>,
    pub status: ShiftApplicationStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize, Validate, ToSchema)]
pub struct ShiftApplicationRequest {
    pub clinician_id: Uuid,
    #[validate(length(min = 2, max = 200))]
    pub applicant_name: String,
    #[validate(length(min = 2, max = 100))]
    pub license_number: String,
    #[validate(length(min = 2, max = 100))]
    pub role: String,
    #[validate(range(min = 0, max = 60))]
    pub years_experience: i32,
    #[validate(length(max = 2000))]
    pub experience_summary: Option<String>,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct ShiftApplicationsQuery {
    pub requester_user_id: Uuid,
    pub page: Option<i64>,
    pub page_size: Option<i64>,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct ShiftListQuery {
    pub status: Option<ShiftStatus>,
    pub page: Option<i64>,
    pub page_size: Option<i64>,
}
// ---------------------------------------------------------------------------
// Request / response types
// ---------------------------------------------------------------------------

/// Payload for creating a new shift posting.
#[derive(Debug, Clone, Serialize, Deserialize, Validate, ToSchema)]
pub struct CreateShiftRequest {
    pub role_category: RoleCategory,

    #[validate(length(min = 2, max = 255, message = "Role title is required"))]
    pub role_title: String,

    pub specialty: Option<String>,

    #[validate(length(max = 255))]
    pub department: Option<String>,

    pub shift_type: ShiftType,
    pub priority: ShiftPriority,
    pub urgency_bonus_pct: Option<i16>,

    pub scheduled_start: DateTime<Utc>,
    #[validate(range(min = 0.5, max = 24.0, message = "Duration must be between 0.5 and 24 hours"))]
    pub duration_hours: f32,

    // --- Step 2: Compensation ---
    pub pay_type: PayType,
    /// Required when pay_type = hourly_rate
    pub rate_kobo_per_hour: Option<i64>,
    /// Required when pay_type = fixed_rate
    pub fixed_rate_kobo: Option<i64>,
    /// STAT bonus fixed amount in kobo (e.g. 500000 = ₦5,000)
    pub stat_bonus_kobo: Option<i64>,

    #[validate(length(max = 100))]
    pub shift_label: Option<String>,

    #[validate(length(max = 1000))]
    pub notes: Option<String>,

    /// Step 4: institutional verification consent (must be true to publish)
    pub broadcast_consent_confirmed: bool,
}

/// Payload for saving a wizard step as a draft.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SaveShiftDraftRequest {
    pub current_step: ShiftWizardStep,
    // Step 1
    pub role_category: Option<RoleCategory>,
    pub role_title: Option<String>,
    pub specialty: Option<String>,
    pub shift_type: Option<ShiftType>,
    pub scheduled_start: Option<DateTime<Utc>>,
    pub duration_hours: Option<f32>,
    pub priority: Option<ShiftPriority>,
    pub urgency_bonus_pct: Option<i16>,
    // Step 2
    pub pay_type: Option<PayType>,
    pub rate_kobo_per_hour: Option<i64>,
    pub fixed_rate_kobo: Option<i64>,
    pub stat_bonus_kobo: Option<i64>,
    pub shift_label: Option<String>,
    // Step 3
    pub department: Option<String>,
    pub job_description: Option<String>,
    pub notes: Option<String>,
    // Step 4
    pub broadcast_consent_confirmed: Option<bool>,
}

/// Shift card shown in "Today's Active Shifts".
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActiveShiftCard {
    pub id: Uuid,
    pub role_title: String,
    pub department: Option<String>,
    pub status: ShiftStatus,
    pub priority: ShiftPriority,
    pub scheduled_start: DateTime<Utc>,
    pub scheduled_end: DateTime<Utc>,
    /// Assigned clinician display name
    pub clinician_name: Option<String>,
    pub clinician_avatar_url: Option<String>,
    /// Duration string, e.g. "08:00 - 20:00"
    pub duration_display: String,
    /// Minutes until shift starts (for UPCOMING shifts)
    pub starts_in_minutes: Option<i64>,
}

/// Shift card shown in "Open Shifts Needing Staff".
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenShiftCard {
    pub id: Uuid,
    pub role_title: String,
    pub department: Option<String>,
    pub priority: ShiftPriority,
    pub scheduled_start: DateTime<Utc>,
    pub interested_count: i64,
    pub top_match_name: Option<String>,
    pub is_waitlisted: bool,
}
