//! Error types.

use std::fmt;

/// Reasons parsing can fail. Mirrors libphonenumber's `NumberParseException`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ParseError {
    /// The string did not look like a phone number at all.
    NotANumber,
    /// The number is too short after the IDD/country code.
    TooShortAfterIdd,
    /// The national number is shorter than any valid length.
    TooShortNsn,
    /// The number is longer than the maximum (17 digits).
    TooLong,
    /// A country code could not be determined and no valid default region was given.
    InvalidCountryCode,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let msg = match self {
            ParseError::NotANumber => "the string supplied did not seem to be a phone number",
            ParseError::TooShortAfterIdd => "phone number too short after IDD",
            ParseError::TooShortNsn => "the string is too short to be a phone number",
            ParseError::TooLong => "the string supplied is too long to be a phone number",
            ParseError::InvalidCountryCode => "invalid country calling code",
        };
        f.write_str(msg)
    }
}

impl std::error::Error for ParseError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_and_eq_all_variants() {
        let all = [
            ParseError::NotANumber,
            ParseError::TooShortAfterIdd,
            ParseError::TooShortNsn,
            ParseError::TooLong,
            ParseError::InvalidCountryCode,
        ];
        for e in &all {
            let s = e.to_string();
            assert!(!s.is_empty());
        }
        assert_eq!(ParseError::TooLong, ParseError::TooLong);
        assert_ne!(ParseError::TooLong, ParseError::NotANumber);
        // Debug + Clone.
        let c = all[0].clone();
        assert_eq!(format!("{c:?}"), "NotANumber");
        // std::error::Error is implemented.
        let _: &dyn std::error::Error = &all[0];
    }
}
