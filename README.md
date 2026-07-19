# phonenumber

A ground-up Rust rewrite of Google's
[libphonenumber](https://github.com/google/libphonenumber): parse, validate, and
format international phone numbers, backed by the full country calling-code ↔
region ("calling regions") metadata.

**More than a port.** On top of parity it adds:

- **One-call JSON analysis** — `analyze()` returns a `PhoneNumberInfo` (country
  code, English country name, every calling region, validity, type, geocoding,
  carrier, time zones) that serializes straight to JSON.
- **Region + city geocoding** — a location for the number, extended to **40
  countries libphonenumber omits** (France, Mexico, Chile, Lebanon, Tunisia,
  Uzbekistan, …), with a country-name fallback so it is never empty.
- **CLDR country names** for every region.

Metadata is **generated at build time** from the upstream XML
(`resources/PhoneNumberMetadata.xml`, `resources/ShortNumberMetadata.xml`) into a
compact `bincode` blob embedded in the binary — no hand-written country tables,
fully reproducible from upstream.

## Status

254 territories (245 geographical regions + 9 non-geographical calling codes),
206 country calling codes. The example-number sweep validates **919/919** of the
upstream example numbers and round-trips **919/919** through E.164.

**78 tests** (unit + integration), **~97% line coverage** (`cargo llvm-cov`),
clippy-clean, deterministic build.

| Area | Status |
|------|--------|
| Parse (`+`, IDD, national, national-prefix stripping, extensions) | ✅ |
| Validate (`is_valid_number`, `is_valid_number_for_region`) | ✅ |
| Number type (`number_type`) | ✅ |
| Format (E164 / International / National / RFC3966) | ✅ |
| Calling regions (code ↔ region, shared-code disambiguation) | ✅ |
| Example numbers (`example_number*`) | ✅ |
| Short numbers + emergency (`ShortNumberInfo`) | ✅ |
| Possibility check (`is_possible_number`, `validate_length`) | ✅ |
| Mobile portability (`is_mobile_number_portable_region`) | ✅ |
| `can_be_internationally_dialled` | ✅ |
| `format_out_of_country_calling_number` | ✅ |
| `format_national_number_with_carrier_code` | ✅ |
| AsYouTypeFormatter (live formatting) | ✅ |
| Geocoder / Carrier / Timezone mappers (`en` locale) | ✅ |

### Geocoding coverage

City/area geocoding comes from upstream libphonenumber (`en`) for ~150 country
codes. Countries upstream omits (France, plus ~40 others such as Mexico, Chile,
Lebanon, Tunisia, Uzbekistan) are filled with curated `# CUSTOM` tables in
`resources/geocoding/en/`, merged so upstream data always wins. Where no area
data exists, `PhoneNumberInfo.location` falls back to the country name.

### Not yet exposed

- Non-`en` geocoding / carrier locales — one blob per language (same pipeline).

## Usage

```rust
use phonenumber::{parse, format, is_valid_number, number_type, PhoneNumberFormat};

let n = parse(Some("US"), "(650) 253-0000").unwrap();
assert_eq!(n.country_code, 1);
assert!(is_valid_number(&n));
assert_eq!(format(&n, PhoneNumberFormat::E164), "+16502530000");
assert_eq!(format(&n, PhoneNumberFormat::International), "+1 650-253-0000");
assert_eq!(format(&n, PhoneNumberFormat::National), "(650) 253-0000");
```

### Calling regions

```rust
use phonenumber::{regions_for_country_code, main_region_for_country_code, country_code_for_region};

assert_eq!(country_code_for_region("FR"), Some(33));
assert_eq!(main_region_for_country_code(1), Some("US"));      // NANP main country
assert!(regions_for_country_code(1).contains(&"CA".to_string())); // Canada shares +1
```

### Short numbers & emergency

```rust
use phonenumber::{is_emergency_number, is_valid_short_number_for_region};

assert!(is_emergency_number("112", "FR"));
assert!(is_emergency_number("911", "US"));
assert!(is_valid_short_number_for_region("112", "FR"));
```

### As-you-type formatting

```rust
use phonenumber::AsYouTypeFormatter;

let mut f = AsYouTypeFormatter::new("US");
let mut out = String::new();
for c in "6502530000".chars() {
    out = f.input_digit(c); // "(6", "(65", "(650", ... "(650) 253-0000"
}
assert_eq!(out, "(650) 253-0000");
```

### Geocoding, carrier & time zone

```rust
use phonenumber::{parse, description_for_number, carrier_for_number, time_zones_for_number};

let n = parse(None, "+12012345678").unwrap();
assert_eq!(description_for_number(&n), Some("New Jersey"));
assert!(time_zones_for_number(&n).contains(&"America/New_York"));

let gb = parse(Some("GB"), "07106123456").unwrap();
assert_eq!(carrier_for_number(&gb), Some("O2"));
```

### JSON summary

`analyze` (or `analyze_str`) returns a `PhoneNumberInfo` with everything about a
number — country code, region + English country name, all calling regions,
validity, possibility, type (toll-free / mobile / …), geocoding, carrier, and
time zones — and serializes to JSON.

```rust
let info = phonenumber::analyze_str(None, "+33805123456").unwrap();
assert_eq!(info.region_name.as_deref(), Some("France"));
assert_eq!(info.number_type, "toll_free");
assert!(info.is_toll_free && info.valid);
println!("{}", info.to_json_pretty());
```

```json
{
  "country_code": 33,
  "region": "FR",
  "region_name": "France",
  "national_number": "805123456",
  "e164": "+33805123456",
  "international": "+33 805 12 34 56",
  "national": "0 805 12 34 56",
  "rfc3966": "tel:+33-805-12-34-56",
  "valid": true,
  "possible": true,
  "can_be_internationally_dialled": true,
  "type": "toll_free",
  "is_toll_free": true,
  "time_zones": ["Europe/Paris"]
}
```

## Architecture

```
resources/*.xml           upstream metadata (vendored)
build.rs                  XML -> bincode blob in OUT_DIR (uses roxmltree)
src/model.rs              serde data model, shared by build.rs and runtime
src/metadata.rs           blob loader + lazy anchored/prefix regex cache
src/number.rs             PhoneNumber, PhoneNumberType, CountryCodeSource
src/parser.rs             parse() pipeline
src/validator.rs          type classification, validity, region resolution
src/format.rs             the four output formats
src/short.rs              ShortNumberInfo (emergency, short codes)
src/ayt.rs                AsYouTypeFormatter (live formatting)
src/mapper.rs             geocoder / carrier / timezone (longest-prefix maps)
src/info.rs               PhoneNumberInfo + analyze() -> JSON
src/example.rs            getExampleNumber* helpers
tests/integration.rs      unit tests + full example-number sweep
```

## Regenerating metadata

Re-download the upstream resources into `resources/` and rebuild; `build.rs`
reruns automatically when the XML changes:

```sh
curl -sSL -o resources/PhoneNumberMetadata.xml \
  https://raw.githubusercontent.com/google/libphonenumber/master/resources/PhoneNumberMetadata.xml
cargo build
```

## License

Apache-2.0, matching upstream libphonenumber. Metadata is derived from Google's
libphonenumber, © The libphonenumber Authors.
