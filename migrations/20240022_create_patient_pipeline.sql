-- Patient pipeline tables: Bronze/Silver/Gold medallion architecture

CREATE TYPE disease_type AS ENUM ('Infectious', 'Chronic', 'Genetic', 'MentalHealth');
CREATE TYPE severity_level AS ENUM ('Mild', 'Moderate', 'Severe', 'Critical');
CREATE TYPE mortality_risk AS ENUM ('Low', 'Medium', 'High');
CREATE TYPE patient_category AS ENUM ('Child', 'Teenager', 'Adult', 'Elderly');
CREATE TYPE pipeline_source AS ENUM ('clinical', 'patient_app', 'emr_export', 'sensor');
CREATE TYPE feedback_type AS ENUM ('correction', 'outcome', 'emr_discharge');

CREATE TABLE patients (
    id                      UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    patient_id              VARCHAR(30) NOT NULL UNIQUE,

    -- Basic
    full_name               VARCHAR(255) NOT NULL,
    gender                  VARCHAR(20),
    date_of_birth           DATE,
    age                     INTEGER,
    marital_status          VARCHAR(30),
    nationality             VARCHAR(100),
    state                   VARCHAR(100),
    city                    VARCHAR(100),
    address                 TEXT,
    phone_number            VARCHAR(30),
    emergency_contact       VARCHAR(100),

    -- Health & Medical
    blood_group             VARCHAR(10),
    genotype                VARCHAR(5),
    height_cm               FLOAT,
    weight_kg               FLOAT,
    allergies               TEXT,
    existing_conditions     TEXT,
    disability_status       VARCHAR(100),
    pregnancy_status        BOOLEAN DEFAULT FALSE,
    vaccination_history     TEXT,
    current_medications     TEXT,

    -- Disease & Illness
    disease_type            disease_type,
    symptoms                TEXT,
    severity_level          severity_level,
    symptom_start_date      DATE,
    previous_medical_history TEXT,
    family_medical_history  TEXT,

    -- Environmental & Lifestyle
    weather_condition       VARCHAR(50),
    temperature             FLOAT,
    humidity                FLOAT,
    occupation              VARCHAR(100),
    smoking_status          BOOLEAN DEFAULT FALSE,
    alcohol_consumption     BOOLEAN DEFAULT FALSE,
    exercise_habits         VARCHAR(50),
    diet_type               VARCHAR(50),
    water_source            VARCHAR(50),

    -- Patient Category
    patient_category        patient_category,

    -- AI/Analytics Output (written by Gold/ML)
    disease_trends          TEXT,
    outbreak_detection      BOOLEAN DEFAULT FALSE,
    predictive_risk_score   FLOAT,
    readmission_prediction  VARCHAR(20),
    mortality_risk          mortality_risk,
    drug_recommendation     TEXT,
    pattern_recognition     BOOLEAN DEFAULT FALSE,

    -- Pipeline metadata
    source                  pipeline_source DEFAULT 'clinical',
    raw_blob_path           VARCHAR(500),
    pipeline_processed      BOOLEAN DEFAULT FALSE,
    silver_processed        BOOLEAN DEFAULT FALSE,
    gold_processed          BOOLEAN DEFAULT FALSE,
    created_at              TIMESTAMPTZ DEFAULT NOW(),
    updated_at              TIMESTAMPTZ DEFAULT NOW()
);

CREATE TABLE feedback (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    patient_id      VARCHAR(30) NOT NULL,
    feedback_type   feedback_type NOT NULL,
    field           VARCHAR(100),
    predicted       TEXT,
    corrected       TEXT,
    doctor_id       VARCHAR(100),
    was_readmitted  BOOLEAN,
    treatment_worked BOOLEAN,
    notes           TEXT,
    created_at      TIMESTAMPTZ DEFAULT NOW()
);

CREATE INDEX idx_patients_pipeline_processed ON patients(pipeline_processed) WHERE pipeline_processed = FALSE;
CREATE INDEX idx_patients_silver_processed ON patients(silver_processed) WHERE silver_processed = FALSE;
CREATE INDEX idx_patients_mortality_risk ON patients(mortality_risk);
CREATE INDEX idx_patients_outbreak ON patients(outbreak_detection) WHERE outbreak_detection = TRUE;
CREATE INDEX idx_patients_state_disease ON patients(state, disease_type, created_at);
