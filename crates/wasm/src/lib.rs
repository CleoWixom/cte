/// WASM bindings for the trieval triangulation engine.

use trieval_core::{
    lm::{lm_solve, weighted_centroid, TowerObservation},
    metrics::compute_metrics,
    montecarlo::{monte_carlo, MonteCarloInput},
    spatial::{Measurement, SpatialModel},
};

use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

// ── Input / output types ──────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct TowerInput {
    pub id: i64,
    pub radio: String,
    pub lat: f64,
    pub lon: f64,
    pub range_m: Option<f64>,
}

#[derive(Deserialize)]
pub struct MeasurementInput {
    pub cell_id: i64,
    pub lat: f64,
    pub lon: f64,
    pub signal_dbm: f64,
    pub reliability: Option<f64>,
}

#[derive(Serialize)]
pub struct CloudPoint {
    pub lat: f64,
    pub lon: f64,
}

#[derive(Serialize)]
pub struct TriangulationResult {
    pub lat: f64,
    pub lon: f64,
    pub cep50_m: f64,
    pub cep95_m: f64,
    pub gdop: f64,
    pub ellipse_semi_major_m: f64,
    pub ellipse_semi_minor_m: f64,
    pub ellipse_angle_deg: f64,
    pub cloud: Vec<CloudPoint>,
    pub n_towers_used: usize,
    pub n_measurements: usize,
    pub converged: bool,
    pub model_type: String,
}

// ── Engine ────────────────────────────────────────────────────────────────────

#[wasm_bindgen]
pub struct TriangulationEngine {
    towers: Vec<TowerInput>,
    measurements: Vec<MeasurementInput>,
    mc_iterations: usize,
}

#[wasm_bindgen]
impl TriangulationEngine {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        // Better panic messages — only available when targeting wasm32
        #[cfg(target_arch = "wasm32")]
        console_error_panic_hook::set_once();

        Self {
            towers: Vec::new(),
            measurements: Vec::new(),
            mc_iterations: 300,
        }
    }

    /// Load tower array from JS: `[{id, radio, lat, lon, range_m?}]`
    pub fn load_towers(&mut self, data: JsValue) -> Result<(), JsValue> {
        self.towers = serde_wasm_bindgen::from_value(data)
            .map_err(|e| JsValue::from_str(&format!("towers parse error: {e}")))?;
        Ok(())
    }

    /// Load measurement array from JS: `[{cell_id, lat, lon, signal_dbm, reliability?}]`
    pub fn load_measurements(&mut self, data: JsValue) -> Result<(), JsValue> {
        self.measurements = serde_wasm_bindgen::from_value(data)
            .map_err(|e| JsValue::from_str(&format!("measurements parse error: {e}")))?;
        Ok(())
    }

    /// Interpolation model hint: "kriging" | "idw" — auto-selected per tower.
    pub fn set_model(&mut self, _model: &str) {}

    /// Monte Carlo sample count [50, 1000]. Default: 300.
    pub fn set_mc_iterations(&mut self, n: usize) {
        self.mc_iterations = n.clamp(50, 1000);
    }

    /// Run triangulation for the given query point. Returns `TriangulationResult` as JsValue.
    pub fn solve(&self, query_lat: f64, query_lon: f64) -> JsValue {
        match self.run(query_lat, query_lon) {
            Ok(r) => serde_wasm_bindgen::to_value(&r).unwrap_or(JsValue::NULL),
            Err(_e) => {
                #[cfg(target_arch = "wasm32")]
                web_sys::console::error_1(&JsValue::from_str(&format!("trieval: {_e}")));
                JsValue::NULL
            }
        }
    }

    fn run(&self, query_lat: f64, query_lon: f64) -> Result<TriangulationResult, String> {
        if self.towers.is_empty() {
            return Err("no towers loaded".into());
        }

        let mut tower_lats = Vec::new();
        let mut tower_lons = Vec::new();
        let mut tower_weights = Vec::new();
        let mut spatial_models: Vec<SpatialModel> = Vec::new();
        let mut observed_rssi: Vec<f64> = Vec::new();
        let mut total_measurements = 0usize;

        for tower in &self.towers {
            let ms: Vec<Measurement> = self
                .measurements
                .iter()
                .filter(|m| m.cell_id == tower.id)
                .map(|m| Measurement {
                    lat: m.lat,
                    lon: m.lon,
                    signal_dbm: m.signal_dbm * m.reliability.unwrap_or(1.0),
                })
                .collect();

            if ms.is_empty() {
                continue;
            }
            total_measurements += ms.len();

            let model = SpatialModel::build(ms);
            let (rssi_at_query, _) = model.predict(query_lat, query_lon);
            let range_m = tower.range_m.unwrap_or(1000.0).max(1.0);

            tower_lats.push(tower.lat);
            tower_lons.push(tower.lon);
            tower_weights.push(1.0 / range_m);
            observed_rssi.push(rssi_at_query);
            spatial_models.push(model);
        }

        let n_towers = tower_lats.len();
        if n_towers < 2 {
            return Err(format!("need ≥2 towers with measurements, got {n_towers}"));
        }

        let (init_lat, init_lon) = weighted_centroid(&tower_lats, &tower_lons, &tower_weights);

        let observations: Vec<TowerObservation> = (0..n_towers)
            .map(|i| TowerObservation {
                tower_lat: tower_lats[i],
                tower_lon: tower_lons[i],
                observed_rssi: observed_rssi[i],
                model: &spatial_models[i],
            })
            .collect();

        let lm_result = lm_solve(&observations, init_lat, init_lon);

        let mc_input = MonteCarloInput {
            tower_lats: &tower_lats,
            tower_lons: &tower_lons,
            observed_rssi: &observed_rssi,
            models: &spatial_models,
            init_lat,
            init_lon,
        };
        let cloud = monte_carlo(&mc_input, self.mc_iterations);

        let metrics = compute_metrics(
            &cloud,
            lm_result.lat,
            lm_result.lon,
            &tower_lats,
            &tower_lons,
        );

        let cloud_points = cloud
            .iter()
            .map(|p| CloudPoint { lat: p.lat, lon: p.lon })
            .collect();

        let model_type = spatial_models
            .iter()
            .any(|m| matches!(m, SpatialModel::Kriging(_)))
            .then_some("kriging")
            .unwrap_or("idw")
            .into();

        Ok(TriangulationResult {
            lat: lm_result.lat,
            lon: lm_result.lon,
            cep50_m: metrics.cep50_m,
            cep95_m: metrics.cep95_m,
            gdop: metrics.gdop,
            ellipse_semi_major_m: metrics.ellipse.semi_major_m,
            ellipse_semi_minor_m: metrics.ellipse.semi_minor_m,
            ellipse_angle_deg: metrics.ellipse.angle_deg,
            cloud: cloud_points,
            n_towers_used: n_towers,
            n_measurements: total_measurements,
            converged: lm_result.converged,
            model_type,
        })
    }
}
