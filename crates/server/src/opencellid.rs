/// OpenCelliD API client.

use anyhow::{Context, Result};
use serde::Deserialize;

const OCI_API_BASE: &str = "https://us1.unwiredlabs.com/v2/process";
const OCI_CELL_URL: &str = "https://opencellid.org/ajax/searchForCellsByLatLng";

/// Tower returned by OpenCelliD `cell/getInArea`.
#[derive(Debug, Deserialize)]
pub struct OciTower {
    pub radio: String,
    pub mcc: i16,
    pub mnc: i16,
    pub lac: i32,
    #[serde(alias = "cellid")]
    pub cid: i64,
    pub lat: f64,
    pub lon: f64,
    #[serde(default)]
    pub range: Option<i32>,
    #[serde(default)]
    pub samples: Option<i32>,
    #[serde(rename = "averageSignal", default)]
    pub avg_signal: i32,
}

#[derive(Debug, Deserialize)]
struct OciResponse {
    #[serde(default)]
    cells: Vec<OciTower>,
    #[serde(default)]
    total: Option<u32>,
}

/// Fetch towers in a bounding box from OpenCelliD.
/// OCI API limits: bbox area ≤ 4 000 000 m² (~2 km × 2 km).
pub async fn fetch_towers_in_area(
    client: &reqwest::Client,
    api_key: &str,
    lat: f64,
    lon: f64,
    radius_m: f64,
) -> Result<Vec<OciTower>> {
    if api_key.is_empty() {
        return Ok(Vec::new());
    }

    // Convert radius to lat/lon degrees
    let lat_delta = radius_m / 111_320.0;
    let lon_delta = radius_m / (111_320.0 * f64::cos(lat.to_radians()));

    let bbox = format!(
        "{},{},{},{}",
        lon - lon_delta,
        lat - lat_delta,
        lon + lon_delta,
        lat + lat_delta,
    );

    let url = format!(
        "https://opencellid.org/cell/getInArea?key={api_key}&BBOX={bbox}&format=json&limit=1000"
    );

    let resp = client
        .get(&url)
        .send()
        .await
        .context("OCI API request failed")?;

    if !resp.status().is_success() {
        anyhow::bail!("OCI API returned {}", resp.status());
    }

    let oci: OciResponse = resp
        .json()
        .await
        .context("OCI API JSON parse failed")?;

    tracing::debug!(
        "OCI returned {} towers for bbox={bbox}",
        oci.cells.len()
    );

    Ok(oci.cells)
}
