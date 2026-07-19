//! The parsed [`PhoneNumber`] value and the [`PhoneNumberType`] enum.

/// A parsed phone number, equivalent to libphonenumber's `PhoneNumber` proto.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PhoneNumber {
    /// Country calling code, e.g. 1, 33, 44.
    pub country_code: u16,
    /// National (significant) number, without national prefix, as an integer.
    /// Stored as a string of digits to preserve leading zeros faithfully.
    pub national_number: u64,
    /// Number of leading zeros of the national number (Italian leading-zero case).
    pub number_of_leading_zeros: u8,
    /// True when the national number carries a significant leading zero.
    pub italian_leading_zero: bool,
    /// Extension digits, if any.
    pub extension: Option<String>,
    /// Raw string as originally parsed (best-effort, for round-tripping).
    pub raw_input: Option<String>,
    /// How the country code was determined during parsing.
    pub country_code_source: CountryCodeSource,
    /// Preferred domestic carrier code, when supplied.
    pub preferred_domestic_carrier_code: Option<String>,
}

impl Default for PhoneNumber {
    fn default() -> Self {
        PhoneNumber {
            country_code: 0,
            national_number: 0,
            number_of_leading_zeros: 0,
            italian_leading_zero: false,
            extension: None,
            raw_input: None,
            country_code_source: CountryCodeSource::Unspecified,
            preferred_domestic_carrier_code: None,
        }
    }
}

impl PhoneNumber {
    /// The national significant number as a zero-padded digit string, matching
    /// how libphonenumber reconstructs it for pattern matching and formatting.
    pub fn national_significant_number(&self) -> String {
        let mut s = String::new();
        if self.italian_leading_zero {
            for _ in 0..self.number_of_leading_zeros.max(1) {
                s.push('0');
            }
        }
        s.push_str(&self.national_number.to_string());
        s
    }
}

/// The source of the country code in a parsed number.
/// Mirrors libphonenumber's `CountryCodeSource`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum CountryCodeSource {
    #[default]
    Unspecified,
    /// Number began with "+".
    FromNumberWithPlusSign,
    /// Number began with the IDD (e.g. "00").
    FromNumberWithIdd,
    /// Number began with a country code but no leading "+" or IDD.
    FromNumberWithoutPlusSign,
    /// Country code came from the default region, not the number itself.
    FromDefaultCountry,
}

/// The category of a phone number. Mirrors libphonenumber's `PhoneNumberType`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PhoneNumberType {
    FixedLine,
    Mobile,
    /// Regions where fixed line and mobile share the same pattern.
    FixedLineOrMobile,
    TollFree,
    PremiumRate,
    SharedCost,
    Voip,
    PersonalNumber,
    Pager,
    Uan,
    Voicemail,
    Unknown,
}

#[cfg(test)]
#[allow(clippy::field_reassign_with_default)]
mod tests {
    use super::*;

    #[test]
    fn nsn_and_leading_zero() {
        let mut n = PhoneNumber::default();
        n.country_code = 39;
        n.national_number = 236618300;
        assert_eq!(n.national_significant_number(), "236618300");
        // Italian leading zero adds the zeros back.
        n.italian_leading_zero = true;
        n.number_of_leading_zeros = 1;
        assert_eq!(n.national_significant_number(), "0236618300");
        n.number_of_leading_zeros = 2;
        assert_eq!(n.national_significant_number(), "00236618300");
        // leading-zero flag with 0 count still adds at least one.
        n.number_of_leading_zeros = 0;
        assert_eq!(n.national_significant_number(), "0236618300");
    }

    #[test]
    fn defaults_and_enums() {
        let n = PhoneNumber::default();
        assert_eq!(n.country_code_source, CountryCodeSource::Unspecified);
        assert_eq!(CountryCodeSource::default(), CountryCodeSource::Unspecified);
        // Enum Debug/Clone/Eq.
        assert_eq!(PhoneNumberType::Mobile, PhoneNumberType::Mobile);
        assert_ne!(PhoneNumberType::Mobile, PhoneNumberType::FixedLine);
        let _ = format!("{:?}", PhoneNumberType::Voip);
        let _ = n.clone();
    }
}
