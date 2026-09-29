# cshm contract as implemented

Implements the `cshm` profile-schema leaf contract dated 2026-09-30, within the
cvld v0.4 community policy facade (`cmnt` → `cplc` → `cshm`).

## Binding scope and boundaries

- Rust, current stable, native and `wasm32-unknown-unknown`.
- LGPL-3.0-only WITH LGPL-3.0-linking-exception; no GPL/AGPL-only dependencies.
- Thin facade over maintained serde, JSON Schema, and decimal libraries; the
  candidate survey and reasons are recorded in the README.
- Pure logic only. No storage, database queries, login dates, request logs, raw
  gate data, clocks, signing keys, signatures, or own cryptography. If a caller
  persists schemas, storage is its responsibility through `crlt`; signing is
  `csgn`'s responsibility. This crate does neither.
- Profile schemas have public and private parts, typed fields, labels,
  required/filterable flags, change presets, a text-only no-contact-details
  flag, versions, and per-field change classification. Pictures are unsupported.
- Real round-trip and failure tests; fmt, clippy with warnings denied, tests,
  and wasm check run on GitHub Actions. No workstation builds or registry
  publication.

## Definitions and revisions

`Schema` contains a nonblank, opaque `community`, nonzero `u32` `version`, and
ordered `public`/`private` field lists. Field IDs must be nonblank and unique
across both parts. IDs and community keys are compared exactly: no trimming,
case folding, or normalization is performed. Labels must be nonblank. Choice
options have unique nonblank IDs and nonblank display labels.

All schema, field, profile-envelope, and field-type objects reject unknown
properties during serde deserialization. Unknown enum variants, including
pictures, fail. Deserialization validates shape; `validate_definition` or
`compile` must be called to establish semantic validity. These calls reject
inverted bounds, duplicate identifiers, empty choices, invalid labels, and a
contact flag on a non-text field. Empty schema parts are valid.

`compile` takes an owned snapshot of a checked schema and generates reusable
Draft 2020-12 validators. It accepts no arbitrary JSON Schema, extensions,
regular expressions, or external references. The dependency's network and
file retrieval features are disabled.

`classify_changes(previous, next)` validates both definitions, requires equal
communities and `next.version > previous.version`, and returns revisions plus
field changes sorted by ID. Revision gaps are allowed. The caller assigns
revisions and prevents conflicting definitions of one revision. This crate
does not keep history or allocate version numbers. An unchanged definition
with a larger revision produces no field changes. Field order alone does not
produce changes; option order/labels are a non-tightening definition edit.

## Answers and validation

| Type | Accepted answers and bounds |
|---|---|
| Choice | A string matching one declared option ID; single selection |
| Integer | An integral JSON number within signed 64-bit limits and optional inclusive `min`/`max`; `18.0` also denotes an integer |
| Decimal | A JSON number within optional inclusive decimal bounds; exact comparisons, including beyond binary float precision |
| Yes/no | JSON `true` or `false` |
| Location | Exactly `latitude` and `longitude`, numeric degrees within [-90, 90] and [-180, 180] |
| Short/long text | JSON string within inclusive `min_length`/`max_length`, measured in Unicode scalar values; explicit limits, no hidden defaults |

Decimal schema bounds serialize as strings and use `rust_decimal`'s 96-bit
coefficient with a scale from 0 to 28. Missing/null numeric bounds mean no
additional bound; integers still retain their signed 64-bit domain. Profile
numbers use arbitrary-precision JSON; parse them from original text to preserve
digits. Already rounded floating-point values cannot be repaired by validation.
Numbers are values, never age bands or categories. Nonfinite numbers are not
JSON numbers. Text is not normalized, trimmed, or implicitly restricted to one
line. Short/long affects presentation; equal constraints accept equal strings.
Decimal bound strings use the upstream exact parser: empty strings, overflow,
and excess precision fail instead of becoming unbounded or being rounded.

`Profile` binds two answer maps to `community` and `schema_version`.
`validate_profile` checks both. `PublicProfile` contains only the public map;
`validate_public` deliberately makes no claim about private validity. Convert
with `PublicProfile::from(&profile)` before crossing a public-only boundary.
Both methods reject community/version mismatches before inspecting answers.
Both reject undeclared or misplaced fields and require all marked fields in
their scope. A missing optional field is valid; explicit null is invalid.
An empty string satisfies presence and still has to satisfy its length bounds.
Duplicate top-level answer IDs are rejected when deserializing profile JSON.
Nested JSON values have normal serde_json semantics; in particular duplicate
location coordinate keys are resolved by serde_json before validation.

`ValidationReport` contains every issue in public-then-private field order;
per-field reasons are ordered and deduplicated. Unknown fields yield a generic
part-level issue, not their submitted names. Reports and their debug form do
not contain submitted values. They identify known schema field IDs. Serde's
own syntax/deserialization errors and the profile objects themselves are not
value-free diagnostics and must not be logged as if they were.

### No contact details

The flag is schema policy for text. Structural validation alone cannot certify
that a text contains no contact details. For each present, structurally valid
flagged text, the `_with` validation methods call `Fn(&str) -> ContactCheck`:

- `Clear`: accept under the caller's policy.
- `Found`: report `ContactDetails`.
- `Unavailable`: report `ContactCheckRequired`.

Methods without a checker behave as `Unavailable`. Missing optional fields,
unflagged fields, and structurally invalid/oversized values do not invoke the
checker. Public validation never checks private text. The embedding guard must
supply consistent contact detection on native and device callers. This crate
does not invent a detector or claim resistance to obfuscation.

## Change classification

Each field receives one conservative class plus all changed aspects as reasons.
Unchanged fields are omitted. Stable IDs, rather than labels or positions,
establish identity. An ID rename is a removal plus an addition.

| Edit | Class | Consequence for the caller |
|---|---|---|
| Remove a public field or move it to private | `hide` | Strip the old public value immediately; never copy it automatically |
| Add an optional public field | `loosen` | Existing profiles need no new answer |
| Add a required public field; make optional required | `tighten` | Some members need input |
| Make required optional | `loosen` | Existing answers remain valid |
| Add choice IDs, widen numeric/text bounds | `loosen` | Existing answers remain valid |
| Remove/replace choice IDs, narrow either bound | `tighten` | Some old answers may fail |
| Integer → decimal with a containing interval | `loosen` | All old integers still fit |
| Decimal → integer, other answer-type changes | `tighten` | Conservatively require reevaluation/member input |
| Short ↔ long text with equal/wider limits | `loosen` | Presentation change; old strings remain valid |
| Enable no-contact-details | `tighten` | New content check may require input |
| Disable no-contact-details | `loosen` | Removes a restriction |
| Move preset toward stable (`free` → `occasional` → `stable`) | `tighten` | Stricter change policy may require member participation/pinning |
| Move preset toward free | `loosen` | Removes a change restriction |
| Change label, filterability, choice labels/order only | `loosen` | No new member input; this class includes neutral metadata edits |
| Move private field to public, even optional | `tighten` | Member input/consent is required before public disclosure |
| Add, remove, or edit wholly private fields | `private_only` | Clients validate privately; no public matching consequence |

Hiding wins for a public-to-private move, even if private constraints also
change. Otherwise a field with any public tightening is `tighten`, even when
other constraints loosen. Private-only edits can still invalidate private
answers; the class describes their public impact, not private validity.
Unrelated type conversions and decimal-to-integer changes are deliberately
conservative even if a special set of values might fit both types.

Tightening identifies potential input, not which specific members must edit.
Strict enforcement versus grandfathering, including the meaning of a "small"
change, is an operator decision outside this crate. Classification neither
enforces grace periods nor supplies community-admin timing settings.
`filterable` and the change presets are metadata; this library does not execute
matching rules, enforce tokens/pins, or make admission decisions.

## Verification and remaining integration work

Tests cover real schema/profile/report/change-set JSON round trips, all types,
exact large-integer and decimal comparisons, Unicode scalar lengths, location
failures, required/unknown/misplaced fields, duplicate answer IDs, public-only
validation, contact-check outcomes, envelope mismatches, immutable compiled
revisions, definition errors, and the classification table. An interval matrix
compares loosening to actual profile validation. No mocks of schema or validator
logic, and no database fixtures for this storage-free library.

CI runs native tests and compile-checks wasm32. Browser runtime execution and
generated language bindings remain integration work. Callers own payload and
schema size limits before parsing, contact detection, cryptographic bundle
binding, schema authenticity, pin/token enforcement, persistence, and applying
classification to live entries.
