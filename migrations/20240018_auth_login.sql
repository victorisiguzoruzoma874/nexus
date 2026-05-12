-- Login OTP codes (phone-based login, separate from registration OTPs)
CREATE TABLE login_otp_codes (
    id         UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    phone      VARCHAR(20) NOT NULL,
    code       VARCHAR(6)  NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    used       BOOLEAN     NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_login_otp_codes_phone ON login_otp_codes (phone);

-- Refresh tokens for session persistence (AC-04)
CREATE TABLE refresh_tokens (
    id         UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id    UUID        NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    token_hash TEXT        NOT NULL UNIQUE,
    expires_at TIMESTAMPTZ NOT NULL,
    revoked    BOOLEAN     NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_refresh_tokens_user_id ON refresh_tokens (user_id);

-- Password reset tokens
CREATE TABLE password_reset_tokens (
    id         UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id    UUID        NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    token_hash TEXT        NOT NULL UNIQUE,
    expires_at TIMESTAMPTZ NOT NULL,
    used       BOOLEAN     NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Add phone column to users for phone-based login
ALTER TABLE users ADD COLUMN IF NOT EXISTS phone VARCHAR(20) UNIQUE;
