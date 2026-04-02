use axum::{
    extract::{Multipart, State},
    http::StatusCode,
    Json,
};
use serde::Serialize;
use std::sync::Arc;

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
        match field.name().unwrap_or("") {
            "source" => { source = field.text().await.unwrap_or_default(); }
            "file" => {
                csv_bytes = Some(field.bytes().await.map_err(|e| {
                    (StatusCode::BAD_REQUEST, format!("file read: {e}"))
                })?);
            }
            _ => {}
        }
    }

    let bytes = csv_bytes.ok_or((StatusCode::BAD_REQUEST, "no file field".into()))?;

    // Grab the tokio handle BEFORE entering spawn_blocking
    let rt = tokio::runtime::Handle::current();
    let source_c = source.clone();
    let state_c = Arc::clone(&state);

    let (imported_towers, imported_measurements, skipped) =
        tokio::task::spawn_blocking(move || {
            parse_and_insert_sync(bytes, source_c, state_c, rt)
        })
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("task panic: {e}")))?;

    Ok(Json(UploadResponse { imported_towers, imported_measurements, skipped, source }))
}

fn parse_and_insert_sync(
    bytes: bytes::Bytes,
    source: String,
    state: Arc<AppState>,
    rt: tokio::runtime::Handle,
) -> (u64, u64, u64) {
    let mut rdr = csv::ReaderBuilder::new()
        .has_headers(true)
        .flexible(true)
        .from_reader(bytes.as_ref());

    let mut imported_towers = 0u64;
    let mut imported_measurements = 0u64;
    let mut skipped = 0u64;
    let mut batch: Vec<(i64, f64, f64, Option<i16>, Option<i16>, String, String)> = Vec::new();

    let flush_batch = |batch: &mut Vec<_>, rt: &tokio::runtime::Handle,
                       state: &Arc<AppState>, count: &mut u64| {
        if batch.is_empty() { return; }
        let refs: Vec<_> = batch.iter()
            .map(|(a,b,c,d,e,f,g): &(i64,f64,f64,Option<i16>,Option<i16>,String,String)| {
                (*a,*b,*c,*d,*e,f.as_str(),g.as_str())
            }).collect();
        *count += rt.block_on(insert_measurements(&state.db, &refs)).unwrap_or(0);
        batch.clear();
    };

    for result in rdr.records() {
        let record = match result {
            Ok(r) => r,
            Err(_) => { skipped += 1; continue; }
        };
        let norm = match parse_oci_csv_row(&record) {
            Some(n) => n,
            None => { skipped += 1; continue; }
        };

        match rt.block_on(upsert_tower(
            &state.db, &norm.radio, norm.mcc, norm.mnc, norm.lac, norm.cid,
            norm.lat, norm.lon, None, None,
        )) {
            Ok(tower_id) => {
                imported_towers += 1;
                if norm.signal_dbm.is_some() {
                    batch.push((tower_id, norm.lat, norm.lon,
                        norm.signal_dbm, norm.raw_signal,
                        source.clone(), norm.radio.clone()));
                }
                if batch.len() >= 1000 {
                    flush_batch(&mut batch, &rt, &state, &mut imported_measurements);
                }
            }
            Err(_) => { skipped += 1; }
        }
    }
    flush_batch(&mut batch, &rt, &state, &mut imported_measurements);

    (imported_towers, imported_measurements, skipped)
}
