use crate::models::admin_registration::{Address, Coordinates};
use crate::models::here_maps::*;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum HereError {
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),
    #[error("API error: {0}")]
    Api(String),
    #[error("Invalid coordinates: {0}")]
    InvalidCoordinates(String),
    #[error("Location not found: {0}")]
    LocationNotFound(String),
    #[error("No route found")]
    NoRouteFound,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HereRouteResponse {
    pub routes: Vec<HereRoute>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HereRoute {
    pub id: String,
    pub sections: Vec<HereSection>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HereSection {
    pub departure: HerePlace,
    pub arrival: HerePlace,
    pub summary: HereSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HerePlace {
    pub place: HereLocation,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HereLocation {
    #[serde(rename = "location")]
    pub coordinates: HereCoordinates,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HereCoordinates {
    pub lat: f64,
    pub lng: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HereSummary {
    pub duration: i32,      // seconds
    pub length: i32,        // meters
    pub base_duration: i32, // seconds without traffic
}

#[derive(Debug, Serialize, Deserialize)]
pub struct HereGeocodeResponse {
    pub items: Vec<HereGeocodeItem>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct HereGeocodeItem {
    pub position: HereCoordinates,
    pub address: HereAddress,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct HereAddress {
    pub label: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct HereDiscoverResponse {
    pub items: Vec<HereDiscoverItem>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct HereDiscoverItem {
    pub id: String,
    pub title: String,
    #[serde(rename = "resultType")]
    pub result_type: String,
    pub address: HereAddressDetail,
    pub position: HereCoordinates,
    pub distance: Option<i32>,
    pub categories: Option<Vec<HereCategory>>,
    pub contacts: Option<Vec<HereContact>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct HereAddressDetail {
    pub label: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct HereCategory {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct HereContact {
    pub mobile: Option<Vec<HereContactValue>>,
    pub www: Option<Vec<HereContactValue>>,
    pub email: Option<Vec<HereContactValue>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct HereContactValue {
    pub value: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct HereAutosuggestResponse {
    pub items: Vec<HereAutosuggestItem>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct HereAutosuggestItem {
    pub title: String,
    pub id: String,
    #[serde(rename = "resultType")]
    pub result_type: String,
    pub address: Option<HereAddressDetail>,
    pub position: Option<HereCoordinates>,
    pub distance: Option<i32>,
}

pub struct HereMapsClient {
    client: Client,
    api_key: String,
}

impl HereMapsClient {
    pub fn new(api_key: String) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(10))
            .user_agent("NexusCare/1.0")
            .build()
            .expect("Failed to create HTTP client");

        Self { client, api_key }
    }

    pub async fn geocode_address(&self, address: &Address) -> Result<Coordinates, HereError> {
        let address_string = self.format_address(address);
        self.geocode_address_string(&address_string).await
    }

    pub async fn geocode_address_string(
        &self,
        address_string: &str,
    ) -> Result<Coordinates, HereError> {
        let url = "https://geocode.search.hereapi.com/v1/geocode";
        let response = self
            .client
            .get(url)
            .query(&[
                ("q", address_string),
                ("apiKey", &self.api_key),
                ("limit", "1"),
            ])
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(HereError::Api(format!(
                "HTTP {}: {}",
                response.status(),
                response.text().await.unwrap_or_default()
            )));
        }

        let result: HereGeocodeResponse = response.json().await?;

        if result.items.is_empty() {
            return Err(HereError::LocationNotFound("Address not found".to_string()));
        }

        Ok(Coordinates {
            latitude: result.items[0].position.lat,
            longitude: result.items[0].position.lng,
        })
    }

    pub async fn calculate_route(
        &self,
        origin: &Coordinates,
        destination: &Coordinates,
        transport_mode: Option<&str>,
    ) -> Result<HereRoute, HereError> {
        let url = "https://router.hereapi.com/v8/routes";

        let origin_str = format!("{},{}", origin.latitude, origin.longitude);
        let destination_str = format!("{},{}", destination.latitude, destination.longitude);
        let mode = transport_mode.unwrap_or("car");

        let response = self
            .client
            .get(url)
            .query(&[
                ("transportMode", mode),
                ("origin", &origin_str),
                ("destination", &destination_str),
                ("return", "summary"),
                ("apiKey", &self.api_key),
            ])
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(HereError::Api(format!(
                "HTTP {}: {}",
                response.status(),
                response.text().await.unwrap_or_default()
            )));
        }

        let result: HereRouteResponse = response.json().await?;

        if result.routes.is_empty() {
            return Err(HereError::NoRouteFound);
        }

        Ok(result.routes[0].clone())
    }

    pub async fn reverse_geocode(&self, coordinates: &Coordinates) -> Result<String, HereError> {
        let url = "https://revgeocode.search.hereapi.com/v1/revgeocode";

        let at_param = format!("{},{}", coordinates.latitude, coordinates.longitude);

        let response = self
            .client
            .get(url)
            .query(&[("at", &at_param), ("apiKey", &self.api_key)])
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(HereError::Api(format!("HTTP {}", response.status())));
        }

        let result: HereGeocodeResponse = response.json().await?;

        if result.items.is_empty() {
            return Ok(format!(
                "{}, {}",
                coordinates.latitude, coordinates.longitude
            ));
        }

        Ok(result.items[0].address.label.clone())
    }

    pub async fn discover_facilities(
        &self,
        query: &str,
        center: &Coordinates,
        radius_meters: i32,
        limit: i32,
    ) -> Result<Vec<Facility>, HereError> {
        let url = "https://discover.search.hereapi.com/v1/discover";
        let in_param = format!(
            "circle:{},{};r={}",
            center.latitude, center.longitude, radius_meters
        );

        let response = self
            .client
            .get(url)
            .query(&[
                ("q", query),
                ("in", &in_param),
                ("limit", &limit.to_string()),
                ("apiKey", &self.api_key),
            ])
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(HereError::Api(format!(
                "HTTP {}: {}",
                response.status(),
                response.text().await.unwrap_or_default()
            )));
        }

        let result: HereDiscoverResponse = response.json().await?;

        let facilities = result
            .items
            .into_iter()
            .filter(|item| item.result_type == "place")
            .map(|item| self.convert_discover_item_to_facility(item))
            .collect();

        Ok(facilities)
    }

    pub async fn autosuggest(
        &self,
        query: &str,
        center: &Coordinates,
        radius_meters: i32,
    ) -> Result<Vec<AddressSuggestion>, HereError> {
        let url = "https://autosuggest.search.hereapi.com/v1/autosuggest";
        let in_param = format!(
            "circle:{},{};r={}",
            center.latitude, center.longitude, radius_meters
        );

        let response = self
            .client
            .get(url)
            .query(&[("q", query), ("in", &in_param), ("apiKey", &self.api_key)])
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(HereError::Api(format!(
                "HTTP {}: {}",
                response.status(),
                response.text().await.unwrap_or_default()
            )));
        }

        let result: HereAutosuggestResponse = response.json().await?;

        let suggestions = result
            .items
            .into_iter()
            .map(|item| AddressSuggestion {
                title: item.title,
                address: item
                    .address
                    .map_or_else(|| "".to_string(), |addr| addr.label),
                position: item.position.map(|pos| Position {
                    latitude: pos.lat,
                    longitude: pos.lng,
                }),
                result_type: item.result_type,
            })
            .collect();

        Ok(suggestions)
    }

    fn convert_discover_item_to_facility(&self, item: HereDiscoverItem) -> Facility {
        let categories = item
            .categories
            .unwrap_or_default()
            .into_iter()
            .map(|cat| cat.name)
            .collect();

        let contacts = item.contacts.and_then(|contacts| {
            if !contacts.is_empty() {
                let contact = &contacts[0];
                Some(ContactInfo {
                    phone: contact
                        .mobile
                        .as_ref()
                        .and_then(|mobile| mobile.first())
                        .map(|m| m.value.clone()),
                    email: contact
                        .email
                        .as_ref()
                        .and_then(|email| email.first())
                        .map(|e| e.value.clone()),
                    website: contact
                        .www
                        .as_ref()
                        .and_then(|www| www.first())
                        .map(|w| w.value.clone()),
                })
            } else {
                None
            }
        });

        Facility {
            id: item.id,
            title: item.title,
            address: item.address.label,
            position: Position {
                latitude: item.position.lat,
                longitude: item.position.lng,
            },
            distance_meters: item.distance.unwrap_or(0),
            categories,
            contacts,
        }
    }

    fn format_address(&self, address: &Address) -> String {
        let mut parts = vec![address.line1.clone()];

        if let Some(line2) = &address.line2 {
            if !line2.is_empty() {
                parts.push(line2.clone());
            }
        }

        parts.push(address.city.clone());
        parts.push(address.state.clone());
        parts.push(address.country.clone());

        parts.join(", ")
    }
}
