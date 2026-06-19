-- =============================================================================
-- Tier 3.3 — Platform revenue ledger (10% fee per shift, FRS §3.8.2)
-- =============================================================================
-- One row per shift payout. Lets ops reconcile our take-rate without scanning
-- the larger wallet ledger or billing_transactions tables.

CREATE TABLE IF NOT EXISTS platform_revenue_ledger (
    id             UUID         PRIMARY KEY DEFAULT gen_random_uuid(),
    shift_id       UUID         NOT NULL UNIQUE REFERENCES shifts (id) ON DELETE CASCADE,
    hospital_id    UUID         NOT NULL REFERENCES hospitals (id) ON DELETE CASCADE,
    gross_kobo     BIGINT       NOT NULL CHECK (gross_kobo > 0),
    fee_kobo       BIGINT       NOT NULL CHECK (fee_kobo >= 0),
    net_kobo       BIGINT       NOT NULL CHECK (net_kobo > 0),
    created_at     TIMESTAMPTZ  NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_platform_revenue_hospital
    ON platform_revenue_ledger (hospital_id, created_at DESC);
