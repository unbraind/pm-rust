# Native capability discovery contracts

The product exposes `pm-rust capabilities` and
`discovery::discover_capabilities` under `pm-rust/capabilities/v1`.
Both return the evidenced compiled surface without reading a tracker or opening
an adapter. Unknown operations and protocols refuse before any source access.
The output is complete or refused, bounded in both native TOON and compact JSON.

## Overlap check

The implementation was checked against the current published pm 2026.11.4
`contracts` surface, the repository-pinned 2026.10.21 public SDK capability and
flag declarations, pm-web's `contract-explorer.ts`, this crate's
`native_adapter` ABI and its bounded read sessions.

| Existing contract | Scope | Native discovery relationship |
| --- | --- | --- |
| Upstream command/SDK contracts | Host command grammar, runtime provider availability, schemas | Oracle for tested slices; no version-based compatibility inference |
| pm-web explorer | Provider-backed review and availability | Separate hosted product; no hosted state or provider probe here |
| Native adapter ABI | Explicit transport operation grants, correlated canonical envelopes | Reuses manifest admission; unspecified adapters advertise no available operations |
| Read sessions | Immutable snapshot and per-page resource admission | Listed native operation; discovery does not execute or allocate a session |

## Production boundary

`src/cli.rs` is shared by the executable and SDK discovery, so flags, aliases,
defaults, conflicts, delimiter rules and hidden controls come from execution.
A new parser operation without evidence causes `contract_drift` rather than an
optimistic availability claim. Each advertised operation carries a real native
invocation receipt; oracle slices additionally link their differential tests.
Native-only wrapper/transport semantics remain explicitly distinct.

Discovery adds no tracker mutations, import/export/sync paths, journaled plans,
retry keys, leases, shared checkpoints, ambient scope defaults, count-ratio
decisions or status interpretation. Existing mutation, registry and ownership
implementations remain authoritative. Thus mutation replay, lease interleaving
and import state-transition requirements introduce no new paths in this change.
The native runtime acquires no JavaScript SDK. The private root npm package only
aliases the native gates; npm/Bun global extension installation is not a shipping
surface of this Rust crate. Validation tooling uses the existing pinned public
SDK installation, independently of executable arguments and symlinks.

## Targeted evidence

- `tests/capability_contract.rs::every_advertised_operation_is_invoked` executes
  every discovered command against a tracker initialized with the published CLI
  in a non-git temporary directory, with neither Git nor Node on the native PATH.
- A fixed-seed model runs 500 random operation/request sequences, replaying older
  inputs and whitespace/CRLF variants. Both wire producers decode through the
  typed response schema. Complete SDK-read item state, including native fields,
  relationships and foreign metadata, remains equal to the reference snapshot.
- Another fixed-seed property test covers 500 extreme numeric values and exact
  domain boundaries. Nonfinite, fractional, negative, oversized, sparse, duplicate
  and unknown-field inputs have explicit refusal cases alongside valid inputs.
- All 32 adapter grant subsets are checked. Unsupported operations never appear
  as granted; invalid declarations refuse without dispatching transport calls.
- Recovery JSON Schemas derive from the same exported field descriptors and
  native error enums; admission limits come directly from the producer constant.
- `scripts/native-adapter-capabilities-revert.mts` replaces only the discovery
  function in a disposable non-git copy and requires a compiled runtime assertion
  failure. A second control replaces only CLI delivery with false success and
requires the writer-refusal regression to fail. The reviewed source is untouched,
then passing tests run again. Its Cargo target lives inside the disposable copy,
so neither successful nor failed controls replace the checkout executable.

## Review regression controls

All five first-round review threads were valid. Discovery now refuses positional
struct arrays at the request and adapter boundaries, reports public lifecycle
wrappers and option-dependent SDK entry points, filters both backend lists by
the requested operation, and advertises JSON alone for the four ownership
commands. Actual default CLI ownership output is decoded in the contract test;
session refusal producers are also compared with their recovery descriptor.

`node scripts/native-adapter-capabilities-review-revert.mts` independently reverts
each admission, mapping, selection and encoding behavior in a disposable non-git
copy, requiring a compiled assertion failure for each Rust control. A descriptor
rename proves the session producer comparison. A separate runner-artifact control
reintroduces the shared target and requires the artifact-isolation tests to fail.
Another delivery control replaces a producer refusal with successful rediscovery;
the writer test requires the original typed refusal and no emitted bytes.
Those tests exercise existing and absent executables and both runner failure
paths with a simulated Cargo child. Rust controls use real compilation and the
pinned oracle in genuine non-git trackers. Fixed-source tests run after restoration.

Round 2 also checks malformed nested manifests against both SDK admission and
the real CLI, including exact refusal codes and remedies. Parser conflicts are
advertised in both directions, with independent assertions for every declared
pair and real conflicting list invocations. Both scratch-copy filters are tested
with Windows, POSIX and mixed separators plus allowed near-name paths; each
filter is independently reverted to prove the corresponding assertion fails.

The review body's proposed `Arg::get_default_missing_values()` getter is absent
from pinned [Clap 4.6.7's public API](https://docs.rs/clap_builder/4.6.7/clap_builder/builder/struct.Arg.html).
The sole missing-value default is already shared with execution through
`VALIDATE_CLOSE_DEFAULT`. An independent test now executes every bare
optional-value flag with the real parser and compares the resulting raw values
to discovery, so later parser additions cannot silently omit their defaults.
Changing only discovery's missing default makes this test fail.

## Public-copy validation

The reproducible tool is `scripts/native-adapter-capabilities-validate.mts`.
It archives only the committed public tracker, certifies its complete list with
the published SDK, preserves every archived byte during SDK preparation, then
makes the owned copy read-only. Native discovery and actual native reads run
without Git or Node. A full SDK read-back compares every item and relationship;
the frozen file inventory is also fingerprinted before and after.

The [aggregate receipt](benchmarks/native-capability-discovery.json) records public
main `a4e9e18ea2aec4aab08a662252c5f664ea95d720`:

| Measurement | Result |
| --- | ---: |
| SDK-certified items | 94 |
| Frozen authoritative files, including 7 SDK preparation files | 201 |
| Frozen authoritative bytes | 7,088,530 |
| Discovered operations | 25 |
| Oracle slices / native-only surfaces | 17 / 8 |
| Argument contracts | 236 |
| Real native calls | 129 |
| Complete JSON / TOON bytes, including newline | 55,754 / 66,443 |
| Largest selected-operation JSON response | 9,312 bytes |
| Total observed output / largest read response | 1,486,020 / 95,779 bytes |
| Native validation wall time | 6,212 ms |

Every archived authoritative byte survived SDK preparation, and the frozen
fingerprint remained `61529901412a9141a3247588aba210fd6018e2d5ae4038956f8e030c3c0b7ae7`
after discovery and native reads. The shared snapshot helper excludes derived
root search/lock/checkpoint directories; permissions were frozen across the
whole owned copy, and complete SDK item/body/relationship read-back also agreed.
Both the CLI and SDK example emitted identical discovery data.

## Release evidence

Local validation includes the PM-linked 13 discovery units, 6 contract tests and
17 existing CLI dispatch tests, plus a dedicated writer-failure test and 6 runner tests. The two
seeded models each run 500 cases. The disposable behavioral revert compiles and
fails the tracker-free acceptance assertion; the fixed implementation passes
again afterward. Strict Rust lint/docs, TypeScript and ESLint are checked too.

The unchanged CI workflow runs the npm and Bun `release:check` aggregates,
including the full published-oracle suite and 100 percent line, function,
region and branch thresholds. The PR records exact-head run URLs and counts;
those final receipts stay external so recording them does not invalidate the
head they certify.

The first review-round local aggregate passed 607 cases before an existing five-second
native child deadline stopped the orientation suite. That exact case passed an
unchanged focused retry. The stopped invocation did not reach full coverage;
fresh final-head CI must certify the complete release aggregate. The writer test
also directly exercises producer-refusal propagation, the one region missed by
the prior head's full CI coverage run. The 100 percent thresholds are unchanged.

Round 2's unchanged local aggregate passed 611 cases before the same existing
five-second native-child deadline stopped orientation. An unchanged focused
retry also reached the child limit, on a different input. This remains a local
validation blocker on `pm-rust-local-packet-deadline`; no timeout or threshold was
relaxed. The stopped aggregate did not reach repository coverage, so component
checks cannot certify the full gate. Final-head CI receipts belong on the PR.

The renderer independently checks the 1024..131072 output-byte domain before
encoding. Caller-modified or received response metadata cannot raise that hard
ceiling. The added regression fails with the original renderer and passes with
the guard, covering both encodings and oversized received metadata.
