// Shared, self-contained metadata data model.
//
// This file is `include!`-d by BOTH build.rs (to serialize the parsed XML) and
// the library at runtime (to deserialize the embedded blob). It must therefore
// depend on nothing but `serde`. Do not add `use` statements for crate-internal
// modules here.

use serde::{Deserialize, Serialize};

/// Validation description for one number "type" (fixed line, mobile, ...).
///
/// Mirrors libphonenumber's `PhoneNumberDesc`.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct PhoneNumberDesc {
    /// National number pattern (a Java/RE2-style regex source, not yet anchored).
    /// Empty means "type not supported / no specific pattern".
    pub national_number_pattern: String,
    /// Allowed total national-number lengths (digits). Empty = inherit/any.
    pub possible_lengths: Vec<u8>,
    /// Allowed local-only lengths (digits), used for lenient validation.
    pub possible_local_lengths: Vec<u8>,
    /// Example national number for this type, if provided.
    pub example_number: String,
}

impl PhoneNumberDesc {
    pub fn has_pattern(&self) -> bool {
        !self.national_number_pattern.is_empty()
    }
}

/// A single formatting rule. Mirrors libphonenumber's `NumberFormat`.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct NumberFormat {
    /// Capturing regex applied to the national (significant) number.
    pub pattern: String,
    /// Replacement template using `$1`..`$9`.
    pub format: String,
    /// Leading-digit patterns (ordered); the last one is the most specific.
    pub leading_digits: Vec<String>,
    /// e.g. `$NP$FG` — how the national prefix is combined with the first group.
    pub national_prefix_formatting_rule: String,
    /// If true the national prefix may be omitted when formatting nationally.
    pub national_prefix_optional_when_formatting: bool,
    /// Formatting rule when a carrier code is supplied.
    pub domestic_carrier_code_formatting_rule: String,
}

/// Full metadata for one territory (region or non-geographical entity).
/// Mirrors libphonenumber's `PhoneMetadata`.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct PhoneMetadata {
    /// CLDR region id (e.g. "US", "FR") or "001" for non-geographical.
    pub id: String,
    pub country_code: u16,

    pub international_prefix: String,
    pub preferred_international_prefix: String,
    pub national_prefix: String,
    pub preferred_extn_prefix: String,
    /// Regex used to strip the national prefix during parsing.
    pub national_prefix_for_parsing: String,
    /// Transform applied after stripping (with capture groups).
    pub national_prefix_transform_rule: String,
    /// Default national-prefix formatting rule for the whole territory.
    pub national_prefix_formatting_rule: String,
    pub national_prefix_optional_when_formatting: bool,

    /// True for the primary region owning a shared country code (e.g. US for +1).
    pub main_country_for_code: bool,
    /// Leading-digit pattern distinguishing regions that share a country code.
    pub leading_digits: String,
    pub mobile_number_portable_region: bool,

    pub general_desc: PhoneNumberDesc,
    pub fixed_line: PhoneNumberDesc,
    pub mobile: PhoneNumberDesc,
    pub toll_free: PhoneNumberDesc,
    pub premium_rate: PhoneNumberDesc,
    pub shared_cost: PhoneNumberDesc,
    pub personal_number: PhoneNumberDesc,
    pub voip: PhoneNumberDesc,
    pub pager: PhoneNumberDesc,
    pub uan: PhoneNumberDesc,
    pub voicemail: PhoneNumberDesc,
    pub no_international_dialling: PhoneNumberDesc,

    pub number_formats: Vec<NumberFormat>,
    pub intl_number_formats: Vec<NumberFormat>,
    /// True when `same_mobile_and_fixed_line_pattern` in upstream (mobile == fixed).
    pub same_mobile_and_fixed_line_pattern: bool,
}

/// Short-number metadata for one region. Mirrors the relevant subset of
/// libphonenumber's short-number `PhoneMetadata`.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ShortMetadata {
    pub id: String,
    pub general_desc: PhoneNumberDesc,
    pub short_code: PhoneNumberDesc,
    pub toll_free: PhoneNumberDesc,
    pub premium_rate: PhoneNumberDesc,
    pub standard_rate: PhoneNumberDesc,
    pub emergency: PhoneNumberDesc,
    pub expanded_emergency: PhoneNumberDesc,
    pub sms_services: PhoneNumberDesc,
    pub carrier_specific: PhoneNumberDesc,
}

/// The deserialized short-number dataset (region id -> metadata).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ShortDb {
    pub regions: Vec<ShortMetadata>,
}

/// A longest-prefix-match table: dialed-number prefix -> text (location,
/// carrier, or `&`-joined time zones). Entries are keyed by the full
/// international prefix, i.e. country code followed by leading national digits.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct PrefixMap {
    pub entries: Vec<(String, String)>,
}

/// The complete deserialized dataset.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct MetadataDb {
    /// region id -> metadata (geographical regions only).
    pub regions: Vec<PhoneMetadata>,
    /// country calling code -> metadata (non-geographical entities, keyed by code).
    pub non_geo: Vec<PhoneMetadata>,
    /// country calling code -> ordered list of region ids ("calling regions").
    /// The main country for the code is first.
    pub code_to_regions: Vec<(u16, Vec<String>)>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desc_has_pattern_and_defaults() {
        let mut d = PhoneNumberDesc::default();
        assert!(!d.has_pattern());
        d.national_number_pattern = "\\d+".to_string();
        assert!(d.has_pattern());

        // Defaults construct cleanly and are Clone/Debug.
        let m = PhoneMetadata::default();
        assert_eq!(m.country_code, 0);
        let _ = format!("{:?}", m.clone());
        let s = ShortMetadata::default();
        let _ = format!("{:?}", s.clone());
        let pm = PrefixMap::default();
        assert!(pm.entries.is_empty());
        let db = MetadataDb::default();
        assert!(db.regions.is_empty());
        let sd = ShortDb::default();
        assert!(sd.regions.is_empty());
        let nf = NumberFormat::default();
        assert!(nf.pattern.is_empty());
    }
}
