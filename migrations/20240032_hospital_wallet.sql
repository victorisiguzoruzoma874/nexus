-- =============================================================================
-- Tier 2.1 — Hospital wallet (SafeHaven sub-account + DB ledger)
-- =============================================================================
-- A hospital cannot create a shift unless its wallet balance covers the full
-- shift cost. On create_shift we move `grand_total_kobo` from `balance_kobo`
-- to `held_kobo`. On cancel we release. On clock-out + approval (Tier 3) we
-- debit `held_kobo` and transfer the net pay to the clinician.
--
-- Real money lives in the hospital's SafeHaven sub-account; this table is the
-- local cache + ledger so we don't round-trip SafeHaven on every read.

CREATE TABLE IF NOT EXISTS hospital_wallets (
    hospital_id              UUID         PRIMARY KEY REFERENCES hospitals (id) ON DELETE CASCADE,

    -- SafeHaven sub-account that holds this hospital's real funds.
    -- NULL while pending provisioning (e.g. hospital approved before Tier 2 shipped).
    safehaven_account_id     TEXT,
    safehaven_account_number TEXT,
    safehaven_bank_code      TEXT,
    safehaven_account_name   TEXT,

    -- Ledger balances in kobo. balance + held must always equal the total
    -- real funds we expect at SafeHaven (subject to in-flight webhooks).
    balance_kobo             BIGINT       NOT NULL DEFAULT 0 CHECK (balance_kobo >= 0),
    held_kobo                BIGINT       NOT NULL DEFAULT 0 CHECK (held_kobo    >= 0),

    created_at               TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    updated_at               TIMESTAMPTZ  NOT NULL DEFAULT NOW()
);

CREATE TRIGGER trg_hospital_wallets_updated_at
    BEFORE UPDATE ON hospital_wallets
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

-- ---------------------------------------------------------------------------
-- Ledger entries — append-only audit log of every wallet mutation.
-- One row per state change. The wallet balances are recoverable by summing
-- the deltas.
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS wallet_ledger_entries (
    id                  UUID         PRIMARY KEY DEFAULT gen_random_uuid(),
    hospital_id         UUID         NOT NULL REFERENCES hospitals (id) ON DELETE CASCADE,

    -- 'deposit_credit' | 'shift_hold' | 'shift_release' | 'payout_debit'
    -- | 'platform_fee' | 'refund'  (free-form for forward compatibility).
    kind                TEXT         NOT NULL,

    -- Sign convention: positive = credit to balance_kobo; negative = debit.
    delta_balance_kobo  BIGINT       NOT NULL,
    -- Sign convention: positive = funds moved INTO held_kobo; negative = released.
    delta_held_kobo     BIGINT       NOT NULL,

    -- Optional linkage to a shift (escrow, payout, refund).
    shift_id            UUID         REFERENCES shifts (id),
    -- SafeHaven payment_reference / sessionId, when applicable.
    provider_reference  TEXT,
    notes               TEXT,

    created_at          TIMESTAMPTZ  NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_wallet_ledger_hospital_at
    ON wallet_ledger_entries (hospital_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_wallet_ledger_shift_id
    ON wallet_ledger_entries (shift_id) WHERE shift_id IS NOT NULL;

-- ---------------------------------------------------------------------------
-- Deposit requests — virtual accounts we've handed out, not yet funded.
-- ---------------------------------------------------------------------------
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_type WHERE typname = 'wallet_deposit_status') THEN
        CREATE TYPE wallet_deposit_status AS ENUM ('pending', 'received', 'expired');
    END IF;
END$$;

CREATE TABLE IF NOT EXISTS wallet_deposit_requests (
    id                     UUID         PRIMARY KEY DEFAULT gen_random_uuid(),
    hospital_id            UUID         NOT NULL REFERENCES hospitals (id) ON DELETE CASCADE,

    -- Amount the hospital declared they want to deposit.
    amount_kobo            BIGINT       NOT NULL CHECK (amount_kobo > 0),

    -- SafeHaven virtual-account details we returned to the hospital.
    virtual_account_number TEXT         NOT NULL,
    virtual_bank_code      TEXT,
    virtual_account_name   TEXT,
    valid_until            TIMESTAMPTZ  NOT NULL,

    -- Our own UUID-prefixed reference (e.g. "dep_<uuid>"); also stored on
    -- SafeHaven side as `externalReference` so the webhook can be correlated.
    external_reference     TEXT         NOT NULL UNIQUE,

    status                 wallet_deposit_status NOT NULL DEFAULT 'pending',
    received_at            TIMESTAMPTZ,
    -- Final amount actually received (may differ from amount_kobo when the
    -- virtual account is configured for OverPayment).
    received_amount_kobo   BIGINT       CHECK (received_amount_kobo IS NULL OR received_amount_kobo > 0),

    -- Raw SafeHaven webhook payload kept for audit + replay.
    safehaven_payload      JSONB,

    created_at             TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    updated_at             TIMESTAMPTZ  NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_wallet_deposits_hospital
    ON wallet_deposit_requests (hospital_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_wallet_deposits_virtual_account
    ON wallet_deposit_requests (virtual_account_number);
CREATE INDEX IF NOT EXISTS idx_wallet_deposits_pending
    ON wallet_deposit_requests (valid_until)
    WHERE status = 'pending';

CREATE TRIGGER trg_wallet_deposit_requests_updated_at
    BEFORE UPDATE ON wallet_deposit_requests
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

-- ---------------------------------------------------------------------------
-- Webhook audit + idempotency.
-- SafeHaven payloads include `_id` (and sometimes `sessionId`). We dedupe on
-- `provider_event_id` so retries from SafeHaven are no-ops on our side.
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS webhook_events (
    id                 UUID         PRIMARY KEY DEFAULT gen_random_uuid(),
    provider           TEXT         NOT NULL DEFAULT 'safehaven',
    -- SafeHaven `_id` / `sessionId`. NULL only when the payload omits both,
    -- which means we treat the event as a "process always" audit row.
    provider_event_id  TEXT,
    event_type         TEXT,
    raw_payload        JSONB        NOT NULL,
    -- Whether we successfully processed the side effects.
    processed          BOOLEAN      NOT NULL DEFAULT FALSE,
    processed_at       TIMESTAMPTZ,
    error_message      TEXT,
    received_at        TIMESTAMPTZ  NOT NULL DEFAULT NOW(),

    -- Idempotency: a given provider_event_id from a given provider should
    -- only ever produce one row. The partial UNIQUE skips events without an id.
    CONSTRAINT uq_webhook_event UNIQUE (provider, provider_event_id)
);

CREATE INDEX IF NOT EXISTS idx_webhook_events_received_at
    ON webhook_events (received_at DESC);
CREATE INDEX IF NOT EXISTS idx_webhook_events_unprocessed
    ON webhook_events (received_at) WHERE processed = FALSE;
