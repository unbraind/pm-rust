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
