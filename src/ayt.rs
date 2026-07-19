//! As-you-type formatting, mirroring libphonenumber's `AsYouTypeFormatter`.
//!
//! Feed digits one at a time with [`AsYouTypeFormatter::input_digit`]; each call
//! returns the best formatting of everything typed so far. The approach follows
//! upstream: build a formatting *template* from the chosen `numberFormat` (each
//! matched digit becomes a placeholder), then fill placeholders left-to-right as
//! the user types, truncating at the first unfilled slot for partial input.

use crate::format::replace_first_group;
use crate::metadata;
use crate::model::{NumberFormat, PhoneMetadata};
use regex::Regex;

/// Placeholder standing in for a not-yet-typed digit inside a template.
const PLACEHOLDER: char = '\u{2008}';
/// A string of digits long enough to build any template.
const DUMMY: &str = "999999999999999";

/// Live phone-number formatter for a single default region.
pub struct AsYouTypeFormatter {
    meta: Option<&'static PhoneMetadata>,
    /// Every meaningful character typed so far (`+` and digits).
    raw: String,
}

impl AsYouTypeFormatter {
    /// Create a formatter for a default region (CLDR id like `"US"`).
    pub fn new(region: &str) -> Self {
        AsYouTypeFormatter {
            meta: metadata::for_region(region),
            raw: String::new(),
        }
    }

    /// Discard all typed input.
    pub fn clear(&mut self) {
        self.raw.clear();
    }

    /// Feed one character (digit or `+`); returns the formatting so far.
    pub fn input_digit(&mut self, c: char) -> String {
        if c == '+' && self.raw.is_empty() {
            self.raw.push('+');
        } else if c.is_ascii_digit() {
            self.raw.push(c);
        }
        // Non-meaningful characters are ignored.
        self.current()
    }

    /// The current formatted string without adding input.
    pub fn current(&self) -> String {
        self.reformat().unwrap_or_else(|| self.raw.clone())
    }

    fn reformat(&self) -> Option<String> {
        if let Some(rest) = self.raw.strip_prefix('+') {
            self.reformat_international(rest)
        } else {
            self.reformat_national(&self.raw)
        }
    }

    /// International input: `+<cc><national>`.
    fn reformat_international(&self, after_plus: &str) -> Option<String> {
        if after_plus.is_empty() {
            return Some("+".to_string());
        }
        // Extract the country code greedily (1-3 digits).
        let mut cc = 0u16;
        let mut cc_len = 0usize;
        for i in 1..=after_plus.len().min(3) {
            if let Ok(code) = after_plus[..i].parse::<u16>() {
                if metadata::is_valid_calling_code(code) {
                    cc = code;
                    cc_len = i;
                    break;
                }
            }
        }
        if cc_len == 0 {
            // Country code not yet resolvable.
            return Some(format!("+{after_plus}"));
        }
        let national = &after_plus[cc_len..];
        let prefix = format!("+{cc} ");
        if national.is_empty() {
            return Some(format!("+{cc}"));
        }
        // Format the national part using the code's main region, intl formats.
        let region = metadata::main_region_for_country_code(cc);
        let meta = match region {
            Some(r) if r != "001" => metadata::for_region(r),
            _ => metadata::for_non_geo_code(cc),
        };
        match meta {
            Some(m) => {
                let formatted = format_significant(national, m, /* intl */ true);
                Some(format!("{prefix}{}", formatted))
            }
            None => Some(format!("{prefix}{national}")),
        }
    }

    /// National input for the formatter's default region.
    fn reformat_national(&self, typed: &str) -> Option<String> {
        let meta = self.meta?;
        if typed.is_empty() {
            return Some(String::new());
        }
        Some(format_significant_national(typed, meta))
    }
}

/// Format `national` digits (already stripped of any country code) using the
/// intl or national templates, without a national prefix.
fn format_significant(national: &str, meta: &PhoneMetadata, intl: bool) -> String {
    let formats = if intl && !meta.intl_number_formats.is_empty() {
        &meta.intl_number_formats
    } else {
        &meta.number_formats
    };
    match choose_and_fill(national, formats, None) {
        Some(s) => s,
        None => national.to_string(),
    }
}

/// Format nationally-typed digits, handling the national prefix the user typed.
fn format_significant_national(typed: &str, meta: &PhoneMetadata) -> String {
    // Strip a simple national prefix to obtain the significant number, so the
    // template's own prefix rule can re-apply it (e.g. FR "0" + "1 42 ...").
    let np = &meta.national_prefix;
    let significant = if !np.is_empty() && typed.starts_with(np.as_str()) {
        &typed[np.len()..]
    } else {
        typed
    };
    if significant.is_empty() {
        return typed.to_string();
    }
    match choose_and_fill(significant, &meta.number_formats, Some(())) {
        Some(s) => s,
        None => typed.to_string(),
    }
}

/// Leading-digits gate: the most specific (last) pattern must prefix-match `sig`.
fn leading_ok(f: &NumberFormat, sig: &str) -> bool {
    match f.leading_digits.last() {
        None => true,
        Some(ld) => metadata::prefix_regex(ld)
            .map(|re| re.is_match(sig))
            .unwrap_or(false),
    }
}

/// The replacement template for a format, with the national-prefix rule spliced
/// in when `with_np` is set.
fn replacement_for(f: &NumberFormat, with_np: Option<()>) -> String {
    if with_np.is_some() && !f.national_prefix_formatting_rule.is_empty() {
        replace_first_group(&f.format, &f.national_prefix_formatting_rule)
    } else {
        f.format.clone()
    }
}

/// Format `sig` for display so far.
///
/// Two passes, mirroring how a complete number should settle to the canonical
/// grouping while partial input still shows a live template:
///  1. **Complete:** if a format's anchored pattern fully matches `sig`, apply it
///     by direct regex replacement — identical to the offline formatter's output.
///  2. **Partial:** otherwise fill the first template long enough to hold `sig`.
fn choose_and_fill(sig: &str, formats: &[NumberFormat], with_np: Option<()>) -> Option<String> {
    // Pass 1: exact/complete match.
    for f in formats {
        if !leading_ok(f, sig) {
            continue;
        }
        if let Some(re) = metadata::anchored_regex(&f.pattern) {
            if re.is_match(sig) {
                let replacement = replacement_for(f, with_np);
                return Some(re.replace(sig, replacement.as_str()).into_owned());
            }
        }
    }
    // Pass 2: partial template fill.
    for f in formats {
        if !leading_ok(f, sig) {
            continue;
        }
        let replacement = replacement_for(f, with_np);
        let Some(template) = build_template(&f.pattern, &replacement) else {
            continue;
        };
        let slots = template.chars().filter(|&c| c == PLACEHOLDER).count();
        if slots < sig.len() {
            continue;
        }
        return Some(fill(&template, sig));
    }
    None
}

/// Build a template such as `(   )    -    `
/// from a pattern and a replacement rule.
fn build_template(pattern: &str, replacement: &str) -> Option<String> {
    // Match the (unanchored) pattern against a long run of 9s.
    let re = Regex::new(&format!("^(?:{pattern})")).ok()?;
    let m = re.find(DUMMY)?;
    let matched = m.as_str();
    // Apply the replacement to get e.g. "(999) 999-9999", then swap 9 -> slot.
    let filled = re.replace(matched, replacement).into_owned();
    Some(filled.replace('9', &PLACEHOLDER.to_string()))
}

/// Fill placeholders in `template` with `digits`, stopping at the first slot
/// that has no digit, and trimming a trailing separator.
fn fill(template: &str, digits: &str) -> String {
    let mut out = String::new();
    let mut it = digits.chars();
    for ch in template.chars() {
        if ch == PLACEHOLDER {
            match it.next() {
                Some(d) => out.push(d),
                None => break,
            }
        } else {
            out.push(ch);
        }
    }
    out.trim_end().to_string()
}

/// Convenience: format an entire string as if typed character by character.
pub fn format_as_you_type(region: &str, input: &str) -> String {
    let mut f = AsYouTypeFormatter::new(region);
    let mut last = String::new();
    for c in input.chars() {
        last = f.input_digit(c);
    }
    last
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn us_progression() {
        let mut f = AsYouTypeFormatter::new("US");
        assert_eq!(f.input_digit('6'), "6");
        f.input_digit('5');
        f.input_digit('0');
        let mut out = String::new();
        for d in "2530000".chars() {
            out = f.input_digit(d);
        }
        assert_eq!(out, "(650) 253-0000");
        // current() without new input.
        assert_eq!(f.current(), "(650) 253-0000");
        // clear() resets.
        f.clear();
        assert_eq!(f.current(), "");
    }

    #[test]
    fn plus_and_intl() {
        let mut f = AsYouTypeFormatter::new("US");
        assert_eq!(f.input_digit('+'), "+");
        // Non-digit, non-plus is ignored.
        assert_eq!(f.input_digit('-'), "+");
        let out = format_as_you_type("US", "+33142685300");
        assert_eq!(out, "+33 1 42 68 53 00");
    }

    #[test]
    fn national_with_prefix() {
        assert_eq!(format_as_you_type("FR", "0142685300"), "01 42 68 53 00");
    }

    #[test]
    fn unknown_region_returns_raw() {
        // No metadata for region -> echoes typed digits.
        let out = format_as_you_type("ZZ", "12345");
        assert_eq!(out, "12345");
    }

    #[test]
    fn intl_incomplete_country_code() {
        // '+' then a digit that isn't yet a full valid code.
        let mut f = AsYouTypeFormatter::new("US");
        f.input_digit('+');
        let out = f.input_digit('9'); // 9 alone is not a valid calling code
        assert!(out.starts_with('+'));
    }
}
