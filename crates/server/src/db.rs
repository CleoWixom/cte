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

/// Fetch towers within radius_m of (lat, lon) using PostGIS ST_DWithin.
pub async fn get_towers_in_radius(
    pool: &PgPool,
    lat: f64,
    lon: f64,
    radius_m: f64,
) -> Result<Vec<CellTower>> {
    let rows = sqlx::query_as::<_, CellTower>(
        r#"
        SELECT id, radio, mcc, mnc, lac, cid, lat, lon, range_m, samples
        FROM cell_towers
        WHERE ST_DWithin(
            geom::geography,
            ST_SetSRID(ST_MakePoint($2, $1), 4326)::geography,
            $3
        )
        ORDER BY ST_Distance(geom::geography, ST_SetSRID(ST_MakePoint($2, $1), 4326)::geography)
        LIMIT 100
        "#,
    )
    .bind(lat)
    .bind(lon)
    .bind(radius_m)
    .fetch_all(pool)
    .await?;

    Ok(rows)
}

/// Fetch measurements for towers within radius_m.
pub async fn get_measurements_in_radius(
    pool: &PgPool,
    lat: f64,
    lon: f64,
    radius_m: f64,
) -> Result<Vec<CellMeasurement>> {
    let rows = sqlx::query_as::<_, CellMeasurement>(
        r#"
        SELECT m.id, m.cell_id, m.lat, m.lon, m.signal_dbm, m.source, m.reliability
        FROM measurements m
        JOIN cell_towers t ON t.id = m.cell_id
        WHERE ST_DWithin(
            t.geom::geography,
            ST_SetSRID(ST_MakePoint($2, $1), 4326)::geography,
            $3
        )
        LIMIT 5000
        "#,
    )
    .bind(lat)
    .bind(lon)
    .bind(radius_m)
    .fetch_all(pool)
    .await?;

    Ok(rows)
}

/// Upsert a tower. Returns the tower id.
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
    let row = sqlx::query!(
        r#"
        INSERT INTO cell_towers (radio, mcc, mnc, lac, cid, lat, lon, range_m, samples)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
        ON CONFLICT (radio, mcc, mnc, lac, cid) DO UPDATE
            SET lat = EXCLUDED.lat,
                lon = EXCLUDED.lon,
                range_m = COALESCE(EXCLUDED.range_m, cell_towers.range_m),
                samples = GREATEST(COALESCE(EXCLUDED.samples, 0), COALESCE(cell_towers.samples, 0))
        RETURNING id
        "#,
        radio, mcc, mnc, lac, cid, lat, lon, range_m, samples,
    )
    .fetch_one(pool)
    .await?;

    Ok(row.id)
}

/// Batch insert measurements.
pub async fn insert_measurements(
    pool: &PgPool,
    measurements: &[(i64, f64, f64, Option<i16>, Option<i16>, &str, &str)],
) -> Result<u64> {
    let mut count = 0u64;
    for chunk in measurements.chunks(1000) {
        for (cell_id, lat, lon, signal_dbm, raw_signal, source, radio) in chunk {
            sqlx::query!(
                r#"
                INSERT INTO measurements (cell_id, lat, lon, signal_dbm, raw_signal, source, radio)
                VALUES ($1, $2, $3, $4, $5, $6, $7)
                ON CONFLICT DO NOTHING
                "#,
                cell_id, lat, lon, *signal_dbm, *raw_signal, source, radio,
            )
            .execute(pool)
            .await?;
            count += 1;
        }
    }
    Ok(count)
}
