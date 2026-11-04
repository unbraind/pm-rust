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
  failure. The reviewed source is untouched, then passing tests run again.

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
| Frozen authoritative bytes | 7,088,848 |
| Discovered operations | 25 |
| Oracle slices / native-only surfaces | 17 / 8 |
| Argument contracts | 236 |
| Real native calls | 129 |
| Complete JSON / TOON bytes, including newline | 55,534 / 66,215 |
| Largest selected-operation JSON response | 9,528 bytes |
| Total observed output / largest read response | 1,490,481 / 95,779 bytes |
| Native validation wall time | 3,848 ms |

Every archived authoritative byte survived SDK preparation, and the frozen
fingerprint remained `78035830b50e238abded4e20537785b09b6a7866ef32e868ac5696c628431d90`
after discovery and native reads. The shared snapshot helper excludes derived
root search/lock/checkpoint directories; permissions were frozen across the
whole owned copy, and complete SDK item/body/relationship read-back also agreed.
Both the CLI and SDK example emitted identical discovery data.

## Release evidence

Local validation includes the PM-linked 6 discovery units, 3 contract tests and
17 existing CLI dispatch tests, plus a dedicated writer-failure test. The two
seeded models each run 500 cases. The disposable behavioral revert compiles and
fails the tracker-free acceptance assertion; the fixed implementation passes
again afterward. Strict Rust lint/docs, TypeScript and ESLint are checked too.

The unchanged CI workflow runs the npm and Bun `release:check` aggregates,
including the full published-oracle suite and 100 percent line, function,
region and branch thresholds. The PR records exact-head run URLs and counts;
those final receipts stay external so recording them does not invalidate the
head they certify.
