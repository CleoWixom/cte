/// Geodesic calculations: haversine distance, bearing, etc.

use libm::{asin, atan2, cos, sin, sqrt};

pub const EARTH_RADIUS_M: f64 = 6_371_000.0;

/// Haversine distance in metres between two WGS-84 points.
pub fn haversine_m(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let dlat = (lat2 - lat1).to_radians();
    let dlon = (lon2 - lon1).to_radians();
    let a = sin(dlat / 2.0) * sin(dlat / 2.0)
        + cos(lat1.to_radians()) * cos(lat2.to_radians()) * sin(dlon / 2.0) * sin(dlon / 2.0);
    let c = 2.0 * asin(sqrt(a));
    EARTH_RADIUS_M * c
}

/// Bearing in radians from point 1 → point 2.
pub fn bearing_rad(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let dlon = (lon2 - lon1).to_radians();
    let y = sin(dlon) * cos(lat2.to_radians());
    let x = cos(lat1.to_radians()) * sin(lat2.to_radians())
        - sin(lat1.to_radians()) * cos(lat2.to_radians()) * cos(dlon);
    atan2(y, x)
}

/// Partial derivatives of haversine distance wrt (lat, lon) of point `p`.
/// Returns (∂d/∂lat_p, ∂d/∂lon_p) in metres/radian.
pub fn haversine_grad(
    lat_p: f64, lon_p: f64,
    lat_t: f64, lon_t: f64,
) -> (f64, f64) {
    let d = haversine_m(lat_p, lon_p, lat_t, lon_t);
    if d < 1.0 {
        return (0.0, 0.0);
    }
    let dlat = (lat_t - lat_p).to_radians();
    let dlon = (lon_t - lon_p).to_radians();

    let sin_dlat2 = sin(dlat / 2.0);
    let cos_dlat2 = cos(dlat / 2.0);
    let sin_dlon2 = sin(dlon / 2.0);
    let cos_dlon2 = cos(dlon / 2.0);
    let cos_latp = cos(lat_p.to_radians());
    let sin_latp = sin(lat_p.to_radians());
    let cos_latt = cos(lat_t.to_radians());

    let a = sin_dlat2 * sin_dlat2
        + cos_latp * cos_latt * sin_dlon2 * sin_dlon2;
    let sqrt_a = sqrt(a);
    let sqrt_1ma = sqrt(1.0 - a);
    let c_factor = EARTH_RADIUS_M / (sqrt_a * sqrt_1ma);

    // ∂a/∂lat_p (radians)
    let da_dlat = -sin_dlat2 * cos_dlat2 /* chain: -1 for -dlat */
        + (-sin_latp) * cos_latt * sin_dlon2 * sin_dlon2;
    // ∂a/∂lon_p (radians)
    let da_dlon = -cos_latp * cos_latt * sin_dlon2 * cos_dlon2;

    let dd_dlat = c_factor * da_dlat;
    let dd_dlon = c_factor * da_dlon;

    // Convert from per-radian to per-degree
    (dd_dlat * (180.0 / core::f64::consts::PI), dd_dlon * (180.0 / core::f64::consts::PI))
}

/// Move a point by (delta_lat_m, delta_lon_m) metres.
pub fn move_point_m(lat: f64, lon: f64, dlat_m: f64, dlon_m: f64) -> (f64, f64) {
    let new_lat = lat + dlat_m / EARTH_RADIUS_M * (180.0 / core::f64::consts::PI);
    let new_lon = lon + dlon_m / (EARTH_RADIUS_M * cos(lat.to_radians()))
        * (180.0 / core::f64::consts::PI);
    (new_lat, new_lon)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn haversine_known() {
        // Moscow Kremlin → Red Square ≈ 300 m (actual ~295 m)
        let d = haversine_m(55.7520, 37.6175, 55.7539, 37.6208);
        assert!(d > 200.0 && d < 500.0, "d = {}", d);
    }

    #[test]
    fn haversine_zero() {
        let d = haversine_m(55.0, 37.0, 55.0, 37.0);
        assert!(d < 1e-6);
    }
}
