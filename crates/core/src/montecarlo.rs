/// Monte Carlo triangulation quality estimator.
/// Runs N iterations with Gaussian noise on RSSI to estimate position uncertainty.

use crate::lm::{lm_solve, TowerObservation};
use crate::spatial::SpatialModel;
use alloc::vec::Vec;

/// Standard deviation of RSSI measurement noise (dBm).
pub const RSSI_NOISE_STD: f64 = 8.0;

/// Result of one Monte Carlo iteration.
#[derive(Debug, Clone)]
pub struct McPoint {
    pub lat: f64,
    pub lon: f64,
}

pub struct MonteCarloInput<'a> {
    pub tower_lats: &'a [f64],
    pub tower_lons: &'a [f64],
    pub observed_rssi: &'a [f64],
    pub models: &'a [SpatialModel],
    pub init_lat: f64,
    pub init_lon: f64,
}

/// Run Monte Carlo simulation and return cloud of position estimates.
///
/// Uses a simple LCG pseudo-random number generator (no_std compatible).
pub fn monte_carlo(input: &MonteCarloInput, n_iterations: usize) -> Vec<McPoint> {
    let mut results = Vec::with_capacity(n_iterations);
    let n_towers = input.tower_lats.len();

    // Simple LCG for no_std compatibility
    let mut rng = LcgRng::new(12345);

    for _ in 0..n_iterations {
        // Add Gaussian noise to each observed RSSI
        let noisy_rssi: Vec<f64> = (0..n_towers)
            .map(|i| input.observed_rssi[i] + rng.gaussian(RSSI_NOISE_STD))
            .collect();

        let observations: Vec<TowerObservation> = (0..n_towers)
            .map(|i| TowerObservation {
                tower_lat: input.tower_lats[i],
                tower_lon: input.tower_lons[i],
                observed_rssi: noisy_rssi[i],
                model: &input.models[i],
            })
            .collect();

        let result = lm_solve(&observations, input.init_lat, input.init_lon);

        // Only keep converged solutions within reasonable range (< 50 km from init)
        let dist = crate::geo::haversine_m(result.lat, result.lon, input.init_lat, input.init_lon);
        if dist < 50_000.0 {
            results.push(McPoint { lat: result.lat, lon: result.lon });
        }
    }

    results
}

/// Linear Congruential Generator for portable pseudo-randomness.
struct LcgRng {
    state: u64,
}

impl LcgRng {
    fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    fn next_u64(&mut self) -> u64 {
        self.state = self.state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.state
    }

    /// Uniform [0, 1)
    fn uniform(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    /// Box-Muller transform → N(0, std)
    fn gaussian(&mut self, std: f64) -> f64 {
        let u1 = self.uniform().max(1e-15);
        let u2 = self.uniform();
        let mag = std * libm::sqrt(-2.0 * libm::log(u1));
        mag * libm::cos(2.0 * core::f64::consts::PI * u2)
    }
}
