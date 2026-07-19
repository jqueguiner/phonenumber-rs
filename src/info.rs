//! A JSON-serializable summary of everything the library knows about a number.
//!
//! ```
//! let n = phonenumber::parse(Some("US"), "(650) 253-0000").unwrap();
//! let info = phonenumber::analyze(&n);
//! assert_eq!(info.country_code, 1);
//! assert_eq!(info.region_name.as_deref(), Some("United States"));
//! assert!(info.valid && info.is_fixed_line_or_mobile);
//! let json = info.to_json_pretty();
//! assert!(json.contains("\"country_code\": 1"));
//! ```

use serde::Serialize;

use crate::format::{format, PhoneNumberFormat};
use crate::mapper;
use crate::metadata;
use crate::number::{PhoneNumber, PhoneNumberType};
use crate::validator;

/// One region that shares the number's calling code.
#[derive(Clone, Debug, Serialize)]
pub struct CallingRegion {
    /// CLDR region id, e.g. `"US"`.
    pub code: String,
    /// English display name, e.g. `"United States"`.
    pub name: Option<String>,
    /// True for the main country of the calling code.
    pub main: bool,
}

/// A complete, serializable description of a parsed phone number.
#[derive(Clone, Debug, Serialize)]
pub struct PhoneNumberInfo {
    /// Country calling code, e.g. `1`, `33`, `44`.
    pub country_code: u16,
    /// The number's resolved CLDR region id (e.g. `"US"`), or `"001"` for
    /// non-geographical entities.
    pub region: Option<String>,
    /// English country/region name (e.g. `"United States"`).
    pub region_name: Option<String>,
    /// Every region served by this calling code, main country first.
    pub calling_regions: Vec<CallingRegion>,

    /// National significant number as digits (with any Italian leading zero).
    pub national_number: String,
    /// Extension, if the input carried one.
    pub extension: Option<String>,

    /// `+16502530000`
    pub e164: String,
    /// `+1 650-253-0000`
    pub international: String,
    /// `(650) 253-0000`
    pub national: String,
    /// `tel:+1-650-253-0000`
    pub rfc3966: String,

    /// Passes full validation for its region.
    pub valid: bool,
    /// Has a plausible length for its region (weaker than `valid`).
    pub possible: bool,
    /// Can be dialled from outside its own region.
    pub can_be_internationally_dialled: bool,

    /// Number type as a string: `"mobile"`, `"fixed_line"`, `"toll_free"`, ...
    #[serde(rename = "type")]
    pub number_type: String,
    pub is_fixed_line: bool,
    pub is_mobile: bool,
    pub is_fixed_line_or_mobile: bool,
    pub is_toll_free: bool,
    pub is_premium_rate: bool,
    pub is_voip: bool,

    /// Approximate geographic location (English), e.g. `"Mountain View, CA"`.
    pub location: Option<String>,
    /// Original carrier/operator (English), when known.
    pub carrier: Option<String>,
    /// IANA time zones the number maps to.
    pub time_zones: Vec<String>,
}

impl PhoneNumberInfo {
    /// Serialize to a compact JSON string.
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).expect("PhoneNumberInfo is always serializable")
    }

    /// Serialize to a pretty-printed JSON string.
    pub fn to_json_pretty(&self) -> String {
        serde_json::to_string_pretty(self).expect("PhoneNumberInfo is always serializable")
    }
}

/// String tag for a number type, matching libphonenumber's enum names in
/// lower_snake_case.
fn type_name(t: PhoneNumberType) -> &'static str {
    match t {
        PhoneNumberType::FixedLine => "fixed_line",
        PhoneNumberType::Mobile => "mobile",
        PhoneNumberType::FixedLineOrMobile => "fixed_line_or_mobile",
        PhoneNumberType::TollFree => "toll_free",
        PhoneNumberType::PremiumRate => "premium_rate",
        PhoneNumberType::SharedCost => "shared_cost",
        PhoneNumberType::Voip => "voip",
        PhoneNumberType::PersonalNumber => "personal_number",
        PhoneNumberType::Pager => "pager",
        PhoneNumberType::Uan => "uan",
        PhoneNumberType::Voicemail => "voicemail",
        PhoneNumberType::Unknown => "unknown",
    }
}

/// Build a full [`PhoneNumberInfo`] for a parsed number.
pub fn analyze(number: &PhoneNumber) -> PhoneNumberInfo {
    let region = validator::region_code_for_number(number).map(|s| s.to_string());
    let region_name = region.as_deref().and_then(metadata::region_name).map(str::to_string);

    let regions = metadata::regions_for_country_code(number.country_code);
    let calling_regions: Vec<CallingRegion> = regions
        .iter()
        .enumerate()
        .map(|(i, code)| CallingRegion {
            code: code.clone(),
            name: metadata::region_name(code).map(str::to_string),
            main: i == 0,
        })
        .collect();

    let t = validator::number_type(number);

    PhoneNumberInfo {
        country_code: number.country_code,
        region,
        region_name,
        calling_regions,
        national_number: number.national_significant_number(),
        extension: number.extension.clone(),
        e164: format(number, PhoneNumberFormat::E164),
        international: format(number, PhoneNumberFormat::International),
        national: format(number, PhoneNumberFormat::National),
        rfc3966: format(number, PhoneNumberFormat::Rfc3966),
        valid: validator::is_valid_number(number),
        possible: validator::is_possible_number(number),
        can_be_internationally_dialled: validator::can_be_internationally_dialled(number),
        number_type: type_name(t).to_string(),
        is_fixed_line: matches!(t, PhoneNumberType::FixedLine | PhoneNumberType::FixedLineOrMobile),
        is_mobile: matches!(t, PhoneNumberType::Mobile | PhoneNumberType::FixedLineOrMobile),
        is_fixed_line_or_mobile: t == PhoneNumberType::FixedLineOrMobile,
        is_toll_free: t == PhoneNumberType::TollFree,
        is_premium_rate: t == PhoneNumberType::PremiumRate,
        is_voip: t == PhoneNumberType::Voip,
        location: mapper::description_or_country(number).map(str::to_string),
        carrier: mapper::carrier_for_number(number).map(str::to_string),
        time_zones: mapper::time_zones_for_number(number)
            .into_iter()
            .map(str::to_string)
            .collect(),
    }
}

/// Parse `input` (with an optional default region) and analyze it in one step.
/// Returns a JSON-ready [`PhoneNumberInfo`], or a [`ParseError`](crate::ParseError).
pub fn analyze_str(
    default_region: Option<&str>,
    input: &str,
) -> Result<PhoneNumberInfo, crate::error::ParseError> {
    crate::parser::parse(default_region, input).map(|n| analyze(&n))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse;

    #[test]
    fn analyze_us_fixed_or_mobile() {
        let n = parse(Some("US"), "(650) 253-0000").unwrap();
        let info = analyze(&n);
        assert_eq!(info.country_code, 1);
        assert_eq!(info.region.as_deref(), Some("US"));
        assert_eq!(info.region_name.as_deref(), Some("United States"));
        assert_eq!(info.e164, "+16502530000");
        assert_eq!(info.national, "(650) 253-0000");
        assert!(info.valid && info.possible);
        assert_eq!(info.number_type, "fixed_line_or_mobile");
        assert!(info.is_fixed_line && info.is_mobile && info.is_fixed_line_or_mobile);
        assert!(!info.is_toll_free);
        // Calling regions include the whole NANP, US first.
        assert!(info.calling_regions.first().map(|r| r.main).unwrap_or(false));
        assert!(info.calling_regions.iter().any(|r| r.code == "CA"));
    }

    #[test]
    fn analyze_toll_free_and_json() {
        let info = analyze_str(None, "+33805123456").unwrap();
        assert_eq!(info.region_name.as_deref(), Some("France"));
        assert_eq!(info.number_type, "toll_free");
        assert!(info.is_toll_free && !info.is_mobile);
        // JSON round-trips through serde_json.
        let json = info.to_json();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["type"], "toll_free");
        assert_eq!(v["country_code"], 33);
        assert_eq!(v["region_name"], "France");
        assert!(info.to_json_pretty().contains("\"toll_free\""));
    }

    #[test]
    fn analyze_populates_geo_carrier_and_non_geo() {
        // A US number that hits geocoding + time zone data.
        let us = analyze_str(None, "+12012345678").unwrap();
        assert_eq!(us.location.as_deref(), Some("New Jersey"));
        assert!(us.time_zones.iter().any(|z| z == "America/New_York"));
        // A UK mobile with a known carrier.
        let gb = analyze_str(Some("GB"), "07106123456").unwrap();
        assert_eq!(gb.carrier.as_deref(), Some("O2"));
        // A non-geographical (+800) number resolves to region "001" / World.
        let ng = analyze_str(None, "+80012345678").unwrap();
        assert_eq!(ng.region.as_deref(), Some("001"));
        assert_eq!(ng.region_name.as_deref(), Some("World"));
    }

    #[test]
    fn france_city_zone_and_country_fallback() {
        // Custom French department/city geocoding.
        let rennes = analyze_str(None, "+33299010203").unwrap();
        assert_eq!(rennes.region_name.as_deref(), Some("France"));
        assert_eq!(rennes.location.as_deref(), Some("Rennes (Ille-et-Vilaine)"));
        let paris = analyze_str(None, "+33142685300").unwrap();
        assert_eq!(paris.location.as_deref(), Some("Paris"));
        // A prefix with only a zone entry falls back to that zone.
        let zone = analyze_str(None, "+33269010203").unwrap();
        assert_eq!(zone.location.as_deref(), Some("Northwest France"));
        // A French number with no zone entry falls back to the country name.
        let mobile = analyze_str(None, "+33612345678").unwrap();
        assert_eq!(mobile.location.as_deref(), Some("France"));
    }

    #[test]
    fn analyze_extension_and_type_names() {
        let info = analyze_str(Some("US"), "(650) 253-0000 ext 42").unwrap();
        assert_eq!(info.extension.as_deref(), Some("42"));
        // region_name lookups.
        assert_eq!(crate::metadata::region_name("GB"), Some("United Kingdom"));
        assert_eq!(crate::metadata::region_name("001"), Some("World"));
        assert_eq!(crate::metadata::region_name("ZZ"), None);
    }
}
