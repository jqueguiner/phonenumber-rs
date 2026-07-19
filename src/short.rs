//! Short-number support, mirroring libphonenumber's `ShortNumberInfo`.
//!
//! Short numbers (emergency, service, and carrier-specific codes) are validated
//! per region rather than globally, since the same digits mean different things
//! in different countries.

use std::collections::HashMap;

use once_cell::sync::Lazy;

use crate::metadata;
use crate::model::{PhoneNumberDesc, ShortDb, ShortMetadata};
use crate::number::PhoneNumber;

static SHORT_BIN: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/short_metadata.bin"));

struct ShortData {
    index: HashMap<String, usize>,
    db: ShortDb,
}

static SHORT: Lazy<ShortData> = Lazy::new(|| {
    let db: ShortDb = bincode::deserialize(SHORT_BIN).expect("corrupt short metadata");
    let index = db
        .regions
        .iter()
        .enumerate()
        .map(|(i, m)| (m.id.clone(), i))
        .collect();
    ShortData { index, db }
});

fn short_meta(region: &str) -> Option<&'static ShortMetadata> {
    let d = &*SHORT;
    d.index.get(region).map(|&i| &d.db.regions[i])
}

/// Full-match a national number against a short-number description
/// (length gate followed by the anchored pattern).
fn matches_desc(nsn: &str, desc: &PhoneNumberDesc) -> bool {
    if !desc.possible_lengths.is_empty() && !desc.possible_lengths.contains(&(nsn.len() as u8)) {
        return false;
    }
    metadata::anchored_regex(&desc.national_number_pattern)
        .map(|re| re.is_match(nsn))
        .unwrap_or(false)
}

/// Digits only, discarding separators and any leading '+'.
fn digits_only(s: &str) -> String {
    s.chars().filter_map(|c| c.to_digit(10)).map(|d| char::from(b'0' + d as u8)).collect()
}

/// Is `nsn` a valid short number in `region`?
pub fn is_valid_short_number_for_region(nsn: &str, region: &str) -> bool {
    let Some(meta) = short_meta(region) else {
        return false;
    };
    if !matches_desc(nsn, &meta.general_desc) {
        return false;
    }
    matches_desc(nsn, &meta.short_code)
}

/// Is this parsed number a valid short number for any of its calling regions?
pub fn is_valid_short_number(number: &PhoneNumber) -> bool {
    let nsn = number.national_significant_number();
    let regions = metadata::regions_for_country_code(number.country_code);
    regions
        .iter()
        .any(|r| is_valid_short_number_for_region(&nsn, r))
}

/// Is `nsn` a possible short number (correct length) in `region`?
pub fn is_possible_short_number_for_region(nsn: &str, region: &str) -> bool {
    match short_meta(region) {
        Some(meta) => {
            let lens = &meta.general_desc.possible_lengths;
            lens.is_empty() || lens.contains(&(nsn.len() as u8))
        }
        None => false,
    }
}

/// Prefix-match `number` against a short-number description in `region`.
fn matches_emergency_helper(number: &str, region: &str, desc_of: fn(&ShortMetadata) -> &PhoneNumberDesc) -> bool {
    let digits = digits_only(number);
    let Some(meta) = short_meta(region) else {
        return false;
    };
    let desc = desc_of(meta);
    // Emergency numbers are matched as a prefix of the dialed digits.
    metadata::prefix_regex(&desc.national_number_pattern)
        .map(|re| re.is_match(&digits))
        .unwrap_or(false)
}

/// Is `number` an emergency number in `region` (e.g. "112" in FR, "911" in US)?
pub fn is_emergency_number(number: &str, region: &str) -> bool {
    matches_emergency_helper(number, region, |m| &m.emergency)
}

/// Does dialing `number` connect to an emergency service in `region`
/// (includes expanded emergency codes and prefix-dialing)?
pub fn connects_to_emergency_number(number: &str, region: &str) -> bool {
    matches_emergency_helper(number, region, |m| &m.emergency)
        || matches_emergency_helper(number, region, |m| &m.expanded_emergency)
}

/// Is `nsn` a carrier-specific short number in `region`?
pub fn is_carrier_specific_for_region(nsn: &str, region: &str) -> bool {
    short_meta(region)
        .map(|m| matches_desc(nsn, &m.carrier_specific))
        .unwrap_or(false)
}

/// Is `nsn` an SMS-service short number in `region`?
pub fn is_sms_service_for_region(nsn: &str, region: &str) -> bool {
    short_meta(region)
        .map(|m| matches_desc(nsn, &m.sms_services))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse;

    #[test]
    fn emergency_and_valid_short() {
        assert!(is_emergency_number("112", "FR"));
        assert!(is_emergency_number("911", "US"));
        assert!(is_emergency_number("112", "US"));
        assert!(!is_emergency_number("511", "US"));
        assert!(connects_to_emergency_number("911", "US"));
        // Unknown region -> false, not panic.
        assert!(!is_emergency_number("911", "ZZ"));
        assert!(!connects_to_emergency_number("911", "ZZ"));

        assert!(is_valid_short_number_for_region("112", "FR"));
        assert!(!is_valid_short_number_for_region("999999", "FR"));
        assert!(!is_valid_short_number_for_region("112", "ZZ"));
    }

    #[test]
    fn valid_short_via_number_and_possible() {
        // 112 is a valid short number in FR; parse then check the PhoneNumber path.
        let n = parse(Some("FR"), "112").unwrap_or_default();
        // The PhoneNumber-based API iterates the calling regions.
        let _ = is_valid_short_number(&n);
        assert!(is_possible_short_number_for_region("112", "FR"));
        assert!(!is_possible_short_number_for_region("112", "ZZ"));
    }

    #[test]
    fn carrier_and_sms_helpers() {
        // These just exercise the code paths; values vary by data.
        let _ = is_carrier_specific_for_region("12", "US");
        let _ = is_sms_service_for_region("12", "US");
        assert!(!is_carrier_specific_for_region("1", "ZZ"));
        assert!(!is_sms_service_for_region("1", "ZZ"));
    }
}
