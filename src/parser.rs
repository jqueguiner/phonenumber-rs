//! Parsing free-form input into a [`PhoneNumber`].
//!
//! A step-for-step port of `parseHelper` from Go's `github.com/nyaruka/phonenumbers` v1.8.1
//! (phonenumberutil.go:2680), itself a port of libphonenumber. Each function names the Go function
//! and line it follows. Go slices strings by byte offset and measures them in bytes; so does this
//! port (`str::len`, regex match offsets).
//!
//! The crate's `parse` behaves as Go's `ParseAndKeepRawInput`: the number fields are the ones
//! `Parse` gives, and `raw_input`, `country_code_source` and `preferred_domestic_carrier_code` are
//! filled in as Go fills them when it keeps the raw input.

use crate::digits::{is_decimal_digit, normalized_digit};
use crate::error::ParseError;
use crate::metadata;
use crate::model::PhoneMetadata;
use crate::number::{CountryCodeSource, PhoneNumber};
use crate::parse_patterns::{
    EXTN_PATTERN, PLUS_CHARS_PATTERN, RFC3966_DOMAINNAME_PATTERN,
    RFC3966_GLOBAL_NUMBER_DIGITS_PATTERN, SECOND_NUMBER_START_PATTERN, UNWANTED_END_CHAR_PATTERN,
    VALID_ALPHA_PHONE_PATTERN, VALID_PHONE_NUMBER_PATTERN, VALID_START_CHAR_PATTERN,
};
use crate::validator::{test_number_length_general, PossibleResult};

const MIN_LENGTH_FOR_NSN: usize = 2;
const MAX_LENGTH_FOR_NSN: usize = 17;
const MAX_LENGTH_COUNTRY_CODE: usize = 3;
const MAX_INPUT_STRING_LENGTH: usize = 250;
const RFC3966_PREFIX: &str = "tel:";
const RFC3966_PHONE_CONTEXT: &str = ";phone-context=";
const RFC3966_ISDN_SUBADDRESS: &str = ";isub=";
const REGION_CODE_FOR_NON_GEO_ENTITY: &str = "001";
const UNKNOWN_REGION: &str = "ZZ";

/// Go's `extractPossibleNumber` (phonenumberutil.go:513).
fn extract_possible_number(number: &str) -> &str {
    let Some(start) = VALID_START_CHAR_PATTERN.find(number) else {
        return "";
    };
    let mut number = &number[start.start()..];
    if let Some(m) = UNWANTED_END_CHAR_PATTERN.find(number) {
        number = &number[..m.start()];
    }
    if let Some(m) = SECOND_NUMBER_START_PATTERN.find(number) {
        number = &number[..m.start()];
    }
    number
}

/// Go's `isViablePhoneNumber` (phonenumberutil.go:539).
fn is_viable_phone_number(number: &str) -> bool {
    number.len() >= MIN_LENGTH_FOR_NSN && VALID_PHONE_NUMBER_PATTERN.is_match(number)
}

/// Go's `normalize` (phonenumberutil.go:566).
fn normalize(number: &str) -> String {
    if VALID_ALPHA_PHONE_PATTERN.is_match(number) {
        normalize_alpha_phone(number)
    } else {
        normalize_digits_only(number)
    }
}

/// Go's `NormalizeDigitsOnly` / `normalizeDigits(number, false)` (phonenumberutil.go:576, 615).
fn normalize_digits_only(number: &str) -> String {
    number
        .chars()
        .filter(|&c| is_decimal_digit(c))
        .map(normalized_digit)
        .collect()
}

/// Go's `unicode.ToUpper` where the result can be a key of `ALPHA_PHONE_MAPPINGS` (ASCII digits
/// and capitals): ASCII letters, and the two non-ASCII letters whose simple upper case is ASCII,
/// U+0131 (dotless i, to `I`) and U+017F (long s, to `S`). Any other character's upper case is
/// not a key, so it is returned unchanged.
fn go_upper_for_mapping(c: char) -> char {
    match c {
        'a'..='z' => c.to_ascii_uppercase(),
        '\u{0131}' => 'I',
        '\u{017F}' => 'S',
        _ => c,
    }
}

/// Go's `ALPHA_PHONE_MAPPINGS` (phonenumberutil.go:133), the E.161 keypad plus ASCII digits.
fn alpha_phone_mapping(upper: char) -> Option<char> {
    Some(match upper {
        '0'..='9' => upper,
        'A'..='C' => '2',
        'D'..='F' => '3',
        'G'..='I' => '4',
        'J'..='L' => '5',
        'M'..='O' => '6',
        'P'..='S' => '7',
        'T'..='V' => '8',
        'W'..='Z' => '9',
        _ => return None,
    })
}

/// Go's `normalizeHelper(number, ALPHA_PHONE_MAPPINGS, true)` (phonenumberutil.go:791): every
/// character not in the map is dropped, non-ASCII digits included.
fn normalize_alpha_phone(number: &str) -> String {
    number
        .chars()
        .filter_map(|c| alpha_phone_mapping(go_upper_for_mapping(c)))
        .collect()
}

/// Go's `extractPhoneContext` (phonenumberutil.go:2805), for an index that was found.
fn extract_phone_context(number: &str, index_of_phone_context: usize) -> &str {
    let start = index_of_phone_context + RFC3966_PHONE_CONTEXT.len();
    if start >= number.len() {
        return "";
    }
    match number[start..].find(';') {
        Some(end) => &number[start..start + end],
        None => &number[start..],
    }
}

/// Go's `isPhoneContextValid` (phonenumberutil.go:2832).
fn is_phone_context_valid(phone_context: &str) -> bool {
    !phone_context.is_empty()
        && (RFC3966_GLOBAL_NUMBER_DIGITS_PATTERN.is_match(phone_context)
            || RFC3966_DOMAINNAME_PATTERN.is_match(phone_context))
}

/// Go's `buildNationalNumberForParsing` (phonenumberutil.go:2844).
fn build_national_number_for_parsing(number_to_parse: &str) -> Result<String, ParseError> {
    let index_of_phone_context = number_to_parse.find(RFC3966_PHONE_CONTEXT);
    let mut national = String::new();
    match index_of_phone_context {
        Some(index) => {
            let phone_context = extract_phone_context(number_to_parse, index);
            if !is_phone_context_valid(phone_context) {
                return Err(ParseError::NotANumber);
            }
            if index > 0 {
                if phone_context.starts_with('+') {
                    national.push_str(phone_context);
                }
                let start = match number_to_parse.find(RFC3966_PREFIX) {
                    Some(tel) if tel < index => tel + RFC3966_PREFIX.len(),
                    _ => 0,
                };
                national.push_str(&number_to_parse[start..index]);
            } else {
                national.push_str(extract_possible_number(number_to_parse));
            }
        }
        None => national.push_str(extract_possible_number(number_to_parse)),
    }
    if let Some(index_of_isdn) = national.find(RFC3966_ISDN_SUBADDRESS) {
        if index_of_isdn > 0 {
            national.truncate(index_of_isdn);
        }
    }
    Ok(national)
}

/// Go's `checkRegionForParsing` (phonenumberutil.go:2580).
fn check_region_for_parsing(number: &str, region_is_valid: bool) -> bool {
    region_is_valid || (!number.is_empty() && PLUS_CHARS_PATTERN.is_match(number))
}

/// Go's `maybeStripExtension` (phonenumberutil.go:2554): returns the extension, or "" (and the
/// number untouched) when there is none.
fn maybe_strip_extension(number: &mut String) -> String {
    let Some(caps) = EXTN_PATTERN.captures(number) else {
        return String::new();
    };
    let start = caps.get(0).map_or(0, |m| m.start());
    if !is_viable_phone_number(&number[..start]) {
        return String::new();
    }
    let extension = caps
        .iter()
        .skip(1)
        .flatten()
        .map(|m| m.as_str())
        .find(|m| !m.is_empty())
        .map(str::to_owned);
    match extension {
        Some(extension) => {
            number.truncate(start);
            extension
        }
        None => String::new(),
    }
}

/// Does `number` match the general description's pattern, `^(?:pattern)$` (the
/// `nationalNumberRule` / `validNumberPattern` of phonenumberutil.go:2368 and 2470)? An empty
/// pattern matches only the empty string, as Go's regex `^(?:)$` does.
fn matches_general_pattern(number: &str, meta: &PhoneMetadata) -> bool {
    let pattern = &meta.general_desc.national_number_pattern;
    if pattern.is_empty() {
        return number.is_empty();
    }
    metadata::anchored_regex(pattern).is_some_and(|re| re.is_match(number))
}

/// Go's `maybeStripNationalPrefixAndCarrierCode` (phonenumberutil.go:2455): the number with the
/// national prefix stripped and the carrier code (group 1) if any, or None when nothing is
/// stripped.
fn maybe_strip_national_prefix_and_carrier_code(
    number: &str,
    meta: &PhoneMetadata,
) -> Option<(String, Option<String>)> {
    let possible_national_prefix = &meta.national_prefix_for_parsing;
    if number.is_empty() || possible_national_prefix.is_empty() {
        return None;
    }
    // `^(?:prefix)`, matched from the start as Go's `regexFor` + `MatchString` does.
    let prefix_matcher = metadata::prefix_regex(possible_national_prefix)?;
    let groups = prefix_matcher.captures(number)?;
    let is_viable_original_number = matches_general_pattern(number, meta);
    let num_of_groups = groups.len() - 1;
    let transform_rule = &meta.national_prefix_transform_rule;
    let carrier_of = |at_least: usize| {
        (num_of_groups >= at_least)
            .then(|| groups.get(1))
            .flatten()
            .map(|m| m.as_str().to_owned())
            .filter(|c| !c.is_empty())
    };
    // Go: `groups[numOfGroups*2] < 0`, the last group did not take part in the match (with no
    // groups, that is the whole match, which always takes part).
    if transform_rule.is_empty() || groups.get(num_of_groups).is_none() {
        let end = groups.get(0).map_or(0, |m| m.end());
        let rest = &number[end..];
        if is_viable_original_number && !matches_general_pattern(rest, meta) {
            return None;
        }
        Some((rest.to_owned(), carrier_of(1)))
    } else {
        // Go's `ReplaceAllString` with `$n` templates; `^` lets only the match at 0 be replaced.
        let transformed = prefix_matcher.replace(number, transform_rule.as_str()).into_owned();
        if is_viable_original_number && !matches_general_pattern(&transformed, meta) {
            return None;
        }
        Some((transformed, carrier_of(2)))
    }
}

/// Go's `strconv.Atoi` on 1-3 bytes of a normalized number: only ASCII digits read as a number
/// (a normalized number holds no sign), anything else is Go's error, i.e. code 0, which no
/// region has.
fn ascii_number(bytes: &[u8]) -> Option<u16> {
    if bytes.is_empty() || !bytes.iter().all(u8::is_ascii_digit) {
        return None;
    }
    Some(bytes.iter().fold(0u16, |acc, b| acc * 10 + u16::from(b - b'0')))
}

/// Go's `extractCountryCode` (phonenumberutil.go:2281): the code and the rest of the number.
fn extract_country_code(full_number: &str) -> Option<(u16, &str)> {
    let bytes = full_number.as_bytes();
    if bytes.is_empty() || bytes[0] == b'0' {
        return None;
    }
    (1..=MAX_LENGTH_COUNTRY_CODE.min(bytes.len())).find_map(|i| {
        ascii_number(&bytes[..i])
            .filter(|&code| metadata::is_valid_calling_code(code))
            .map(|code| (code, &full_number[i..]))
    })
}

/// Go's `parsePrefixAsIdd` (phonenumberutil.go:2402): the number after the IDD, if it starts with
/// one that is not followed by a 0.
fn parse_prefix_as_idd(idd_pattern: &str, number: &str) -> Option<String> {
    // Go compiles even an empty prefix; it matches the empty string at 0.
    let match_end = if idd_pattern.is_empty() {
        0
    } else {
        metadata::prefix_regex(idd_pattern)?.find(number)?.end()
    };
    let rest = &number[match_end..];
    // CAPTURING_DIGIT_PATTERN's first digit, through NormalizeDigitsOnly.
    if rest.chars().find(|&c| is_decimal_digit(c)).map(normalized_digit) == Some('0') {
        return None;
    }
    Some(rest.to_owned())
}

/// Go's `maybeStripInternationalPrefixAndNormalize` (phonenumberutil.go:2426). `idd_pattern` is
/// None where Go uses the never-matching "NonMatch" (no default region).
fn maybe_strip_international_prefix_and_normalize(
    number: &str,
    idd_pattern: Option<&str>,
) -> (String, CountryCodeSource) {
    if number.is_empty() {
        return (String::new(), CountryCodeSource::FromDefaultCountry);
    }
    if let Some(plus) = PLUS_CHARS_PATTERN.find(number) {
        if plus.start() == 0 {
            return (normalize(&number[plus.end()..]), CountryCodeSource::FromNumberWithPlusSign);
        }
    }
    let normalized = normalize(number);
    if let Some(after_idd) = idd_pattern.and_then(|idd| parse_prefix_as_idd(idd, &normalized)) {
        return (after_idd, CountryCodeSource::FromNumberWithIdd);
    }
    (normalized, CountryCodeSource::FromDefaultCountry)
}

/// A country calling code found in the number: the code, the national number after it, and how
/// it was found.
type ExtractedCountryCode = (u16, String, CountryCodeSource);

/// Go's `maybeExtractCountryCode` (phonenumberutil.go:2320). `Ok(None)` is Go's code 0, "no
/// country calling code present".
fn maybe_extract_country_code(
    number: &str,
    default_meta: Option<&PhoneMetadata>,
) -> Result<Option<ExtractedCountryCode>, ParseError> {
    if number.is_empty() {
        return Ok(None);
    }
    let idd = default_meta.map(|m| m.international_prefix.as_str());
    let (full_number, source) = maybe_strip_international_prefix_and_normalize(number, idd);
    if source != CountryCodeSource::FromDefaultCountry {
        if full_number.len() <= MIN_LENGTH_FOR_NSN {
            return Err(ParseError::TooShortAfterIdd);
        }
        return match extract_country_code(&full_number) {
            Some((code, national)) => Ok(Some((code, national.to_owned(), source))),
            None => Err(ParseError::InvalidCountryCode),
        };
    }
    if let Some(meta) = default_meta {
        let default_country_code = meta.country_code;
        if let Some(potential) = full_number.strip_prefix(&default_country_code.to_string()) {
            let potential = maybe_strip_national_prefix_and_carrier_code(potential, meta)
                .map_or_else(|| potential.to_owned(), |(stripped, _)| stripped);
            let full_valid = matches_general_pattern(&full_number, meta);
            let national_valid = matches_general_pattern(&potential, meta);
            let length = test_number_length_general(&full_number, meta);
            if (!full_valid && national_valid) || length == PossibleResult::TooLong {
                return Ok(Some((
                    default_country_code,
                    potential,
                    CountryCodeSource::FromNumberWithoutPlusSign,
                )));
            }
        }
    }
    Ok(None)
}

/// Go's `getMetadataForRegionOrCallingCode` after `GetRegionCodeForCountryCode`.
fn metadata_for_country_code(code: u16) -> (&'static str, Option<&'static PhoneMetadata>) {
    let region = metadata::main_region_for_country_code(code).unwrap_or(UNKNOWN_REGION);
    let meta = if region == REGION_CODE_FOR_NON_GEO_ENTITY {
        metadata::for_non_geo_code(code)
    } else {
        metadata::for_region(region)
    };
    (region, meta)
}

/// Go's `setItalianLeadingZerosForPhoneNumber` (phonenumberutil.go:2650): (italian_leading_zero,
/// number_of_leading_zeros). The last digit of an all-zero number is not counted.
fn italian_leading_zeros(national: &str) -> (bool, u8) {
    let bytes = national.as_bytes();
    if bytes.len() < 2 || bytes[0] != b'0' {
        return (false, 0);
    }
    let mut zeros = 1;
    while zeros < bytes.len() - 1 && bytes[zeros] == b'0' {
        zeros += 1;
    }
    (true, zeros as u8)
}

/// Go's `strconv.ParseUint(s, 10, 64)` with the error ignored, as `parseHelper` does
/// (phonenumberutil.go:2797): a national number holding a non-ASCII digit becomes 0.
fn go_parse_uint_or_zero(national: &str) -> u64 {
    if national.is_empty() || !national.bytes().all(|b| b.is_ascii_digit()) {
        return 0;
    }
    national.parse().unwrap_or(0)
}

/// Parse free-form `input` into a [`PhoneNumber`]: Go's `parseHelper` (phonenumberutil.go:2680)
/// with `checkRegion` true.
///
/// `default_region` (a CLDR id like `"US"`) supplies the country code when the input is in
/// national form; None is Go's empty region, which needs a `+` in the input.
pub fn parse(default_region: Option<&str>, input: &str) -> Result<PhoneNumber, ParseError> {
    let default_region = default_region.unwrap_or("");
    if input.is_empty() {
        return Err(ParseError::NotANumber);
    }
    if input.len() > MAX_INPUT_STRING_LENGTH {
        return Err(ParseError::TooLong);
    }

    let mut national_number = build_national_number_for_parsing(input)?;
    if !is_viable_phone_number(&national_number) {
        return Err(ParseError::NotANumber);
    }
    // Go's `getMetadataForRegion` is nil exactly when `isValidRegionCode` is false.
    let mut region_meta = metadata::for_region(default_region);
    if !check_region_for_parsing(&national_number, region_meta.is_some()) {
        return Err(ParseError::InvalidCountryCode);
    }

    let extension = maybe_strip_extension(&mut national_number);

    let extracted = match maybe_extract_country_code(&national_number, region_meta) {
        Ok(extracted) => extracted,
        Err(ParseError::InvalidCountryCode) => {
            // There might be a plus further in: strip up to it and try again.
            let Some(plus) = PLUS_CHARS_PATTERN.find(&national_number) else {
                return Err(ParseError::InvalidCountryCode);
            };
            match maybe_extract_country_code(&national_number[plus.end()..], region_meta)? {
                None => return Err(ParseError::InvalidCountryCode),
                found => found,
            }
        }
        Err(e) => return Err(e),
    };

    let (country_code, mut normalized_national, source) = match extracted {
        Some((code, national, source)) => {
            let (region, meta) = metadata_for_country_code(code);
            if region != default_region {
                region_meta = meta;
            }
            (code, national, source)
        }
        None => {
            let national = normalize(&national_number);
            if default_region.is_empty() {
                (0, national, CountryCodeSource::Unspecified)
            } else {
                // A nil metadata's GetCountryCode() is 0 in Go.
                let code = region_meta.map_or(0, |m| m.country_code);
                (code, national, CountryCodeSource::FromDefaultCountry)
            }
        }
    };
    if normalized_national.len() < MIN_LENGTH_FOR_NSN {
        return Err(ParseError::TooShortNsn);
    }

    let mut carrier = None;
    if let Some(meta) = region_meta {
        if let Some((potential, carrier_code)) =
            maybe_strip_national_prefix_and_carrier_code(&normalized_national, meta)
        {
            // Keep the stripped number only when its length is possible for the region;
            // otherwise the original could be a valid short number.
            let validation = test_number_length_general(&potential, meta);
            if !matches!(
                validation,
                PossibleResult::TooShort
                    | PossibleResult::IsPossibleLocalOnly
                    | PossibleResult::InvalidLength
            ) {
                normalized_national = potential;
                carrier = carrier_code;
            }
        }
    }
    let length = normalized_national.len();
    if length < MIN_LENGTH_FOR_NSN {
        return Err(ParseError::TooShortNsn);
    }
    if length > MAX_LENGTH_FOR_NSN {
        return Err(ParseError::TooLong);
    }
    let (italian_leading_zero, number_of_leading_zeros) =
        italian_leading_zeros(&normalized_national);

    Ok(PhoneNumber {
        country_code,
        national_number: go_parse_uint_or_zero(&normalized_national),
        number_of_leading_zeros,
        italian_leading_zero,
        extension: (!extension.is_empty()).then_some(extension),
        raw_input: Some(input.to_string()),
        country_code_source: source,
        preferred_domestic_carrier_code: carrier,
    })
}

#[cfg(test)]
#[path = "parser_tests.rs"]
mod tests;
