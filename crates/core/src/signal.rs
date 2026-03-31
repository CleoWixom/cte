/// RSSI normalization across radio technologies.
/// All values are normalized to dBm.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum RadioType {
    Gsm,
    Umts,
    Lte,
    Nr,
}

impl RadioType {
    pub fn from_str(s: &str) -> Self {
        match s.to_uppercase().as_str() {
            "GSM" | "CDMA" => Self::Gsm,
            "UMTS" | "WCDMA" | "3G" => Self::Umts,
            "LTE" | "4G" => Self::Lte,
            "NR" | "5G" => Self::Nr,
            _ => Self::Lte,
        }
    }
}

/// Convert raw signal value from OpenCelliD/Android format to dBm.
///
/// | Technology | Formula               | Range (dBm)   |
/// |------------|-----------------------|---------------|
/// | GSM        | 2 × ASU − 113         | −113 … −51    |
/// | UMTS       | ASU − 116             | −121 … −25    |
/// | LTE        | ASU − 140             | −140 … −44    |
/// | NR         | direct dBm            | −140 … −44    |
pub fn normalize_to_dbm(raw: i16, radio: RadioType) -> i16 {
    match radio {
        RadioType::Gsm => {
            if raw >= 0 && raw <= 31 {
                2 * raw - 113
            } else {
                // Already in dBm or unknown — clamp
                raw.clamp(-113, -51)
            }
        }
        RadioType::Umts => {
            if raw >= 0 && raw <= 91 {
                raw - 116
            } else {
                raw.clamp(-121, -25)
            }
        }
        RadioType::Lte => {
            if raw >= 0 && raw <= 97 {
                raw - 140
            } else {
                raw.clamp(-140, -44)
            }
        }
        RadioType::Nr => raw.clamp(-140, -44),
    }
}

/// Signal quality bucket for UI colouring.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignalQuality {
    Excellent, // > −70 dBm
    Good,      // −70 … −85
    Fair,      // −85 … −100
    Poor,      // < −100 dBm
}

pub fn signal_quality(dbm: i16) -> SignalQuality {
    if dbm > -70 {
        SignalQuality::Excellent
    } else if dbm > -85 {
        SignalQuality::Good
    } else if dbm > -100 {
        SignalQuality::Fair
    } else {
        SignalQuality::Poor
    }
}

/// Normalize 0-based avgSignal from OpenCelliD CSV (which may already be dBm
/// or 0 for unknown). Returns None if the value is clearly missing/invalid.
pub fn oci_avg_signal_to_dbm(avg_signal: i32, radio: RadioType) -> Option<i16> {
    if avg_signal == 0 {
        return None; // Missing — known OCI issue
    }
    let raw = avg_signal as i16;
    // If the value is clearly already in dBm range (negative and reasonable)
    if raw < -20 && raw > -160 {
        return Some(raw.clamp(-140, -25));
    }
    // Otherwise treat as ASU
    Some(normalize_to_dbm(raw.abs(), radio))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gsm_asu_conversion() {
        // ASU 15 → 2×15 − 113 = −83 dBm
        assert_eq!(normalize_to_dbm(15, RadioType::Gsm), -83);
        // ASU 0 → −113
        assert_eq!(normalize_to_dbm(0, RadioType::Gsm), -113);
        // ASU 31 → −51
        assert_eq!(normalize_to_dbm(31, RadioType::Gsm), -51);
    }

    #[test]
    fn lte_asu_conversion() {
        // ASU 50 → 50 − 140 = −90 dBm
        assert_eq!(normalize_to_dbm(50, RadioType::Lte), -90);
    }

    #[test]
    fn umts_asu_conversion() {
        // ASU 30 → 30 − 116 = −86 dBm
        assert_eq!(normalize_to_dbm(30, RadioType::Umts), -86);
    }

    #[test]
    fn oci_missing_signal() {
        assert!(oci_avg_signal_to_dbm(0, RadioType::Lte).is_none());
    }

    #[test]
    fn oci_direct_dbm() {
        assert_eq!(oci_avg_signal_to_dbm(-95, RadioType::Lte), Some(-95));
    }
}
