use axum::{
    extract::{Query, State},
    response::{sse::{Event, KeepAlive, Sse}, IntoResponse},
};
use futures_util::stream::{self, StreamExt};
use serde::Deserialize;
use tokio_stream::wrappers::BroadcastStream;

use crate::{routes::AppState, services::pipeline_event_service::PipelineEvent};

#[derive(Debug, Deserialize)]
pub struct SseQuery {
    /// Optional: subscribe to one patient only
    pub patient_id: Option<String>,
    /// Role filter: "nurse" | "admin" | "doctor" (default: all events)
    pub role: Option<String>,
}

/// GET /api/v1/pipeline/events
/// SSE stream — clients connect once and receive real-time pipeline updates.
///
/// Events pushed:
///   - pipeline:status       → all roles
///   - patient:assessment    → all roles
///   - alert:high-risk       → role=nurse
///   - alert:outbreak        → role=admin
///   - pipeline:error        → all roles
pub async fn pipeline_events(
    State(state): State<AppState>,
    Query(query): Query<SseQuery>,
) -> impl IntoResponse {
    let rx = state.event_service.subscribe();
    let patient_filter = query.patient_id.clone();
    let role = query.role.clone().unwrap_or_default();

    let stream = BroadcastStream::new(rx)
        .filter_map(move |msg| {
            let patient_filter = patient_filter.clone();
            let role = role.clone();
            async move {
                let event = msg.ok()?;

                // Role-based filter
                let allowed = match &event {
                    PipelineEvent::HighRiskAlert { .. } => role == "nurse" || role.is_empty(),
                    PipelineEvent::OutbreakAlert { .. } => role == "admin" || role.is_empty(),
                    _ => true,
                };
                if !allowed {
                    return None;
                }

                // Optional patient_id filter
                if let Some(ref pid) = patient_filter {
                    let event_pid = match &event {
                        PipelineEvent::Ingested { patient_id, .. } => Some(patient_id),
                        PipelineEvent::SilverComplete { patient_id } => Some(patient_id),
                        PipelineEvent::GoldComplete { patient_id, .. } => Some(patient_id),
                        PipelineEvent::AssessmentReady { patient_id, .. } => Some(patient_id),
                        PipelineEvent::HighRiskAlert { patient_id, .. } => Some(patient_id),
                        PipelineEvent::OutbreakAlert { patient_id, .. } => Some(patient_id),
                        PipelineEvent::PipelineError { patient_id, .. } => Some(patient_id),
                    };
                    if event_pid.map(|id| id != pid).unwrap_or(true) {
                        return None;
                    }
                }

                let (event_name, data) = match &event {
                    PipelineEvent::Ingested { .. } => ("pipeline:status", serde_json::to_string(&event).ok()?),
                    PipelineEvent::SilverComplete { .. } => ("pipeline:status", serde_json::to_string(&event).ok()?),
                    PipelineEvent::GoldComplete { .. } => ("pipeline:status", serde_json::to_string(&event).ok()?),
                    PipelineEvent::AssessmentReady { .. } => ("patient:assessment", serde_json::to_string(&event).ok()?),
                    PipelineEvent::HighRiskAlert { .. } => ("alert:high-risk", serde_json::to_string(&event).ok()?),
                    PipelineEvent::OutbreakAlert { .. } => ("alert:outbreak", serde_json::to_string(&event).ok()?),
                    PipelineEvent::PipelineError { .. } => ("pipeline:error", serde_json::to_string(&event).ok()?),
                };

                Some(Ok::<Event, axum::Error>(Event::default().event(event_name).data(data)))
            }
        });

    // Prepend a "connected" confirmation event
    let connect_event = stream::once(async {
        Ok::<Event, axum::Error>(
            Event::default()
                .event("connected")
                .data(r#"{"status":"connected"}"#),
        )
    });

    Sse::new(connect_event.chain(stream))
        .keep_alive(KeepAlive::default())
}
