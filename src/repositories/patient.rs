use sqlx::PgPool;
use uuid::Uuid;

use crate::models::patient::{IngestPatientDto, MortalityRisk, Patient, PatientCategory, PipelineStats};

pub struct PatientRepository {
    pool: PgPool,
}

impl PatientRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn create(&self, patient_id: &str, dto: &IngestPatientDto, blob_path: &str) -> Result<Patient, sqlx::Error> {
        sqlx::query_as!(
            Patient,
            r#"INSERT INTO patients (
                patient_id, full_name, gender, date_of_birth, age, marital_status,
                nationality, state, city, address, phone_number, emergency_contact,
                blood_group, genotype, height_cm, weight_kg, allergies, existing_conditions,
                disability_status, pregnancy_status, vaccination_history, current_medications,
                disease_type, symptoms, severity_level, symptom_start_date,
                previous_medical_history, family_medical_history, weather_condition,
                temperature, humidity, occupation, smoking_status, alcohol_consumption,
                exercise_habits, diet_type, water_source, patient_category, source,
                raw_blob_path, pipeline_processed
            ) VALUES (
                $1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,
                $19,$20,$21,$22,$23,$24,$25,$26,$27,$28,$29,$30,$31,$32,$33,$34,
                $35,$36,$37,$38,$39,$40,FALSE
            ) RETURNING
                id, patient_id, full_name, gender, date_of_birth, age, marital_status,
                nationality, state, city, address, phone_number, emergency_contact,
                blood_group, genotype, height_cm, weight_kg, allergies, existing_conditions,
                disability_status, pregnancy_status, vaccination_history, current_medications,
                disease_type AS "disease_type: _",
                symptoms, severity_level AS "severity_level: _",
                symptom_start_date, previous_medical_history, family_medical_history,
                weather_condition, temperature, humidity, occupation, smoking_status,
                alcohol_consumption, exercise_habits, diet_type, water_source,
                patient_category AS "patient_category: _",
                disease_trends, outbreak_detection, predictive_risk_score,
                readmission_prediction, mortality_risk AS "mortality_risk: _",
                drug_recommendation, pattern_recognition,
                source AS "source: _",
                raw_blob_path, pipeline_processed, silver_processed, gold_processed,
                created_at, updated_at"#,
            patient_id,
            dto.full_name,
            dto.gender,
            dto.date_of_birth,
            dto.age,
            dto.marital_status,
            dto.nationality,
            dto.state,
            dto.city,
            dto.address,
            dto.phone_number,
            dto.emergency_contact,
            dto.blood_group,
            dto.genotype,
            dto.height_cm,
            dto.weight_kg,
            dto.allergies,
            dto.existing_conditions,
            dto.disability_status,
            dto.pregnancy_status,
            dto.vaccination_history,
            dto.current_medications,
            dto.disease_type as _,
            dto.symptoms,
            dto.severity_level as _,
            dto.symptom_start_date,
            dto.previous_medical_history,
            dto.family_medical_history,
            dto.weather_condition,
            dto.temperature,
            dto.humidity,
            dto.occupation,
            dto.smoking_status,
            dto.alcohol_consumption,
            dto.exercise_habits,
            dto.diet_type,
            dto.water_source,
            dto.patient_category as _,
            dto.source as _,
            blob_path,
        )
        .fetch_one(&self.pool)
        .await
    }

    pub async fn find_by_patient_id(&self, patient_id: &str) -> Result<Option<Patient>, sqlx::Error> {
        sqlx::query_as!(
            Patient,
            r#"SELECT
                id, patient_id, full_name, gender, date_of_birth, age, marital_status,
                nationality, state, city, address, phone_number, emergency_contact,
                blood_group, genotype, height_cm, weight_kg, allergies, existing_conditions,
                disability_status, pregnancy_status, vaccination_history, current_medications,
                disease_type AS "disease_type: _",
                symptoms, severity_level AS "severity_level: _",
                symptom_start_date, previous_medical_history, family_medical_history,
                weather_condition, temperature, humidity, occupation, smoking_status,
                alcohol_consumption, exercise_habits, diet_type, water_source,
                patient_category AS "patient_category: _",
                disease_trends, outbreak_detection, predictive_risk_score,
                readmission_prediction, mortality_risk AS "mortality_risk: _",
                drug_recommendation, pattern_recognition,
                source AS "source: _",
                raw_blob_path, pipeline_processed, silver_processed, gold_processed,
                created_at, updated_at
            FROM patients WHERE patient_id = $1"#,
            patient_id
        )
        .fetch_optional(&self.pool)
        .await
    }

    pub async fn find_by_uuid(&self, id: Uuid) -> Result<Option<Patient>, sqlx::Error> {
        sqlx::query_as!(
            Patient,
            r#"SELECT
                id, patient_id, full_name, gender, date_of_birth, age, marital_status,
                nationality, state, city, address, phone_number, emergency_contact,
                blood_group, genotype, height_cm, weight_kg, allergies, existing_conditions,
                disability_status, pregnancy_status, vaccination_history, current_medications,
                disease_type AS "disease_type: _",
                symptoms, severity_level AS "severity_level: _",
                symptom_start_date, previous_medical_history, family_medical_history,
                weather_condition, temperature, humidity, occupation, smoking_status,
                alcohol_consumption, exercise_habits, diet_type, water_source,
                patient_category AS "patient_category: _",
                disease_trends, outbreak_detection, predictive_risk_score,
                readmission_prediction, mortality_risk AS "mortality_risk: _",
                drug_recommendation, pattern_recognition,
                source AS "source: _",
                raw_blob_path, pipeline_processed, silver_processed, gold_processed,
                created_at, updated_at
            FROM patients WHERE id = $1"#,
            id
        )
        .fetch_optional(&self.pool)
        .await
    }

    pub async fn list(
        &self,
        risk: Option<&str>,
        state: Option<&str>,
        page: i64,
        page_size: i64,
    ) -> Result<(Vec<Patient>, i64), sqlx::Error> {
        let offset = (page - 1) * page_size;

        let patients = sqlx::query_as!(
            Patient,
            r#"SELECT
                id, patient_id, full_name, gender, date_of_birth, age, marital_status,
                nationality, state, city, address, phone_number, emergency_contact,
                blood_group, genotype, height_cm, weight_kg, allergies, existing_conditions,
                disability_status, pregnancy_status, vaccination_history, current_medications,
                disease_type AS "disease_type: _",
                symptoms, severity_level AS "severity_level: _",
                symptom_start_date, previous_medical_history, family_medical_history,
                weather_condition, temperature, humidity, occupation, smoking_status,
                alcohol_consumption, exercise_habits, diet_type, water_source,
                patient_category AS "patient_category: _",
                disease_trends, outbreak_detection, predictive_risk_score,
                readmission_prediction, mortality_risk AS "mortality_risk: _",
                drug_recommendation, pattern_recognition,
                source AS "source: _",
                raw_blob_path, pipeline_processed, silver_processed, gold_processed,
                created_at, updated_at
            FROM patients
            WHERE ($1::text IS NULL OR mortality_risk::text = $1)
              AND ($2::text IS NULL OR state ILIKE $2)
            ORDER BY predictive_risk_score DESC NULLS LAST
            LIMIT $3 OFFSET $4"#,
            risk,
            state,
            page_size,
            offset
        )
        .fetch_all(&self.pool)
        .await?;

        let total: i64 = sqlx::query_scalar!(
            "SELECT COUNT(*) FROM patients
             WHERE ($1::text IS NULL OR mortality_risk::text = $1)
               AND ($2::text IS NULL OR state ILIKE $2)",
            risk,
            state
        )
        .fetch_one(&self.pool)
        .await?
        .unwrap_or(0);

        Ok((patients, total))
    }

    pub async fn find_unprocessed_silver(&self) -> Result<Vec<Patient>, sqlx::Error> {
        sqlx::query_as!(
            Patient,
            r#"SELECT
                id, patient_id, full_name, gender, date_of_birth, age, marital_status,
                nationality, state, city, address, phone_number, emergency_contact,
                blood_group, genotype, height_cm, weight_kg, allergies, existing_conditions,
                disability_status, pregnancy_status, vaccination_history, current_medications,
                disease_type AS "disease_type: _",
                symptoms, severity_level AS "severity_level: _",
                symptom_start_date, previous_medical_history, family_medical_history,
                weather_condition, temperature, humidity, occupation, smoking_status,
                alcohol_consumption, exercise_habits, diet_type, water_source,
                patient_category AS "patient_category: _",
                disease_trends, outbreak_detection, predictive_risk_score,
                readmission_prediction, mortality_risk AS "mortality_risk: _",
                drug_recommendation, pattern_recognition,
                source AS "source: _",
                raw_blob_path, pipeline_processed, silver_processed, gold_processed,
                created_at, updated_at
            FROM patients WHERE pipeline_processed = FALSE
            ORDER BY created_at ASC LIMIT 100"#
        )
        .fetch_all(&self.pool)
        .await
    }

    pub async fn find_unprocessed_gold(&self) -> Result<Vec<Patient>, sqlx::Error> {
        sqlx::query_as!(
            Patient,
            r#"SELECT
                id, patient_id, full_name, gender, date_of_birth, age, marital_status,
                nationality, state, city, address, phone_number, emergency_contact,
                blood_group, genotype, height_cm, weight_kg, allergies, existing_conditions,
                disability_status, pregnancy_status, vaccination_history, current_medications,
                disease_type AS "disease_type: _",
                symptoms, severity_level AS "severity_level: _",
                symptom_start_date, previous_medical_history, family_medical_history,
                weather_condition, temperature, humidity, occupation, smoking_status,
                alcohol_consumption, exercise_habits, diet_type, water_source,
                patient_category AS "patient_category: _",
                disease_trends, outbreak_detection, predictive_risk_score,
                readmission_prediction, mortality_risk AS "mortality_risk: _",
                drug_recommendation, pattern_recognition,
                source AS "source: _",
                raw_blob_path, pipeline_processed, silver_processed, gold_processed,
                created_at, updated_at
            FROM patients WHERE pipeline_processed = TRUE AND silver_processed = FALSE
            ORDER BY created_at ASC LIMIT 100"#
        )
        .fetch_all(&self.pool)
        .await
    }

    pub async fn mark_silver_complete(
        &self,
        patient_id: &str,
        symptoms: Option<&str>,
        existing_conditions: Option<&str>,
        patient_category: Option<&PatientCategory>,
        severity_level: Option<&crate::models::patient::SeverityLevel>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"UPDATE patients SET
                pipeline_processed = TRUE,
                symptoms = COALESCE($2, symptoms),
                existing_conditions = COALESCE($3, existing_conditions),
                patient_category = COALESCE($4, patient_category),
                severity_level = COALESCE($5, severity_level),
                updated_at = NOW()
            WHERE patient_id = $1"#,
            patient_id,
            symptoms,
            existing_conditions,
            patient_category as _,
            severity_level as _,
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn mark_gold_complete(
        &self,
        patient_id: &str,
        risk_score: f64,
        mortality_risk: &MortalityRisk,
        readmission: &str,
        drug: &str,
        outbreak: bool,
        pattern: bool,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"UPDATE patients SET
                silver_processed = TRUE,
                gold_processed = TRUE,
                predictive_risk_score = $2,
                mortality_risk = $3,
                readmission_prediction = $4,
                drug_recommendation = $5,
                outbreak_detection = $6,
                pattern_recognition = $7,
                updated_at = NOW()
            WHERE patient_id = $1"#,
            patient_id,
            risk_score,
            mortality_risk as _,
            readmission,
            drug,
            outbreak,
            pattern,
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn high_risk_queue(&self) -> Result<Vec<Patient>, sqlx::Error> {
        sqlx::query_as!(
            Patient,
            r#"SELECT
                id, patient_id, full_name, gender, date_of_birth, age, marital_status,
                nationality, state, city, address, phone_number, emergency_contact,
                blood_group, genotype, height_cm, weight_kg, allergies, existing_conditions,
                disability_status, pregnancy_status, vaccination_history, current_medications,
                disease_type AS "disease_type: _",
                symptoms, severity_level AS "severity_level: _",
                symptom_start_date, previous_medical_history, family_medical_history,
                weather_condition, temperature, humidity, occupation, smoking_status,
                alcohol_consumption, exercise_habits, diet_type, water_source,
                patient_category AS "patient_category: _",
                disease_trends, outbreak_detection, predictive_risk_score,
                readmission_prediction, mortality_risk AS "mortality_risk: _",
                drug_recommendation, pattern_recognition,
                source AS "source: _",
                raw_blob_path, pipeline_processed, silver_processed, gold_processed,
                created_at, updated_at
            FROM patients WHERE mortality_risk = 'High'
            ORDER BY predictive_risk_score DESC"#
        )
        .fetch_all(&self.pool)
        .await
    }

    pub async fn outbreak_alerts(&self) -> Result<Vec<Patient>, sqlx::Error> {
        sqlx::query_as!(
            Patient,
            r#"SELECT
                id, patient_id, full_name, gender, date_of_birth, age, marital_status,
                nationality, state, city, address, phone_number, emergency_contact,
                blood_group, genotype, height_cm, weight_kg, allergies, existing_conditions,
                disability_status, pregnancy_status, vaccination_history, current_medications,
                disease_type AS "disease_type: _",
                symptoms, severity_level AS "severity_level: _",
                symptom_start_date, previous_medical_history, family_medical_history,
                weather_condition, temperature, humidity, occupation, smoking_status,
                alcohol_consumption, exercise_habits, diet_type, water_source,
                patient_category AS "patient_category: _",
                disease_trends, outbreak_detection, predictive_risk_score,
                readmission_prediction, mortality_risk AS "mortality_risk: _",
                drug_recommendation, pattern_recognition,
                source AS "source: _",
                raw_blob_path, pipeline_processed, silver_processed, gold_processed,
                created_at, updated_at
            FROM patients WHERE outbreak_detection = TRUE
            ORDER BY created_at DESC"#
        )
        .fetch_all(&self.pool)
        .await
    }

    pub async fn stats(&self) -> Result<PipelineStats, sqlx::Error> {
        let row = sqlx::query!(
            r#"SELECT
                COUNT(*) AS total,
                COUNT(*) FILTER (WHERE pipeline_processed = FALSE) AS pending_silver,
                COUNT(*) FILTER (WHERE pipeline_processed = TRUE AND silver_processed = FALSE) AS pending_gold,
                COUNT(*) FILTER (WHERE mortality_risk = 'High') AS high_risk,
                COUNT(*) FILTER (WHERE outbreak_detection = TRUE) AS outbreak_active
            FROM patients"#
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(PipelineStats {
            total: row.total.unwrap_or(0),
            pending_silver: row.pending_silver.unwrap_or(0),
            pending_gold: row.pending_gold.unwrap_or(0),
            high_risk: row.high_risk.unwrap_or(0),
            outbreak_active: row.outbreak_active.unwrap_or(0),
        })
    }

    /// Count infectious patients in same state within last 7 days (for outbreak detection)
    pub async fn count_infectious_in_state_recent(&self, state: &str) -> Result<i64, sqlx::Error> {
        sqlx::query_scalar!(
            r#"SELECT COUNT(*) FROM patients
            WHERE state ILIKE $1
              AND disease_type = 'Infectious'
              AND created_at >= NOW() - INTERVAL '7 days'"#,
            state
        )
        .fetch_one(&self.pool)
        .await
        .map(|c| c.unwrap_or(0))
    }

    /// Persist ML inference results back to the patient record.
    pub async fn save_ml_assessment(
        &self,
        patient_id: &str,
        probable_condition: &str,
        ml_risk_level: &str,
        drug: &str,
        route_to: &str,
        department: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"UPDATE patients SET
                drug_recommendation = $2,
                disease_trends      = $3,
                updated_at          = NOW()
            WHERE patient_id = $1"#,
            patient_id,
            format!("{} — routed to {} / {}", drug, route_to, department),
            format!("ML: {} | risk={} | condition={}", drug, ml_risk_level, probable_condition),
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
