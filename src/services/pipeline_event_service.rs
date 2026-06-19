use std::sync::Arc;
use tokio::sync::broadcast;
use serde::{Deserialize, Serialize};

/// Every stage completion broadcasts one of these
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum PipelineEvent {
    /// Bronze write complete — patient received
    Ingested {
        patient_id: String,
        message: String,
    },
    /// Silver normalization complete
    SilverComplete {
        patient_id: String,
    },
    /// Gold feature engineering complete — risk score available
    GoldComplete {
        patient_id: String,
        risk_score: f64,
        mortality_risk: String,
    },
    /// Full ML inference complete
    AssessmentReady {
        patient_id: String,
        risk_score: f64,
        mortality_risk: String,
        probable_condition: String,
        route_to: String,
        department: String,
        alert_priority: u8,
    },
    /// High-risk alert (for nurse room)
    HighRiskAlert {
        patient_id: String,
        patient_name: String,
        department: String,
        alert_priority: u8,
    },
    /// Outbreak detected in a state
    OutbreakAlert {
        patient_id: String,
        state: String,
        disease_type: String,
    },
    /// Stage error
    PipelineError {
        patient_id: String,
        stage: String,
        error: String,
    },
}

/// Shared event bus — clone freely, all clones share the same channel
#[derive(Clone)]
pub struct PipelineEventService {
    sender: Arc<broadcast::Sender<PipelineEvent>>,
}

impl PipelineEventService {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(256);
        Self { sender: Arc::new(tx) }
    }

    pub fn publish(&self, event: PipelineEvent) {
        // send() only fails if there are no receivers — that's fine
        let _ = self.sender.send(event);
    }

    pub fn subscribe(&self) -> broadcast::Receiver<PipelineEvent> {
        self.sender.subscribe()
    }
}

impl Default for PipelineEventService {
    fn default() -> Self {
        Self::new()
    }
}
