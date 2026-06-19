// ! Re-broadcast cadence scheduler (FRS ..14).

use std::sync::Arc;
use std::time::Duration;

use crate::services::shift_service::ShiftService;

pub struct BroadcastScheduler {
    service: Arc<ShiftService>,
    poll_secs: u64,
}

impl BroadcastScheduler {
    pub fn new(service: Arc<ShiftService>) -> Self {
        let poll_secs = std::env::var("BROADCAST_SCHEDULER_POLL_SECS")
            .ok()
            .and_then(|v| v.parse(). ok())
            .unwrap_or(60);
        Self { service, poll_secs }
    }

    pub async fn run(self) {
        let mut interval = tokio::time::interval(Duration::from_secs(self.poll_secs));
        // Skip the initial tick that fires immediately so we don't double up
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        interval.tick(). await;

        loop {
            interval.tick(). await;
            match self.service.rebroadcast_due_shifts(). await {
                Ok(0) => {}
                Ok(n) => tracing::info!("Broadcast scheduler re-broadcast {n} shift(s)"),
                Err(e) => tracing::error!("Broadcast scheduler tick failed: {e}"),
            }
        }
    }
}
