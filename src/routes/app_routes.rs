use axum::{
    routing::{get, patch, post},
    Router,
};
use sqlx::PgPool;
use std::sync::Arc;
use tower_http::{
    cors::{Any, CorsLayer},
    trace::TraceLayer,
};
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

use crate::handlers::{auth, health, hospitals, registration, clinician_registration, shifts};
use crate::repositories::{
    audit::AuditRepository,
    billing::BillingRepository,
    hospital::HospitalRepository,
    location::LocationRepository,
    clinician::ClinicianRepository,
    shift::ShiftRepository,
};
use crate::services::{
    audit_service::AuditService,
    auth_service::AuthService,
    encryption::EncryptionService,
    geocoding::GeocodingClient,
    location_service::LocationService,
    notification_service::NotificationService,
    payment_service::PaymentService,
    paystack::PaystackClient,
    registration_service::RegistrationService,
    sms_service::SmsService,
    clinician_registration_service::ClinicianRegistrationService,
    shift_service::ShiftService,
};

/// Shared application state
#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub registration_service: Arc<RegistrationService>,
    pub clinician_registration_service: Arc<ClinicianRegistrationService>,
    pub auth_service: Arc<AuthService>,
    pub shift_service: Arc<ShiftService>,
}

/// API Documentation
#[derive(OpenApi)]
#[openapi(
    paths(
        // Health
        crate::handlers::health::health_check,
        crate::handlers::health::db_health_check,
        // Auth
        crate::handlers::auth::register,
        crate::handlers::auth::login,
        crate::handlers::auth::phone_otp_send,
        crate::handlers::auth::phone_otp_verify,
        crate::handlers::auth::forgot_password,
        crate::handlers::auth::reset_password,
        crate::handlers::auth::refresh_token,
        crate::handlers::auth::logout,
        // Hospital Registration (Admin)
        crate::handlers::registration::register_hospital,
        crate::handlers::registration::list_hospitals,
        crate::handlers::registration::get_registration_status,
        crate::handlers::registration::approve_hospital,
        crate::handlers::registration::reject_hospital,
        // Clinician Registration
        crate::handlers::clinician_registration::send_otp,
        crate::handlers::clinician_registration::verify_otp,
        crate::handlers::clinician_registration::complete_profile,
        crate::handlers::clinician_registration::add_bank_account,
        // Shifts
        crate::handlers::shifts::create_shift,
        crate::handlers::shifts::preview_shift,
        crate::handlers::shifts::get_shift,
    ),
    components(
        schemas(
            // Registration
            crate::handlers::registration::HospitalRegistrationResponse,
            crate::handlers::registration::StatusChangeResponse,
            crate::handlers::registration::ApprovalRequest,
            crate::handlers::registration::RejectionRequest,
            crate::handlers::registration::ErrorResponse,
            crate::handlers::registration::ListHospitalsQuery,
            // Shifts
            crate::handlers::shifts::ShiftPreviewResponse,
            crate::handlers::shifts::ErrorResponse,
            // Models
            crate::models::admin_registration::HospitalRegistrationRequest,
            crate::models::admin_registration::Address,
            crate::models::admin_registration::PaymentDetails,
            crate::models::admin_registration::PaymentMethodType,
            crate::models::shift::Shift,
            crate::models::shift::CreateShiftRequest,
            crate::models::shift::ShiftStatus,
            crate::models::shift::ShiftPriority,
            crate::models::shift::ShiftType,
            crate::models::shift::RoleCategory,
            crate::models::shift::PayType,
            crate::models::user::UserResponse,
            crate::models::user::CreateUserRequest,
            crate::models::user::LoginRequest,
            crate::models::user::LoginResponse,
            crate::models::user::PhoneLoginRequest,
            crate::models::user::OtpVerifyRequest,
            crate::models::user::ForgotPasswordRequest,
            crate::models::user::ResetPasswordRequest,
            crate::models::user::RefreshTokenRequest,
            crate::models::user::LogoutRequest,
            crate::models::clinician_registration::SendOtpRequest,
            crate::models::clinician_registration::SendOtpResponse,
            crate::models::clinician_registration::VerifyOtpRequest,
            crate::models::clinician_registration::VerifyOtpResponse,
            crate::models::clinician_registration::CompleteProfileRequest,
            crate::models::clinician_registration::ProfileResponse,
            crate::models::clinician_registration::AddBankAccountRequest,
            crate::models::clinician_registration::BankAccountResponse,
            // Services
            crate::services::registration_service::RegistrationStatusResponse,
            crate::services::registration_service::HospitalListResponse,
            crate::services::registration_service::HospitalSummary,
            crate::services::registration_service::PaginationMetadata,
        )
    ),
    info(
        title = "NexusCare Hospital Management API",
        version = "1.0.0",
        description = "Complete API for hospital management, clinician registration, authentication, and shift creation",
        contact(
            name = "NexusCare Support",
            email = "support@nexuscare.com"
        )
    ),
    servers(
        (url = "http://localhost:8080", description = "Local development server"),
        (url = "https://api.nexuscare.com", description = "Production server")
    ),
    tags(
        (name = "health", description = "Health check endpoints"),
        (name = "auth", description = "Authentication and authorization endpoints"),
        (name = "hospitals", description = "Hospital management endpoints"),
        (name = "clinicians", description = "Clinician registration and management endpoints"),
        (name = "shifts", description = "Shift creation and management endpoints"),
        (name = "admin", description = "Admin-only endpoints")
    )
)]
struct ApiDoc;

pub fn create_router(pool: PgPool) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    // Initialize repositories
    let hospital_repo = Arc::new(HospitalRepository::new(pool.clone()));
    let location_repo = Arc::new(LocationRepository::new(pool.clone()));
    let billing_repo = Arc::new(BillingRepository::new(pool.clone()));
    let audit_repo = Arc::new(AuditRepository::new(pool.clone()));
    let clinician_repo = Arc::new(ClinicianRepository::new(pool.clone()));
    let shift_repo = Arc::new(ShiftRepository::new(pool.clone()));

    // Initialize external services
    let geocoding_client = Arc::new(GeocodingClient::new(
        std::env::var("GEOCODING_API_URL").ok(),
    ));

    let paystack_client = Arc::new(PaystackClient::new(
        std::env::var("PAYSTACK_SECRET_KEY")
            .unwrap_or_else(|_| "sk_test_dummy".to_string()),
        std::env::var("PAYSTACK_API_URL").ok(),
    ));

    let encryption_service = Arc::new({
        let key_hex = std::env::var("ENCRYPTION_KEY")
            .unwrap_or_else(|_| "0".repeat(64));
        let key_bytes = hex::decode(&key_hex)
            .unwrap_or_else(|_| vec![0u8; 32]);
        EncryptionService::new(key_bytes)
            .expect("Failed to create encryption service")
    });

    // Initialize business services
    let location_service = Arc::new(LocationService::new(
        geocoding_client,
        location_repo.clone(),
    ));

    let payment_service = Arc::new(PaymentService::new(
        paystack_client.clone(),
        billing_repo.clone(),
        encryption_service.clone(),
    ));

    let audit_service = Arc::new(AuditService::new(audit_repo));

    let notification_service = Arc::new(NotificationService::new());

    // Initialize registration service
    let registration_service = Arc::new(RegistrationService::new(
        hospital_repo,
        location_service,
        payment_service,
        audit_service,
        notification_service.clone(),
        pool.clone(),
    ));

    // Initialize clinician registration service
    let sms_service = Arc::new(SmsService::new(
        std::env::var("TERMII_API_KEY").unwrap_or_else(|_| "mock".to_string()),
        std::env::var("TERMII_API_URL").ok(),
    ));

    let clinician_registration_service = Arc::new(ClinicianRegistrationService::new(
        clinician_repo,
        sms_service.clone(),
        paystack_client.clone(),
        encryption_service.clone(),
        pool.clone(),
    ));

    // Initialize auth service
    let auth_service = Arc::new(AuthService::new(pool.clone(), sms_service, notification_service.clone()));

    // Initialize shift service
    let shift_service = Arc::new(ShiftService::new(shift_repo, pool.clone(), notification_service.clone()));

    // Create shared state
    let state = AppState {
        pool: pool.clone(),
        registration_service,
        clinician_registration_service,
        auth_service,
        shift_service,
    };

    // Create API routes
    let api_router = Router::new()
        // Health
        .route("/health", get(health::health_check))
        .route("/health/db", get(health::db_health_check))
        // Auth
        .route("/api/v1/auth/register", post(auth::register))
        .route("/api/v1/auth/login", post(auth::login))
        .route("/api/v1/auth/otp/send", post(auth::phone_otp_send))
        .route("/api/v1/auth/otp/verify", post(auth::phone_otp_verify))
        .route("/api/v1/auth/forgot-password", post(auth::forgot_password))
        .route("/api/v1/auth/reset-password", post(auth::reset_password))
        .route("/api/v1/auth/refresh", post(auth::refresh_token))
        .route("/api/v1/auth/logout", post(auth::logout))
        // Hospital Registration
        .route(
            "/api/v1/hospitals/register",
            post(registration::register_hospital),
        )
        .route(
            "/api/v1/hospitals",
            get(registration::list_hospitals),
        )
        .route(
            "/api/v1/hospitals/{hospital_id}/status",
            get(registration::get_registration_status),
        )
        // Admin endpoints
        .route(
            "/api/v1/admin/hospitals/{hospital_id}/approve",
            post(registration::approve_hospital),
        )
        .route(
            "/api/v1/admin/hospitals/{hospital_id}/reject",
            post(registration::reject_hospital),
        )
        // Existing Hospitals endpoints (legacy - for backward compatibility)
        .route("/api/v1/hospitals/create", post(hospitals::create_hospital))
        .route("/api/v1/hospitals/{id}", get(hospitals::get_hospital))
        .route("/api/v1/hospitals/{id}", patch(hospitals::update_hospital))
        .route(
            "/api/v1/hospitals/{id}/advance-step",
            patch(hospitals::advance_registration_step),
        )
        // Clinician registration
        .route(
            "/api/v1/clinicians/otp/send",
            post(clinician_registration::send_otp),
        )
        .route(
            "/api/v1/clinicians/otp/verify",
            post(clinician_registration::verify_otp),
        )
        .route(
            "/api/v1/clinicians/{clinician_id}/profile",
            axum::routing::put(clinician_registration::complete_profile),
        )
        .route(
            "/api/v1/clinicians/{clinician_id}/bank-account",
            post(clinician_registration::add_bank_account),
        )
        // Shifts
        .route("/api/v1/shifts", post(shifts::create_shift))
        .route("/api/v1/shifts/preview", post(shifts::preview_shift))
        .route("/api/v1/shifts/{shift_id}", get(shifts::get_shift))
        .layer(TraceLayer::new_for_http())
        .layer(cors)
        .with_state(state);

    // Merge with Swagger UI
    Router::new()
        .merge(SwaggerUi::new("/api/docs").url("/api/openapi.json", ApiDoc::openapi()))
        .merge(api_router)
}
