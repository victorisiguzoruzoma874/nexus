pub mod hospital;
pub mod location;
pub mod billing;
pub mod audit;
pub mod clinician;
pub mod shift;
pub mod email_outbox;

pub use hospital::HospitalRepository;
pub use location::LocationRepository;
pub use billing::BillingRepository;
pub use audit::AuditRepository;
pub use clinician::{ClinicianRepository, ClinicianRepoError};
pub use email_outbox::EmailOutboxRepository;
