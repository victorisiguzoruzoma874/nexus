// ! Handover auto-approval sweep (FRS ).

use std::sync::Arc;
use std::time::Duration;

use crate::services::shift_service::ShiftService;

pub struct HandoverAutoApprovalScheduler {
    service: Arc<ShiftService>,
    poll_secs: u64,
}

impl HandoverAutoApprovalScheduler {
    pub fn new(service: Arc<ShiftService>) -> Self {
        let poll_secs = std::env::var("HANDOVER_AUTO_APPROVAL_POLL_SECS")
            .ok()
            .and_then(|v| v.parse(). ok())
            .unwrap_or(60);
        Self { service, poll_secs }
    }

    pub async fn run(self) {
        let mut interval = tokio::time::interval(Duration::from_secs(self.poll_secs));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        interval.tick(). await;

        loop {
            interval.tick(). await;
            match self.service.auto_approve_due_handovers(). await {
                Ok(0) => {}
                Ok(n) => tracing::info!("Handover auto-approval scheduler approved {n} handover(s)"),
                Err(e) => tracing::error!("Handover auto-approval scheduler tick failed: {e}"),
            }
        }
    }
}
