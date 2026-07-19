use phonenumber::{
    country_code_for_region, example_number_for_type, format, is_valid_number,
    main_region_for_country_code, number_type, parse, regions_for_country_code, supported_regions,
    PhoneNumberFormat, PhoneNumberType,
};

#[test]
fn parse_us_national() {
    let n = parse(Some("US"), "(650) 253-0000").unwrap();
    assert_eq!(n.country_code, 1);
    assert_eq!(n.national_number, 6502530000);
    assert!(is_valid_number(&n));
    assert_eq!(format(&n, PhoneNumberFormat::E164), "+16502530000");
    assert_eq!(format(&n, PhoneNumberFormat::International), "+1 650-253-0000");
    assert_eq!(format(&n, PhoneNumberFormat::National), "(650) 253-0000");
    assert_eq!(
        format(&n, PhoneNumberFormat::Rfc3966),
        "tel:+1-650-253-0000"
    );
}

#[test]
fn parse_us_e164() {
    let n = parse(None, "+16502530000").unwrap();
    assert_eq!(n.country_code, 1);
    assert!(is_valid_number(&n));
}

#[test]
fn parse_fr_national_with_prefix() {
    // National prefix 0 should be stripped.
    let n = parse(Some("FR"), "01 42 68 53 00").unwrap();
    assert_eq!(n.country_code, 33);
    assert_eq!(n.national_number, 142685300);
    assert!(is_valid_number(&n));
    assert_eq!(format(&n, PhoneNumberFormat::E164), "+33142685300");
    assert_eq!(format(&n, PhoneNumberFormat::National), "01 42 68 53 00");
    assert_eq!(
        format(&n, PhoneNumberFormat::International),
        "+33 1 42 68 53 00"
    );
}

#[test]
fn parse_gb_international_and_national() {
    let intl = parse(None, "+44 20 7031 3000").unwrap();
    assert_eq!(intl.country_code, 44);
    assert!(is_valid_number(&intl));
    let nat = parse(Some("GB"), "020 7031 3000").unwrap();
    assert_eq!(canonical(&nat), canonical(&intl));
}

// Helper: compare only the identity-bearing fields (ignore raw_input/source).
fn canonical(n: &phonenumber::PhoneNumber) -> phonenumber::PhoneNumber {
    let mut c = n.clone();
    c.raw_input = None;
    c.country_code_source = phonenumber::CountryCodeSource::Unspecified;
    c.preferred_domestic_carrier_code = None;
    c
}

#[test]
fn calling_regions_plus1() {
    // +1 is North American Numbering Plan: US is the main country, CA included.
    assert_eq!(main_region_for_country_code(1), Some("US"));
    let regions = regions_for_country_code(1);
    assert!(regions.contains(&"US".to_string()));
    assert!(regions.contains(&"CA".to_string()));
    assert_eq!(regions[0], "US"); // main country first
    assert!(regions.len() > 20); // NANP has many territories
}

#[test]
fn calling_regions_shared_code_disambiguation() {
    // A Canadian number should resolve to CA, not US, despite sharing +1.
    let ca = parse(None, "+15062345678").unwrap(); // 506 = New Brunswick, CA
    assert_eq!(
        phonenumber::validator::region_code_for_number(&ca),
        Some("CA")
    );
}

#[test]
fn country_code_lookups() {
    assert_eq!(country_code_for_region("FR"), Some(33));
    assert_eq!(country_code_for_region("US"), Some(1));
    assert_eq!(country_code_for_region("JP"), Some(81));
    assert_eq!(country_code_for_region("ZZ"), None);
}

#[test]
fn number_type_detection() {
    let mobile = parse(Some("GB"), "07400 123456").unwrap();
    assert_eq!(number_type(&mobile), PhoneNumberType::Mobile);
}

#[test]
fn invalid_number_rejected() {
    let n = parse(Some("US"), "+1234").unwrap_or_default();
    assert!(!is_valid_number(&n));
}

/// The big sweep: every region's example numbers must validate, classify to a
/// compatible type, and survive an E164 round-trip.
#[test]
fn example_numbers_roundtrip_all_regions() {
    let types = [
        PhoneNumberType::FixedLine,
        PhoneNumberType::Mobile,
        PhoneNumberType::TollFree,
        PhoneNumberType::PremiumRate,
        PhoneNumberType::Voip,
    ];

    let mut checked = 0usize;
    let mut valid = 0usize;
    let mut roundtrip_ok = 0usize;
    let mut failures: Vec<String> = Vec::new();

    for region in supported_regions() {
        for &t in &types {
            let Some(n) = example_number_for_type(region, t) else {
                continue;
            };
            checked += 1;

            if is_valid_number(&n) {
                valid += 1;
            } else {
                failures.push(format!("{region}/{t:?}: not valid"));
            }

            // E164 round-trip: format then reparse must be equal.
            let e164 = format(&n, PhoneNumberFormat::E164);
            match parse(None, &e164) {
                Ok(re) => {
                    if re.country_code == n.country_code
                        && re.national_significant_number() == n.national_significant_number()
                    {
                        roundtrip_ok += 1;
                    } else {
                        failures.push(format!(
                            "{region}/{t:?}: roundtrip mismatch {e164} -> +{}{}",
                            re.country_code,
                            re.national_significant_number()
                        ));
                    }
                }
                Err(e) => failures.push(format!("{region}/{t:?}: reparse {e164} failed: {e}")),
            }
        }
    }

    eprintln!(
        "example sweep: checked={checked} valid={valid} roundtrip_ok={roundtrip_ok} failures={}",
        failures.len()
    );
    for f in failures.iter().take(40) {
        eprintln!("  FAIL {f}");
    }

    // Demand a high pass rate. Perfect parity is a later goal; this guards the core.
    assert!(checked > 400, "expected many example numbers, got {checked}");
    let valid_rate = valid as f64 / checked as f64;
    let rt_rate = roundtrip_ok as f64 / checked as f64;
    assert!(valid_rate > 0.95, "validity rate too low: {valid_rate:.3}");
    assert!(rt_rate > 0.95, "roundtrip rate too low: {rt_rate:.3}");
}


#[test]
fn short_numbers_and_emergency() {
    use phonenumber::{connects_to_emergency_number, is_emergency_number, is_valid_short_number_for_region};
    // Emergency numbers.
    assert!(is_emergency_number("112", "FR"));
    assert!(is_emergency_number("911", "US"));
    assert!(is_emergency_number("999", "GB"));
    assert!(is_emergency_number("112", "US")); // 112 routes to emergency in the US too
    assert!(!is_emergency_number("511", "US")); // not an emergency code
    assert!(connects_to_emergency_number("911", "US"));
    // Valid short codes.
    assert!(is_valid_short_number_for_region("112", "FR"));
    assert!(!is_valid_short_number_for_region("999999", "FR"));
}

#[test]
fn as_you_type_us_progression() {
    use phonenumber::AsYouTypeFormatter;
    let mut f = AsYouTypeFormatter::new("US");
    let steps = ["6", "65", "650", "6502", "65025", "650253", "6502530000"];
    let mut out = String::new();
    for d in "6502530000".chars() {
        out = f.input_digit(d);
    }
    eprintln!("final US AYT = {out}");
    assert_eq!(out, "(650) 253-0000");
    let _ = steps;
}

#[test]
fn as_you_type_intl_fr() {
    use phonenumber::format_as_you_type;
    // International input resolves the country code then formats nationally.
    let out = format_as_you_type("US", "+33142685300");
    eprintln!("FR intl AYT = {out}");
    assert!(out.starts_with("+33 "));
    assert_eq!(out, "+33 1 42 68 53 00");
}

#[test]
fn as_you_type_fr_national() {
    use phonenumber::format_as_you_type;
    let out = format_as_you_type("FR", "0142685300");
    eprintln!("FR national AYT = {out}");
    assert_eq!(out, "01 42 68 53 00");
}

#[test]
fn geocoding_carrier_timezone() {
    use phonenumber::{carrier_for_number, description_for_number, time_zones_for_number};
    // US number in New Jersey.
    let us = parse(None, "+12012345678").unwrap();
    eprintln!("geo={:?} tz={:?}", description_for_number(&us), time_zones_for_number(&us));
    assert_eq!(description_for_number(&us), Some("New Jersey"));
    assert!(time_zones_for_number(&us).contains(&"America/New_York"));

    // UK mobile -> carrier.
    let gb = parse(Some("GB"), "07106123456").unwrap();
    eprintln!("gb carrier={:?}", carrier_for_number(&gb));
    assert_eq!(carrier_for_number(&gb), Some("O2"));

    // Unknown number -> unknown time zone fallback.
    let odd = parse(None, "+9990000000").unwrap_or_default();
    assert_eq!(time_zones_for_number(&odd), vec!["Etc/Unknown"]);
}

#[test]
fn dump_all_regions_coverage() {
    use phonenumber::{country_code_for_region, example_number, is_valid_number, supported_regions, supported_calling_codes};
    let mut regions: Vec<&str> = supported_regions().collect();
    regions.sort();
    let mut with_example = 0;
    let mut valid = 0;
    for r in &regions {
        let cc = country_code_for_region(r).unwrap();
        if let Some(n) = example_number(r) {
            with_example += 1;
            if is_valid_number(&n) { valid += 1; }
        }
        eprint!("{r}(+{cc}) ");
    }
    eprintln!("\n--- {} geo regions, {} calling codes, example_ok={}/{} valid={} ---",
        regions.len(), supported_calling_codes().len(), with_example, regions.len(), valid);
    assert_eq!(regions.len(), 245);
    assert_eq!(valid, with_example); // every region with an example validates
}

#[test]
fn dump_non_geo() {
    use phonenumber::{supported_calling_codes, main_region_for_country_code, regions_for_country_code};
    let mut ng = vec![];
    for c in supported_calling_codes() {
        if regions_for_country_code(c).is_empty() {
            ng.push(c);
        }
    }
    eprintln!("non-geo codes ({}): {:?}", ng.len(), ng);
    for c in &ng { assert_eq!(main_region_for_country_code(*c), Some("001")); }
    assert_eq!(ng.len(), 9);
}

/// Exhaustive per-region exercise of every module across all 245 regions.
#[test]
fn all_regions_full_exercise() {
    use phonenumber::{
        carrier_for_number, description_for_number, example_number_for_type, format,
        format_as_you_type, is_valid_number, number_type, parse, supported_regions,
        time_zones_for_number, PhoneNumberFormat, PhoneNumberType,
    };
    let types = [
        PhoneNumberType::FixedLine, PhoneNumberType::Mobile, PhoneNumberType::TollFree,
        PhoneNumberType::PremiumRate, PhoneNumberType::SharedCost, PhoneNumberType::Voip,
        PhoneNumberType::PersonalNumber, PhoneNumberType::Pager, PhoneNumberType::Uan,
        PhoneNumberType::Voicemail,
    ];
    let (mut nums, mut valid, mut e164_rt, mut natl_rt, mut intl_rt, mut ayt_ok) =
        (0u32, 0u32, 0u32, 0u32, 0u32, 0u32);
    let (mut geo, mut carr, mut tz) = (0u32, 0u32, 0u32);
    let mut regions_done = 0u32;
    let mut fails: Vec<String> = vec![];

    for region in supported_regions() {
        regions_done += 1;
        for &t in &types {
            let Some(n) = example_number_for_type(region, t) else { continue };
            nums += 1;
            if is_valid_number(&n) { valid += 1; } else { fails.push(format!("{region}/{t:?} invalid")); }
            let _ = number_type(&n);

            // E164 roundtrip (region-independent).
            let e164 = format(&n, PhoneNumberFormat::E164);
            if let Ok(r) = parse(None, &e164) {
                if r.national_significant_number() == n.national_significant_number() && r.country_code == n.country_code { e164_rt += 1; }
                else { fails.push(format!("{region}/{t:?} e164 rt")); }
            }
            // National roundtrip (reparse with region).
            let natl = format(&n, PhoneNumberFormat::National);
            if let Ok(r) = parse(Some(region), &natl) {
                if r.national_significant_number() == n.national_significant_number() { natl_rt += 1; }
                else { fails.push(format!("{region}/{t:?} natl rt: {natl}")); }
            }
            // International roundtrip.
            let intl = format(&n, PhoneNumberFormat::International);
            if let Ok(r) = parse(None, &intl) {
                if r.national_significant_number() == n.national_significant_number() { intl_rt += 1; }
            }
            // AsYouType on the E164 digits reproduces International.
            let ayt = format_as_you_type(region, &e164);
            if ayt == intl { ayt_ok += 1; }

            // Mappers (coverage counts; not all numbers are mapped).
            if description_for_number(&n).is_some() { geo += 1; }
            if carrier_for_number(&n).is_some() { carr += 1; }
            if time_zones_for_number(&n).iter().any(|z| *z != "Etc/Unknown") { tz += 1; }
        }
    }
    eprintln!("REGIONS={regions_done} NUMS={nums}");
    eprintln!("  valid={valid}  e164_rt={e164_rt}  natl_rt={natl_rt}  intl_rt={intl_rt}  ayt=={ayt_ok}");
    eprintln!("  mapped: geo={geo} carrier={carr} timezone={tz}");
    for f in fails.iter().take(25) { eprintln!("  FAIL {f}"); }
    assert_eq!(regions_done, 245);
    assert_eq!(valid, nums, "all example numbers must validate");
    assert_eq!(e164_rt, nums, "all must E164-roundtrip");
    assert_eq!(natl_rt, nums, "all must National-roundtrip");
}


#[test]
fn audit_data_integrity() {
    use phonenumber::*;
    // No region appears twice; every region resolves to a code that lists it back.
    let mut seen = std::collections::HashSet::new();
    for r in supported_regions() {
        assert!(seen.insert(r), "duplicate region {r}");
        let cc = country_code_for_region(r).unwrap();
        assert!(regions_for_country_code(cc).iter().any(|x| x == r),
            "region {r} (cc {cc}) not in its own calling-region list");
    }
    // Every calling code's main region resolves and owns the code.
    for c in supported_calling_codes() {
        let main = main_region_for_country_code(c).expect("main region");
        if main != "001" {
            assert_eq!(country_code_for_region(main), Some(c), "main {main} cc mismatch {c}");
        }
    }
    // Non-geo codes are disjoint from geo codes.
    for c in supported_calling_codes() {
        let geo = !regions_for_country_code(c).is_empty();
        let non_geo = phonenumber::metadata::is_non_geo_code(c);
        assert!(geo ^ non_geo, "code {c} must be exactly one of geo / non-geo");
    }
}

#[test]
fn audit_known_good_outputs() {
    use phonenumber::*;
    // (default_region, input, E164, International, National, type)
    struct C(&'static str,&'static str,&'static str,&'static str,&'static str);
    let cases = [
        C("US","+1 650 253 0000","+16502530000","+1 650-253-0000","(650) 253-0000"),
        C("GB","+44 20 7031 3000","+442070313000","+44 20 7031 3000","020 7031 3000"),
        C("FR","+33 1 42 68 53 00","+33142685300","+33 1 42 68 53 00","01 42 68 53 00"),
        C("DE","+49 30 12345678","+493012345678","+49 30 12345678","030 12345678"),
        C("JP","+81 3 3224 9999","+81332249999","+81 3-3224-9999","03-3224-9999"),
        C("IT","+39 02 3661 8300","+390236618300","+39 02 3661 8300","02 3661 8300"),
    ];
    for c in &cases {
        let n = parse(Some(c.0), c.1).unwrap_or_else(|e| panic!("{}: parse err {e}", c.1));
        assert_eq!(format(&n, PhoneNumberFormat::E164), c.2, "E164 {}", c.1);
        assert_eq!(format(&n, PhoneNumberFormat::International), c.3, "INTL {}", c.1);
        assert_eq!(format(&n, PhoneNumberFormat::National), c.4, "NATL {}", c.1);
        assert!(is_valid_number(&n), "valid {}", c.1);
    }
}

#[test]
fn audit_italian_leading_zero() {
    use phonenumber::*;
    let n = parse(None, "+390236618300").unwrap();
    assert!(n.italian_leading_zero, "IT number must carry leading zero");
    assert_eq!(n.national_significant_number(), "0236618300");
    assert_eq!(format(&n, PhoneNumberFormat::E164), "+390236618300");
}

#[test]
fn audit_no_panics_on_garbage() {
    use phonenumber::*;
    let regions = ["US","FR","GB","JP","IT","ZZ","001"];
    let inputs = [
        "", "+", "++", "+++++", "0", "00", "tel:", "tel:+", "tel:;ext=",
        "+9999999999999999999999", "abcdefg", "()-.  ", "+0", "+1",
        "＋３３", "٠١٢٣٤٥", "１２３４５６７８", ";ext=999", "#", "x", "1x2x3",
        "+1 (650) 253-0000 ext 12345678901234567890",
        "\u{202e}+33 1 42", "   +   4 4   ", "+99999", "0000000000000000000000",
    ];
    for r in &regions {
        for inp in &inputs {
            // Must never panic; Ok or Err both fine.
            let res = parse(Some(r), inp);
            if let Ok(n) = res {
                // Downstream calls must also not panic.
                let _ = format(&n, PhoneNumberFormat::E164);
                let _ = format(&n, PhoneNumberFormat::International);
                let _ = format(&n, PhoneNumberFormat::National);
                let _ = format(&n, PhoneNumberFormat::Rfc3966);
                let _ = is_valid_number(&n);
                let _ = number_type(&n);
                let _ = description_for_number(&n);
                let _ = carrier_for_number(&n);
                let _ = time_zones_for_number(&n);
            }
            // AsYouType must never panic on any char stream.
            let mut f = AsYouTypeFormatter::new(r);
            for c in inp.chars() { let _ = f.input_digit(c); }
        }
    }
}

#[test]
fn audit_new_apis() {
    use phonenumber::*;
    let us = parse(None, "+16502530000").unwrap();
    assert!(is_possible_number(&us));
    assert_eq!(validate_length(&us), PossibleResult::IsPossible);
    // Too long.
    let long = parse(None, "+1234567890123456").unwrap_or_default();
    assert_ne!(validate_length(&long), PossibleResult::IsPossible);
    // Portability.
    assert!(is_mobile_number_portable_region("US"));
    assert!(!is_mobile_number_portable_region("ZZ"));
    // A possible-but-not-valid number (right length, wrong pattern).
    let odd = PhoneNumber { country_code: 1, national_number: 5555555555, ..Default::default() };
    let _ = is_possible_number(&odd);
}

/// Exercise every new secondary API across all 245 regions: no panics, and
/// possibility must agree with validity for valid example numbers.
#[test]
fn all_regions_secondary_apis() {
    use phonenumber::*;
    let types = [
        PhoneNumberType::FixedLine, PhoneNumberType::Mobile, PhoneNumberType::TollFree,
        PhoneNumberType::PremiumRate, PhoneNumberType::Voip,
    ];
    let (mut nums, mut possible, mut valid_and_possible) = (0u32, 0u32, 0u32);
    let (mut ooc_ok, mut carrier_ok, mut intl_diallable) = (0u32, 0u32, 0u32);
    let mut bad: Vec<String> = vec![];

    for region in supported_regions() {
        // Portability lookup must not panic for any region.
        let _ = is_mobile_number_portable_region(region);
        for &t in &types {
            let Some(n) = example_number_for_type(region, t) else { continue };
            nums += 1;

            // A valid number must also be a *possible* number (validity ⊆ possibility).
            let is_valid = is_valid_number(&n);
            let is_poss = is_possible_number(&n);
            if is_poss { possible += 1; }
            if is_valid {
                if is_poss { valid_and_possible += 1; }
                else { bad.push(format!("{region}/{t:?}: valid but NOT possible")); }
            }
            // validate_length must never say InvalidCountryCode for a real example.
            if validate_length(&n) == PossibleResult::InvalidCountryCode {
                bad.push(format!("{region}/{t:?}: validate_length=InvalidCountryCode"));
            }

            // Dialability + out-of-country + carrier must not panic and be non-empty.
            if can_be_internationally_dialled(&n) { intl_diallable += 1; }
            let ooc = format_out_of_country_calling_number(&n, "US");
            if !ooc.is_empty() { ooc_ok += 1; }
            let car = format_national_number_with_preferred_carrier_code(&n, "15");
            if !car.is_empty() { carrier_ok += 1; }
        }
    }
    eprintln!("secondary sweep: nums={nums} possible={possible} valid&possible={valid_and_possible}");
    eprintln!("  intl_diallable={intl_diallable} ooc_ok={ooc_ok} carrier_ok={carrier_ok} bad={}", bad.len());
    for b in bad.iter().take(30) { eprintln!("  BAD {b}"); }
    // Every valid example is possible; every call produced output.
    assert!(bad.is_empty(), "{} inconsistencies", bad.len());
    assert_eq!(ooc_ok, nums);
    assert_eq!(carrier_ok, nums);
}

#[test]
fn geocoding_newly_added_countries() {
    use phonenumber::{analyze_str, description_for_number, parse};
    // Countries libphonenumber omits, now covered by merged/custom data.
    let mx = parse(None, "+525512345678").unwrap();
    assert_eq!(description_for_number(&mx), Some("Mexico City, FD"));
    let cl = analyze_str(None, "+56221234567").unwrap();
    assert_eq!(cl.location.as_deref(), Some("Santiago, Metropolitan Region"));
    let lb = analyze_str(None, "+9611234567").unwrap();
    assert_eq!(lb.location.as_deref(), Some("Beirut"));
    // No regression: US stays city-level.
    let us = parse(None, "+12125551234").unwrap();
    assert_eq!(description_for_number(&us), Some("New York, NY"));
}
