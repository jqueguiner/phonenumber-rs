//! Example numbers, mirroring libphonenumber's `getExampleNumber*` helpers.

use crate::metadata;
use crate::model::{PhoneMetadata, PhoneNumberDesc};
use crate::number::{PhoneNumber, PhoneNumberType};
use crate::parser::parse;

fn desc_for_type(meta: &PhoneMetadata, t: PhoneNumberType) -> &PhoneNumberDesc {
    match t {
        PhoneNumberType::FixedLine => &meta.fixed_line,
        PhoneNumberType::Mobile => &meta.mobile,
        PhoneNumberType::FixedLineOrMobile => &meta.fixed_line,
        PhoneNumberType::TollFree => &meta.toll_free,
        PhoneNumberType::PremiumRate => &meta.premium_rate,
        PhoneNumberType::SharedCost => &meta.shared_cost,
        PhoneNumberType::Voip => &meta.voip,
        PhoneNumberType::PersonalNumber => &meta.personal_number,
        PhoneNumberType::Pager => &meta.pager,
        PhoneNumberType::Uan => &meta.uan,
        PhoneNumberType::Voicemail => &meta.voicemail,
        PhoneNumberType::Unknown => &meta.general_desc,
    }
}

/// An example number of the given type for a region, if one is provided.
pub fn example_number_for_type(region: &str, t: PhoneNumberType) -> Option<PhoneNumber> {
    let meta = metadata::for_region(region)?;
    let desc = desc_for_type(meta, t);
    if desc.example_number.is_empty() {
        return None;
    }
    parse(Some(region), &desc.example_number).ok()
}

/// A representative valid example number for a region (fixed line, then mobile).
pub fn example_number(region: &str) -> Option<PhoneNumber> {
    example_number_for_type(region, PhoneNumberType::FixedLine)
        .or_else(|| example_number_for_type(region, PhoneNumberType::Mobile))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn example_for_types() {
        assert!(example_number_for_type("US", PhoneNumberType::FixedLine).is_some());
        assert!(example_number_for_type("US", PhoneNumberType::Mobile).is_some());
        // Unknown region -> None.
        assert!(example_number_for_type("ZZ", PhoneNumberType::Mobile).is_none());
        // Unknown type maps to general desc (usually has no example) -> None or Some.
        let _ = example_number_for_type("US", PhoneNumberType::Unknown);
        // Fallback helper.
        assert!(example_number("FR").is_some());
        assert!(example_number("ZZ").is_none());
    }
}
