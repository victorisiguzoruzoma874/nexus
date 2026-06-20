-- Stores the synthetic training dataset so the ML service can train
-- directly from PostgreSQL instead of the CSV file.
-- Run after the patients pipeline migration.

CREATE TABLE IF NOT EXISTS patient_training_data (
    id                    SERIAL PRIMARY KEY,
    patient_id            TEXT NOT NULL,
    gender                TEXT,
    age                   INTEGER,
    blood_group           TEXT,
    genotype              TEXT,
    height_cm             NUMERIC(5,1),
    weight_kg             NUMERIC(5,1),
    disease_type          TEXT NOT NULL,
    symptoms              TEXT,
    existing_conditions   TEXT,
    severity_level        TEXT NOT NULL,
    severity_ordinal      INTEGER NOT NULL,
    weather_condition     TEXT,
    smoking_status        BOOLEAN NOT NULL DEFAULT FALSE,
    alcohol_consumption   BOOLEAN NOT NULL DEFAULT FALSE,
    exercise_habits       TEXT,
    diet_type             TEXT,
    water_source          TEXT,
    patient_category      TEXT NOT NULL,
    state                 TEXT,
    occupation            TEXT,
    predictive_risk_score NUMERIC(6,4) NOT NULL,
    mortality_risk        TEXT NOT NULL,
    readmission_prediction TEXT NOT NULL,
    drug_recommendation   TEXT NOT NULL,
    was_readmitted        INTEGER NOT NULL DEFAULT 0,
    source                TEXT NOT NULL DEFAULT 'synthetic',
    created_at            TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_training_mortality  ON patient_training_data (mortality_risk);
CREATE INDEX IF NOT EXISTS idx_training_disease    ON patient_training_data (disease_type);
CREATE INDEX IF NOT EXISTS idx_training_source     ON patient_training_data (source);
