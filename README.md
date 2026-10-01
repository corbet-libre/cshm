# cshm

**Typed community profile schemas, validation, and per-field change classification.**

Part of `cvld`, the permanent door of the cmtymeet trust stack. This Rust library
contains pure logic for native and `wasm32-unknown-unknown` callers. It has no
storage, signing, login dates, request logs, or gate data. Interfaces are still
in development; registry publication is disabled.

## API

- `Schema`, `Field`, `FieldType`, `ChangePreset`: serde schema definitions with
  public and private fields, labels, required/filterable flags, change presets,
  and a text-only `no_contact_details` flag.
- `Schema::validate_definition()` checks schema invariants;
  `Schema::compile()` produces an owned, reusable `CompiledSchema`.
- `CompiledSchema::validate_profile()` checks both parts against the exact
  community and revision. `validate_public()` accepts a separate `PublicProfile`
  that cannot carry private answers. Reports contain declared field IDs and
  machine-readable reasons, never submitted values or undeclared keys.
- The corresponding `_with` methods accept a caller-supplied text checker.
  Restricted text fails with `ContactCheckRequired` when no checker is available.
- `classify_changes(old, new)` returns a serializable `ChangeSet`: hide, loosen,
  tighten, or private-only for each changed field. The caller chooses what to do
  with the classification. No profile is migrated or published by this library.

```rust
use cshm::{ChangeClass, Profile, Schema, classify_changes};

let schema: Schema = serde_json::from_str(r#"{
  "community": "walking-club",
  "version": 1,
  "public": [{
    "id": "weekends", "label": "Do weekends suit you?",
    "kind": {"type": "yes_no"},
    "required": true, "filterable": true,
    "change_preset": "free", "no_contact_details": false
  }],
  "private": []
}"#)?;
let profile: Profile = serde_json::from_str(r#"{
  "community": "walking-club", "schema_version": 1,
  "public": {"weekends": true}, "private": {}
}"#)?;
let validator = schema.compile()?;
assert!(validator.validate_profile(&profile).is_valid());

let mut next = schema.clone();
next.version = 2;
next.private = std::mem::take(&mut next.public);
let changes = classify_changes(&schema, &next)?;
assert_eq!(changes.fields[0].class, ChangeClass::Hide);
# Ok::<(), Box<dyn std::error::Error>>(())
```

## Types and wire format

`kind.type` is one of `choice`, `integer`, `decimal`, `yes_no`, `location`,
`short_text`, or `long_text`. Choices contain `{id, label}` options; answers use
one ID. Integers use inclusive `min`/`max` bounds within `i64`. Decimal bounds
are **strings** (for example `"0.1000000000000000001"`), while profile answers
are JSON numbers. Missing or null number bounds mean unbounded, subject to the
integer type's limits. Decimal bounds use `rust_decimal`'s 96-bit coefficient
and scale 0–28; profile numbers are validated with arbitrary precision.
Schema deserialization rejects empty, overflowing, or excessively precise
decimal bounds instead of rounding them. Use `Decimal::from_str_exact` when
constructing bounds in Rust.

Parse precise profile numbers directly from JSON text. Passing through a
JavaScript `Number`, a Rust float, or a `json!` float literal may already have
rounded them before validation. Preserve the original numeric JSON text for
large integers and precise decimals in device bindings.

Locations are `{ "latitude": 47.3, "longitude": 8.5 }` in degrees, within
[-90, 90] and [-180, 180], with no extra properties. Both text types require
explicit inclusive `min_length` and `max_length` in Unicode scalar values
(not UTF-8 bytes or grapheme clusters). Their difference is presentation;
neither silently trims whitespace or rejects newlines. `required` means
present; use `min_length: 1` when an empty string must fail. Null is never a
valid answer, even for an optional field.

See [the implemented contract](docs/CONTRACT.md) for the classification table,
validation rules, and integration boundaries.

## Dependency survey and choice

Surveyed on 2026-09-30 against crates.io entries, upstream repositories, and
documentation. These are dependencies, not copied implementations.

| Candidate | Choice and reason |
|---|---|
| [`jsonschema`](https://crates.io/crates/jsonschema), [upstream](https://github.com/Stranger6667/jsonschema) | Selected, 0.58.2 compatible series, MIT. Maintained runtime JSON Schema validation, reusable validators, exact numeric validation and wasm support. `cshm` compiles its closed field types into Draft 2020-12 constraints. Default features are disabled: no HTTP/file resolution or TLS stack. `arbitrary-precision` preserves numerical boundaries. |
| [`serde`](https://crates.io/crates/serde), [`serde_json`](https://github.com/serde-rs/json) | Selected, 1.x, MIT/Apache-2.0. Standard typed serialization and JSON values; `arbitrary_precision` retains numeric tokens. |
| [`rust_decimal`](https://crates.io/crates/rust_decimal), [upstream](https://github.com/paupino/rust-decimal) | Selected, 1.43 compatible series, MIT. Exact bounded decimal metadata and comparisons for change classification; avoids implementing decimal arithmetic. Only `std` and string serde adapters are enabled. |
| [`garde`](https://crates.io/crates/garde), [upstream](https://github.com/jprochazk/garde) | Not selected. Its derive-based validation of Rust types is useful for fixed forms; community-defined runtime field sets would still need custom dispatch and constraints. |
| [`validator`](https://crates.io/crates/validator), [upstream](https://github.com/Keats/validator) | Not selected for the same reason: fixed Rust struct validation does not supply runtime community schemas or their change semantics. |

Only community identity, field metadata, definition invariants, change
classification, and error adaptation are implemented here. Value constraints
are delegated to `jsonschema`. No cryptographic primitive is implemented.
No dependency is available only under GPL or AGPL.

## Validation and CI

GitHub Actions runs current stable Rust with:

```text
cargo fmt --all --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo check --locked --target wasm32-unknown-unknown
```

Tests exercise actual serde round trips and upstream validators, all field
types, invalid definitions, exact numerical boundaries, public/private
separation, safe diagnostics, contact-check outcomes, and change classification.
A small exhaustive interval matrix checks loosening against real validation.
The README example is a doctest. `Cargo.lock` records CI's resolved dependencies;
the workflow also retains it as an artifact. No database tests are applicable
because this crate has no storage. Tests currently execute natively; wasm is
compile-checked, not executed in a browser.

## Integration boundaries

- The caller supplies contact detection; there is no universal detector here
  for obfuscated URLs, email addresses, phone numbers, and platform handles.
  Returning `Clear` is the caller's policy verdict, not a proof about the text.
- `stable`/`occasional`/`free` are metadata, not token quotas. Pin verification,
  signatures (`csgn`), bundle admission (`cgrd`), storage (`crlt`), and decisions
  on grandfathering belong to their respective callers.
- Schema sizes and input payload limits must be bounded by the embedding
  application before parsing. Serde parse errors are external errors and may
  contain input; do not treat them as the value-free validation reports.
- No TypeScript bindings, UI editor, or browser runtime harness is included.

## License

Copyright 2026 Julian Y. Richard Corbet. Licensed under [LGPL-3.0-only](LICENSES/LGPL-3.0-only.txt) [WITH LGPL-3.0-linking-exception](LICENSES/LGPL-3.0-linking-exception.txt). See [LICENSE.md](LICENSE.md).

## Continuous verification

Dependency updates follow main and are tested against one CI-resolved lockfile.
Line and branch coverage target 100%; failures remain blocking. See
[the coverage contract](docs/COVERAGE.md) for measurement and exclusions.
