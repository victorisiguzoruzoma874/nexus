-- Tier 2.2 — track shifts a clinician dismissed from their "Shifts Near You"
-- list (§3.3.4). Dismissals do not affect interest or the shift itself; they
-- simply hide the shift from this clinician's discovery feed.
CREATE TABLE IF NOT EXISTS shift_dismissals (
    id             UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    shift_id       UUID        NOT NULL REFERENCES shifts (id) ON DELETE CASCADE,
    clinician_id   UUID        NOT NULL REFERENCES clinicians (id) ON DELETE CASCADE,
    dismissed_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT uq_shift_dismissal UNIQUE (shift_id, clinician_id)
);

CREATE INDEX IF NOT EXISTS idx_dismissals_clinician_id ON shift_dismissals (clinician_id);
CREATE INDEX IF NOT EXISTS idx_dismissals_shift_id     ON shift_dismissals (shift_id);
