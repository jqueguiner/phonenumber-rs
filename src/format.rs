//! Formatting a [`PhoneNumber`] into the standard output forms.

use crate::metadata;
use crate::model::{NumberFormat, PhoneMetadata};
use crate::number::PhoneNumber;
use crate::validator::REGION_CODE_FOR_NON_GEO;

/// Output formats. Mirrors libphonenumber's `PhoneNumberFormat`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PhoneNumberFormat {
    /// `+16502530000`
    E164,
    /// `+1 650-253-0000`
    International,
    /// `(650) 253-0000`
    National,
    /// `tel:+1-650-253-0000`
    Rfc3966,
}

/// Format a parsed number into the requested form.
pub fn format(number: &PhoneNumber, fmt: PhoneNumberFormat) -> String {
    let nsn = number.national_significant_number();
    let code = number.country_code;

    if fmt == PhoneNumberFormat::E164 {
        let mut out = String::new();
        prefix_country_code(&mut out, code, fmt, &nsn);
        append_extension(&mut out, number, fmt);
        return out;
    }

    // Resolve governing metadata (region or non-geo).
    let meta = metadata_for_number(number);
    let Some(meta) = meta else {
        // Unknown code: fall back to E164-style.
        let mut out = String::new();
        prefix_country_code(&mut out, code, fmt, &nsn);
        append_extension(&mut out, number, fmt);
        return out;
    };

    let formatted_nsn = format_nsn(&nsn, meta, fmt);
    let mut out = String::new();
    prefix_country_code(&mut out, code, fmt, &formatted_nsn);
    append_extension(&mut out, number, fmt);
    out
}

/// Convenience: national format for a number's own region.
pub fn format_national(number: &PhoneNumber) -> String {
    format(number, PhoneNumberFormat::National)
}

/// Format a number as it should be dialled from `region_calling_from`,
/// choosing the caller's IDD prefix. Mirrors libphonenumber's
/// `formatOutOfCountryCallingNumber`.
pub fn format_out_of_country_calling_number(
    number: &PhoneNumber,
    region_calling_from: &str,
) -> String {
    // Unknown caller region: fall back to plain international format.
    let Some(from_meta) = metadata::for_region(region_calling_from) else {
        return format(number, PhoneNumberFormat::International);
    };
    let code = number.country_code;
    let nsn = number.national_significant_number();

    if !metadata::is_valid_calling_code(code) {
        return nsn;
    }

    // North American Numbering Plan: dialling within the NANP uses "1 " + national.
    if code == 1 && from_meta.country_code == 1 {
        return format!("1 {}", format(number, PhoneNumberFormat::National));
    }
    // Same country: a domestic call, use the national format.
    if code == from_meta.country_code {
        return format(number, PhoneNumberFormat::National);
    }

    // Pick the IDD: a single simple prefix is used verbatim, otherwise the
    // region's preferred international prefix.
    let idd = if is_simple_prefix(&from_meta.international_prefix) {
        from_meta.international_prefix.clone()
    } else {
        from_meta.preferred_international_prefix.clone()
    };

    let formatted_nsn = match metadata_for_code(code) {
        Some(m) => format_nsn(&nsn, m, PhoneNumberFormat::International),
        None => nsn.clone(),
    };

    let mut out = if !idd.is_empty() {
        format!("{idd} {code} {formatted_nsn}")
    } else {
        let mut s = String::new();
        prefix_country_code(&mut s, code, PhoneNumberFormat::International, &formatted_nsn);
        s
    };
    append_extension(&mut out, number, PhoneNumberFormat::International);
    out
}

/// Format nationally using the carrier code captured during parsing (if any),
/// mirroring libphonenumber's `formatNationalNumberWithPreferredCarrierCode`.
/// A non-empty `fallback_carrier_code` is used when the number carries none.
pub fn format_national_number_with_preferred_carrier_code(
    number: &PhoneNumber,
    fallback_carrier_code: &str,
) -> String {
    let cc = number
        .preferred_domestic_carrier_code
        .as_deref()
        .filter(|s| !s.is_empty())
        .unwrap_or(fallback_carrier_code);
    format_national_number_with_carrier_code(number, cc)
}

/// Format nationally, inserting a domestic `carrier_code` (carrier selection
/// code). Mirrors libphonenumber's `formatNationalNumberWithCarrierCode`.
pub fn format_national_number_with_carrier_code(
    number: &PhoneNumber,
    carrier_code: &str,
) -> String {
    let code = number.country_code;
    let nsn = number.national_significant_number();
    if !metadata::is_valid_calling_code(code) {
        return nsn;
    }
    let Some(meta) = metadata_for_code(code) else {
        return nsn;
    };
    let mut out = format_nsn_carrier(&nsn, meta, carrier_code);
    append_extension(&mut out, number, PhoneNumberFormat::National);
    out
}

/// True when an international prefix is a single literal (digits only), so it can
/// be used directly for formatting rather than needing a preferred prefix.
fn is_simple_prefix(prefix: &str) -> bool {
    !prefix.is_empty() && prefix.chars().all(|c| c.is_ascii_digit())
}

/// Metadata governing formatting for a calling code (main region or non-geo).
fn metadata_for_code(code: u16) -> Option<&'static PhoneMetadata> {
    match metadata::main_region_for_country_code(code) {
        Some(r) if r != REGION_CODE_FOR_NON_GEO => metadata::for_region(r),
        _ => metadata::for_non_geo_code(code),
    }
}

/// National formatting with a carrier code spliced into the chosen rule.
fn format_nsn_carrier(nsn: &str, meta: &PhoneMetadata, carrier_code: &str) -> String {
    let Some(rule) = choose_format(&meta.number_formats, nsn) else {
        return nsn.to_string();
    };
    let Some(pattern) = metadata::anchored_regex(&rule.pattern) else {
        return nsn.to_string();
    };
    let template = if !carrier_code.is_empty()
        && !rule.domestic_carrier_code_formatting_rule.is_empty()
    {
        // $CC -> carrier code, then splice into the format's first group.
        let cc_rule = rule
            .domestic_carrier_code_formatting_rule
            .replace("$CC", carrier_code);
        replace_first_group(&rule.format, &cc_rule)
    } else if !rule.national_prefix_formatting_rule.is_empty() {
        replace_first_group(&rule.format, &rule.national_prefix_formatting_rule)
    } else {
        rule.format.clone()
    };
    pattern.replace(nsn, template.as_str()).into_owned()
}

/// libphonenumber formats using the MAIN region for the calling code
/// (`getRegionCodeForCountryCode`), not the number's specific region — so NANP
/// territories (AG, AI, BB, ...) that carry no formats of their own reuse the
/// main country's (US) formatting rules.
fn metadata_for_number(number: &PhoneNumber) -> Option<&'static PhoneMetadata> {
    metadata_for_code(number.country_code)
}

/// Prefix the (already formatted) national number with its calling code
/// according to the output format. Mirrors `prefixNumberWithCountryCallingCode`.
fn prefix_country_code(out: &mut String, code: u16, fmt: PhoneNumberFormat, formatted: &str) {
    match fmt {
        PhoneNumberFormat::E164 => {
            out.push('+');
            out.push_str(&code.to_string());
            out.push_str(formatted);
        }
        PhoneNumberFormat::International => {
            out.push('+');
            out.push_str(&code.to_string());
            out.push(' ');
            out.push_str(formatted);
        }
        PhoneNumberFormat::Rfc3966 => {
            out.push_str("tel:+");
            out.push_str(&code.to_string());
            out.push('-');
            out.push_str(formatted);
        }
        PhoneNumberFormat::National => {
            out.push_str(formatted);
        }
    }
}

fn append_extension(out: &mut String, number: &PhoneNumber, fmt: PhoneNumberFormat) {
    let Some(ext) = &number.extension else {
        return;
    };
    if ext.is_empty() {
        return;
    }
    match fmt {
        PhoneNumberFormat::Rfc3966 => {
            out.push_str(";ext=");
            out.push_str(ext);
        }
        _ => {
            // libphonenumber uses the preferred extn prefix or " ext. ".
            out.push_str(" ext. ");
            out.push_str(ext);
        }
    }
}

/// Format the national significant number using the best-matching rule.
fn format_nsn(nsn: &str, meta: &PhoneMetadata, fmt: PhoneNumberFormat) -> String {
    // INTERNATIONAL/RFC3966 prefer the intl format list when present.
    let intl = &meta.intl_number_formats;
    let use_national = fmt == PhoneNumberFormat::National || intl.is_empty();
    let formats: &[NumberFormat] = if use_national {
        &meta.number_formats
    } else {
        intl
    };

    let Some(rule) = choose_format(formats, nsn) else {
        return nsn.to_string();
    };
    format_nsn_using_pattern(nsn, rule, fmt)
}

/// Pick the formatting rule whose leading digits and pattern match `nsn`.
fn choose_format<'a>(formats: &'a [NumberFormat], nsn: &str) -> Option<&'a NumberFormat> {
    for f in formats {
        let leading_ok = match f.leading_digits.last() {
            None => true,
            Some(ld) => metadata::prefix_regex(ld)
                .map(|re| re.is_match(nsn))
                .unwrap_or(false),
        };
        if !leading_ok {
            continue;
        }
        if let Some(re) = metadata::anchored_regex(&f.pattern) {
            if re.is_match(nsn) {
                return Some(f);
            }
        }
    }
    None
}

/// Apply a chosen rule to `nsn`. Mirrors `formatNsnUsingPattern`.
fn format_nsn_using_pattern(nsn: &str, rule: &NumberFormat, fmt: PhoneNumberFormat) -> String {
    let Some(pattern) = metadata::anchored_regex(&rule.pattern) else {
        return nsn.to_string();
    };

    // For NATIONAL formatting, splice the national-prefix rule into the first
    // group of the template (already $NP/$FG-expanded at build time).
    let template = if fmt == PhoneNumberFormat::National
        && !rule.national_prefix_formatting_rule.is_empty()
    {
        replace_first_group(&rule.format, &rule.national_prefix_formatting_rule)
    } else {
        rule.format.clone()
    };

    let mut formatted = pattern.replace(nsn, template.as_str()).into_owned();

    if fmt == PhoneNumberFormat::Rfc3966 {
        // Collapse whitespace to '-' and trim stray separators.
        formatted = formatted.trim().replace(' ', "-");
    }
    formatted
}

/// Replace the first `$1` group reference in `format` with `np_rule`
/// (which itself contains the literal national prefix plus `$1`).
pub(crate) fn replace_first_group(format: &str, np_rule: &str) -> String {
    match format.find("$1") {
        Some(idx) => {
            let mut s = String::with_capacity(format.len() + np_rule.len());
            s.push_str(&format[..idx]);
            s.push_str(np_rule);
            s.push_str(&format[idx + 2..]);
            s
        }
        None => format.to_string(),
    }
}

#[cfg(test)]
#[allow(clippy::field_reassign_with_default)]
mod tests {
    use super::*;
    use crate::parser::parse;

    #[test]
    fn four_formats_us() {
        let n = parse(Some("US"), "(650) 253-0000").unwrap();
        assert_eq!(format(&n, PhoneNumberFormat::E164), "+16502530000");
        assert_eq!(format(&n, PhoneNumberFormat::International), "+1 650-253-0000");
        assert_eq!(format(&n, PhoneNumberFormat::National), "(650) 253-0000");
        assert_eq!(format(&n, PhoneNumberFormat::Rfc3966), "tel:+1-650-253-0000");
        assert_eq!(format_national(&n), "(650) 253-0000");
    }

    #[test]
    fn extension_rendering() {
        let mut n = parse(None, "+16502530000").unwrap();
        n.extension = Some("42".to_string());
        assert!(format(&n, PhoneNumberFormat::National).ends_with(" ext. 42"));
        assert!(format(&n, PhoneNumberFormat::Rfc3966).ends_with(";ext=42"));
        assert!(format(&n, PhoneNumberFormat::E164).ends_with(" ext. 42"));
        // Empty extension is ignored.
        n.extension = Some(String::new());
        assert_eq!(format(&n, PhoneNumberFormat::E164), "+16502530000");
    }

    #[test]
    fn shared_code_uses_main_country_formats() {
        // Antigua (+1) has no formats of its own -> uses US grouping.
        let n = parse(None, "+12684601234").unwrap();
        assert_eq!(format(&n, PhoneNumberFormat::International), "+1 268-460-1234");
    }

    #[test]
    fn unknown_code_falls_back_to_e164_like() {
        let mut n = PhoneNumber::default();
        n.country_code = 999;
        n.national_number = 123456;
        // No metadata: international format still prefixes the code.
        assert!(format(&n, PhoneNumberFormat::International).starts_with("+999 "));
    }

    #[test]
    fn replace_first_group_helper() {
        assert_eq!(replace_first_group("$1 $2", "0$1"), "0$1 $2");
        assert_eq!(replace_first_group("no groups", "0$1"), "no groups");
    }
}

#[cfg(test)]
mod secondary_api_tests {
    use super::*;
    use crate::parser::parse;

    #[test]
    fn out_of_country_basic() {
        let us = parse(None, "+16502530000").unwrap();
        // From FR: IDD 00 + 1 + intl-formatted national.
        assert_eq!(format_out_of_country_calling_number(&us, "FR"), "00 1 650-253-0000");
        // From GB (IDD 00) to FR number.
        let fr = parse(None, "+33142685300").unwrap();
        assert_eq!(format_out_of_country_calling_number(&fr, "GB"), "00 33 1 42 68 53 00");
    }

    #[test]
    fn out_of_country_nanp_and_same_country() {
        let us = parse(None, "+16502530000").unwrap();
        // Within NANP (US->US): "1 " + national.
        assert_eq!(format_out_of_country_calling_number(&us, "US"), "1 (650) 253-0000");
        // Same country non-NANP (FR->FR): national format.
        let fr = parse(None, "+33142685300").unwrap();
        assert_eq!(format_out_of_country_calling_number(&fr, "FR"), "01 42 68 53 00");
        // Unknown caller region -> plain international.
        assert_eq!(
            format_out_of_country_calling_number(&us, "ZZ"),
            format(&us, PhoneNumberFormat::International)
        );
    }

    #[test]
    fn out_of_country_preferred_idd_and_invalid_code() {
        let us = parse(None, "+16502530000").unwrap();
        // AU has a regex international prefix, so its preferred IDD (0011) is used.
        assert_eq!(
            format_out_of_country_calling_number(&us, "AU"),
            "0011 1 650-253-0000"
        );
        // Invalid calling code returns the bare national significant number.
        let bogus = PhoneNumber { country_code: 9999, national_number: 12345, ..Default::default() };
        assert_eq!(format_out_of_country_calling_number(&bogus, "FR"), "12345");
        assert_eq!(format_national_number_with_carrier_code(&bogus, "12"), "12345");
    }

    #[test]
    fn out_of_country_preserves_extension() {
        let mut us = parse(None, "+16502530000").unwrap();
        us.extension = Some("99".to_string());
        let out = format_out_of_country_calling_number(&us, "FR");
        assert!(out.starts_with("00 1 650-253-0000"));
        assert!(out.ends_with(" ext. 99"));
    }

    #[test]
    fn carrier_code_formatting() {
        // Brazil uses carrier selection codes: $NP $CC ($FG).
        let br = parse(Some("BR"), "1132316555").unwrap();
        assert_eq!(
            format_national_number_with_carrier_code(&br, "12"),
            "0 12 (11) 3231-6555"
        );
        // A region without carrier rule falls back to national formatting.
        let us = parse(None, "+16502530000").unwrap();
        let out = format_national_number_with_carrier_code(&us, "99");
        assert_eq!(out, "(650) 253-0000");
    }
}

#[cfg(test)]
mod preferred_carrier_tests {
    use super::*;
    use crate::parser::parse;

    #[test]
    fn preferred_carrier_uses_parsed_then_fallback() {
        let mut br = parse(Some("BR"), "1132316555").unwrap();
        // No carrier captured: falls back to the supplied code.
        assert_eq!(
            format_national_number_with_preferred_carrier_code(&br, "15"),
            "0 15 (11) 3231-6555"
        );
        // Carrier captured on the number takes precedence over the fallback.
        br.preferred_domestic_carrier_code = Some("21".to_string());
        assert_eq!(
            format_national_number_with_preferred_carrier_code(&br, "15"),
            "0 21 (11) 3231-6555"
        );
    }
}
