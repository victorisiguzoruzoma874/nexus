-- Email OTP codes for clinician registration and login

CREATE TABLE clinician_email_otp_codes (
    id         UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    email      VARCHAR(255) NOT NULL,
    code       VARCHAR(6)  NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    used       BOOLEAN     NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_clinician_email_otp_codes_email
    ON clinician_email_otp_codes (email);

CREATE TABLE login_email_otp_codes (
    id         UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    email      VARCHAR(255) NOT NULL,
    code       VARCHAR(6)  NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    used       BOOLEAN     NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_login_email_otp_codes_email
    ON login_email_otp_codes (email);
