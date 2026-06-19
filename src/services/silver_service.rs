use std::{collections::HashMap, sync::Arc};

use crate::{
    models::patient::{Patient, PatientCategory, SeverityLevel},
    repositories::patient::PatientRepository,
};

pub struct SilverService {
    patient_repo: Arc<PatientRepository>,
}

/// Medical term normalization maps
fn symptom_map() -> HashMap<&'static str, &'static str> {
    let mut m = HashMap::new();
    m.insert("fever", "Pyrexia");
    m.insert("headache", "Cephalalgia");
    m.insert("stomach ache", "Abdominal pain");
    m.insert("stomach pain", "Abdominal pain");
    m.insert("high bp", "Hypertension");
    m.insert("sugar", "Diabetes mellitus");
    m.insert("fits", "Seizure");
    m.insert("fits ", "Seizure");
    m.insert("cough", "Cough");
    m.insert("chest pain", "Chest pain");
    m.insert("shortness of breath", "Dyspnoea");
    m.insert("fatigue", "Fatigue");
    m.insert("dizziness", "Vertigo");
    m.insert("vomiting", "Emesis");
    m.insert("nausea", "Nausea");
    m.insert("joint pain", "Arthralgia");
    m.insert("back pain", "Dorsalgia");
    m
}

fn condition_map() -> HashMap<&'static str, &'static str> {
    let mut m = HashMap::new();
    m.insert("high bp", "Hypertension");
    m.insert("sugar", "Diabetes mellitus");
    m.insert("diabetes", "Diabetes mellitus");
    m.insert("hypertension", "Hypertension");
    m.insert("heart disease", "Cardiovascular disease");
    m.insert("asthma", "Asthma");
    m.insert("cancer", "Malignancy");
    m.insert("sickle cell", "Sickle cell disease");
    m
}

fn normalize_text(raw: &str, map: &HashMap<&str, &str>) -> String {
    let lower = raw.to_lowercase();
    let mut result = raw.to_string();
    for (raw_term, medical_term) in map {
        if lower.contains(raw_term) {
            // Replace case-insensitively
            let re = regex::Regex::new(&format!("(?i){}", regex::escape(raw_term))).unwrap();
            result = re.replace_all(&result, *medical_term).to_string();
        }
    }
    result
}

fn infer_patient_category(age: i32) -> PatientCategory {
    match age {
        0..=12 => PatientCategory::Child,
        13..=19 => PatientCategory::Teenager,
        20..=64 => PatientCategory::Adult,
        _ => PatientCategory::Elderly,
    }
}

fn infer_severity(risk_score: f64) -> SeverityLevel {
    match risk_score {
        s if s >= 0.7 => SeverityLevel::Critical,
        s if s >= 0.5 => SeverityLevel::Severe,
        s if s >= 0.3 => SeverityLevel::Moderate,
        _ => SeverityLevel::Mild,
    }
}

impl SilverService {
    pub fn new(patient_repo: Arc<PatientRepository>) -> Self {
        Self { patient_repo }
    }

    pub async fn process_patient(&self, patient: &Patient) -> Result<(), sqlx::Error> {
        let sym_map = symptom_map();
        let cond_map = condition_map();

        let symptoms = patient.symptoms.as_deref()
            .map(|s| normalize_text(s, &sym_map));

        let conditions = patient.existing_conditions.as_deref()
            .map(|c| normalize_text(c, &cond_map));

        let category = if patient.patient_category.is_none() {
            patient.age.map(infer_patient_category)
        } else {
            None
        };

        let severity = if patient.severity_level.is_none() {
            patient.predictive_risk_score.map(infer_severity)
        } else {
            None
        };

        self.patient_repo.mark_silver_complete(
            &patient.patient_id,
            symptoms.as_deref(),
            conditions.as_deref(),
            category.as_ref(),
            severity.as_ref(),
        ).await
    }

    pub async fn process_pending(&self) -> Result<usize, sqlx::Error> {
        let pending = self.patient_repo.find_unprocessed_silver().await?;
        let count = pending.len();
        for patient in pending {
            self.process_patient(&patient).await.ok();
        }
        Ok(count)
    }
}
