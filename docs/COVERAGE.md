# Coverage contract

CI targets 100% of reachable production lines and branches. Stable Rust runs
the existing native and wasm checks. Nightly Rust is used only for LLVM branch
instrumentation, which currently requires it. Both jobs use the same resolved
Cargo.lock snapshot; all build and test commands after resolution use --locked.

The coverage job executes real tests with cargo-llvm-cov and retains the raw JSON
even when the gate fails. The checker compares integer covered/total counts for
both metrics; rounded percentages cannot pass. An empty report cannot pass.
The report excludes integration-test harness files under tests/, not production
code. The source-specific proposed exceptions below require independent review.

A failing gate is missing evidence, not permission to lower the threshold or
change domain behavior. Add meaningful failure and round-trip tests. Document
any genuinely unreachable defensive branch precisely before excluding it. Native
coverage does not establish browser execution; keep the actual wasm vectors.

First-party dependencies follow main. Their resolved full revisions remain in
Cargo.lock, with exactly one source per first-party crate. Dependabot maintains
committed snapshots; CI refreshes once per run and retains the tested snapshot.
Auto-merge requires protected main and successful substantive checks on the
exact current Dependabot head. It never executes PR code with write permissions.

## Emitted source metric

The acceptance metric is now 100% of upstream LLVM LCOV's emitted production
source line counters (DA) and branch counters (BRDA), from the same execution as
the retained raw JSON. This is source coverage, not every generic instantiation.
LLVM JSON and LCOV summary totals can count generic copies differently from
the merged source records. The original JSON checker remains diagnostic code;
its stricter instantiation totals are not described as passed.

The source gate requires a nonempty report, an exact file inventory matching the
companion JSON, matching raw summary metadata, complete emitted branch counts
and no duplicate or unknown counters. Every reachable source counter must be positive.
Only the exact documented exceptions below are proposed; no function or file is omitted.
Actual browser/device execution remains separate from native coverage.

## Proposed unreachable defensive paths

Native and actual Node wasm validation/classification tests passed at
3218fafea65b38c08edbce635500c561fd10c981 in
[CI36914282870](https://github.com/corbet-foss/cshm/actions/runs/36914282870).
Before exclusions, native source evidence was430/441 lines and126/128 branches.
No domain behavior was changed to produce that measurement. These13 entries
exclude11 exact unexecuted lines and2 exact branch arms, pending independent
review. The full JSON, LCOV and annotated report inventories remain mandatory.
A source change anywhere in the three invariant-owning files, a moved source
line, an absent counter or execution of an excluded counter fails the gate.
The opposite arm remains measured. The upstream lock from that run is retained;
normal dependency refresh and all substantive tests remain required.

### Closed schema compilation

`Schema::compile` first calls `validate_definition`, then constructs each
validator only through `field_schema`. The seven closed field variants generate
Draft2020-12 type/enum/numeric/text/location constraints. Numeric bounds are typed,
text bounds are validated, and the caller cannot inject JSON Schema keywords or
references. The current maintained jsonschema0.58.4 accepts this subset in real
native and wasm tests. The error mapping at validation.rs185–191 is retained for
a future backend contract change. No fake backend or invalid private compiled
object is supplied merely to execute it.

### Numeric dispatch and closed error kinds

`number::validate` always returns `Some` for integer, decimal and location
fields, including invalid values. Their exact comparison/range/shape refusals
therefore finish before the `else` at validation.rs290. The Location true arm at
292 and its mapper line293 are unreachable. For the remaining choice, boolean
and text variants, generated constraints can yield only Type, Enum, MinLength
and MaxLength. Minimum/Maximum at298–299 and the catch-all at302 remain defensive
for a future field kind. They cannot execute through the current closed schema.
The actual numerical-boundary and malformed-location tests continue to run.

### Validated contact text

`validate_definition` rejects `no_contact_details` on non-text fields. Before
line311, empty `reasons` means that the upstream text type constraint accepted
the value; thus `value.as_str()` must be `Some`. Non-string values and oversized
text have real refusal tests and never invoke the content checker. Only the
`None` branch arm is excluded; all three real content-check outcomes stay gated.
