// Build script: parse the vendored PhoneNumberMetadata.xml into a compact
// bincode blob placed in OUT_DIR, which the library embeds via include_bytes!.
//
// Doing this at build time keeps the source tree free of a 1MB generated .rs
// file while remaining fully reproducible from the upstream XML.

use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::Path;

// Pull in the shared, serde-only data model.
include!("src/model.rs");

const METADATA_XML: &str = "resources/PhoneNumberMetadata.xml";
const SHORT_XML: &str = "resources/ShortNumberMetadata.xml";
const GEOCODING_DIR: &str = "resources/geocoding/en";
const CARRIER_DIR: &str = "resources/carrier/en";
const TIMEZONE_FILE: &str = "resources/timezones/map_data.txt";
const REGION_NAMES_FILE: &str = "resources/region_names.txt";

fn parse_doc(xml: &str) -> roxmltree::Document<'_> {
    let opts = roxmltree::ParsingOptions {
        allow_dtd: true,
        ..roxmltree::ParsingOptions::default()
    };
    roxmltree::Document::parse_with_options(xml, opts).expect("invalid metadata XML")
}

fn main() {
    println!("cargo:rerun-if-changed={METADATA_XML}");
    println!("cargo:rerun-if-changed={SHORT_XML}");
    println!("cargo:rerun-if-changed={GEOCODING_DIR}");
    println!("cargo:rerun-if-changed={CARRIER_DIR}");
    println!("cargo:rerun-if-changed={TIMEZONE_FILE}");
    println!("cargo:rerun-if-changed={REGION_NAMES_FILE}");
    println!("cargo:rerun-if-changed=src/model.rs");
    println!("cargo:rerun-if-changed=build.rs");

    build_main_metadata();
    build_short_metadata();
    build_prefix_map_dir("geo.bin", GEOCODING_DIR);
    build_prefix_map_dir("carrier.bin", CARRIER_DIR);
    build_prefix_map_file("timezone.bin", TIMEZONE_FILE);
    build_prefix_map_file("names.bin", REGION_NAMES_FILE);
}

/// Parse a `prefix|value` file into sorted key/value pairs.
fn parse_prefix_lines(content: &str, map: &mut std::collections::BTreeMap<String, String>) {
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = line.split_once('|') {
            let key = key.trim();
            let value = value.trim();
            if !key.is_empty() && !value.is_empty() {
                map.insert(key.to_string(), value.to_string());
            }
        }
    }
}

fn write_prefix_map(out_name: &str, map: std::collections::BTreeMap<String, String>) {
    // BTreeMap keeps keys sorted, enabling binary-search longest-prefix lookup.
    let db = PrefixMap {
        entries: map.into_iter().collect(),
    };
    let bytes = bincode::serialize(&db).expect("serialize prefix map");
    let out_dir = env::var("OUT_DIR").expect("OUT_DIR");
    fs::write(Path::new(&out_dir).join(out_name), &bytes).unwrap_or_else(|e| {
        panic!("write {out_name}: {e}");
    });
    println!(
        "cargo:warning=phonenumber: generated {out_name} ({} entries, {} bytes)",
        db.entries.len(),
        bytes.len()
    );
}

/// Build a prefix map from every `*.txt` file in a directory.
fn build_prefix_map_dir(out_name: &str, dir: &str) {
    let mut map = std::collections::BTreeMap::new();
    let entries = fs::read_dir(dir).unwrap_or_else(|e| panic!("read_dir {dir}: {e}"));
    for entry in entries {
        let path = entry.expect("dir entry").path();
        if path.extension().and_then(|s| s.to_str()) != Some("txt") {
            continue;
        }
        let content = fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path:?}: {e}"));
        parse_prefix_lines(&content, &mut map);
    }
    write_prefix_map(out_name, map);
}

/// Build a prefix map from a single file (used for time zones).
fn build_prefix_map_file(out_name: &str, file: &str) {
    let mut map = std::collections::BTreeMap::new();
    let content = fs::read_to_string(file).unwrap_or_else(|e| panic!("read {file}: {e}"));
    parse_prefix_lines(&content, &mut map);
    write_prefix_map(out_name, map);
}

fn build_short_metadata() {
    let xml = fs::read_to_string(SHORT_XML)
        .unwrap_or_else(|e| panic!("cannot read {SHORT_XML}: {e}"));
    let doc = parse_doc(&xml);

    let regions: Vec<ShortMetadata> = doc
        .descendants()
        .filter(|n| n.has_tag_name("territory"))
        .map(|t| ShortMetadata {
            id: attr(&t, "id").unwrap_or("").to_string(),
            general_desc: child_desc(&t, "generalDesc"),
            short_code: child_desc(&t, "shortCode"),
            toll_free: child_desc(&t, "tollFree"),
            premium_rate: child_desc(&t, "premiumRate"),
            standard_rate: child_desc(&t, "standardRate"),
            emergency: child_desc(&t, "emergency"),
            expanded_emergency: child_desc(&t, "expandedEmergency"),
            sms_services: child_desc(&t, "smsServices"),
            carrier_specific: child_desc(&t, "carrierSpecific"),
        })
        .collect();

    let db = ShortDb { regions };
    let bytes = bincode::serialize(&db).expect("serialize short metadata");
    let out_dir = env::var("OUT_DIR").expect("OUT_DIR");
    fs::write(Path::new(&out_dir).join("short_metadata.bin"), &bytes)
        .expect("write short_metadata.bin");
    println!(
        "cargo:warning=phonenumber: generated short-number metadata for {} regions",
        db.regions.len()
    );
}

fn build_main_metadata() {
    let xml = fs::read_to_string(METADATA_XML)
        .unwrap_or_else(|e| panic!("cannot read {METADATA_XML}: {e}"));
    let doc = parse_doc(&xml);

    let mut regions: Vec<PhoneMetadata> = Vec::new();
    let mut non_geo: Vec<PhoneMetadata> = Vec::new();
    // Preserve upstream (alphabetical) order but keep the main country first.
    let mut code_to_regions: BTreeMap<u16, Vec<String>> = BTreeMap::new();

    for territory in doc
        .descendants()
        .filter(|n| n.has_tag_name("territory"))
    {
        let meta = parse_territory(&territory);
        let code = meta.country_code;
        let id = meta.id.clone();
        let is_main = meta.main_country_for_code;

        if id == "001" {
            non_geo.push(meta);
        } else {
            let entry = code_to_regions.entry(code).or_default();
            if is_main {
                entry.insert(0, id);
            } else {
                entry.push(id);
            }
            regions.push(meta);
        }
    }

    let db = MetadataDb {
        regions,
        non_geo,
        code_to_regions: code_to_regions.into_iter().collect(),
    };

    let bytes = bincode::serialize(&db).expect("serialize metadata");
    let out_dir = env::var("OUT_DIR").expect("OUT_DIR");
    let out_path = Path::new(&out_dir).join("metadata.bin");
    fs::write(&out_path, &bytes).expect("write metadata.bin");

    println!(
        "cargo:warning=phonenumber: generated {} bytes of metadata ({} regions, {} non-geo, {} country codes)",
        bytes.len(),
        db.regions.len(),
        db.non_geo.len(),
        db.code_to_regions.len(),
    );
}

/// Remove all whitespace — upstream pretty-prints regex sources for readability.
fn strip_ws(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}

fn attr<'a>(n: &'a roxmltree::Node, name: &str) -> Option<&'a str> {
    n.attribute(name)
}

fn attr_bool(n: &roxmltree::Node, name: &str) -> bool {
    matches!(n.attribute(name), Some("true"))
}

/// Text of the first direct child element with the given tag, whitespace-stripped.
fn child_pattern(n: &roxmltree::Node, tag: &str) -> String {
    n.children()
        .find(|c| c.has_tag_name(tag))
        .and_then(|c| c.text())
        .map(strip_ws)
        .unwrap_or_default()
}

/// Text of the first direct child element, preserving internal spacing.
fn child_text_raw(n: &roxmltree::Node, tag: &str) -> String {
    n.children()
        .find(|c| c.has_tag_name(tag))
        .and_then(|c| c.text())
        .map(|t| t.trim().to_string())
        .unwrap_or_default()
}

/// Expand a possibleLengths spec into sorted unique lengths.
///
/// Upstream grammar is a comma-separated list where each element is either a
/// single length (`10`) or an inclusive range written in brackets (`[6-8]`).
fn parse_lengths(spec: &str) -> Vec<u8> {
    let mut out = Vec::new();
    for tok in spec.split(',') {
        // Strip surrounding brackets: "[6-8]" -> "6-8".
        let tok = tok.trim().trim_start_matches('[').trim_end_matches(']');
        if tok.is_empty() {
            continue;
        }
        if let Some((a, b)) = tok.split_once('-') {
            if let (Ok(a), Ok(b)) = (a.trim().parse::<u8>(), b.trim().parse::<u8>()) {
                for v in a..=b {
                    out.push(v);
                }
            }
        } else if let Ok(v) = tok.parse::<u8>() {
            out.push(v);
        }
    }
    out.sort_unstable();
    out.dedup();
    out
}

/// Expand a national-prefix formatting rule: `$NP` -> national prefix,
/// `$FG` -> `$1`. Result is a replacement template usable against a match.
fn expand_np_rule(rule: &str, national_prefix: &str) -> String {
    if rule.is_empty() {
        return String::new();
    }
    rule.replacen("$NP", national_prefix, 1)
        .replacen("$FG", "$1", 1)
}

fn parse_desc(n: &roxmltree::Node) -> PhoneNumberDesc {
    let national_number_pattern = child_pattern(n, "nationalNumberPattern");
    let example_number = child_text_raw(n, "exampleNumber");
    let (possible_lengths, possible_local_lengths) = n
        .children()
        .find(|c| c.has_tag_name("possibleLengths"))
        .map(|pl| {
            (
                parse_lengths(pl.attribute("national").unwrap_or("")),
                parse_lengths(pl.attribute("localOnly").unwrap_or("")),
            )
        })
        .unwrap_or_default();
    PhoneNumberDesc {
        national_number_pattern,
        possible_lengths,
        possible_local_lengths,
        example_number,
    }
}

/// Find a direct child <tag> and parse it as a PhoneNumberDesc (empty if absent).
fn child_desc(t: &roxmltree::Node, tag: &str) -> PhoneNumberDesc {
    t.children()
        .find(|c| c.has_tag_name(tag))
        .map(|c| parse_desc(&c))
        .unwrap_or_default()
}

fn parse_territory(t: &roxmltree::Node) -> PhoneMetadata {
    let id = attr(t, "id").unwrap_or("").to_string();
    let country_code = attr(t, "countryCode")
        .and_then(|s| s.parse::<u16>().ok())
        .unwrap_or(0);

    let national_prefix = attr(t, "nationalPrefix").unwrap_or("").to_string();
    // Defaults: parsing prefix falls back to the plain national prefix.
    let national_prefix_for_parsing = attr(t, "nationalPrefixForParsing")
        .map(strip_ws)
        .unwrap_or_else(|| national_prefix.clone());
    let national_prefix_transform_rule = attr(t, "nationalPrefixTransformRule")
        .unwrap_or("")
        .to_string();
    let territory_np_formatting_rule = attr(t, "nationalPrefixFormattingRule")
        .unwrap_or("")
        .to_string();
    let territory_np_optional = attr_bool(t, "nationalPrefixOptionalWhenFormatting");

    let general_desc = child_desc(t, "generalDesc");
    let fixed_line = child_desc(t, "fixedLine");
    let mobile = child_desc(t, "mobile");
    let same_mobile_and_fixed_line_pattern = !mobile.national_number_pattern.is_empty()
        && mobile.national_number_pattern == fixed_line.national_number_pattern;

    // Parse formatting rules.
    let mut number_formats = Vec::new();
    let mut intl_number_formats = Vec::new();
    if let Some(af) = t.children().find(|c| c.has_tag_name("availableFormats")) {
        for nf in af.children().filter(|c| c.has_tag_name("numberFormat")) {
            let pattern = attr(&nf, "pattern").map(strip_ws).unwrap_or_default();
            let format = child_text_raw(&nf, "format");
            let leading_digits: Vec<String> = nf
                .children()
                .filter(|c| c.has_tag_name("leadingDigits"))
                .filter_map(|c| c.text())
                .map(strip_ws)
                .filter(|s| !s.is_empty())
                .collect();
            let np_rule_raw = attr(&nf, "nationalPrefixFormattingRule")
                .map(|s| s.to_string())
                .unwrap_or_else(|| territory_np_formatting_rule.clone());
            // Pre-expand $NP (national prefix) and $FG (first group => $1),
            // exactly like upstream's metadata build step.
            let np_rule = expand_np_rule(&np_rule_raw, &national_prefix);
            let np_optional = nf
                .attribute("nationalPrefixOptionalWhenFormatting")
                .map(|s| s == "true")
                .unwrap_or(territory_np_optional);
            // Carrier rule: $NP => national prefix, $FG => $1; $CC is preserved
            // for substitution with the actual carrier code at format time.
            let carrier_rule = attr(&nf, "carrierCodeFormattingRule")
                .map(|s| {
                    s.replacen("$NP", &national_prefix, 1)
                        .replacen("$FG", "$1", 1)
                })
                .unwrap_or_default();

            let intl_text = nf
                .children()
                .find(|c| c.has_tag_name("intlFormat"))
                .and_then(|c| c.text())
                .map(|s| s.trim().to_string());

            let national = NumberFormat {
                pattern: pattern.clone(),
                format: format.clone(),
                leading_digits: leading_digits.clone(),
                national_prefix_formatting_rule: np_rule,
                national_prefix_optional_when_formatting: np_optional,
                domestic_carrier_code_formatting_rule: carrier_rule,
            };

            // Build the international list exactly like upstream:
            //  - absent intlFormat  => copy the national format
            //  - intlFormat == "NA" => no international format for this rule
            //  - otherwise          => use the given template
            match intl_text.as_deref() {
                None => intl_number_formats.push(national.clone()),
                Some("NA") => { /* not dialable internationally with this rule */ }
                Some(other) => intl_number_formats.push(NumberFormat {
                    format: other.to_string(),
                    ..national.clone()
                }),
            }

            number_formats.push(national);
        }
    }

    PhoneMetadata {
        id,
        country_code,
        international_prefix: attr(t, "internationalPrefix").unwrap_or("").to_string(),
        preferred_international_prefix: attr(t, "preferredInternationalPrefix")
            .unwrap_or("")
            .to_string(),
        national_prefix,
        preferred_extn_prefix: attr(t, "preferredExtnPrefix").unwrap_or("").to_string(),
        national_prefix_for_parsing,
        national_prefix_transform_rule,
        national_prefix_formatting_rule: territory_np_formatting_rule,
        national_prefix_optional_when_formatting: territory_np_optional,
        main_country_for_code: attr_bool(t, "mainCountryForCode"),
        leading_digits: attr(t, "leadingDigits").map(strip_ws).unwrap_or_default(),
        mobile_number_portable_region: attr_bool(t, "mobileNumberPortableRegion"),
        general_desc,
        fixed_line,
        mobile,
        toll_free: child_desc(t, "tollFree"),
        premium_rate: child_desc(t, "premiumRate"),
        shared_cost: child_desc(t, "sharedCost"),
        personal_number: child_desc(t, "personalNumber"),
        voip: child_desc(t, "voip"),
        pager: child_desc(t, "pager"),
        uan: child_desc(t, "uan"),
        voicemail: child_desc(t, "voicemail"),
        no_international_dialling: child_desc(t, "noInternationalDialling"),
        number_formats,
        intl_number_formats,
        same_mobile_and_fixed_line_pattern,
    }
}
