use axum::{
    extract::{Multipart, State},
    http::StatusCode,
    Json,
};
use serde::Serialize;
use std::sync::Arc;
use tokio::io::AsyncReadExt;

use crate::{
    db::{insert_measurements, upsert_tower},
    normalizer::parse_oci_csv_row,
    state::AppState,
};

#[derive(Serialize)]
pub struct UploadResponse {
    pub imported_towers: u64,
    pub imported_measurements: u64,
    pub skipped: u64,
    pub source: String,
}

pub async fn upload_handler(
    State(state): State<Arc<AppState>>,
    mut multipart: Multipart,
) -> Result<Json<UploadResponse>, (StatusCode, String)> {
    let mut source = "upload".to_string();
    let mut csv_bytes: Option<bytes::Bytes> = None;

    while let Some(field) = multipart.next_field().await.map_err(|e| {
        (StatusCode::BAD_REQUEST, format!("multipart error: {e}"))
    })? {
        let name = field.name().unwrap_or("").to_string();
        match name.as_str() {
            "source" => {
                source = field.text().await.unwrap_or_default();
            }
            "file" => {
                csv_bytes = Some(field.bytes().await.map_err(|e| {
                    (StatusCode::BAD_REQUEST, format!("file read error: {e}"))
                })?);
            }
            _ => {}
        }
    }

    let bytes = csv_bytes.ok_or((StatusCode::BAD_REQUEST, "no file field".into()))?;

    let mut imported_towers = 0u64;
    let mut imported_measurements = 0u64;
    let mut skipped = 0u64;

    // Parse CSV (OCI format)
    let cursor = std::io::Cursor::new(bytes.as_ref());
    let mut rdr = csv_async::AsyncReader::from_reader(tokio::io::BufReader::new(
        tokio_util::io::SyncIoBridge::new(cursor),
    ));

    let mut records = rdr.records();
    let mut batch: Vec<(i64, f64, f64, Option<i16>, Option<i16>, String, String)> = Vec::new();

    use futures::StreamExt;
    while let Some(record) = records.next().await {
        let record = match record {
            Ok(r) => r,
            Err(_) => { skipped += 1; continue; }
        };

        let norm = match parse_oci_csv_row(&record) {
            Some(n) => n,
            None => { skipped += 1; continue; }
        };

        // Upsert tower
        match upsert_tower(
            &state.db,
            &norm.radio,
            norm.mcc,
            norm.mnc,
            norm.lac,
            norm.cid,
            norm.lat,
            norm.lon,
            None,
            None,
        )
        .await
        {
            Ok(tower_id) => {
                imported_towers += 1;
                if norm.signal_dbm.is_some() {
                    batch.push((
                        tower_id,
                        norm.lat,
                        norm.lon,
                        norm.signal_dbm,
                        norm.raw_signal,
                        source.clone(),
                        norm.radio.clone(),
                    ));
                }

                // Flush batch
                if batch.len() >= 1000 {
                    let refs: Vec<_> = batch.iter().map(|(a, b, c, d, e, f, g)| {
                        (*a, *b, *c, *d, *e, f.as_str(), g.as_str())
                    }).collect();
                    imported_measurements += insert_measurements(&state.db, &refs)
                        .await
                        .unwrap_or(0);
                    batch.clear();
                }
            }
            Err(_) => skipped += 1,
        }
    }

    // Flush remaining
    if !batch.is_empty() {
        let refs: Vec<_> = batch.iter().map(|(a, b, c, d, e, f, g)| {
            (*a, *b, *c, *d, *e, f.as_str(), g.as_str())
        }).collect();
        imported_measurements += insert_measurements(&state.db, &refs)
            .await
            .unwrap_or(0);
    }

    Ok(Json(UploadResponse {
        imported_towers,
        imported_measurements,
        skipped,
        source,
    }))
}
