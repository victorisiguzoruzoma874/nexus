// ! Offer-expiry sweep (FRS ).

use std::sync::Arc;
use std::time::Duration;

use crate::services::shift_service::ShiftService;

pub struct OfferExpiryScheduler {
    service: Arc<ShiftService>,
    poll_secs: u64,
}

impl OfferExpiryScheduler {
    pub fn new(service: Arc<ShiftService>) -> Self {
        let poll_secs = std::env::var("OFFER_EXPIRY_POLL_SECS")
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
            match self.service.expire_due_offers(). await {
                Ok(0) => {}
                Ok(n) => tracing::info!("Offer-expiry scheduler expired {n} offer(s)"),
                Err(e) => tracing::error!("Offer-expiry scheduler tick failed: {e}"),
            }
        }
    }
}
