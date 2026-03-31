use axum::{
    extract::{Query, State},
    http::StatusCode,
    Json,
};
use redis::AsyncCommands;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::{
    db::{get_measurements_in_radius, get_towers_in_radius, upsert_tower, CellMeasurement, CellTower},
    opencellid::fetch_towers_in_area,
    state::AppState,
};

#[derive(Deserialize)]
pub struct AreaQuery {
    pub lat: f64,
    pub lon: f64,
    #[serde(default = "default_radius")]
    pub radius_m: f64,
    pub cell_id: Option<i64>,
}

fn default_radius() -> f64 {
    1000.0
}

#[derive(Serialize)]
pub struct CellsResponse {
    pub towers: Vec<CellTower>,
    pub measurements_count: usize,
    pub source: String,
}

#[derive(Serialize)]
pub struct MeasurementsResponse {
    pub measurements: Vec<CellMeasurement>,
}

pub async fn cells_handler(
    State(state): State<Arc<AppState>>,
    Query(q): Query<AreaQuery>,
) -> Result<Json<CellsResponse>, StatusCode> {
    let radius = q.radius_m.clamp(100.0, 5000.0);

    // 1. Check Redis cache
    let cache_key = format!("cells:{:.4}:{:.4}:{:.0}", q.lat, q.lon, radius);
    if let Ok(mut conn) = state.redis.get_multiplexed_async_connection().await {
        if let Ok(cached) = conn.get::<_, String>(&cache_key).await {
            if let Ok(resp) = serde_json::from_str::<CellsResponse>(&cached) {
                return Ok(Json(resp));
            }
        }
    }

    // 2. Query DB
    let towers = get_towers_in_radius(&state.db, q.lat, q.lon, radius)
        .await
        .map_err(|e| {
            tracing::error!("DB error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let (towers, source) = if towers.len() < 3 && !state.oci_key.is_empty() {
        // 3. Fetch from OCI API and save
        match fetch_towers_in_area(&state.http, &state.oci_key, q.lat, q.lon, radius).await {
            Ok(oci_towers) => {
                for t in &oci_towers {
                    let _ = upsert_tower(
                        &state.db,
                        &t.radio,
                        t.mcc,
                        t.mnc,
                        t.lac,
                        t.cid,
                        t.lat,
                        t.lon,
                        t.range,
                        t.samples,
                    )
                    .await;
                }
                // Re-fetch from DB with proper types
                let fresh = get_towers_in_radius(&state.db, q.lat, q.lon, radius)
                    .await
                    .unwrap_or_default();
                (fresh, "api".to_string())
            }
            Err(e) => {
                tracing::warn!("OCI API error: {e}");
                (towers, "db".to_string())
            }
        }
    } else {
        (towers, "db".to_string())
    };

    // Get measurement count for area
    let measurements_count = sqlx::query_scalar!(
        "SELECT COUNT(*) FROM measurements m
         JOIN cell_towers t ON t.id = m.cell_id
         WHERE ST_DWithin(t.geom::geography, ST_SetSRID(ST_MakePoint($2, $1), 4326)::geography, $3)",
        q.lat, q.lon, radius,
    )
    .fetch_one(&state.db)
    .await
    .unwrap_or(Some(0))
    .unwrap_or(0) as usize;

    let resp = CellsResponse { towers, measurements_count, source };

    // Cache for 1 hour
    if let Ok(mut conn) = state.redis.get_multiplexed_async_connection().await {
        if let Ok(json) = serde_json::to_string(&resp) {
            let _: Result<(), _> = conn.set_ex(&cache_key, json, 3600).await;
        }
    }

    Ok(Json(resp))
}

pub async fn measurements_handler(
    State(state): State<Arc<AppState>>,
    Query(q): Query<AreaQuery>,
) -> Result<Json<MeasurementsResponse>, StatusCode> {
    let radius = q.radius_m.clamp(100.0, 5000.0);
    let measurements = get_measurements_in_radius(&state.db, q.lat, q.lon, radius)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(MeasurementsResponse { measurements }))
}
