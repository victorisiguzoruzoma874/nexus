use chrono::{DateTime, Timelike, Utc};
use validator::ValidationError;

/// F1-F05: Validates that a shift start time falls on a 15-minute boundary

pub fn validate_15min_boundary(ts: &DateTime<Utc>) -> Result<(), ValidationError> {
    if ts.second() != 0 || ts.nanosecond() != 0 || ts.minute() % 15 != 0 {
        let mut error = ValidationError::new("invalid_time_boundary");
        error.message = Some("Start time must be on a 15-minute boundary".into());
        return Err(error);
    }
    Ok(())
}

/// Validates email format according to RFC 5322
pub fn validate_email_rfc5322(email: &str) -> Result<(), ValidationError> {
    // Basic RFC 5322 validation
    let email_regex = regex::Regex::new(
        r"^[a-zA-Z0-9.!#$%&'*+/=?^_`{|}~-]+@[a-zA-Z0-9](?:[a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?(?:\.[a-zA-Z0-9](?:[a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?)*$"
    ).unwrap(); if !email_regex.is_match(email) {
        return Err(ValidationError::new("invalid_email_format"));
    }
    
    Ok(())
}

/// Validates phone number format according to E.164 international format
pub fn validate_phone_e164(phone: &str) -> Result<(), ValidationError> {
    // E.164 format: starts with +, followed by 1-15 digits
    let phone_regex = regex::Regex::new(r"^\+[1-9]\d{1,14}$").unwrap(); if !phone_regex.is_match(phone) {
        let mut error = ValidationError::new("invalid_phone_format");
        error.message = Some("Phone number must be in E.164 format (e.g., +2348012345678)".into());
        return Err(error);
    }
    
    Ok(())
}

/// Validates coordinates are within valid geographic ranges
pub fn validate_coordinates(latitude: f64, longitude: f64) -> Result<(), ValidationError> {
    if latitude < -90.0 || latitude > 90.0 {
        let mut error = ValidationError::new("invalid_latitude");
        error.message = Some("Latitude must be between -90 and 90".into());
        return Err(error);
    }
    
    if longitude < -180.0 || longitude > 180.0 {
        let mut error = ValidationError::new("invalid_longitude");
        error.message = Some("Longitude must be between -180 and 180".into());
        return Err(error);
    }
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_email_rfc5322() {
        // Valid emails
        assert!(validate_email_rfc5322("test@example.com").is_ok());
        assert!(validate_email_rfc5322("user.name+tag@example.co.uk").is_ok());
        
        // Invalid emails
        assert!(validate_email_rfc5322("invalid").is_err());
        assert!(validate_email_rfc5322("@example.com").is_err());
        assert!(validate_email_rfc5322("test@").is_err());
    }

    #[test]
    fn test_validate_phone_e164() {
        // Valid E.164 phones
        assert!(validate_phone_e164("+2348012345678").is_ok());
        assert!(validate_phone_e164("+14155552671").is_ok());
        assert!(validate_phone_e164("+442071838750").is_ok());
        
        // Invalid phones
        assert!(validate_phone_e164("08012345678").is_err()); // Missing +
        assert!(validate_phone_e164("+0123456789").is_err()); // Starts with 0
        assert!(validate_phone_e164("234801234567").is_err()); // Missing +
    }

    #[test]
    fn test_validate_coordinates() {
        // Valid coordinates
        assert!(validate_coordinates(6.5244, 3.3792).is_ok()); // Lagos
        assert!(validate_coordinates(0.0, 0.0).is_ok()); // Null Island
        assert!(validate_coordinates(-90.0, -180.0).is_ok()); // Boundaries
        assert!(validate_coordinates(90.0, 180.0).is_ok()); // Boundaries
        
        // Invalid coordinates
        assert!(validate_coordinates(91.0, 0.0).is_err()); // Latitude too high
        assert!(validate_coordinates(-91.0, 0.0).is_err()); // Latitude too low
        assert!(validate_coordinates(0.0, 181.0).is_err()); // Longitude too high
        assert!(validate_coordinates(0.0, -181.0).is_err()); // Longitude too low
    }
}
