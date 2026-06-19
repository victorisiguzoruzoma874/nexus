-- =============================================================================
-- Tier 1.4 — Paystack → SafeHaven schema cutover
-- =============================================================================
-- * Drop hospital_payment_methods (Paystack-only; the wallet sub-account in
--   Tier 2 replaces the "stored card on file" concept entirely).
-- * On billing_transactions:
--     - drop the FK to the now-deleted payment_methods table,
--     - rename paystack_reference        → provider_reference,
--     - rename paystack_transaction_id   → provider_transaction_id,
--     - add `provider TEXT NOT NULL DEFAULT 'safehaven'` so we can carry
--       multi-provider history if we ever need to.
-- * Extend billing_event_type with Deposit / Payout / PlatformFee / Refund
--   (used in Tier 2 for the wallet ledger + Tier 3 payouts).
-- * Record validation provenance on clinician_bank_accounts (was Paystack,
--   becomes SafeHaven).

-- ---------------------------------------------------------------------------
-- 1. Drop the Paystack payment-methods table.
-- ---------------------------------------------------------------------------
DROP TABLE IF EXISTS hospital_payment_methods CASCADE;

-- ---------------------------------------------------------------------------
-- 2. Rename Paystack-specific columns on billing_transactions.
-- ---------------------------------------------------------------------------
-- The FK to hospital_payment_methods went away with the CASCADE above, but
-- the column itself stays around unused; drop it explicitly.
ALTER TABLE billing_transactions
    DROP COLUMN IF EXISTS payment_method_id;

ALTER TABLE billing_transactions
    RENAME COLUMN paystack_reference TO provider_reference;
ALTER TABLE billing_transactions
    RENAME COLUMN paystack_transaction_id TO provider_transaction_id;

-- Drop and recreate the Paystack-named index against the new column name.
DROP INDEX IF EXISTS idx_billing_paystack_ref;
CREATE INDEX IF NOT EXISTS idx_billing_provider_ref
    ON billing_transactions (provider_reference)
    WHERE provider_reference IS NOT NULL;

ALTER TABLE billing_transactions
    ADD COLUMN IF NOT EXISTS provider TEXT NOT NULL DEFAULT 'safehaven';

-- ---------------------------------------------------------------------------
-- 3. Extend billing_event_type for the wallet + payout flows.
--    Postgres ADD VALUE is idempotent via IF NOT EXISTS.
-- ---------------------------------------------------------------------------
ALTER TYPE billing_event_type ADD VALUE IF NOT EXISTS 'deposit';
ALTER TYPE billing_event_type ADD VALUE IF NOT EXISTS 'payout';
ALTER TYPE billing_event_type ADD VALUE IF NOT EXISTS 'platform_fee';
ALTER TYPE billing_event_type ADD VALUE IF NOT EXISTS 'refund';

-- ---------------------------------------------------------------------------
-- 4. clinician_bank_accounts: record which provider validated the account
--    (so audit logs make sense after the cutover). Default 'safehaven' since
--    every new write goes through SafeHaven now.
-- ---------------------------------------------------------------------------
ALTER TABLE clinician_bank_accounts
    ADD COLUMN IF NOT EXISTS validated_by TEXT NOT NULL DEFAULT 'safehaven';
