// ! Payout scheduler. // !

use std::sync::Arc;
use std::time::Duration;

use crate::services::payout_service::PayoutService;

pub struct PayoutScheduler {
    service: Arc<PayoutService>,
    poll_secs: u64,
}

impl PayoutScheduler {
    pub fn new(service: Arc<PayoutService>) -> Self {
        let poll_secs = std::env::var("PAYOUT_SCHEDULER_POLL_SECS")
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
            match self.service.run_tick(). await {
                Ok(0) => {}
                Ok(n) => tracing::info!("Payout scheduler initiated {n} transfer(s)"),
                Err(e) => tracing::error!("Payout scheduler tick failed: {e}"),
            }

            // Settle any transfers still pending at SafeHaven (async completions).
            match self.service.poll_pending_transfers(). await {
                Ok(0) => {}
                Ok(n) => tracing::info!("Payout scheduler settled {n} pending transfer(s)"),
                Err(e) => tracing::error!("Payout status poll failed: {e}"),
            }
        }
    }
}
