pub mod hospital;
pub mod location;
pub mod billing;
pub mod audit;
pub mod clinician;
pub mod shift;
pub mod wallet;
pub mod email_outbox;
pub mod identity_verification;

pub use hospital::HospitalRepository;
pub use location::LocationRepository;
pub use billing::BillingRepository;
pub use audit::AuditRepository;
pub use clinician::{ClinicianRepository, ClinicianRepoError};
pub use wallet::{WalletRepoError, WalletRepository};
pub use email_outbox::EmailOutboxRepository;
pub use identity_verification::{IdentityRepoError, IdentityVerificationRepository};
