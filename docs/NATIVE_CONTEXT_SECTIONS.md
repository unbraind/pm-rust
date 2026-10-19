# Native context sections and relationship packets

Tracked by [pm-rust-s6j4](../.agents/pm/tasks/pm-rust-s6j4.toon), under
[pm-rust-u4al](../.agents/pm/epics/pm-rust-u4al.toon). The published oracle remains
locked to `@unbrained/pm-cli@2026.10.14`.

`context` supports brief, standard, deep and full depth in default TOON and JSON.
The default depth and enabled sections come from workspace settings. Deep adds
hierarchy, compact retained activity, progress, recently created and unparented
items, workload, blockers, hot files, staleness and test health. Focus rows gain
descendant completion when the selected sections require hierarchy metadata.
Agenda and blocked fallback retain their existing selection rules.

`--limit` selects a ranked producer page. `--after` continues that page;
`--cursor` is a native alias for the same control. Later pages retain the
oracle's metadata reference envelope. `--token-budget` applies to both focus
packing and the complete rendered response. Matching corpus totals remain
distinct from delivered focus counts and section limits. Tag inheritance is
resolved across all emitted focus collections.

Brief reads retain the projection introduced in #94: item bodies and unrelated
collection payloads are not decoded. Optional sections select an additional
projection for file links, test definitions, run totals and blocking notes.
Comment, note and learning payloads remain count-only. Compact activity reads
retained history only when its section is selected, including deleted-item
streams. Native reads write neither derived caches nor usage feedback.

The locked CLI exposes relationship packets through `deps <id> --format context`,
rather than embedding them in the orientation response. That command now serves
a bounded native relationship packet. It explains
parent/child and blocker/dependent relationships, with direction and kind
filters, breadth-first distances, root evidence, exact completeness counters
and cursor continuation. Node, edge, depth and rendered token bounds match the
published packet contract. Metadata discovery uses the list projection; only
the selected root is decoded completely for evidence, and summary mode omits
that extra read. No history stream is opened for packet assembly.

Existing context boundaries remain explicit: custom scorer execution,
non-weekly or non-UTC recurrence, workspace-memory rollups at 10,000 items,
unsupported extension policies, and invalid or stale ranking snapshots.
Static minimum-version declarations support canonical triples up to the locked
oracle version; unknown ranges and newer minima retain the explicit refusal.
Context intents, arbitrary explicit sections, subtree/filter flags and canonical
read-output controls remain in the follow-up item.

Oracle compatibility retains published quirks. An initial packet budget unable to fit
the root and evidence is an `unknown_error` refusal. After a successful attempt,
the pinned oracle retains its previous result if further rendered-budget reduction
cannot fit either one node or the root and evidence. That retained result may
exceed the requested rendered budget; native output preserves this behavior. Context continuation may repeat an active blocked item because the
packing pool independently includes the first blocked candidates on each page.
Malformed packet-cursor recovery still refers to `--after`, although packet
continuation accepts `--cursor`; this published recovery text is retained.
Blocker details ignore legacy `depends_on` rows even though packet graph
assembly canonicalizes that alias; native output preserves this inconsistency.
Packet token estimates are settled before legacy alias counts are attached, so
the oracle undercounts that metadata; native packets retain the same estimate.
Hierarchy trims parent identifiers, while progress groups the raw parent strings;
this published inconsistency is retained. Completion follows the same IEEE division and rounding sequence: 29 closed
descendants out of 200 render as 14 percent, matching the pinned CLI.
The upstream compatibility reports are
[pm-cli#1509](https://github.com/unbraind/pm-cli/issues/1509) (legacy blocker details),
[#1510](https://github.com/unbraind/pm-cli/issues/1510) (cursor recovery),
[#1511](https://github.com/unbraind/pm-cli/issues/1511) (repeated blocked pages),
[#1512](https://github.com/unbraind/pm-cli/issues/1512) (rounding), and
[#1513](https://github.com/unbraind/pm-cli/issues/1513) (packet budgets and unknown errors).
Those output contracts stay pinned rather than introducing native-only changes.

External graph keys also follow the oracle: plain `global` and `external`
references retain external identity. Case-insensitive collisions with local,
missing or already reserved graph IDs receive repeated `external:` prefixes.
External node details never reuse local details. Synthetic tests cover plain
namespaces, collisions and URL controls in full/summary TOON and JSON packets.
The real-copy harness selects roots from decoded metadata with local targets;
plain external references, including collisions, cannot satisfy that selection.

Dependency fixtures retain `created_at`: the pinned reader's multiple-row
timestamp-sort defect is already documented by
[pm-rust-tdps](../.agents/pm/tasks/pm-rust-tdps.toon).

## Validation and measurement

The synthetic differentials compare complete stdout, stderr and exit status,
including Unicode, section counts, completion, linked evidence, blocked work,
continuation and rendering budgets. Native API tests exercise the same real
tracker documents without SDK mocks.

`scripts/context-real.mts` accepts only an explicitly marked scratch tracker
copy, a clock-pinned oracle driver and a native binary. It compares default and
deep TOON/JSON output with default, zero and bounded limits, token budgets and
three producer pages. It then alternates fresh native/oracle processes after
warmup and emits only aggregate evidence and timing samples. Private tracker
rows, identities, filesystem paths and raw outputs are never included in its
public receipt. Authoritative item and history hashes must remain unchanged.

The aggregate gate is `just release-check`: this Rust repository has no root
npm manifest, npm tarball or packed-consumer targets. Its oracle installation
uses the locked npm package. Additional Bun acceptance runs the same oracle
orientation suite with `PM_NODE_INTERPRETER=bun`; this changes only the test
interpreter, while the production path remains native Rust.

Measurements use 30 fresh processes per runtime and command, alternating order
after warmup. Both p50 and p95 use nearest-rank percentiles. The clock-pinned
oracle driver also disables its scheduling-dependent 25 ms packing guard so
each comparison and timing sample produces the complete comparable response. Timings describe the
shared validation host and this captured tracker, not a general scale guarantee.

The optimized build from implementation head `63d359e` passed all 56 real-copy
comparisons: 30 context cases and 26 packet cases across two relationship roots.
The captured tracker contained 1,254 items, compared with the request's earlier
1,251-item count. All 2,527 authoritative item/history files remained unchanged.
The frozen binary hash and all 120 timing samples are retained in the
[aggregate receipt](benchmarks/native-context-sections-2026-10-19.json).

| Command | Native p50 ms | Native p95 ms | TS p50 ms | TS p95 ms |
| --- | ---: | ---: | ---: | ---: |
| `context --limit 10` | 81.0 | 291.0 | 1402.2 | 5101.8 |
| `context --depth deep --limit 10` | 445.7 | 598.0 | 1905.0 | 2684.5 |

Each cell has 30 fresh-process samples. Some sampling overlapped other
validation on the shared host; alternating runtime order limits ordering bias.
These results describe this captured tracker and test setup.

The unchanged `just release-check` aggregate passed on all three platforms at
implementation head `63d359e`, including fmt, strict Clippy, private-item rustdoc,
required-oracle tests, branch-instrumented coverage, audit and changelog checks.
Clone-local merge-driver installation and strict tracker health also passed.
All four coverage dimensions are 100 percent on every platform. Exact counts,
source identity, latency ceilings and both behavioral revert proofs are in the
[gate receipt](benchmarks/native-context-gates-2026-10-19.json).

| Platform | Ordinary tests | Instrumented tests | Failed / ignored |
| --- | ---: | ---: | ---: |
| Linux | 502 | 502 | 0 / 0 |
| macOS | 498 | 498 | 0 / 0 |
| Windows | 493 | 493 | 0 / 0 |

Final-source Bun orientation acceptance passed all 24 tests. The three linked
item commands passed seven underlying test executions; project policy keeps
run recording disabled, so the aggregate is retained in item comments. The
context regression fails with only its native method reverted and passes
restored. The two numerical helper regressions also fail with integer shortcuts
and pass when the oracle's floating-point sequence is restored.

## Review round 1 latency diagnosis

Windows context p95 was 339.5 ms on the evidence-only `506b448` head in
[run 39278038751](https://github.com/unbraind/pm-rust/actions/runs/39278038751).
The Rust sources and latency harness were identical to `63d359e`, whose p95 was
186.2 ms in [run 39273842181](https://github.com/unbraind/pm-rust/actions/runs/39273842181).
The preceding [run 39267857005](https://github.com/unbraind/pm-rust/actions/runs/39267857005)
measured 213.5 ms. This establishes shared-runner variation without a source
regression. The Ubuntu Rust job on `506b448` was cancelled after its full test
step succeeded. Its check annotation reports exceeding the 15-minute execution
limit. All three Rust jobs now receive the existing Windows allowance of 20
minutes for setup, tests and cleanup; this does not change latency bounds.

The latency gate now performs five unmeasured warm-up launches per command,
then three fixed windows of 30 fresh processes. It enforces the median of the
three nearest-rank p95 values and retains all 90 samples. There is no retry
until success. Get/list/context bounds remain 50/150/250 ms. Harness regressions
reject a sustained violation and a fast median with a slow p95 tail.

Review regressions include four Node harness tests, a real-tracker external
namespace unit test and an eight-case CLI oracle differential. Reverting only
`Graph::new` to `506b448` fails the namespace unit test (exit 101); restoring it
passes. Isolated behavioral reversions of argument validation, root namespace
filtering, warm-up and median-window selection each fail the Node regressions
(exit 1), which pass 4/4 with all fixes restored. These harness regressions run
through the Rust release suite on every platform.

Review round 1 implementation `ceffe36` passed all nine CI jobs in
[run 39285340550](https://github.com/unbraind/pm-rust/actions/runs/39285340550):

| Platform | Ordinary tests | Instrumented tests | Required coverage |
| --- | ---: | ---: | --- |
| Linux | 505 | 505 | 100% lines/functions/regions/branches |
| macOS | 501 | 501 | 100% lines/functions/regions/branches |
| Windows | 496 | 496 | 100% lines/functions/regions/branches |

Each platform ran 42 suite executions with zero failed or ignored tests.
All three latency jobs passed; context median-window p95 was 106.0/135.6/188.5 ms
on Linux/macOS/Windows against the unchanged 250 ms bound. Bun acceptance passed
25/25 orientation tests. All five linked commands passed 13 underlying test
executions. The end-to-end synthetic harness passed 37 comparisons and retained
authoritative and frozen-binary hashes. Aggregate evidence and revert outcomes:
[review receipt](benchmarks/native-context-review-1-2026-10-19.json).

The first Bun attempt ran during oracle extraction and is excluded. The duplicate
isolated local aggregate was stopped after complete three-OS CI certification
and is also excluded. Clean-clone health passed the exact CI check-only command;
this does not claim a full retained-history audit. The final documentation and
tracker handoff receives a fresh three-OS matrix before the PR summary is final.


## Review round 2

Authoritative-copy snapshots exclude runtime, locks, transactions, checkpoints
and search directories only at the tracker root. Identically named nested item
folders and their history streams remain hashed, so changes invalidate the receipt.
The packet matrix accepts matching status and output bytes, including nonzero
refusals. A real synthetic tracker with evidence exceeding the 800-token packet
budget verifies that the complete harness still writes its receipt.

Relationship packets index incident edges once per immutable graph. Self-loops
appear once; direction, inverse kind filtering and stable output ordering remain
unchanged. Query discovery, direct role counts and cursor fingerprints are computed
once and reused while rendered budgets and missing-reference limits shrink.
A 20,000-edge disconnected component regression bounds 2,000 isolated-node lookups
to one second; it excludes graph construction and does not change public latency
thresholds. Exact-output differential tests remain the semantic acceptance gate.

The request to propagate the root-and-evidence floor after a successful attempt
is declined for pinned-oracle parity (upstream pm-cli#1513). The published SDK's
`isContextBudgetFloorError` accepts both floor messages. A root-only 100-token
TOON/JSON regression confirms identical successful over-budget responses; initial
root/evidence refusals still match the oracle.


| Unresolved thread | Disposition and regression |
| --- | --- |
| `PRRT_kwDOTwgMRs6uG8fo` | Fixed root-only cache exclusions; snapshot regression detects nested TOON and JSONL mutations. Reverting only the exclusion function fails (exit 1), restoration passes. |
| `PRRT_kwDOTwgMRs6uG8f6` | Fixed matching packet refusals, including continuation. The real-tracker harness retains hashes and writes a receipt. Reverting only the packet comparison fails (exit 101), at comparison 1 in the final oversized-evidence fixture. |
| `PRRT_kwDOTwgMRs6uG8gM` | Indexed adjacency and reused immutable discovery across retries. Reverting only `Graph::neighbors` compiles and fails the performance assertion (exit 101; 134.87 seconds); restoration passes all eight packet tests. Self-loop, direction, inverse-kind and ordering assertions also pass. |
| `PRRT_kwDOTwgMRs6uG8gW` | Declined for pinned-oracle parity. Applying the proposed one-node-only fallback makes native exit 1 while the oracle exits 0; the TOON/JSON differential fails (exit 101), and restoring oracle behavior passes. |

The final oversized-evidence harness fixture exercises matching refusals in both
the packet matrix and continuation, while summary packets remain successful.
No review threads are replied to or resolved by this change.


Implementation `a01ca0b` passed all nine CI jobs in
[run 39335689495](https://github.com/unbraind/pm-rust/actions/runs/39335689495),
including the unchanged full `just release-check` on every platform:

| Platform | Ordinary tests | Instrumented tests | Lines covered | Functions covered | Regions covered | Branches covered |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Linux | 509 | 509 | 13,136/13,136 | 1,355/1,355 | 21,941/21,941 | 3,152/3,152 |
| macOS | 505 | 505 | 13,183/13,183 | 1,367/1,367 | 22,028/22,028 | 3,154/3,154 |
| Windows | 500 | 500 | 13,182/13,182 | 1,368/1,368 | 22,029/22,029 | 3,156/3,156 |

Every platform completed 42 suite executions with zero failures or ignored tests.
Lines, functions, regions and branches are all 100 percent at unchanged source
scope and thresholds. Formatting, strict Clippy, private-item rustdoc, audit,
identity and changelog checks passed. Audit retains the pre-existing allowed
`RUSTSEC-2024-0449` unmaintained development-dependency advisory. All three
latency jobs passed the unchanged 50/150/250 ms bounds, and CodeQL passed.

The complete Bun orientation suite passed 27/27 with
`PM_NODE_INTERPRETER=bun PM_RUST_REQUIRE_PUBLISHED_CLI=1 cargo +1.90.0 test --locked --test orientation_differential -- --test-threads=1`.
Node and Bun harness tests passed 7/7 each. All seven linked pm commands passed,
covering 20 underlying test executions; automatic run recording remains disabled
by project policy, so the item comments retain the aggregate and revert proofs.

A fresh no-local clone of the implementation passed strict check-only tracker
health with required merge drivers and no tracked changes. This does not claim
a full retained-history audit. Host-only extension skew and existing advisories
remain outside this change. The duplicate local aggregate was stopped after the
complete Linux CI aggregate passed; it is excluded from completed gate evidence.
The successful clone health check used the locked package from a verified cache
after an earlier package-install setup was excluded. CodeRabbit reported manual
review required; skipped automated reviews are not approval.

The handoff commit changes only this evidence and the paired tracker item/history;
production code, tests, workflow, pins and thresholds match the certified
implementation. Its own exact-head CI matrix is verified before the final PR
handoff. The item remains in progress and is released for orchestrator review.
