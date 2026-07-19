//! Offline geocoding, carrier, and time-zone lookup.
//!
//! Mirrors libphonenumber's `PhoneNumberOfflineGeocoder`,
//! `PhoneNumberToCarrierMapper`, and `PhoneNumberToTimeZonesMapper`. Each is a
//! longest-prefix match over `countryCode + nationalSignificantNumber`.
//!
//! Geocoding and carrier data are the English (`en`) locale, matching upstream's
//! default fallback.

use std::collections::HashMap;

use once_cell::sync::Lazy;

use crate::model::PrefixMap;
use crate::number::PhoneNumber;

/// Returned by [`time_zones_for_number`] when nothing matches.
pub const UNKNOWN_TIME_ZONE: &str = "Etc/Unknown";

struct Table {
    map: HashMap<String, String>,
    max_len: usize,
}

impl Table {
    fn load(bytes: &[u8]) -> Table {
        let db: PrefixMap = bincode::deserialize(bytes).expect("corrupt prefix map");
        let max_len = db.entries.iter().map(|(k, _)| k.len()).max().unwrap_or(0);
        let map = db.entries.into_iter().collect();
        Table { map, max_len }
    }

    /// Longest prefix of `query` present in the table.
    fn lookup(&self, query: &str) -> Option<&str> {
        let max = query.len().min(self.max_len);
        for i in (1..=max).rev() {
            if let Some(v) = self.map.get(&query[..i]) {
                return Some(v.as_str());
            }
        }
        None
    }
}

static GEO: Lazy<Table> =
    Lazy::new(|| Table::load(include_bytes!(concat!(env!("OUT_DIR"), "/geo.bin"))));
static CARRIER: Lazy<Table> =
    Lazy::new(|| Table::load(include_bytes!(concat!(env!("OUT_DIR"), "/carrier.bin"))));
static TIMEZONE: Lazy<Table> =
    Lazy::new(|| Table::load(include_bytes!(concat!(env!("OUT_DIR"), "/timezone.bin"))));

/// The dialed-number key used for prefix matching: country code + NSN.
fn prefix_key(number: &PhoneNumber) -> String {
    format!("{}{}", number.country_code, number.national_significant_number())
}

/// A human-readable location for a number (e.g. `"Jersey City, NJ"`), if a
/// geographic prefix mapping exists. Returns `None` when no area is known.
pub fn description_for_number(number: &PhoneNumber) -> Option<&'static str> {
    GEO.lookup(&prefix_key(number))
}

/// Like [`description_for_number`], but falls back to the country/region name
/// when no finer area is known — mirroring libphonenumber's
/// `getDescriptionForNumber`, which never returns empty for a valid geographic
/// number.
pub fn description_or_country(number: &PhoneNumber) -> Option<&'static str> {
    if let Some(area) = GEO.lookup(&prefix_key(number)) {
        return Some(area);
    }
    // Fall back to the resolved region's English country name.
    match crate::validator::region_code_for_number(number) {
        Some(r) if r != crate::validator::REGION_CODE_FOR_NON_GEO => crate::metadata::region_name(r),
        _ => None,
    }
}

/// The carrier/operator originally associated with a number (e.g. `"O2"`),
/// if known. Note: number portability means this is the *original* carrier.
pub fn carrier_for_number(number: &PhoneNumber) -> Option<&'static str> {
    CARRIER.lookup(&prefix_key(number))
}

/// The IANA time zones a number maps to (e.g. `["America/New_York"]`).
/// Returns `["Etc/Unknown"]` when no mapping is found.
pub fn time_zones_for_number(number: &PhoneNumber) -> Vec<&'static str> {
    match TIMEZONE.lookup(&prefix_key(number)) {
        Some(v) => v.split('&').collect(),
        None => vec![UNKNOWN_TIME_ZONE],
    }
}
