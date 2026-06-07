//! Geospatial helpers shared across the shift marketplace.

/// Mean Earth radius in kilometres.
const EARTH_RADIUS_KM: f64 = 6371.0088;

/// Great-circle distance between two `(lat, lng)` pairs in kilometres,
/// computed with the haversine formula. Inputs are degrees.
pub fn haversine_km(lat1: f64, lng1: f64, lat2: f64, lng2: f64) -> f64 {
    let lat1_r = lat1.to_radians();
    let lat2_r = lat2.to_radians();
    let d_lat = (lat2 - lat1).to_radians();
    let d_lng = (lng2 - lng1).to_radians();

    let a = (d_lat / 2.0).sin().powi(2)
        + lat1_r.cos() * lat2_r.cos() * (d_lng / 2.0).sin().powi(2);
    let c = 2.0 * a.sqrt().atan2((1.0 - a).sqrt());

    EARTH_RADIUS_KM * c
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_distance() {
        let d = haversine_km(6.5244, 3.3792, 6.5244, 3.3792);
        assert!(d < 0.001);
    }

    #[test]
    fn lagos_to_abuja_roughly_530km() {
        // Lagos (6.5244, 3.3792) → Abuja (9.0765, 7.3986) ≈ 535 km.
        let d = haversine_km(6.5244, 3.3792, 9.0765, 7.3986);
        assert!((525.0..545.0).contains(&d), "expected ~535km, got {d}");
    }

    #[test]
    fn symmetric() {
        let a = haversine_km(6.5244, 3.3792, 9.0765, 7.3986);
        let b = haversine_km(9.0765, 7.3986, 6.5244, 3.3792);
        assert!((a - b).abs() < 1e-9);
    }
}
