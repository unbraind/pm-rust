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

Oracle compatibility retains published quirks. A packet budget unable to fit
the root and evidence is an `unknown_error` refusal. A packet can also retain
its smallest useful result when further rendered-budget reduction cannot fit
one node. Context continuation may repeat an active blocked item because the
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
