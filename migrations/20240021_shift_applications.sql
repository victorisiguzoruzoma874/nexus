-- Shift applications

CREATE TYPE shift_application_status AS ENUM (
    'submitted',
    'withdrawn',
    'accepted',
    'rejected'
);

CREATE TABLE shift_applications (
    id                  UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    shift_id            UUID        NOT NULL REFERENCES shifts (id) ON DELETE CASCADE,
    clinician_id        UUID        NOT NULL REFERENCES clinicians (id) ON DELETE CASCADE,
    applicant_name      VARCHAR(200) NOT NULL,
    license_number      VARCHAR(100) NOT NULL,
    role                VARCHAR(100) NOT NULL,
    years_experience    INTEGER     NOT NULL CHECK (years_experience >= 0),
    experience_summary  TEXT,
    status              shift_application_status NOT NULL DEFAULT 'submitted',
    created_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT uq_shift_application UNIQUE (shift_id, clinician_id)
);

CREATE INDEX idx_shift_applications_shift_id ON shift_applications (shift_id);
CREATE INDEX idx_shift_applications_clinician_id ON shift_applications (clinician_id);
CREATE INDEX idx_shift_applications_status ON shift_applications (status);

CREATE TRIGGER trg_shift_applications_updated_at
    BEFORE UPDATE ON shift_applications
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();
