//! Runtime access to the embedded metadata blob, plus a lazy regex cache.
//!
//! The blob is produced by `build.rs` from the vendored XML and deserialized
//! once on first use. Regexes are compiled on demand and memoized, because
//! compiling all ~1500 territory patterns up front would be wasteful.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use once_cell::sync::Lazy;
use regex::Regex;

use crate::model::{MetadataDb, PhoneMetadata};

/// The metadata blob generated at build time.
static METADATA_BIN: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/metadata.bin"));

struct Db {
    /// region id -> index into `db.regions`.
    region_index: HashMap<String, usize>,
    /// country code -> index into `db.non_geo`.
    non_geo_index: HashMap<u16, usize>,
    /// country code -> region ids (main country first).
    code_to_regions: HashMap<u16, Vec<String>>,
    db: MetadataDb,
}

static DB: Lazy<Db> = Lazy::new(|| {
    let db: MetadataDb = bincode::deserialize(METADATA_BIN).expect("corrupt embedded metadata");
    let region_index = db
        .regions
        .iter()
        .enumerate()
        .map(|(i, m)| (m.id.clone(), i))
        .collect();
    let non_geo_index = db
        .non_geo
        .iter()
        .enumerate()
        .map(|(i, m)| (m.country_code, i))
        .collect();
    let code_to_regions = db.code_to_regions.iter().cloned().collect();
    Db {
        region_index,
        non_geo_index,
        code_to_regions,
        db,
    }
});

/// Metadata for a geographical region (e.g. "US", "FR"), if known.
pub fn for_region(region: &str) -> Option<&'static PhoneMetadata> {
    let db = &*DB;
    db.region_index.get(region).map(|&i| &db.db.regions[i])
}

/// Metadata for a non-geographical calling code (e.g. 800, 870), if known.
pub fn for_non_geo_code(code: u16) -> Option<&'static PhoneMetadata> {
    let db = &*DB;
    db.non_geo_index.get(&code).map(|&i| &db.db.non_geo[i])
}

/// All region ids served by a country calling code ("calling regions"),
/// with the main country first. Empty for unknown codes.
pub fn regions_for_country_code(code: u16) -> &'static [String] {
    DB.code_to_regions
        .get(&code)
        .map(|v| v.as_slice())
        .unwrap_or(&[])
}

/// The main region for a country calling code, or "001" for non-geographical
/// entities, or `None` if the code is unknown.
pub fn main_region_for_country_code(code: u16) -> Option<&'static str> {
    if let Some(v) = DB.code_to_regions.get(&code) {
        v.first().map(|s| s.as_str())
    } else if DB.non_geo_index.contains_key(&code) {
        Some("001")
    } else {
        None
    }
}

/// The country calling code for a region, if the region is known.
pub fn country_code_for_region(region: &str) -> Option<u16> {
    for_region(region).map(|m| m.country_code)
}

/// Every supported geographical region id, in upstream (alphabetical) order.
pub fn supported_regions() -> impl Iterator<Item = &'static str> {
    DB.db.regions.iter().map(|m| m.id.as_str())
}

/// Every supported country calling code.
pub fn supported_calling_codes() -> Vec<u16> {
    let mut codes: Vec<u16> = DB.code_to_regions.keys().copied().collect();
    codes.extend(DB.non_geo_index.keys().copied());
    codes.sort_unstable();
    codes.dedup();
    codes
}

/// Does this country calling code map to a single, non-geographical entity?
pub fn is_non_geo_code(code: u16) -> bool {
    DB.non_geo_index.contains_key(&code) && !DB.code_to_regions.contains_key(&code)
}

/// Is this a known country calling code (geographical or non-geographical)?
pub fn is_valid_calling_code(code: u16) -> bool {
    DB.code_to_regions.contains_key(&code) || DB.non_geo_index.contains_key(&code)
}

/// Does the region support mobile number portability?
pub fn is_mobile_number_portable_region(region: &str) -> bool {
    for_region(region)
        .map(|m| m.mobile_number_portable_region)
        .unwrap_or(false)
}

/// English CLDR display name for a region id (e.g. `"US"` → `"United States"`,
/// `"001"` → `"World"`), if known.
pub fn region_name(region: &str) -> Option<&'static str> {
    static NAMES: Lazy<HashMap<String, String>> = Lazy::new(|| {
        static BIN: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/names.bin"));
        let db: crate::model::PrefixMap = bincode::deserialize(BIN).expect("corrupt names blob");
        db.entries.into_iter().collect()
    });
    NAMES.get(region).map(|s| s.as_str())
}

// --- Regex cache -----------------------------------------------------------

fn regex_cache() -> &'static Mutex<HashMap<String, &'static Regex>> {
    static CACHE: OnceLock<Mutex<HashMap<String, &'static Regex>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Rewrite a metadata pattern so that `\d` and `\D` mean what they mean in Go's RE2, which
/// compiles every metadata pattern in Go's `regexFor` (phonenumberutil.go:494): `\d` is
/// `[0-9]` there, while the `regex` crate's Unicode `\d` is every Nd digit. A national number
/// can hold a non-ASCII digit (Go's `normalizeDigits` keeps Devanagari, Bengali, ... digits), so
/// the two readings differ. `[0-9]` / `[^0-9]` is valid inside a class as well (the `regex`
/// crate takes a nested class as a union), so the rewrite needs no class tracking. Escaped
/// backslashes (`\\d`) are left alone.
pub(crate) fn go_ascii_digit_classes(pattern: &str) -> String {
    let mut out = String::with_capacity(pattern.len() + 8);
    let mut chars = pattern.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('d') => out.push_str("[0-9]"),
            Some('D') => out.push_str("[^0-9]"),
            Some(next) => {
                out.push('\\');
                out.push(next);
            }
            None => out.push('\\'),
        }
    }
    out
}

/// Compile (and memoize) a national-number pattern, anchored to a full match.
///
/// libphonenumber semantics: a `nationalNumberPattern` must match the whole
/// national number. We wrap it as `^(?:PATTERN)$`. Compiled regexes are leaked
/// into a `'static` cache so callers get a stable reference without re-locking
/// on the hot path after warm-up. Returns `None` if the source fails to compile
/// (a handful of upstream patterns use constructs `regex` rejects; callers treat
/// that as "no match", matching a conservative validation stance).
pub fn anchored_regex(pattern: &str) -> Option<&'static Regex> {
    if pattern.is_empty() {
        return None;
    }
    let key = pattern.to_string();
    let mut cache = regex_cache().lock().unwrap();
    if let Some(r) = cache.get(&key) {
        return Some(*r);
    }
    let anchored = format!("^(?:{})$", go_ascii_digit_classes(pattern));
    match Regex::new(&anchored) {
        Ok(re) => {
            let leaked: &'static Regex = Box::leak(Box::new(re));
            cache.insert(key, leaked);
            Some(leaked)
        }
        Err(_) => None,
    }
}

/// Compile (and memoize) a pattern for a *prefix* / unanchored match at start.
pub fn prefix_regex(pattern: &str) -> Option<&'static Regex> {
    if pattern.is_empty() {
        return None;
    }
    let key = format!("^PFX^{pattern}");
    let mut cache = regex_cache().lock().unwrap();
    if let Some(r) = cache.get(&key) {
        return Some(*r);
    }
    match Regex::new(&format!("^(?:{})", go_ascii_digit_classes(pattern))) {
        Ok(re) => {
            let leaked: &'static Regex = Box::leak(Box::new(re));
            cache.insert(key, leaked);
            Some(leaked)
        }
        Err(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn region_and_code_lookups() {
        assert!(for_region("US").is_some());
        assert!(for_region("ZZ").is_none());
        assert_eq!(country_code_for_region("FR"), Some(33));
        assert_eq!(country_code_for_region("ZZ"), None);
        assert!(regions_for_country_code(1).contains(&"US".to_string()));
        assert!(regions_for_country_code(9999).is_empty());
        assert_eq!(main_region_for_country_code(1), Some("US"));
        assert_eq!(main_region_for_country_code(9999), None);
        assert!(supported_regions().count() >= 200);
        assert!(supported_calling_codes().contains(&33));
    }

    #[test]
    fn non_geo_helpers() {
        assert!(is_valid_calling_code(800));
        assert!(!is_valid_calling_code(9999));
        assert!(is_non_geo_code(800));
        assert!(!is_non_geo_code(1)); // geographical
        assert!(for_non_geo_code(800).is_some());
        assert_eq!(main_region_for_country_code(800), Some("001"));
    }

    #[test]
    fn regex_cache_behaviour() {
        assert!(anchored_regex("").is_none());
        assert!(prefix_regex("").is_none());
        // Valid pattern compiles, memoizes (second call same pointer).
        let a = anchored_regex(r"\d{3}").unwrap();
        let b = anchored_regex(r"\d{3}").unwrap();
        assert!(std::ptr::eq(a, b));
        assert!(a.is_match("123"));
        assert!(!a.is_match("1234"));
        let p = prefix_regex(r"\d{2}").unwrap();
        assert!(p.is_match("12345"));
        // Invalid regex -> None, not a panic.
        assert!(anchored_regex(r"(?P<").is_none());
    }

    #[test]
    fn metadata_digits_are_ascii_as_in_go() {
        assert_eq!(go_ascii_digit_classes(r"[1-9]\d{2}"), "[1-9][0-9]{2}");
        assert_eq!(go_ascii_digit_classes(r"[\d]\D"), "[[0-9]][^0-9]");
        assert_eq!(go_ascii_digit_classes(r"\\d\("), r"\\d\(");
        // Go's RE2 reads `\d` as ASCII: a Devanagari digit is not one.
        let re = anchored_regex(r"\d{2}").unwrap();
        assert!(re.is_match("12"));
        assert!(!re.is_match("1\u{0967}"));
        assert!(anchored_regex(r"[\d]{2}").unwrap().is_match("12"));
    }
}
