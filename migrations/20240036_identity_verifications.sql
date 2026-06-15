-- BVN/NIN identity verifications for hospital admins and clinicians
CREATE TYPE identity_kind AS ENUM ('bvn', 'nin');
CREATE TYPE identity_owner AS ENUM ('hospital', 'clinician');
CREATE TYPE identity_status AS ENUM ('pending', 'verified', 'failed');

CREATE TABLE identity_verifications (
    id                   UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    owner_type           identity_owner  NOT NULL,
    owner_id             UUID            NOT NULL,
    identity_type        identity_kind   NOT NULL,
    identity_number      TEXT            NOT NULL,
    provider_identity_id TEXT,
    status               identity_status NOT NULL DEFAULT 'pending',
    provider_payload     JSONB,
    verified_at          TIMESTAMPTZ,
    created_at           TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at           TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- One row per (owner, identity type); re-initiating overwrites the stale row
CREATE UNIQUE INDEX uq_identity_owner_type
    ON identity_verifications (owner_type, owner_id, identity_type);
