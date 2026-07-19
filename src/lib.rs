//! # phonenumber
//!
//! A Rust rewrite of Google's [libphonenumber]. It parses, validates, and
//! formats international phone numbers, and exposes the full calling-code ↔
//! region ("calling regions") metadata generated from the upstream XML.
//!
//! ```
//! use phonenumber::{parse, format, PhoneNumberFormat};
//!
//! let n = parse(Some("US"), "(650) 253-0000").unwrap();
//! assert_eq!(n.country_code, 1);
//! assert_eq!(format(&n, PhoneNumberFormat::E164), "+16502530000");
//! assert!(phonenumber::is_valid_number(&n));
//! ```
//!
//! [libphonenumber]: https://github.com/google/libphonenumber

mod model;

pub mod ayt;
pub mod error;
pub mod example;
pub mod format;
pub mod info;
pub mod mapper;
pub mod metadata;
pub mod number;
pub mod parser;
pub mod short;
pub mod validator;

pub use ayt::{format_as_you_type, AsYouTypeFormatter};
pub use error::ParseError;
pub use example::{example_number, example_number_for_type};
pub use info::{analyze, analyze_str, CallingRegion, PhoneNumberInfo};
pub use mapper::{
    carrier_for_number, description_for_number, description_or_country, time_zones_for_number,
};
pub use short::{
    connects_to_emergency_number, is_emergency_number, is_valid_short_number,
    is_valid_short_number_for_region,
};
pub use format::{
    format, format_national, format_national_number_with_carrier_code,
    format_national_number_with_preferred_carrier_code, format_out_of_country_calling_number,
    PhoneNumberFormat,
};
pub use number::{CountryCodeSource, PhoneNumber, PhoneNumberType};
pub use parser::parse;
pub use validator::{
    can_be_internationally_dialled, is_possible_number, is_valid_number,
    is_valid_number_for_region, number_type, validate_length, PossibleResult,
};

/// Does the region support mobile number portability (e.g. `"US"` → true)?
pub fn is_mobile_number_portable_region(region: &str) -> bool {
    metadata::is_mobile_number_portable_region(region)
}

// --- Calling-regions convenience re-exports --------------------------------

/// Country calling code for a CLDR region id (e.g. `"FR"` → `33`).
pub fn country_code_for_region(region: &str) -> Option<u16> {
    metadata::country_code_for_region(region)
}

/// English display name for a CLDR region id (e.g. `"FR"` → `"France"`).
pub fn region_name(region: &str) -> Option<&'static str> {
    metadata::region_name(region)
}

/// The region ids served by a country calling code, main country first
/// (e.g. `1` → `["US", "CA", ...]`). This is the "regions lançantes" map.
pub fn regions_for_country_code(code: u16) -> &'static [String] {
    metadata::regions_for_country_code(code)
}

/// The main region for a country calling code (e.g. `1` → `"US"`), or `"001"`
/// for non-geographical entities.
pub fn main_region_for_country_code(code: u16) -> Option<&'static str> {
    metadata::main_region_for_country_code(code)
}

/// Every supported geographical region id.
pub fn supported_regions() -> impl Iterator<Item = &'static str> {
    metadata::supported_regions()
}

/// Every supported country calling code (geographical and non-geographical).
pub fn supported_calling_codes() -> Vec<u16> {
    metadata::supported_calling_codes()
}
