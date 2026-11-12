//! Decimal digits as Go's `github.com/nyaruka/phonenumbers` v1.8.1 sees them.
//!
//! The one home of "is this a decimal digit" ([`is_decimal_digit`], Go's `unicode.IsDigit`), of
//! "which ASCII digit does the parser turn it into" ([`normalized_digit`], Go's
//! `arabicIndicNumberals`), and of the digit class the parse regexes use in place of Go's `\p{Nd}`
//! ([`regex_class_items`]). Keeping all three on [`ND_RANGES`] keeps the predicate and the regexes
//! on one table: pin that table and both follow.

/// General category Nd, Unicode 15.0.0 (Go's `unicode.Digit` table, the one behind
/// `unicode.IsDigit` and RE2's `\p{Nd}`), as inclusive code point ranges. Generated from Python
/// 3.12's `unicodedata` (unidata_version 15.0.0): 64 ranges, 680 code points.
pub(crate) const ND_RANGES: &[(u32, u32)] = &[
    (0x0030, 0x0039), (0x0660, 0x0669), (0x06F0, 0x06F9), (0x07C0, 0x07C9),
    (0x0966, 0x096F), (0x09E6, 0x09EF), (0x0A66, 0x0A6F), (0x0AE6, 0x0AEF),
    (0x0B66, 0x0B6F), (0x0BE6, 0x0BEF), (0x0C66, 0x0C6F), (0x0CE6, 0x0CEF),
    (0x0D66, 0x0D6F), (0x0DE6, 0x0DEF), (0x0E50, 0x0E59), (0x0ED0, 0x0ED9),
    (0x0F20, 0x0F29), (0x1040, 0x1049), (0x1090, 0x1099), (0x17E0, 0x17E9),
    (0x1810, 0x1819), (0x1946, 0x194F), (0x19D0, 0x19D9), (0x1A80, 0x1A89),
    (0x1A90, 0x1A99), (0x1B50, 0x1B59), (0x1BB0, 0x1BB9), (0x1C40, 0x1C49),
    (0x1C50, 0x1C59), (0xA620, 0xA629), (0xA8D0, 0xA8D9), (0xA900, 0xA909),
    (0xA9D0, 0xA9D9), (0xA9F0, 0xA9F9), (0xAA50, 0xAA59), (0xABF0, 0xABF9),
    (0xFF10, 0xFF19), (0x104A0, 0x104A9), (0x10D30, 0x10D39), (0x11066, 0x1106F),
    (0x110F0, 0x110F9), (0x11136, 0x1113F), (0x111D0, 0x111D9), (0x112F0, 0x112F9),
    (0x11450, 0x11459), (0x114D0, 0x114D9), (0x11650, 0x11659), (0x116C0, 0x116C9),
    (0x11730, 0x11739), (0x118E0, 0x118E9), (0x11950, 0x11959), (0x11C50, 0x11C59),
    (0x11D50, 0x11D59), (0x11DA0, 0x11DA9), (0x11F50, 0x11F59), (0x16A60, 0x16A69),
    (0x16AC0, 0x16AC9), (0x16B50, 0x16B59), (0x1D7CE, 0x1D7FF), (0x1E140, 0x1E149),
    (0x1E2F0, 0x1E2F9), (0x1E4F0, 0x1E4F9), (0x1E950, 0x1E959), (0x1FBF0, 0x1FBF9),
];

/// Go's `unicode.IsDigit` (general category Nd), used by `normalizeDigits`
/// (phonenumberutil.go:615).
pub fn is_decimal_digit(c: char) -> bool {
    let cp = c as u32;
    ND_RANGES
        .binary_search_by(|&(lo, hi)| {
            if hi < cp {
                std::cmp::Ordering::Less
            } else if lo > cp {
                std::cmp::Ordering::Greater
            } else {
                std::cmp::Ordering::Equal
            }
        })
        .is_ok()
}

/// The character `normalizeDigits` (phonenumberutil.go:615) writes for a decimal digit: Go maps
/// only the Arabic-Indic, extended Arabic-Indic and fullwidth digits to ASCII
/// (`arabicIndicNumberals`, phonenumberutil.go:582) and keeps every other digit (Devanagari,
/// Bengali, ...) as it is. Such a digit later fails `strconv.Atoi` / `ParseUint`, which only read
/// ASCII.
pub fn normalized_digit(c: char) -> char {
    let cp = c as u32;
    let zero = match cp {
        0x0660..=0x0669 => 0x0660,
        0x06F0..=0x06F9 => 0x06F0,
        0xFF10..=0xFF19 => 0xFF10,
        _ => return c,
    };
    char::from(b'0' + (cp - zero) as u8)
}

/// The members of Go's `\p{Nd}` written for a regex character class without its brackets (so it
/// can sit inside a larger class, as Go's `VALID_START_CHAR` puts `DIGITS` beside `PLUS_CHARS`).
pub(crate) fn regex_class_items() -> String {
    ND_RANGES
        .iter()
        .map(|&(lo, hi)| format!("\\x{{{lo:X}}}-\\x{{{hi:X}}}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digits_are_category_nd() {
        assert!(is_decimal_digit('0') && is_decimal_digit('9'));
        assert!(is_decimal_digit('\u{0966}')); // Devanagari zero
        assert!(is_decimal_digit('\u{1D7FF}')); // last mathematical digit
        assert!(!is_decimal_digit('a'));
        assert!(!is_decimal_digit('\u{00B2}')); // superscript two is No, not Nd
        assert!(!is_decimal_digit('\u{2460}')); // circled one is No
        assert!(!is_decimal_digit('\u{1FBFA}')); // just past the last range
    }

    #[test]
    fn only_arabic_indic_and_fullwidth_become_ascii() {
        assert_eq!(normalized_digit('\u{0663}'), '3');
        assert_eq!(normalized_digit('\u{06F9}'), '9');
        assert_eq!(normalized_digit('\u{FF15}'), '5');
        assert_eq!(normalized_digit('7'), '7');
        // Go keeps a Devanagari digit as it is (normalizeDigits writes the rune back).
        assert_eq!(normalized_digit('\u{0967}'), '\u{0967}');
    }

    #[test]
    fn table_is_sorted_and_disjoint() {
        for w in ND_RANGES.windows(2) {
            assert!(w[0].0 <= w[0].1 && w[0].1 < w[1].0);
        }
        let total: u32 = ND_RANGES.iter().map(|&(lo, hi)| hi - lo + 1).sum();
        assert_eq!(total, 680);
    }

    #[test]
    fn regex_class_matches_the_predicate() {
        let re = regex::Regex::new(&format!("^[{}]$", regex_class_items())).unwrap();
        for cp in 0..=0x1FFFFu32 {
            if let Some(c) = char::from_u32(cp) {
                assert_eq!(re.is_match(c.encode_utf8(&mut [0; 4])), is_decimal_digit(c), "{cp:X}");
            }
        }
    }
}
