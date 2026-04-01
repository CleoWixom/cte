/// Levenberg-Marquardt non-linear least squares for cell triangulation.
/// Finds device position (lat, lon) that best explains observed RSSI values.

use crate::spatial::SpatialModel;
use alloc::vec::Vec;

pub struct TowerObservation<'a> {
    pub tower_lat: f64,
    pub tower_lon: f64,
    pub observed_rssi: f64,
    pub model: &'a SpatialModel,
}

#[derive(Debug, Clone)]
pub struct LmResult {
    pub lat: f64,
    pub lon: f64,
    /// Residual sum of squares at solution.
    pub cost: f64,
    /// Number of iterations taken.
    pub iterations: usize,
    /// Whether the solver converged.
    pub converged: bool,
}

/// Solve triangulation via Levenberg-Marquardt.
///
/// `init_lat`, `init_lon` — starting point (centroid of towers if unknown).
pub fn lm_solve(
    observations: &[TowerObservation],
    init_lat: f64,
    init_lon: f64,
) -> LmResult {
    const MAX_ITER: usize = 100;
    const TOL: f64 = 1e-8;
    const LAMBDA_INIT: f64 = 1e-3;
    const LAMBDA_UP: f64 = 10.0;
    const LAMBDA_DOWN: f64 = 0.1;

    let n = observations.len();
    if n == 0 {
        return LmResult { lat: init_lat, lon: init_lon, cost: f64::INFINITY, iterations: 0, converged: false };
    }

    let mut lat = init_lat;
    let mut lon = init_lon;
    let mut lambda = LAMBDA_INIT;

    let compute_residuals = |lat: f64, lon: f64| -> (Vec<f64>, Vec<f64>, f64) {
        let mut r = Vec::with_capacity(n);
        let mut w = Vec::with_capacity(n);
        let mut cost = 0.0_f64;
        for obs in observations.iter() {
            let (rssi_pred, variance) = obs.model.predict(lat, lon);
            let wi = 1.0 / variance.max(1.0);
            let ri = obs.observed_rssi - rssi_pred;
            r.push(ri);
            w.push(wi);
            cost += wi * ri * ri;
        }
        (r, w, cost * 0.5)
    };

    let compute_jacobian = |lat: f64, lon: f64| -> Vec<[f64; 2]> {
        observations.iter().map(|obs| {
            let (dlat, dlon) = obs.model.gradient(lat, lon);
            // Residual = observed − predicted → ∂r/∂p = −∂rssi_pred/∂p
            [-dlat, -dlon]
        }).collect()
    };

    let (mut r, mut w, mut cost) = compute_residuals(lat, lon);

    for iter in 0..MAX_ITER {
        let jac = compute_jacobian(lat, lon);

        // JᵀWJ (2×2) and JᵀWr (2×1)
        let mut jtj = [[0.0_f64; 2]; 2];
        let mut jtr = [0.0_f64; 2];

        for i in 0..n {
            let wi = w[i];
            let ri = r[i];
            let j = jac[i];
            for a in 0..2 {
                jtr[a] += wi * j[a] * ri;
                for b in 0..2 {
                    jtj[a][b] += wi * j[a] * j[b];
                }
            }
        }

        // Augment diagonal: (JᵀWJ + λI)
        let aug = [
            [jtj[0][0] + lambda, jtj[0][1]],
            [jtj[1][0], jtj[1][1] + lambda],
        ];

        // Solve 2×2 system
        let det = aug[0][0] * aug[1][1] - aug[0][1] * aug[1][0];
        if det.abs() < 1e-20 {
            break;
        }
        let dlat = (aug[1][1] * jtr[0] - aug[0][1] * jtr[1]) / det;
        let dlon = (-aug[1][0] * jtr[0] + aug[0][0] * jtr[1]) / det;

        let new_lat = lat + dlat;
        let new_lon = lon + dlon;
        let (new_r, new_w, new_cost) = compute_residuals(new_lat, new_lon);

        if new_cost < cost {
            lat = new_lat;
            lon = new_lon;
            r = new_r;
            w = new_w;
            cost = new_cost;
            lambda *= LAMBDA_DOWN;

            if dlat.abs() < TOL && dlon.abs() < TOL {
                return LmResult { lat, lon, cost, iterations: iter + 1, converged: true };
            }
        } else {
            lambda *= LAMBDA_UP;
        }
    }

    LmResult { lat, lon, cost, iterations: MAX_ITER, converged: false }
}

/// Weighted centroid of tower positions (weights = 1/range_m).
pub fn weighted_centroid(tower_lats: &[f64], tower_lons: &[f64], weights: &[f64]) -> (f64, f64) {
    let total_w: f64 = weights.iter().sum();
    if total_w == 0.0 {
        let n = tower_lats.len() as f64;
        return (
            tower_lats.iter().sum::<f64>() / n,
            tower_lons.iter().sum::<f64>() / n,
        );
    }
    let lat = tower_lats.iter().zip(weights).map(|(x, w)| x * w).sum::<f64>() / total_w;
    let lon = tower_lons.iter().zip(weights).map(|(x, w)| x * w).sum::<f64>() / total_w;
    (lat, lon)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spatial::{Measurement, SpatialModel};

    #[test]
    fn centroid() {
        let lats = [55.0, 55.0, 56.0];
        let lons = [37.0, 38.0, 37.5];
        let ws = [1.0, 1.0, 1.0];
        let (lat, lon) = weighted_centroid(&lats, &lons, &ws);
        assert!((lat - 55.333).abs() < 0.01);
        assert!((lon - 37.5).abs() < 0.01);
    }
}
