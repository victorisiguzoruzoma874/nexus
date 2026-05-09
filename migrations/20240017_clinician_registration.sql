-- Clinician registration: OTP table + profile fields

-- Enum for clinician role (health worker type)
CREATE TYPE clinician_role AS ENUM (
    'doctor',
    'nurse',
    'lab_technician',
    'pharmacist',
    'radiographer',
    'physiotherapist',
    'other'
);

-- OTP codes for phone verification
CREATE TABLE otp_codes (
    id          UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    phone       VARCHAR(20) NOT NULL,
    code        VARCHAR(6)  NOT NULL,
    expires_at  TIMESTAMPTZ NOT NULL,
    used        BOOLEAN     NOT NULL DEFAULT FALSE,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_otp_codes_phone ON otp_codes (phone);

-- Add phone + license fields to clinicians
ALTER TABLE clinicians
    ADD COLUMN IF NOT EXISTS phone          VARCHAR(20)     UNIQUE,
    ADD COLUMN IF NOT EXISTS license_number VARCHAR(100),
    ADD COLUMN IF NOT EXISTS clinician_role clinician_role;

-- Clinician bank accounts (validated via Paystack, stored encrypted)
CREATE TABLE clinician_bank_accounts (
    id              UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    clinician_id    UUID        NOT NULL UNIQUE REFERENCES clinicians (id) ON DELETE CASCADE,
    account_number  TEXT        NOT NULL,  -- encrypted
    bank_code       VARCHAR(10) NOT NULL,
    account_name    VARCHAR(200) NOT NULL, -- resolved by Paystack
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TRIGGER trg_clinician_bank_accounts_updated_at
    BEFORE UPDATE ON clinician_bank_accounts
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();
