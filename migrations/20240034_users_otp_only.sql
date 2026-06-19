-- =============================================================================
-- OTP-only authentication: passwords are no longer captured during
-- registration. Default users.password_hash to an empty string so callers
-- don't have to provide one.
-- =============================================================================
--
-- The column stays NOT NULL (so existing password-bearing rows are preserved
-- as-is) but new rows can omit it entirely. The empty default can never
-- verify against a real password — `argon2.verify` rejects malformed hashes —
-- so it's a safe sentinel even while the legacy /auth/login endpoint exists.

ALTER TABLE users
    ALTER COLUMN password_hash SET DEFAULT '';
