/// Spatial RSSI models: Inverse Distance Weighting (IDW) and Ordinary Kriging.

use crate::geo::haversine_m;
use alloc::vec::Vec;
use libm::{pow, sqrt};

// ─── Data types ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct Measurement {
    pub lat: f64,
    pub lon: f64,
    pub signal_dbm: f64,
}

// ─── IDW ─────────────────────────────────────────────────────────────────────

/// IDW exponent (power parameter).
const IDW_BETA: f64 = 2.0;
/// Minimum distance to avoid division by zero (1 metre).
const MIN_DIST_M: f64 = 1.0;

/// Interpolate RSSI at `(lat, lon)` using Inverse Distance Weighting.
/// Returns `None` if there are no measurements.
pub fn idw_predict(measurements: &[Measurement], lat: f64, lon: f64) -> Option<f64> {
    if measurements.is_empty() {
        return None;
    }

    let mut num = 0.0_f64;
    let mut den = 0.0_f64;

    for m in measurements {
        let d = haversine_m(lat, lon, m.lat, m.lon).max(MIN_DIST_M);
        let w = 1.0 / pow(d, IDW_BETA);
        num += w * m.signal_dbm;
        den += w;
    }

    if den == 0.0 { None } else { Some(num / den) }
}

/// Analytical gradient of IDW prediction wrt (lat, lon) of query point.
/// Returns `(∂rssi/∂lat, ∂rssi/∂lon)`.
pub fn idw_gradient(measurements: &[Measurement], lat: f64, lon: f64) -> (f64, f64) {
    if measurements.is_empty() {
        return (0.0, 0.0);
    }

    let mut sum_w = 0.0_f64;
    let mut sum_wz = 0.0_f64;
    let mut sum_dw_dlat = 0.0_f64;
    let mut sum_dw_dlon = 0.0_f64;
    let mut sum_dwz_dlat = 0.0_f64;
    let mut sum_dwz_dlon = 0.0_f64;

    for m in measurements {
        let d = haversine_m(lat, lon, m.lat, m.lon).max(MIN_DIST_M);
        let w = 1.0 / pow(d, IDW_BETA);
        sum_w += w;
        sum_wz += w * m.signal_dbm;

        // ∂w/∂d = −β / d^(β+1)
        let dw_dd = -IDW_BETA / pow(d, IDW_BETA + 1.0);

        // ∂d/∂lat, ∂d/∂lon via haversine gradient
        let (dd_dlat, dd_dlon) = crate::geo::haversine_grad(lat, lon, m.lat, m.lon);

        let dw_dlat = dw_dd * dd_dlat;
        let dw_dlon = dw_dd * dd_dlon;

        sum_dw_dlat += dw_dlat;
        sum_dw_dlon += dw_dlon;
        sum_dwz_dlat += dw_dlat * m.signal_dbm;
        sum_dwz_dlon += dw_dlon * m.signal_dbm;
    }

    if sum_w == 0.0 {
        return (0.0, 0.0);
    }

    // Quotient rule: (num/den)' = (num'·den − num·den') / den²
    let drssi_dlat = (sum_dwz_dlat * sum_w - sum_wz * sum_dw_dlat) / (sum_w * sum_w);
    let drssi_dlon = (sum_dwz_dlon * sum_w - sum_wz * sum_dw_dlon) / (sum_w * sum_w);

    (drssi_dlat, drssi_dlon)
}

// ─── Kriging ─────────────────────────────────────────────────────────────────

/// Spherical variogram model parameters.
#[derive(Debug, Clone)]
pub struct VariogramModel {
    pub nugget: f64,
    pub sill: f64,
    pub range_m: f64,
}

impl Default for VariogramModel {
    fn default() -> Self {
        Self { nugget: 5.0, sill: 50.0, range_m: 1000.0 }
    }
}

impl VariogramModel {
    /// γ(h) for spherical model.
    pub fn eval(&self, h: f64) -> f64 {
        if h <= 0.0 {
            return 0.0;
        }
        let r = self.range_m;
        if h < r {
            let hr = h / r;
            self.nugget + self.sill * (1.5 * hr - 0.5 * hr * hr * hr)
        } else {
            self.nugget + self.sill
        }
    }

    /// Fit variogram parameters from empirical lag-gamma pairs via simple WLS.
    pub fn fit(lags: &[f64], gammas: &[f64], counts: &[usize]) -> Self {
        // Grid search over (nugget, sill, range) — simple but robust for our sizes
        let best_range = lags.last().copied().unwrap_or(1000.0);
        let max_gamma = gammas.iter().cloned().fold(f64::NEG_INFINITY, f64::max);

        let mut best = VariogramModel {
            nugget: 0.0,
            sill: max_gamma.max(1.0),
            range_m: best_range,
        };
        let mut best_err = f64::INFINITY;

        for nugget_frac in [0.0_f64, 0.05, 0.1, 0.2] {
            for sill_frac in [0.5_f64, 0.75, 1.0, 1.25] {
                for range_frac in [0.3_f64, 0.5, 0.7, 1.0, 1.3] {
                    let candidate = VariogramModel {
                        nugget: nugget_frac * max_gamma,
                        sill: (sill_frac * max_gamma - nugget_frac * max_gamma).max(0.1),
                        range_m: range_frac * best_range,
                    };
                    let err: f64 = lags
                        .iter()
                        .zip(gammas.iter())
                        .zip(counts.iter())
                        .map(|((h, g), n)| {
                            let w = *n as f64;
                            let diff = candidate.eval(*h) - g;
                            w * diff * diff
                        })
                        .sum();
                    if err < best_err {
                        best_err = err;
                        best = candidate;
                    }
                }
            }
        }
        best
    }
}

/// Compute empirical variogram from measurements.
pub fn empirical_variogram(
    measurements: &[Measurement],
    n_lags: usize,
    max_dist_m: f64,
) -> (Vec<f64>, Vec<f64>, Vec<usize>) {
    let lag_width = max_dist_m / n_lags as f64;
    let mut lags: Vec<f64> = (0..n_lags).map(|i| (i as f64 + 0.5) * lag_width).collect();
    let mut gamma_sum = vec![0.0_f64; n_lags];
    let mut counts = vec![0usize; n_lags];

    for i in 0..measurements.len() {
        for j in (i + 1)..measurements.len() {
            let h = haversine_m(
                measurements[i].lat, measurements[i].lon,
                measurements[j].lat, measurements[j].lon,
            );
            let bin = (h / lag_width) as usize;
            if bin < n_lags {
                let diff = measurements[i].signal_dbm - measurements[j].signal_dbm;
                gamma_sum[bin] += diff * diff;
                counts[bin] += 1;
            }
        }
    }

    let mut result_lags = Vec::new();
    let mut result_gamma = Vec::new();
    let mut result_counts = Vec::new();

    for i in 0..n_lags {
        if counts[i] > 0 {
            result_lags.push(lags[i]);
            result_gamma.push(gamma_sum[i] / (2.0 * counts[i] as f64));
            result_counts.push(counts[i]);
        }
    }

    (result_lags, result_gamma, result_counts)
}

/// Ordinary Kriging interpolator for a single cell tower.
pub struct KrigingModel {
    measurements: Vec<Measurement>,
    variogram: VariogramModel,
    /// Precomputed Γ⁻¹ (extended with Lagrange row/col), shape (n+1)×(n+1).
    gamma_inv: Vec<f64>, // row-major, dimension (n+1)×(n+1)
    n: usize,
}

impl KrigingModel {
    pub const MIN_MEASUREMENTS: usize = 5;

    /// Build Kriging model from measurements.
    /// Falls back to IDW if too few data.
    pub fn new(measurements: Vec<Measurement>) -> Option<Self> {
        let n = measurements.len();
        if n < Self::MIN_MEASUREMENTS {
            return None;
        }

        // Build empirical variogram
        let max_dist = {
            let mut md = 0.0_f64;
            for i in 0..n {
                for j in (i + 1)..n {
                    let h = haversine_m(
                        measurements[i].lat, measurements[i].lon,
                        measurements[j].lat, measurements[j].lon,
                    );
                    if h > md { md = h; }
                }
            }
            md
        };

        let (lags, gammas, counts) = empirical_variogram(&measurements, 8, max_dist.max(1.0));
        let variogram = if !lags.is_empty() {
            VariogramModel::fit(&lags, &gammas, &counts)
        } else {
            VariogramModel::default()
        };

        // Build extended Γ matrix (n+1 × n+1) with Lagrange multiplier row/col
        let size = n + 1;
        let mut gamma_mat = vec![0.0_f64; size * size];

        for i in 0..n {
            for j in 0..n {
                let h = haversine_m(
                    measurements[i].lat, measurements[i].lon,
                    measurements[j].lat, measurements[j].lon,
                );
                gamma_mat[i * size + j] = variogram.eval(h);
            }
            // Lagrange column/row
            gamma_mat[i * size + n] = 1.0;
            gamma_mat[n * size + i] = 1.0;
        }
        // Bottom-right corner = 0
        gamma_mat[n * size + n] = 0.0;

        // Invert via Gaussian elimination
        let gamma_inv = lu_inverse(&gamma_mat, size)?;

        Some(Self { measurements, variogram, gamma_inv, n })
    }

    /// Predict RSSI and kriging variance at `(lat, lon)`.
    /// Returns `(rssi_hat, sigma2)`.
    pub fn predict(&self, lat: f64, lon: f64) -> (f64, f64) {
        let n = self.n;
        let size = n + 1;

        // Build γ(p) vector (length n+1)
        let mut gamma_p = vec![0.0_f64; size];
        for i in 0..n {
            let h = haversine_m(lat, lon, self.measurements[i].lat, self.measurements[i].lon);
            gamma_p[i] = self.variogram.eval(h);
        }
        gamma_p[n] = 1.0; // Lagrange

        // λ = Γ⁻¹ · γ(p)
        let mut lambda = vec![0.0_f64; size];
        for i in 0..size {
            for j in 0..size {
                lambda[i] += self.gamma_inv[i * size + j] * gamma_p[j];
            }
        }

        // Prediction
        let rssi: f64 = (0..n).map(|i| lambda[i] * self.measurements[i].signal_dbm).sum();

        // Kriging variance: σ² = γᵀλ + μ (λ[n] is Lagrange multiplier μ)
        let sigma2: f64 = (0..size).map(|i| gamma_p[i] * lambda[i]).sum();

        (rssi, sigma2.max(0.0))
    }
}

/// Simple Gaussian elimination matrix inverse.
/// `mat` is row-major of size `n × n`. Returns flattened inverse or None if singular.
fn lu_inverse(mat: &[f64], n: usize) -> Option<Vec<f64>> {
    let mut a = mat.to_vec();
    let mut inv = vec![0.0_f64; n * n];
    // Identity
    for i in 0..n {
        inv[i * n + i] = 1.0;
    }

    for col in 0..n {
        // Find pivot
        let (pivot_row, pivot_val) = (col..n)
            .map(|r| (r, a[r * n + col].abs()))
            .max_by(|x, y| x.1.partial_cmp(&y.1).unwrap())?;

        if pivot_val < 1e-12 {
            return None; // Singular
        }

        // Swap rows
        if pivot_row != col {
            for j in 0..n {
                a.swap(col * n + j, pivot_row * n + j);
                inv.swap(col * n + j, pivot_row * n + j);
            }
        }

        // Eliminate
        let diag = a[col * n + col];
        for j in 0..n {
            a[col * n + j] /= diag;
            inv[col * n + j] /= diag;
        }
        for row in 0..n {
            if row != col {
                let factor = a[row * n + col];
                for j in 0..n {
                    let av = a[col * n + j];
                    let iv = inv[col * n + j];
                    a[row * n + j] -= factor * av;
                    inv[row * n + j] -= factor * iv;
                }
            }
        }
    }

    Some(inv)
}

/// Unified spatial model: uses Kriging if enough data, IDW otherwise.
pub enum SpatialModel {
    Kriging(KrigingModel),
    Idw(Vec<Measurement>),
}

impl SpatialModel {
    pub fn build(measurements: Vec<Measurement>) -> Self {
        match KrigingModel::new(measurements.clone()) {
            Some(k) => SpatialModel::Kriging(k),
            None => SpatialModel::Idw(measurements),
        }
    }

    /// Returns (rssi_hat, variance).
    pub fn predict(&self, lat: f64, lon: f64) -> (f64, f64) {
        match self {
            SpatialModel::Kriging(k) => k.predict(lat, lon),
            SpatialModel::Idw(m) => {
                let rssi = idw_predict(m, lat, lon).unwrap_or(-100.0);
                (rssi, 64.0) // 8 dBm std → 64 variance
            }
        }
    }

    pub fn gradient(&self, lat: f64, lon: f64) -> (f64, f64) {
        match self {
            SpatialModel::Idw(m) => idw_gradient(m, lat, lon),
            SpatialModel::Kriging(k) => {
                // Numerical gradient for Kriging (small step)
                let eps = 1e-6;
                let (r1, _) = k.predict(lat + eps, lon);
                let (r2, _) = k.predict(lat - eps, lon);
                let (r3, _) = k.predict(lat, lon + eps);
                let (r4, _) = k.predict(lat, lon - eps);
                ((r1 - r2) / (2.0 * eps), (r3 - r4) / (2.0 * eps))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn synthetic_measurements() -> Vec<Measurement> {
        // Fake grid of measurements around (55.75, 37.62)
        let mut v = Vec::new();
        for dlat in [-0.01_f64, 0.0, 0.01] {
            for dlon in [-0.01_f64, 0.0, 0.01] {
                let dist = haversine_m(55.75, 37.62, 55.75 + dlat, 37.62 + dlon);
                v.push(Measurement {
                    lat: 55.75 + dlat,
                    lon: 37.62 + dlon,
                    signal_dbm: -70.0 - dist / 100.0, // Decreases with distance
                });
            }
        }
        v
    }

    #[test]
    fn idw_monotonic() {
        let m = synthetic_measurements();
        let r_near = idw_predict(&m, 55.75, 37.62).unwrap();
        let r_far = idw_predict(&m, 55.80, 37.62).unwrap();
        assert!(r_near > r_far, "IDW should give stronger signal closer to tower area");
    }

    #[test]
    fn kriging_builds() {
        let m = synthetic_measurements();
        let km = KrigingModel::new(m);
        assert!(km.is_some(), "Kriging should build with 9 measurements");
    }

    #[test]
    fn variogram_model_eval() {
        let v = VariogramModel { nugget: 5.0, sill: 50.0, range_m: 1000.0 };
        assert!(v.eval(0.0) == 0.0);
        assert!(v.eval(500.0) > v.eval(0.0));
        assert!((v.eval(2000.0) - 55.0).abs() < 0.1);
    }
}
