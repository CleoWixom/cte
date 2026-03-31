/// Normalizes various CSV/JSON source formats into our internal representation.

use trieval_core::signal::{normalize_to_dbm, RadioType};

pub struct NormalizedRecord {
    pub radio: String,
    pub mcc: i16,
    pub mnc: i16,
    pub lac: i32,
    pub cid: i64,
    pub lat: f64,
    pub lon: f64,
    pub signal_dbm: Option<i16>,
    pub raw_signal: Option<i16>,
}

/// Parse a CSV row from OpenCelliD measurements dump.
/// Format: `radio,mcc,mnc,lac,cellid,unit,lon,lat,signal,ta,measured_at,rating,speed,direction`
pub fn parse_oci_csv_row(record: &csv_async::StringRecord) -> Option<NormalizedRecord> {
    let radio = record.get(0)?.to_uppercase();
    let mcc: i16 = record.get(1)?.parse().ok()?;
    let mnc: i16 = record.get(2)?.parse().ok()?;
    let lac: i32 = record.get(3)?.parse().ok()?;
    let cid: i64 = record.get(4)?.parse().ok()?;
    let lon: f64 = record.get(6)?.parse().ok()?;
    let lat: f64 = record.get(7)?.parse().ok()?;
    let raw_signal: i16 = record.get(8)?.parse().ok()?;

    // Validate coordinates
    if lat.abs() > 90.0 || lon.abs() > 180.0 {
        return None;
    }
    // Skip clearly invalid cells
    if cid == 0 || lac == 0 {
        return None;
    }

    let radio_type = RadioType::from_str(&radio);
    let signal_dbm = if raw_signal != 0 {
        Some(normalize_to_dbm(raw_signal, radio_type))
    } else {
        None
    };

    Some(NormalizedRecord {
        radio,
        mcc,
        mnc,
        lac,
        cid,
        lat,
        lon,
        signal_dbm,
        raw_signal: if raw_signal != 0 { Some(raw_signal) } else { None },
    })
}

/// Parse Android WebSocket measurement JSON.
pub fn parse_android_cell(value: &serde_json::Value) -> Option<NormalizedRecord> {
    let radio = value.get("radio")?.as_str()?.to_uppercase();
    let mcc: i16 = value.get("mcc")?.as_i64()? as i16;
    let mnc: i16 = value.get("mnc")?.as_i64()? as i16;
    let lac: i32 = value.get("lac")?.as_i64()? as i32;
    let cid: i64 = value.get("cid")?.as_i64()?;
    let lat: f64 = value.get("lat")?.as_f64()?;
    let lon: f64 = value.get("lon")?.as_f64()?;
    let rssi: i16 = value.get("rssi")?.as_i64()? as i16;

    let radio_type = RadioType::from_str(&radio);
    let signal_dbm = Some(normalize_to_dbm(rssi, radio_type));

    Some(NormalizedRecord {
        radio,
        mcc,
        mnc,
        lac,
        cid,
        lat,
        lon,
        signal_dbm,
        raw_signal: Some(rssi),
    })
}
