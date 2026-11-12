//! Tests of the parser. Expected answers are those of Go's `github.com/nyaruka/phonenumbers`
//! v1.8.1 (`backend/testdata/go/phone/phones.txt` holds Go's answers for the corpus inputs).

#![allow(clippy::field_reassign_with_default)]

use super::*;
use crate::model::PhoneNumberDesc;

fn e164(input: &str) -> Option<String> {
    let n = parse(None, input).ok()?;
    crate::is_valid_number(&n).then(|| crate::format(&n, crate::PhoneNumberFormat::E164))
}

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
    for input in [
        "+1 650 253 0000 ext. 1234",
        "+16502530000x1234",
        "+16502530000;ext=1234",
        "+16502530000#1234",
    ] {
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
fn rfc3966_phone_context() {
    // Go's buildNationalNumberForParsing: a "+" phone-context is put in front of the number.
    let n = parse(None, "tel:253-0000;phone-context=+1-650").unwrap();
    assert_eq!((n.country_code, n.national_number), (1, 6502530000));
    // A domain phone-context is dropped; the number then needs a region.
    let d = parse(Some("US"), "tel:650-253-0000;phone-context=example.com").unwrap();
    assert_eq!(d.national_number, 6502530000);
    // An invalid or empty phone-context is not a number.
    assert_eq!(parse(None, "tel:+16502530000;phone-context="), Err(ParseError::NotANumber));
    assert_eq!(parse(None, "tel:+16502530000;phone-context=+"), Err(ParseError::NotANumber));
    // An isdn-subaddress and everything after it is dropped.
    let s = parse(None, "+16502530000;isub=123").unwrap();
    assert_eq!(s.national_number, 6502530000);
}

#[test]
fn unicode_and_letters_normalized() {
    // Fullwidth digits + separators.
    let n = parse(None, "＋１ ６５０ ２５３ ００００").unwrap();
    assert_eq!(n.country_code, 1);
    // Three or more letters: a vanity number on the E.161 keypad (Go's normalize).
    let v = parse(None, "+1 800 FLOWERS").unwrap();
    assert_eq!(v.national_number, 8003569377);
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
    // Longer than Go's MAX_INPUT_STRING_LENGTH (250 bytes).
    assert_eq!(parse(None, &format!("+1{}", "0".repeat(250))), Err(ParseError::TooLong));
    // A single digit is not viable (Go's isViablePhoneNumber).
    assert_eq!(parse(Some("US"), "1"), Err(ParseError::NotANumber));
    // Too short after the IDD / plus sign.
    assert_eq!(parse(None, "+1 2-"), Err(ParseError::NotANumber));
    assert_eq!(parse(Some("US"), "011 1"), Err(ParseError::TooShortAfterIdd));
}

#[test]
fn two_digits_after_a_plus_are_not_viable() {
    // Go's VALID_PHONE_NUMBER allows a bare two-digit number, but "+" needs three digits: Go
    // refuses "+12" as not a number (the crate said TooShortNsn before).
    assert_eq!(parse(None, "+12"), Err(ParseError::NotANumber));
    // Three digits are viable: country code 1 and the two-digit national number 23.
    let n = parse(None, "+123").unwrap();
    assert_eq!((n.country_code, n.national_number), (1, 23));
    assert!(!crate::is_valid_number(&n));
}

#[test]
fn characters_go_does_not_take_as_punctuation_refuse_the_number() {
    // From the corpus: Go answers "not a number" for each (isViablePhoneNumber).
    for input in [
        "+1 650\u{202F}253 0000", // narrow no-break space
        "+1 650\u{2007}253 0000", // figure space
        "+1 650+253 0000",
        "+1 650#253 0000",
        "+1 650 253 0000 #",
        "+1 650 253 0000\u{FEFF}",
        "+1 650 253 0000\u{180E}",
        "+1 650 253 0000 \u{1F600}",
    ] {
        assert_eq!(parse(None, input), Err(ParseError::NotANumber), "input={input:?}");
    }
}

#[test]
fn star_is_punctuation_and_a_hash_ends_an_extension() {
    // VALID_PHONE_NUMBER takes "*" between digits; "[- ]+(digits{1,6})#" is an extension.
    let n = parse(None, "+1 650*253 0000#").unwrap();
    assert_eq!((n.national_number, n.extension.as_deref()), (650253, Some("0000")));
}

#[test]
fn comma_extension_is_taken_as_go_does() {
    // Go's EXTN_PATTERNS_FOR_PARSING: ",12" is an auto-dialled extension.
    let n = parse(None, "+16502530000,12").unwrap();
    assert_eq!(n.extension.as_deref(), Some("12"));
    assert_eq!(e164("+16502530000,12").as_deref(), Some("+16502530000"));
}

#[test]
fn a_second_number_and_trailing_text_are_cut() {
    // SECOND_NUMBER_START_PATTERN: "/x" starts another number.
    let n = parse(None, "+1 650 253 0000 x302/x2303").unwrap();
    assert_eq!(n.extension.as_deref(), Some("302"));
    // Leading text before the first plus or digit is dropped (VALID_START_CHAR_PATTERN).
    assert_eq!(parse(None, "Tel: +1 650 253 0000").unwrap().national_number, 6502530000);
}

#[test]
fn non_ascii_digits_are_kept_as_go_keeps_them() {
    // Go's normalizeDigits maps only Arabic-Indic and fullwidth digits; a Devanagari digit stays,
    // so `ParseUint` fails and Go stores national number 0.
    let n = parse(None, "+1 650 253 000\u{0966}").unwrap();
    assert_eq!(n.country_code, 1);
    assert_eq!(n.national_number, 0);
    assert!(!crate::is_valid_number(&n));
    // Arabic-Indic digits do become ASCII.
    let a = parse(None, "+\u{0661} 650 253 0000").unwrap();
    assert_eq!((a.country_code, a.national_number), (1, 6502530000));
}

#[test]
fn vanity_mode_drops_non_ascii_digits() {
    // In vanity mode (three letters) Go's ALPHA_PHONE_MAPPINGS has no fullwidth digits.
    let n = parse(None, "+1 800 FLOWERS\u{FF11}").unwrap();
    assert_eq!(n.national_number, 8003569377);
}

#[test]
fn go_upper_for_mapping_agrees_with_unicode_upper_case() {
    // Every character whose (single) upper case is an ASCII keypad key is mapped as such.
    for cp in 0..=0x10FFFFu32 {
        let Some(c) = char::from_u32(cp) else { continue };
        let mut upper = c.to_uppercase();
        let single = match (upper.next(), upper.next()) {
            (Some(u), None) => u,
            _ => c,
        };
        if single.is_ascii_uppercase() || single.is_ascii_digit() {
            assert_eq!(go_upper_for_mapping(c), single, "U+{cp:04X}");
        } else {
            assert!(alpha_phone_mapping(go_upper_for_mapping(c)).is_none(), "U+{cp:04X}");
        }
    }
}

#[test]
fn italian_leading_zeros_follow_go() {
    assert_eq!(italian_leading_zeros("0236618300"), (true, 1));
    assert_eq!(italian_leading_zeros("00236618300"), (true, 2));
    // The last zero of an all-zero number is not counted.
    assert_eq!(italian_leading_zeros("000"), (true, 2));
    assert_eq!(italian_leading_zeros("0"), (false, 0));
    assert_eq!(italian_leading_zeros("12"), (false, 0));
    let n = parse(None, "+39 02 3661 8300").unwrap();
    assert!(n.italian_leading_zero);
    assert_eq!(n.national_significant_number(), "0236618300");
}

#[test]
fn go_parse_uint_reads_ascii_only() {
    assert_eq!(go_parse_uint_or_zero("123"), 123);
    assert_eq!(go_parse_uint_or_zero("+123"), 0);
    assert_eq!(go_parse_uint_or_zero("1\u{0967}"), 0);
    assert_eq!(go_parse_uint_or_zero(""), 0);
}

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
    assert_eq!(extract_country_code("33123"), Some((33, "123")));
    assert_eq!(extract_country_code("1650"), Some((1, "650")));
    assert!(extract_country_code("0123").is_none()); // leading zero
    assert!(extract_country_code("").is_none());
    assert!(extract_country_code("9990").is_none()); // no valid code
    // Go's Atoi reads ASCII only: a Devanagari digit ends every candidate code.
    assert!(extract_country_code("\u{0967}1").is_none());
}

#[test]
fn parse_prefix_as_idd_cases() {
    assert_eq!(parse_prefix_as_idd("00", "00331234567").as_deref(), Some("331234567"));
    // Remainder starting with 0 is rejected.
    assert!(parse_prefix_as_idd("00", "000123").is_none());
    // No IDD prefix present.
    assert!(parse_prefix_as_idd("00", "331234567").is_none());
    // Go compiles an empty prefix too: it matches nothing at the start.
    assert_eq!(parse_prefix_as_idd("", "331").as_deref(), Some("331"));
}

#[test]
fn strip_national_prefix_simple_and_carrier() {
    let m = meta_with("0", "", r"\d{6,}");
    assert_eq!(
        maybe_strip_national_prefix_and_carrier_code("0123456", &m),
        Some(("123456".to_string(), None))
    );

    // Carrier code captured in group 1, prefix stripped.
    let m2 = meta_with(r"0(\d{2})", "", r"\d{6,}");
    assert_eq!(
        maybe_strip_national_prefix_and_carrier_code("015123456", &m2),
        Some(("123456".to_string(), Some("15".to_string())))
    );

    // Transform rule rebuilds the number from groups.
    let m3 = meta_with(r"0(\d+)", "9$1", r"\d{6,}");
    assert_eq!(
        maybe_strip_national_prefix_and_carrier_code("0123456", &m3).map(|r| r.0).as_deref(),
        Some("9123456")
    );

    // A last group that matched the empty string still takes part (Go checks the group's
    // index, not its length): the transform applies.
    let m4 = meta_with(r"0(\d*)", "9$1", r"\d+");
    assert_eq!(
        maybe_strip_national_prefix_and_carrier_code("0", &m4).map(|r| r.0).as_deref(),
        Some("9")
    );

    // No parsing prefix -> unchanged.
    let m5 = meta_with("", "", r"\d+");
    assert!(maybe_strip_national_prefix_and_carrier_code("123", &m5).is_none());
}

#[test]
fn strip_keeps_original_when_break_viability() {
    // Original is viable (9 digits), stripping the '9' would leave a non-viable 8-digit number.
    let m = meta_with("9", "", r"\d{9}");
    assert!(maybe_strip_national_prefix_and_carrier_code("912345678", &m).is_none());
}
