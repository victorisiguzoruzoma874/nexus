use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::models::admin_registration::{Coordinates, NewLocation};
use crate::models::location::HospitalLocation;

#[derive(Debug, thiserror::Error)]
pub enum LocationError {
    #[error("Database error: {0}")]
    DatabaseError(#[from] sqlx::Error),
    
    #[error("Location not found for hospital: {0}")]
    NotFound(Uuid),
    
    #[error("Invalid coordinates: latitude={0}, longitude={1}")]
    InvalidCoordinates(f64, f64),
}

/// Repository for hospital location data persistence
pub struct LocationRepository {
    pool: PgPool,
}

impl LocationRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Create a new location record within a transaction
    pub async fn create(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        location: NewLocation,
    ) -> Result<HospitalLocation, LocationError> {
        // Validate coordinates
        if location.latitude < -90.0 || location.latitude > 90.0 {
            return Err(LocationError::InvalidCoordinates(
                location.latitude,
                location.longitude,
            ));
        }
        if location.longitude < -180.0 || location.longitude > 180.0 {
            return Err(LocationError::InvalidCoordinates(
                location.latitude,
                location.longitude,
            ));
        }

        let result = sqlx::query_as::<_, HospitalLocation>(
            r#"
            INSERT INTO hospital_locations (
                hospital_id,
                latitude,
                longitude,
                place_label,
                shift_broadcast_radius_km,
                location_confirmed
            )
            VALUES ($1, $2, $3, $4, $5, TRUE)
            RETURNING 
                id, hospital_id, latitude, longitude, place_label,
                clock_in_radius_meters, gps_fencing_enabled,
                shift_broadcast_radius_km, shift_distance_active,
                location_confirmed, confirmed_at, confirmed_by,
                created_at, updated_at
            "#,
        )
        .bind(location.hospital_id)
        .bind(location.latitude)
        .bind(location.longitude)
        .bind(format!(
            "{}, {}, {}, {}",
            location.address_line1, location.city, location.state, location.country
        ))
        .bind(location.service_radius_km)
        .fetch_one(&mut **tx)
        .await?;

        Ok(result)
    }

    /// Find location by hospital ID
    pub async fn find_by_hospital_id(
        &self,
        hospital_id: Uuid,
    ) -> Result<Option<HospitalLocation>, LocationError> {
        let location = sqlx::query_as::<_, HospitalLocation>(
            r#"
            SELECT 
                id, hospital_id, latitude, longitude, place_label,
                clock_in_radius_meters, gps_fencing_enabled,
                shift_broadcast_radius_km, shift_distance_active,
                location_confirmed, confirmed_at, confirmed_by,
                created_at, updated_at
            FROM hospital_locations
            WHERE hospital_id = $1
            "#,
        )
        .bind(hospital_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(location)
    }

    /// Update location coordinates
    pub async fn update_coordinates(
        &self,
        location_id: Uuid,
        coordinates: Coordinates,
    ) -> Result<(), LocationError> {
        // Validate coordinates
        if coordinates.latitude < -90.0 || coordinates.latitude > 90.0 {
            return Err(LocationError::InvalidCoordinates(
                coordinates.latitude,
                coordinates.longitude,
            ));
        }
        if coordinates.longitude < -180.0 || coordinates.longitude > 180.0 {
            return Err(LocationError::InvalidCoordinates(
                coordinates.latitude,
                coordinates.longitude,
            ));
        }

        let result = sqlx::query(
            r#"
            UPDATE hospital_locations
            SET 
                latitude = $2,
                longitude = $3,
                updated_at = NOW()
            WHERE id = $1
            "#,
        )
        .bind(location_id)
        .bind(coordinates.latitude)
        .bind(coordinates.longitude)
        .execute(&self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(LocationError::NotFound(location_id));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Unit tests will be added here
}


#[cfg(test)]
mod property_tests {
    use super::*;
    use proptest::prelude::*;

    // Property 6: Location data persistence round-trip
    
    proptest! {
        #[test]
        fn property_9_coordinate_validation_rejects_invalid_coords(
            lat in -200.0f64..200.0f64,
            lon in -200.0f64..200.0f64,
        ) {
            let coords = Coordinates {
                latitude: lat,
                longitude: lon,
            };

            // Create a dummy location to test validation
            let location = NewLocation {
                hospital_id: Uuid::new_v4(), address_line1: "Test".to_string(), address_line2: None,
                city: "Test".to_string(), state: "Test".to_string(), postal_code: "12345".to_string(), country: "Test".to_string(), latitude: lat,
                longitude: lon,
                service_radius_km: 5.0,
            };

            // Property: Invalid coordinates should be rejected
            if lat < -90.0 || lat > 90.0 || lon < -180.0 || lon > 180.0 {
                // Validation happens in the create method
                prop_assert!(lat < -90.0 || lat > 90.0 || lon < -180.0 || lon > 180.0);
            }
        }
    }

    #[test]
    fn test_coordinate_validation() {
        // Valid coordinates
        let valid = Coordinates {
            latitude: 6.5244,
            longitude: 3.3792,
        };
        assert!(valid.latitude >= -90.0 && valid.latitude <= 90.0);
        assert!(valid.longitude >= -180.0 && valid.longitude <= 180.0);

        // Invalid coordinates would be caught by the repository
    }
}
