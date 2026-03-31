/// Accuracy metrics: CEP50/95, GDOP, PCA error ellipse.

use crate::geo::haversine_m;
use crate::montecarlo::McPoint;
use alloc::vec::Vec;
use libm::{atan2, sqrt};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorEllipse {
    /// Semi-major axis in metres (95% confidence).
    pub semi_major_m: f64,
    /// Semi-minor axis in metres (95% confidence).
    pub semi_minor_m: f64,
    /// Rotation angle in degrees (clockwise from North).
    pub angle_deg: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccuracyMetrics {
    /// Circular Error Probable 50% — radius containing 50% of estimates (m).
    pub cep50_m: f64,
    /// Circular Error Probable 95% — radius containing 95% of estimates (m).
    pub cep95_m: f64,
    /// Geometric Dilution of Precision.
    pub gdop: f64,
    /// 95% error ellipse from PCA.
    pub ellipse: ErrorEllipse,
    /// Mean estimated position.
    pub mean_lat: f64,
    pub mean_lon: f64,
    /// Number of valid MC samples.
    pub n_samples: usize,
}

/// Compute CEP50 and CEP95 from Monte Carlo cloud, given truth point.
/// If truth is None, use the mean of the cloud as reference.
pub fn compute_cep(points: &[McPoint], truth_lat: f64, truth_lon: f64) -> (f64, f64) {
    if points.is_empty() {
        return (f64::NAN, f64::NAN);
    }

    let mut distances: Vec<f64> = points
        .iter()
        .map(|p| haversine_m(truth_lat, truth_lon, p.lat, p.lon))
        .collect();

    distances.sort_by(|a, b| a.partial_cmp(b).unwrap());

    let cep50 = percentile(&distances, 50.0);
    let cep95 = percentile(&distances, 95.0);
    (cep50, cep95)
}

fn percentile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return f64::NAN;
    }
    let n = sorted.len();
    let idx = (p / 100.0 * n as f64).min(n as f64 - 1.0) as usize;
    sorted[idx]
}

/// Compute 95% error ellipse via PCA of the MC cloud.
pub fn compute_error_ellipse(points: &[McPoint]) -> ErrorEllipse {
    let n = points.len();
    if n < 2 {
        return ErrorEllipse { semi_major_m: 0.0, semi_minor_m: 0.0, angle_deg: 0.0 };
    }

    // Compute mean
    let mean_lat = points.iter().map(|p| p.lat).sum::<f64>() / n as f64;
    let mean_lon = points.iter().map(|p| p.lon).sum::<f64>() / n as f64;

    // Convert offsets to metres using haversine approximation
    // Δlat in metres ≈ Δlat_deg × 111_320
    // Δlon in metres ≈ Δlon_deg × 111_320 × cos(lat)
    let lat_scale = 111_320.0_f64;
    let lon_scale = 111_320.0_f64 * libm::cos(mean_lat.to_radians());

    let xs: Vec<f64> = points.iter().map(|p| (p.lat - mean_lat) * lat_scale).collect();
    let ys: Vec<f64> = points.iter().map(|p| (p.lon - mean_lon) * lon_scale).collect();

    // 2×2 covariance matrix
    let cov_xx: f64 = xs.iter().map(|x| x * x).sum::<f64>() / n as f64;
    let cov_yy: f64 = ys.iter().map(|y| y * y).sum::<f64>() / n as f64;
    let cov_xy: f64 = xs.iter().zip(ys.iter()).map(|(x, y)| x * y).sum::<f64>() / n as f64;

    // Eigenvalues of 2×2 symmetric matrix
    let trace = cov_xx + cov_yy;
    let det = cov_xx * cov_yy - cov_xy * cov_xy;
    let disc = sqrt((trace * trace / 4.0 - det).max(0.0));
    let lambda1 = trace / 2.0 + disc;
    let lambda2 = (trace / 2.0 - disc).max(0.0);

    // k = 2.4477 for 95% confidence on 2D normal (chi2 with 2 dof, p=0.95)
    const K95: f64 = 2.4477;
    let semi_major_m = K95 * sqrt(lambda1);
    let semi_minor_m = K95 * sqrt(lambda2);

    // Angle of major eigenvector (in degrees, clockwise from North)
    let angle_rad = atan2(cov_xy, lambda1 - cov_yy);
    let angle_deg = angle_rad.to_degrees();

    ErrorEllipse { semi_major_m, semi_minor_m, angle_deg }
}

/// Compute GDOP from tower geometry relative to estimated position.
///
/// H matrix rows = unit vectors from position to each tower.
/// GDOP = sqrt(trace((HᵀH)⁻¹))
pub fn compute_gdop(
    pos_lat: f64,
    pos_lon: f64,
    tower_lats: &[f64],
    tower_lons: &[f64],
) -> f64 {
    let n = tower_lats.len();
    if n < 2 {
        return f64::INFINITY;
    }

    let lat_scale = 111_320.0_f64;
    let lon_scale = 111_320.0_f64 * libm::cos(pos_lat.to_radians());

    // Build H (n × 2): each row is the unit direction from pos to tower
    let mut hth = [[0.0_f64; 2]; 2];

    for i in 0..n {
        let dx = (tower_lats[i] - pos_lat) * lat_scale;
        let dy = (tower_lons[i] - pos_lon) * lon_scale;
        let dist = sqrt(dx * dx + dy * dy).max(1.0);
        let hx = dx / dist;
        let hy = dy / dist;

        hth[0][0] += hx * hx;
        hth[0][1] += hx * hy;
        hth[1][0] += hx * hy;
        hth[1][1] += hy * hy;
    }

    // Invert 2×2
    let det = hth[0][0] * hth[1][1] - hth[0][1] * hth[1][0];
    if det.abs() < 1e-12 {
        return f64::INFINITY;
    }

    let inv_00 = hth[1][1] / det;
    let inv_11 = hth[0][0] / det;

    sqrt(inv_00 + inv_11)
}

/// Full accuracy metrics from Monte Carlo cloud.
pub fn compute_metrics(
    points: &[McPoint],
    truth_lat: f64,
    truth_lon: f64,
    tower_lats: &[f64],
    tower_lons: &[f64],
) -> AccuracyMetrics {
    let n = points.len();
    let mean_lat = if n > 0 { points.iter().map(|p| p.lat).sum::<f64>() / n as f64 } else { truth_lat };
    let mean_lon = if n > 0 { points.iter().map(|p| p.lon).sum::<f64>() / n as f64 } else { truth_lon };

    let (cep50_m, cep95_m) = compute_cep(points, truth_lat, truth_lon);
    let ellipse = compute_error_ellipse(points);
    let gdop = compute_gdop(mean_lat, mean_lon, tower_lats, tower_lons);

    AccuracyMetrics {
        cep50_m,
        cep95_m,
        gdop,
        ellipse,
        mean_lat,
        mean_lon,
        n_samples: n,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cep_from_tight_cloud() {
        // Cloud tightly around truth
        let pts: Vec<McPoint> = (0..100)
            .map(|i| McPoint { lat: 55.75 + (i as f64 * 0.000001), lon: 37.62 })
            .collect();
        let (cep50, cep95) = compute_cep(&pts, 55.75, 37.62);
        assert!(cep50 < cep95, "CEP50 < CEP95");
        assert!(cep95 < 200.0, "Tight cloud should have small CEP95");
    }

    #[test]
    fn gdop_symmetric() {
        // 4 towers symmetrically around centre → GDOP ≈ √2 ≈ 1.41
        let lats = [55.76, 55.74, 55.75, 55.75];
        let lons = [37.62, 37.62, 37.63, 37.61];
        let g = compute_gdop(55.75, 37.62, &lats, &lons);
        assert!(g > 0.5 && g < 5.0, "GDOP = {}", g);
    }
}
