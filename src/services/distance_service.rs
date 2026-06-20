use crate::models::admin_registration::Coordinates;
use crate::models::distance::*;
use crate::services::here_maps::{HereError, HereMapsClient};
use crate::utils::geo::haversine_km;
use std::sync::Arc;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum DistanceServiceError {
    #[error("Invalid location input: {0}")]
    InvalidInput(String),
    #[error("HERE Maps API error: {0}")]
    HereApiError(#[from] HereError),
    #[error("Location resolution failed: {0}")]
    LocationResolution(String),
}

pub struct DistanceService {
    here_client: Arc<HereMapsClient>,
    fallback_to_haversine: bool,
}

impl DistanceService {
    pub fn new(here_client: Arc<HereMapsClient>, fallback_to_haversine: bool) -> Self {
        Self {
            here_client,
            fallback_to_haversine,
        }
    }

    pub async fn calculate_distance(
        &self,
        request: DistanceRequest,
    ) -> Result<DistanceResponse, DistanceServiceError> {
        // Resolve origin coordinates
        let origin_coords = match self.resolve_coordinates(&request.origin).await {
            Ok(coords) => coords,
            Err(e) => {
                tracing::warn!("Failed to resolve origin coordinates: {}", e);
                return Err(e);
            }
        };

        // Resolve destination coordinates
        let destination_coords = match self.resolve_coordinates(&request.destination).await {
            Ok(coords) => coords,
            Err(e) => {
                tracing::warn!("Failed to resolve destination coordinates: {}", e);
                return Err(e);
            }
        };

        // Get addresses (try HERE API, fallback to coordinates display)
        let origin_address = self
            .resolve_address(&request.origin, &origin_coords)
            .await
            .unwrap_or_else(|_| format!("{}, {}", origin_coords.latitude, origin_coords.longitude));
        let destination_address = self
            .resolve_address(&request.destination, &destination_coords)
            .await
            .unwrap_or_else(|_| {
                format!(
                    "{}, {}",
                    destination_coords.latitude, destination_coords.longitude
                )
            });

        // Calculate route using HERE API or fallback
        let route_result = self
            .here_client
            .calculate_route(
                &origin_coords,
                &destination_coords,
                Some(&request.transport_mode),
            )
            .await;

        match route_result {
            Ok(route) => {
                let section = &route.sections[0];
                let summary = &section.summary;

                Ok(DistanceResponse {
                    origin: LocationDetails {
                        coordinates: origin_coords,
                        address: origin_address,
                    },
                    destination: LocationDetails {
                        coordinates: destination_coords,
                        address: destination_address,
                    },
                    distance: DistanceInfo::new(summary.length),
                    travel_time: TimeInfo::new(summary.duration),
                    route_summary: RouteSummary::new(
                        request.transport_mode,
                        summary.duration,
                        summary.base_duration,
                        route.id,
                    ),
                })
            }
            Err(e) if self.fallback_to_haversine => {
                tracing::warn!("HERE API failed, falling back to Haversine: {}", e);
                self.calculate_haversine_fallback(
                    origin_coords,
                    destination_coords,
                    origin_address,
                    destination_address,
                    request.transport_mode,
                )
                .await
            }
            Err(e) => {
                tracing::error!("HERE API failed and fallback disabled: {}", e);
                Err(DistanceServiceError::HereApiError(e))
            }
        }
    }

    async fn resolve_coordinates(
        &self,
        input: &LocationInput,
    ) -> Result<Coordinates, DistanceServiceError> {
        match &input.location_type {
            LocationType::Coordinates => input.coordinates.clone().ok_or_else(|| {
                DistanceServiceError::InvalidInput(
                    "Coordinates required when type is 'coordinates'".to_string(),
                )
            }),
            LocationType::Address => {
                let address = input.address.as_ref().ok_or_else(|| {
                    DistanceServiceError::InvalidInput(
                        "Address required when type is 'address'".to_string(),
                    )
                })?;

                self.here_client
                    .geocode_address_string(address)
                    .await
                    .map_err(DistanceServiceError::HereApiError)
            }
        }
    }

    async fn resolve_address(
        &self,
        input: &LocationInput,
        coords: &Coordinates,
    ) -> Result<String, DistanceServiceError> {
        match &input.location_type {
            LocationType::Address => {
                if let Some(addr) = &input.address {
                    Ok(addr.clone())
                } else {
                    self.here_client
                        .reverse_geocode(coords)
                        .await
                        .map_err(DistanceServiceError::HereApiError)
                }
            }
            LocationType::Coordinates => self
                .here_client
                .reverse_geocode(coords)
                .await
                .map_err(DistanceServiceError::HereApiError),
        }
    }

    async fn calculate_haversine_fallback(
        &self,
        origin: Coordinates,
        destination: Coordinates,
        origin_address: String,
        destination_address: String,
        transport_mode: String,
    ) -> Result<DistanceResponse, DistanceServiceError> {
        let distance_km = haversine_km(
            origin.latitude,
            origin.longitude,
            destination.latitude,
            destination.longitude,
        );

        let distance_meters = (distance_km * 1000.0) as i32;

        // Estimate travel time based on transport mode
        let estimated_speed_kph = match transport_mode.as_str() {
            "car" => 40.0,       // Average city driving speed
            "pedestrian" => 5.0, // Walking speed
            "truck" => 30.0,     // Slower for trucks
            _ => 40.0,
        };

        let travel_time_seconds = ((distance_km / estimated_speed_kph) * 3600.0) as i32;

        Ok(DistanceResponse {
            origin: LocationDetails {
                coordinates: origin,
                address: origin_address,
            },
            destination: LocationDetails {
                coordinates: destination,
                address: destination_address,
            },
            distance: DistanceInfo::new(distance_meters),
            travel_time: TimeInfo::new(travel_time_seconds),
            route_summary: RouteSummary::new(
                transport_mode,
                travel_time_seconds,
                travel_time_seconds, // No traffic data in fallback
                "haversine-fallback".to_string(),
            ),
        })
    }
}
