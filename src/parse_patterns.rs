//! The parse-side regexes of Go's `github.com/nyaruka/phonenumbers` v1.8.1
//! (phonenumberutil.go:51-444), built from the same string pieces in the same order.
//!
//! Translations from Go's RE2 to the `regex` crate:
//! - `\p{Nd}` (Go's `DIGITS`) is written as explicit ranges from [`crate::digits::ND_RANGES`], so
//!   the regexes and `is_decimal_digit` share one table.
//! - `UNWANTED_END_CHARS` is `[[\P{N}&&\P{L}]&&[^#]]+$`. RE2 has no class intersection: it reads
//!   `[[\P{N}&&\P{L}]` as one class of `[`, `&`, `\P{N}`, `\P{L}` (every character, since nothing
//!   is both N and L), then the literals `&&`, the class `[^#]`, and a literal `]` repeated. The
//!   `regex` crate would read `&&` as intersection, so the class is written out as Go reads it.
//! - Everything else (classes holding `-`, `~`, `.`, `(`, `)`, `\[`, `\]`, `(?i)`, `{n,m}`) means
//!   the same in both engines and is copied as is.

use once_cell::sync::Lazy;
use regex::Regex;

use crate::digits::regex_class_items;

/// Go's `VALID_PUNCTUATION` (phonenumberutil.go:58).
const VALID_PUNCTUATION: &str = "-x\u{2010}-\u{2015}\u{2212}\u{30FC}\u{FF0D}-\u{FF0F} \
\u{00A0}\u{00AD}\u{200B}\u{2060}\u{3000}()\u{FF08}\u{FF09}\u{FF3B}\u{FF3D}.\
\\[\\]/~\u{2053}\u{223C}\u{FF5E}";

/// Go's `VALID_ALPHA` (phonenumberutil.go:66).
const VALID_ALPHA: &str = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ";

/// Go's `PLUS_CHARS` (phonenumberutil.go:67).
const PLUS_CHARS: &str = "+\u{FF0B}";

/// Go's `STAR_SIGN` (phonenumberutil.go:44).
const STAR_SIGN: &str = "*";

/// Go's `MIN_LENGTH_FOR_NSN` (phonenumberutil.go:25).
const MIN_LENGTH_FOR_NSN: usize = 2;

/// Go's `DIGITS` (`\p{Nd}`) as a whole class.
fn digits() -> String {
    format!("[{}]", regex_class_items())
}

fn compile(pattern: &str) -> Regex {
    Regex::new(pattern).expect("a Go phonenumbers pattern that the regex crate accepts")
}

/// Go's `EXTN_PATTERNS_FOR_PARSING` (phonenumberutil.go:380-398).
fn extn_patterns_for_parsing() -> String {
    let d = digits();
    let possible_separators_between_number_and_ext_label = "[ \u{00A0}\\t,]*";
    let possible_chars_after_ext_label = "[:\\.\u{FF0E}]?[ \u{00A0}\\t,-]*";
    let optional_extn_suffix = "#?";
    let explicit_ext_labels = "(?:e?xt(?:ensi(?:o\u{0301}?|\u{00F3}))?n?|\
\u{FF45}?\u{FF58}\u{FF54}\u{FF4E}?|\u{0434}\u{043E}\u{0431}|anexo)";
    let ambiguous_ext_labels = "(?:[x\u{FF58}#\u{FF03}~\u{FF5E}]|int|\u{FF49}\u{FF4E}\u{FF54})";
    let ambiguous_separator = "[- ]+";
    let for_matching = format!(
        ";ext=({d}{{1,20}})|\
{possible_separators_between_number_and_ext_label}{explicit_ext_labels}\
{possible_chars_after_ext_label}({d}{{1,20}}){optional_extn_suffix}|\
{possible_separators_between_number_and_ext_label}{ambiguous_ext_labels}\
{possible_chars_after_ext_label}({d}{{1,9}}){optional_extn_suffix}|\
{ambiguous_separator}({d}{{1,6}})#"
    );
    let possible_separators_number_ext_label_no_comma = "[ \u{00A0}\\t]*";
    let auto_dialling_and_ext_labels_found = "(?:,{2}|;)";
    format!(
        "{for_matching}|\
{possible_separators_number_ext_label_no_comma}{auto_dialling_and_ext_labels_found}\
{possible_chars_after_ext_label}({d}{{1,15}}){optional_extn_suffix}|\
{possible_separators_number_ext_label_no_comma}(?:,)+\
{possible_chars_after_ext_label}({d}{{1,9}}){optional_extn_suffix}"
    )
}

/// Go's `PLUS_CHARS_PATTERN` (phonenumberutil.go:268).
pub(crate) static PLUS_CHARS_PATTERN: Lazy<Regex> =
    Lazy::new(|| compile(&format!("[{PLUS_CHARS}]+")));

/// Go's `VALID_START_CHAR_PATTERN` (phonenumberutil.go:281).
pub(crate) static VALID_START_CHAR_PATTERN: Lazy<Regex> =
    Lazy::new(|| compile(&format!("[{PLUS_CHARS}{}]", regex_class_items())));

/// Go's `SECOND_NUMBER_START_PATTERN` (phonenumberutil.go:291).
pub(crate) static SECOND_NUMBER_START_PATTERN: Lazy<Regex> =
    Lazy::new(|| compile(r"[\\/] *x"));

/// Go's `UNWANTED_END_CHAR_PATTERN` (phonenumberutil.go:298), as RE2 reads it (see the module
/// comment).
pub(crate) static UNWANTED_END_CHAR_PATTERN: Lazy<Regex> =
    Lazy::new(|| compile(r"[\[&\P{N}\P{L}]&&[^#]\]+$"));

/// Go's `VALID_ALPHA_PHONE_PATTERN` (phonenumberutil.go:304).
pub(crate) static VALID_ALPHA_PHONE_PATTERN: Lazy<Regex> =
    Lazy::new(|| compile("^(?:.*?[A-Za-z]){3}.*$"));

/// Go's `EXTN_PATTERN` (phonenumberutil.go:402).
pub(crate) static EXTN_PATTERN: Lazy<Regex> =
    Lazy::new(|| compile(&format!("(?i)(?:{})$", extn_patterns_for_parsing())));

/// Go's `VALID_PHONE_NUMBER_PATTERN` (phonenumberutil.go:326 and 407).
pub(crate) static VALID_PHONE_NUMBER_PATTERN: Lazy<Regex> = Lazy::new(|| {
    let d = digits();
    let items = regex_class_items();
    let valid_phone_number = format!(
        "{d}{{{MIN_LENGTH_FOR_NSN}}}|\
[{PLUS_CHARS}]*(?:[{VALID_PUNCTUATION}{STAR_SIGN}]*{d}){{3,}}\
[{VALID_PUNCTUATION}{STAR_SIGN}{VALID_ALPHA}{items}]*"
    );
    compile(&format!(
        "(?i)^({valid_phone_number}(?:{})?)$",
        extn_patterns_for_parsing()
    ))
});

/// Go's `RFC3966_GLOBAL_NUMBER_DIGITS_PATTERN` (phonenumberutil.go:433-436).
pub(crate) static RFC3966_GLOBAL_NUMBER_DIGITS_PATTERN: Lazy<Regex> = Lazy::new(|| {
    let d = digits();
    let visual_separator = r"[\-\.\(\)]?";
    let phone_digit = format!("({d}|{visual_separator})");
    compile(&format!("^\\+{phone_digit}*{d}{phone_digit}*$"))
});

/// Go's `RFC3966_DOMAINNAME_PATTERN` (phonenumberutil.go:440-444).
pub(crate) static RFC3966_DOMAINNAME_PATTERN: Lazy<Regex> = Lazy::new(|| {
    let alphanum = format!("{VALID_ALPHA}{}", regex_class_items());
    let domainlabel = format!("[{alphanum}]+((\\-)*[{alphanum}])*");
    let toplabel = format!("[{VALID_ALPHA}]+((\\-)*[{alphanum}])*");
    compile(&format!("^({domainlabel}\\.)*{toplabel}\\.?$"))
});

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_pattern_compiles() {
        for re in [
            &PLUS_CHARS_PATTERN,
            &VALID_START_CHAR_PATTERN,
            &SECOND_NUMBER_START_PATTERN,
            &UNWANTED_END_CHAR_PATTERN,
            &VALID_ALPHA_PHONE_PATTERN,
            &EXTN_PATTERN,
            &VALID_PHONE_NUMBER_PATTERN,
            &RFC3966_GLOBAL_NUMBER_DIGITS_PATTERN,
            &RFC3966_DOMAINNAME_PATTERN,
        ] {
            let _ = re.as_str();
        }
    }

    #[test]
    fn unwanted_end_chars_is_read_as_go_reads_it() {
        // RE2 has no class intersection: trailing punctuation alone is not removed ...
        assert!(!UNWANTED_END_CHAR_PATTERN.is_match("+1 650 253 0000 ."));
        assert!(!UNWANTED_END_CHAR_PATTERN.is_match("+1 650 253 0000#"));
        // ... only the literal shape "<any>&&<not #>]..." at the end is.
        assert!(UNWANTED_END_CHAR_PATTERN.is_match("+1650x&&y]]"));
        assert!(!UNWANTED_END_CHAR_PATTERN.is_match("+1650x&&#]"));
    }

    #[test]
    fn viable_number_shapes() {
        let ok = |s: &str| VALID_PHONE_NUMBER_PATTERN.is_match(s);
        assert!(ok("12"));
        assert!(ok("+1 650 253 0000"));
        assert!(ok("+1 650 253 0000 ext. 12"));
        assert!(ok("+1 650 253 0000,12"));
        assert!(ok("+1 800 FLOWERS"));
        // Narrow no-break space and figure space are not Go punctuation.
        assert!(!ok("+1 650\u{202F}253 0000"));
        assert!(!ok("+1 650\u{2007}253 0000"));
        // A "+" or "#" inside the number is not punctuation either.
        assert!(!ok("+1 650+253 0000"));
        assert!(!ok("+1 650 253 0000 #"));
        assert!(!ok("+1 650 253 0000\u{FEFF}"));
    }

    #[test]
    fn extension_labels() {
        let ext = |s: &str| {
            EXTN_PATTERN
                .captures(s)
                .and_then(|c| c.iter().skip(1).flatten().map(|m| m.as_str().to_owned()).find(|m| !m.is_empty()))
        };
        assert_eq!(ext("+16502530000;ext=123").as_deref(), Some("123"));
        assert_eq!(ext("+16502530000 EXT 123").as_deref(), Some("123"));
        assert_eq!(ext("+16502530000 доб 123").as_deref(), Some("123"));
        assert_eq!(ext("+16502530000,,123").as_deref(), Some("123"));
        assert_eq!(ext("+16502530000,12").as_deref(), Some("12"));
        assert_eq!(ext("+16502530000 - 503#").as_deref(), Some("503"));
        assert_eq!(ext("+16502530000 ext"), None);
    }

    #[test]
    fn rfc3966_phone_context() {
        assert!(RFC3966_GLOBAL_NUMBER_DIGITS_PATTERN.is_match("+1-650"));
        assert!(!RFC3966_GLOBAL_NUMBER_DIGITS_PATTERN.is_match("+-"));
        assert!(RFC3966_DOMAINNAME_PATTERN.is_match("example.com"));
        assert!(!RFC3966_DOMAINNAME_PATTERN.is_match("example.1"));
    }
}
