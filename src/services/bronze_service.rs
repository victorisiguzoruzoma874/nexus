use std::io;
use std::fs;
use std::sync::Arc;
use chrono::Utc;
use serde_json::json;

use crate::{
    models::patient::{IngestPatientDto, IngestResponse, Patient},
    repositories::patient::PatientRepository,
    utils::errors::AppError,
};

pub struct BronzeService {
    patient_repo: Arc<PatientRepository>,
    raw_storage_path: String,
}

impl BronzeService {
    pub fn new(patient_repo: Arc<PatientRepository>) -> Self {
        let raw_storage_path = std::env::var("RAW_STORAGE_PATH")
            .unwrap_or_else(|_| "./raw-storage".to_string());
        fs::create_dir_all(&raw_storage_path).ok();
        Self { patient_repo, raw_storage_path }
    }

    pub async fn ingest(&self, dto: IngestPatientDto, source_ip: &str) -> Result<(Patient, IngestResponse), AppError> {
        let patient_id = format!("P{}", Utc::now().timestamp_millis());
        let blob_path = format!("{}/{}.json", self.raw_storage_path, patient_id);

        let blob = json!({
            "patient_id": patient_id,
            "ingested_at": Utc::now(),
            "source_ip": source_ip,
            "payload": dto,
        });

        fs::write(&blob_path, blob.to_string())
            .map_err(|e: io::Error| AppError::Internal(anyhow::anyhow!(e)))?;

        let patient = self.patient_repo
            .create(&patient_id, &dto, &blob_path)
            .await
            .map_err(AppError::Database)?;

        Ok((patient, IngestResponse { patient_id, message: "Received" }))
    }

    pub fn read_blob(&self, patient_id: &str) -> Result<serde_json::Value, AppError> {
        let path = format!("{}/{}.json", self.raw_storage_path, patient_id);
        let content = fs::read_to_string(&path)
            .map_err(|_| AppError::NotFound(format!("Blob not found for {}", patient_id)))?;
        serde_json::from_str(&content)
            .map_err(|e| AppError::Internal(anyhow::anyhow!(e)))
    }
}
