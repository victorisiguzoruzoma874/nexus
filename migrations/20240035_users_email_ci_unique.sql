-- Enforce a single email per user, case-insensitively. The pre-existing
-- `UNIQUE (email)` constraint already catches exact duplicates; this index
-- additionally rejects `Foo@x.com` vs `foo@x.com`. Combined with the
-- service-layer normalisation (we lowercase + trim before insert) it forms
-- a defence in depth: even if a future code path forgets to normalise,
-- Postgres will reject the row.

CREATE UNIQUE INDEX IF NOT EXISTS uq_users_email_ci
    ON users (LOWER(email));
