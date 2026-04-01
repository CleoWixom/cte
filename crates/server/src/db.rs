//! Database queries — all use `sqlx::query_as` (no compile-time macros,
//! so no `.sqlx/` offline directory is needed in CI).

use anyhow::Result;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct CellTower {
    pub id: i64,
    pub radio: String,
    pub mcc: i16,
    pub mnc: i16,
    pub lac: i32,
    pub cid: i64,
    pub lat: f64,
    pub lon: f64,
    pub range_m: Option<i32>,
    pub samples: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct CellMeasurement {
    pub id: i64,
    pub cell_id: i64,
    pub lat: f64,
    pub lon: f64,
    pub signal_dbm: Option<i16>,
    pub source: String,
    pub reliability: Option<f64>,
}

/// Towers within `radius_m` metres of (lat, lon) — PostGIS ST_DWithin.
pub async fn get_towers_in_radius(
    pool: &PgPool,
    lat: f64,
    lon: f64,
    radius_m: f64,
) -> Result<Vec<CellTower>> {
    let rows = sqlx::query_as::<_, CellTower>(
        "SELECT id, radio, mcc, mnc, lac, cid, lat, lon, range_m, samples
         FROM cell_towers
         WHERE ST_DWithin(
             geom::geography,
             ST_SetSRID(ST_MakePoint($2, $1), 4326)::geography,
             $3
         )
         ORDER BY ST_Distance(geom::geography, ST_SetSRID(ST_MakePoint($2, $1), 4326)::geography)
         LIMIT 100",
    )
    .bind(lat)
    .bind(lon)
    .bind(radius_m)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// Measurements whose tower is within `radius_m` of (lat, lon).
pub async fn get_measurements_in_radius(
    pool: &PgPool,
    lat: f64,
    lon: f64,
    radius_m: f64,
) -> Result<Vec<CellMeasurement>> {
    let rows = sqlx::query_as::<_, CellMeasurement>(
        "SELECT m.id, m.cell_id, m.lat, m.lon, m.signal_dbm, m.source, m.reliability
         FROM measurements m
         JOIN cell_towers t ON t.id = m.cell_id
         WHERE ST_DWithin(
             t.geom::geography,
             ST_SetSRID(ST_MakePoint($2, $1), 4326)::geography,
             $3
         )
         LIMIT 5000",
    )
    .bind(lat)
    .bind(lon)
    .bind(radius_m)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// Count measurements near (lat, lon).
pub async fn count_measurements_in_radius(
    pool: &PgPool,
    lat: f64,
    lon: f64,
    radius_m: f64,
) -> i64 {
    sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM measurements m
         JOIN cell_towers t ON t.id = m.cell_id
         WHERE ST_DWithin(t.geom::geography, ST_SetSRID(ST_MakePoint($2, $1), 4326)::geography, $3)",
    )
    .bind(lat)
    .bind(lon)
    .bind(radius_m)
    .fetch_one(pool)
    .await
    .unwrap_or(0)
}

/// Upsert a tower — returns its `id`.
pub async fn upsert_tower(
    pool: &PgPool,
    radio: &str,
    mcc: i16,
    mnc: i16,
    lac: i32,
    cid: i64,
    lat: f64,
    lon: f64,
    range_m: Option<i32>,
    samples: Option<i32>,
) -> Result<i64> {
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO cell_towers (radio, mcc, mnc, lac, cid, lat, lon, range_m, samples)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
         ON CONFLICT (radio, mcc, mnc, lac, cid) DO UPDATE
             SET lat      = EXCLUDED.lat,
                 lon      = EXCLUDED.lon,
                 range_m  = COALESCE(EXCLUDED.range_m, cell_towers.range_m),
                 samples  = GREATEST(COALESCE(EXCLUDED.samples, 0), COALESCE(cell_towers.samples, 0))
         RETURNING id",
    )
    .bind(radio)
    .bind(mcc)
    .bind(mnc)
    .bind(lac)
    .bind(cid)
    .bind(lat)
    .bind(lon)
    .bind(range_m)
    .bind(samples)
    .fetch_one(pool)
    .await?;
    Ok(id)
}

/// Batch-insert measurements (1000 rows per chunk).
pub async fn insert_measurements(
    pool: &PgPool,
    batch: &[(i64, f64, f64, Option<i16>, Option<i16>, &str, &str)],
) -> Result<u64> {
    let mut count = 0u64;
    for (cell_id, lat, lon, signal_dbm, raw_signal, source, radio) in batch {
        let rows = sqlx::query(
            "INSERT INTO measurements (cell_id, lat, lon, signal_dbm, raw_signal, source, radio)
             VALUES ($1, $2, $3, $4, $5, $6, $7)
             ON CONFLICT DO NOTHING",
        )
        .bind(cell_id)
        .bind(lat)
        .bind(lon)
        .bind(signal_dbm)
        .bind(raw_signal)
        .bind(source)
        .bind(radio)
        .execute(pool)
        .await?
        .rows_affected();
        count += rows;
    }
    Ok(count)
}
