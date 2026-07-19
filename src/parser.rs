//! Parsing free-form input into a [`PhoneNumber`].
//!
//! This follows libphonenumber's `parseHelper` pipeline: isolate the candidate,
//! strip any extension, resolve the country calling code (via `+`, IDD, or the
//! default region), strip the national prefix, and record the significant
//! number together with any Italian leading zero.

use once_cell::sync::Lazy;
use regex::Regex;

use crate::error::ParseError;
use crate::metadata;
use crate::model::PhoneMetadata;
use crate::number::{CountryCodeSource, PhoneNumber};

const MIN_LENGTH_FOR_NSN: usize = 2;
const MAX_LENGTH_FOR_NSN: usize = 17;
const MAX_LENGTH_COUNTRY_CODE: usize = 3;

/// Extension separators we recognize (a pragmatic subset of upstream EXTN_PATTERN).
static EXTN_PATTERN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?i)(?:;ext=([0-9]{1,7})|[ \t,]*(?:e?xt(?:ensi(?:o|ó)?n)?|ext[.]?|x)[:\.]?[ \t,-]*([0-9]{1,7})#?|[ \t]*[#]([0-9]{1,7}))\s*$",
    )
    .unwrap()
});

/// Decimal value of a digit character, supporting ASCII plus the Unicode digit
/// ranges libphonenumber normalizes (fullwidth, Arabic-Indic, extended
/// Arabic-Indic).
fn digit_value(c: char) -> Option<u8> {
    match c {
        '0'..='9' => Some(c as u8 - b'0'),
        '\u{FF10}'..='\u{FF19}' => Some(c as u32 as u8 - 0x10), // fullwidth ０-９
        '\u{0660}'..='\u{0669}' => Some((c as u32 - 0x0660) as u8), // Arabic-Indic
        '\u{06F0}'..='\u{06F9}' => Some((c as u32 - 0x06F0) as u8), // extended Arabic-Indic
        _ => None,
    }
}

/// Characters that count as the start of a real number (incl. Unicode digits
/// and the fullwidth plus sign `＋`).
fn is_plus_or_digit(c: char) -> bool {
    c == '+' || c == '\u{FF0B}' || digit_value(c).is_some()
}

/// Reduce a raw candidate to `+` and ASCII digits, converting Unicode digits
/// and common wide-digit forms to ASCII. Non-digit separators are dropped.
fn normalize(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut seen_plus = false;
    for c in input.chars() {
        if (c == '+' || c == '\u{FF0B}') && !seen_plus && out.is_empty() {
            out.push('+');
            seen_plus = true;
        } else if let Some(d) = digit_value(c) {
            out.push(char::from(b'0' + d));
        }
        // everything else (spaces, dashes, parens, letters) is dropped
    }
    out
}

/// Isolate the phone-number candidate from arbitrary text and handle RFC3966.
fn extract_candidate(input: &str) -> String {
    let mut s = input.trim();

    // RFC3966: strip a leading "tel:" and cut off any parameters after ';'
    // except we must keep ";ext=" for extension handling.
    if let Some(rest) = s.strip_prefix("tel:").or_else(|| s.strip_prefix("Tel:")) {
        s = rest;
    }

    // Trim leading junk before the first '+' or digit.
    let start = s.find(is_plus_or_digit).unwrap_or(s.len());
    s[start..].to_string()
}

/// Pull a trailing extension off the candidate, returning (number, ext).
fn strip_extension(candidate: &str) -> (String, Option<String>) {
    if let Some(caps) = EXTN_PATTERN.captures(candidate) {
        let ext = (1..caps.len())
            .filter_map(|i| caps.get(i))
            .map(|m| m.as_str().to_string())
            .next();
        if let Some(ext) = ext {
            let end = caps.get(0).unwrap().start();
            return (candidate[..end].to_string(), Some(ext));
        }
    }
    (candidate.to_string(), None)
}

/// Does a bare national-number-like string satisfy the territory's general
/// description (used as the "is this plausible" gate during parsing)?
fn is_viable(nsn: &str, meta: &PhoneMetadata) -> bool {
    match metadata::anchored_regex(&meta.general_desc.national_number_pattern) {
        Some(re) => re.is_match(nsn),
        None => false,
    }
}

/// Strip the national prefix (and capture a carrier code) using the territory's
/// `nationalPrefixForParsing` regex and optional transform rule.
/// Returns the resulting national number and any carrier code.
fn strip_national_prefix(number: &str, meta: &PhoneMetadata) -> (String, Option<String>) {
    let np_parse = &meta.national_prefix_for_parsing;
    if np_parse.is_empty() || number.is_empty() {
        return (number.to_string(), None);
    }
    let Some(re) = metadata::prefix_regex(np_parse) else {
        return (number.to_string(), None);
    };
    let Some(caps) = re.captures(number) else {
        return (number.to_string(), None);
    };
    let whole = caps.get(0).unwrap();
    // Only a prefix match is meaningful.
    if whole.start() != 0 {
        return (number.to_string(), None);
    }

    let viable_original = is_viable(number, meta);
    let num_groups = caps.len() - 1;
    let transform = &meta.national_prefix_transform_rule;
    let last_group_empty = caps
        .get(num_groups)
        .map(|m| m.as_str().is_empty())
        .unwrap_or(true);

    if transform.is_empty() || last_group_empty {
        // Simple strip: drop the matched prefix.
        let stripped = &number[whole.end()..];
        if viable_original && !is_viable(stripped, meta) {
            // Stripping would break validity — keep the original.
            return (number.to_string(), None);
        }
        let carrier = if num_groups >= 1 {
            caps.get(1).map(|m| m.as_str().to_string()).filter(|s| !s.is_empty())
        } else {
            None
        };
        (stripped.to_string(), carrier)
    } else {
        // Transform: rebuild the number from the capture groups.
        let transformed = re.replace(number, template_to_rust(transform).as_str()).into_owned();
        if viable_original && !is_viable(&transformed, meta) {
            return (number.to_string(), None);
        }
        let carrier = if num_groups > 1 {
            caps.get(1).map(|m| m.as_str().to_string()).filter(|s| !s.is_empty())
        } else {
            None
        };
        (transformed, carrier)
    }
}

/// Upstream transform rules use `$1` group references, matching Rust's syntax,
/// but Rust needs `${1}` when a digit follows. Numbers here never do, so pass
/// through unchanged.
fn template_to_rust(rule: &str) -> String {
    rule.to_string()
}

/// Strip a leading international prefix (IDD) from a national-context number,
/// returning the remainder if the IDD matched.
fn strip_idd(number: &str, meta: &PhoneMetadata) -> Option<String> {
    let idd = &meta.international_prefix;
    if idd.is_empty() {
        return None;
    }
    let re = metadata::prefix_regex(idd)?;
    let m = re.find(number)?;
    if m.start() != 0 || m.end() == 0 {
        return None;
    }
    // The remainder must not start with 0 (which would indicate the IDD was
    // actually part of the number).
    let rest = &number[m.end()..];
    if rest.starts_with('0') {
        return None;
    }
    Some(rest.to_string())
}

/// Try to read a 1–3 digit country calling code off the front of `number`.
fn extract_country_code(number: &str) -> Option<(u16, String)> {
    if number.is_empty() || number.starts_with('0') {
        return None;
    }
    let max = number.len().min(MAX_LENGTH_COUNTRY_CODE);
    for i in 1..=max {
        if let Ok(code) = number[..i].parse::<u16>() {
            if metadata::is_valid_calling_code(code) {
                return Some((code, number[i..].to_string()));
            }
        }
    }
    None
}

/// Parse free-form `input` into a [`PhoneNumber`].
///
/// `default_region` (a CLDR id like `"US"`) supplies the country code when the
/// input is in national form. It is ignored when the input carries a `+` or a
/// recognizable IDD.
pub fn parse(default_region: Option<&str>, input: &str) -> Result<PhoneNumber, ParseError> {
    let candidate = extract_candidate(input);
    if candidate.len() < MIN_LENGTH_FOR_NSN && !candidate.starts_with('+') {
        return Err(ParseError::NotANumber);
    }

    let (candidate, extension) = strip_extension(&candidate);
    let normalized = normalize(&candidate);
    if normalized.is_empty() {
        return Err(ParseError::NotANumber);
    }

    let default_meta = default_region.and_then(metadata::for_region);

    let (country_code, mut national, source) = if let Some(rest) = normalized.strip_prefix('+') {
        // International, explicit '+'.
        let (code, nsn) =
            extract_country_code(rest).ok_or(ParseError::InvalidCountryCode)?;
        (code, nsn, CountryCodeSource::FromNumberWithPlusSign)
    } else if let Some(meta) = default_meta {
        // National context: maybe an IDD prefix, maybe the CC without '+'.
        if let Some(after_idd) = strip_idd(&normalized, meta) {
            match extract_country_code(&after_idd) {
                Some((code, nsn)) => (code, nsn, CountryCodeSource::FromNumberWithIdd),
                None => return Err(ParseError::InvalidCountryCode),
            }
        } else {
            // Plain national number for the default region. Consider the case
            // where it already includes the country code without a '+'.
            let cc = meta.country_code;
            let cc_str = cc.to_string();
            if let Some(without_cc) = normalized.strip_prefix(&cc_str) {
                let (stripped_np, _) = strip_national_prefix(without_cc, meta);
                if !is_viable(&normalized, meta) && is_viable(&stripped_np, meta) {
                    (cc, without_cc.to_string(), CountryCodeSource::FromNumberWithoutPlusSign)
                } else {
                    (cc, normalized.clone(), CountryCodeSource::FromDefaultCountry)
                }
            } else {
                (cc, normalized.clone(), CountryCodeSource::FromDefaultCountry)
            }
        }
    } else {
        return Err(ParseError::InvalidCountryCode);
    };

    // Strip the national prefix using the metadata for the resolved code.
    let region_meta = resolve_meta(country_code, source, default_meta);
    let mut carrier = None;
    if let Some(meta) = region_meta {
        let (stripped, cc) = strip_national_prefix(&national, meta);
        national = stripped;
        carrier = cc;
    }

    if national.len() < MIN_LENGTH_FOR_NSN {
        return Err(ParseError::TooShortNsn);
    }
    if national.len() > MAX_LENGTH_FOR_NSN {
        return Err(ParseError::TooLong);
    }

    // Italian leading zero handling.
    let leading_zeros = national.chars().take_while(|&c| c == '0').count();
    let italian_leading_zero = leading_zeros > 0 && national != "0";
    let national_number: u64 = national.parse().map_err(|_| ParseError::NotANumber)?;

    Ok(PhoneNumber {
        country_code,
        national_number,
        number_of_leading_zeros: leading_zeros as u8,
        italian_leading_zero,
        extension,
        raw_input: Some(input.to_string()),
        country_code_source: source,
        preferred_domestic_carrier_code: carrier,
    })
}

/// Choose the metadata used for national-prefix stripping: the default region
/// when the code came from it, otherwise the main region for the code.
fn resolve_meta(
    code: u16,
    source: CountryCodeSource,
    default_meta: Option<&'static PhoneMetadata>,
) -> Option<&'static PhoneMetadata> {
    if source == CountryCodeSource::FromDefaultCountry {
        return default_meta;
    }
    match metadata::main_region_for_country_code(code) {
        Some(r) if r != "001" => metadata::for_region(r),
        _ => metadata::for_non_geo_code(code),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plus_and_national() {
        let n = parse(None, "+1 650 253 0000").unwrap();
        assert_eq!(n.country_code, 1);
        assert_eq!(n.national_number, 6502530000);
        assert_eq!(n.country_code_source, CountryCodeSource::FromNumberWithPlusSign);

        let d = parse(Some("US"), "650 253 0000").unwrap();
        assert_eq!(d.country_code_source, CountryCodeSource::FromDefaultCountry);
    }

    #[test]
    fn national_prefix_stripped_fr() {
        let n = parse(Some("FR"), "01 42 68 53 00").unwrap();
        assert_eq!(n.national_number, 142685300);
    }

    #[test]
    fn country_code_without_plus() {
        // FR default, but the number carries the country code with no '+'.
        let n = parse(Some("FR"), "33142685300").unwrap();
        assert_eq!(n.country_code, 33);
        assert_eq!(n.national_number, 142685300);
        assert_eq!(n.country_code_source, CountryCodeSource::FromNumberWithoutPlusSign);
    }

    #[test]
    fn idd_prefix() {
        // Dialing internationally from the US: 011 + 33 + national.
        let n = parse(Some("US"), "011 33 1 42 68 53 00").unwrap();
        assert_eq!(n.country_code, 33);
        assert_eq!(n.country_code_source, CountryCodeSource::FromNumberWithIdd);
    }

    #[test]
    fn extensions() {
        for input in ["+1 650 253 0000 ext. 1234", "+16502530000x1234", "+16502530000;ext=1234", "+16502530000#1234"] {
            let n = parse(None, input).unwrap();
            assert_eq!(n.extension.as_deref(), Some("1234"), "input={input}");
        }
        let none = parse(None, "+16502530000").unwrap();
        assert!(none.extension.is_none());
    }

    #[test]
    fn rfc3966_tel_prefix() {
        let n = parse(None, "tel:+1-650-253-0000").unwrap();
        assert_eq!(n.country_code, 1);
        assert_eq!(n.national_number, 6502530000);
    }

    #[test]
    fn unicode_and_letters_normalized() {
        // Fullwidth digits + separators.
        let n = parse(None, "＋１ ６５０ ２５３ ００００").unwrap();
        assert_eq!(n.country_code, 1);
    }

    #[test]
    fn error_cases() {
        assert_eq!(parse(None, ""), Err(ParseError::NotANumber));
        assert_eq!(parse(None, "abc"), Err(ParseError::NotANumber));
        // No default region and no '+': cannot resolve a country code.
        assert_eq!(parse(None, "650 253 0000"), Err(ParseError::InvalidCountryCode));
        // '+' with no valid country code.
        assert_eq!(parse(None, "+9999999"), Err(ParseError::InvalidCountryCode));
        // Too long.
        assert_eq!(parse(None, "+12345678901234567890"), Err(ParseError::TooLong));
        // Too short national number.
        assert!(matches!(parse(Some("US"), "1"), Err(ParseError::NotANumber) | Err(ParseError::TooShortNsn)));
    }
}

#[cfg(test)]
#[allow(clippy::field_reassign_with_default)]
mod helper_tests {
    use super::*;
    use crate::model::{PhoneMetadata, PhoneNumberDesc};

    fn meta_with(np_parse: &str, transform: &str, general: &str) -> PhoneMetadata {
        let mut m = PhoneMetadata::default();
        m.national_prefix_for_parsing = np_parse.to_string();
        m.national_prefix_transform_rule = transform.to_string();
        m.general_desc = PhoneNumberDesc {
            national_number_pattern: general.to_string(),
            ..Default::default()
        };
        m.international_prefix = "00".to_string();
        m
    }

    #[test]
    fn extract_country_code_cases() {
        assert_eq!(extract_country_code("33123"), Some((33, "123".to_string())));
        assert_eq!(extract_country_code("1650"), Some((1, "650".to_string())));
        assert!(extract_country_code("0123").is_none());  // leading zero
        assert!(extract_country_code("").is_none());
        assert!(extract_country_code("9990").is_none());  // no valid code
    }

    #[test]
    fn strip_idd_cases() {
        let m = meta_with("", "", r"\d{5,}");
        assert_eq!(strip_idd("00331234567", &m).as_deref(), Some("331234567"));
        // Remainder starting with 0 is rejected.
        assert!(strip_idd("000123", &m).is_none());
        // No IDD prefix present.
        assert!(strip_idd("331234567", &m).is_none());
        // Empty international prefix -> None.
        let mut m2 = m.clone();
        m2.international_prefix.clear();
        assert!(strip_idd("00331234567", &m2).is_none());
    }

    #[test]
    fn strip_national_prefix_simple_and_carrier() {
        // Simple strip of a leading 0.
        let m = meta_with("0", "", r"\d{6,}");
        let (n, c) = strip_national_prefix("0123456", &m);
        assert_eq!(n, "123456");
        assert!(c.is_none());

        // Carrier code captured in group 1, prefix stripped.
        let m2 = meta_with(r"0(\d{2})", "", r"\d{6,}");
        let (n2, c2) = strip_national_prefix("015123456", &m2);
        assert_eq!(c2.as_deref(), Some("15"));
        assert_eq!(n2, "123456");

        // Transform rule rebuilds the number from groups.
        let m3 = meta_with(r"0(\d+)", "9$1", r"\d{6,}");
        let (n3, _) = strip_national_prefix("0123456", &m3);
        assert_eq!(n3, "9123456");

        // No parsing prefix -> unchanged.
        let m4 = meta_with("", "", r"\d+");
        assert_eq!(strip_national_prefix("123", &m4).0, "123");
    }

    #[test]
    fn strip_keeps_original_when_break_viability() {
        // Original is viable (9 digits), stripping the '9' would leave a
        // non-viable 8-digit number -> keep original.
        let m = meta_with("9", "", r"\d{9}");
        let (n, _) = strip_national_prefix("912345678", &m);
        assert_eq!(n, "912345678");
    }

    #[test]
    fn digit_value_ranges() {
        assert_eq!(digit_value('7'), Some(7));
        assert_eq!(digit_value('\u{FF15}'), Some(5)); // fullwidth 5
        assert_eq!(digit_value('\u{0663}'), Some(3)); // Arabic-Indic 3
        assert_eq!(digit_value('\u{06F9}'), Some(9)); // extended Arabic-Indic 9
        assert_eq!(digit_value('a'), None);
    }

    #[test]
    fn parse_too_short_nsn() {
        assert_eq!(parse(None, "+12"), Err(ParseError::TooShortNsn));
    }
}
