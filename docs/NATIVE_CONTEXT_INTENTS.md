# Native context intents

Tracked by [pm-rust-context-intents](../.agents/pm/features/pm-rust-context-intents.toon)
under [pm-rust-s6j4](../.agents/pm/tasks/pm-rust-s6j4.toon).
The context and dependency oracle is independently locked to pm **2026.11.5** in
`tests/orientation-oracle`; history and the remaining package oracle retain their prior pin.

`context --for orient` selects hierarchy, blockers and activity at standard depth,
with a 3000-token declaration. `context --for handoff` selects activity, progress
and blockers at deep depth, with a 2200-token declaration. Explicit `--depth`
or repeated `--section` controls override section defaults. `--activity-limit`
controls retained history rows. The SDK exposes the same controls through
`ContextOptions` and `Workspace::context`.

The intents derive row ceilings from their token budgets, cap larger requested
limits and disclose the original request, effective limit and recovery budget.
Smaller explicit limits bind. Intent estimates measure compact JSON; the final
read-output ceiling measures the requested JSON or TOON wire representation.
An infeasible declaration returns an omission receipt, rather than an approximate
packet. Budgets below 256 receive the published usage refusal. Numeric parsing
retains the distinction between decimal intent controls and the underlying
context numeric domain.
Option normalization uses ECMAScript whitespace: BOM is trimmed and NEL is
retained, so neither valid controls nor invalid numeric input change meaning.

Deeper sections retain high-level and low-level focus, dependency blockers,
agenda summaries and matching-corpus counters. Current activity uses the
published timestamp comparison before the captured clock, including its lexical
fallback for nonempty invalid timestamps and retained deleted-item streams.
Test-health summaries cover all matching active items independently of focus
packing, and disclose that scope. TOON automatic tables select the smallest
expanded, ordinary-table or compact list/null-column representation.

Dependency context packets use the current required-root-evidence-first ordering:
prerequisites and ancestors precede optional neighbors, node selection reserves
space for root edges, and edge selection gives those edges priority. Truncated
packets declare the omitted relationship bounds, and continuation fingerprints
identify this ordering contract. Tree and graph reads retain their own contracts.
Recovery still checks every candidate required by the oracle's bounded search.
Query-local memoization avoids repeating producer work and retains at most two
MiB of serialized producer packets; the existing five-second debug-process guard
remains unchanged.

Native reads execute no extension code and write no tracker state. Core intents
refuse workspace `context-intents.json` declarations and installed extension
directories with `unsupported_native_context`, even when extension diagnostics
are omitted. Unknown intents receive the same typed native boundary. Ordinary
non-intent reads retain their established extension policy. Nonweekly recurrence,
non-UTC schedules, 10000-item memory rollups and uncertified derived ranking state
continue to refuse explicitly.

Remaining calendar, timezone, memory and derived recovery work is specified in
[pm-rust-context-calendar-memory](../.agents/pm/features/pm-rust-context-calendar-memory.toon).
Richer history, canonical output continuation, remaining filters and custom
intent declarations are specified in
[pm-rust-context-history-output](../.agents/pm/features/pm-rust-context-history-output.toon).

Published recovery receipts are preserved byte-for-byte. Producer `--after`
continuations are native; generic `--output-cursor` and `--output-budget`
continuations require the published CLI and receive `unsupported_native_option`
when invoked natively. The reviewed-contract differential replays an actual
output cursor through the published CLI and checks the explicit native refusal.
It also retains the oracle's separate decimal intent and numeric producer
domains, repeated-section diagnostics, and receipt-only recovery defaults.

Run `just orientation-check` for the generated current-oracle differential
corpus. `node scripts/context-intents-revert.mts` copies text source into a
scratch directory and requires assertion failures after independently reverting
intent preparation, compact table behavior and the previous relationship packet
implementation. All nine new tests and the upgraded existing differentials are
covered. It leaves the working implementation untouched.
The packet revert uses a committed patch, so it does not depend on a moving
`origin/main` reference or remote availability.
The public-copy runner requires an opt-in marker on a scratch copy, compares stdout,
stderr and exit status, checks authoritative item/history hashes and records
alternating native/oracle cold-process latency samples. Installed packages and
rebuildable runtime caches are excluded from these core-tracker copies.
Both wall-clock time and the oracle's monotonic packing clock are fixed during
differentials, so its elapsed-time packing guard cannot vary with host scheduling.
Latency still measures real elapsed time outside each fresh process.

## Generated evidence

The focused current-oracle runs pass 378 context comparisons: 342 rich, empty
and wide intent/section cases, 20 Unicode control cases, and 16 reviewed-contract
cases. The relationship matrices pass 138 comparisons, including the expanded
40-case reference/edge-limit matrix and the unchanged 12-case budget-floor
resource guard. Each comparison checks stdout, stderr and exit status over real
non-git trackers. Published output-cursor replay and its native typed refusal
are additional boundary assertions, not counted as native cursor parity.

The local positive unit run passes 398 library and 42 binary tests. The complete
orientation run initially passed 34 of 35 tests; its concurrent debug timing
guard exceeded five seconds on the shared host. The final focused guard passes
all 12 cases serially, without relaxing the threshold. Full cross-platform
release and coverage gates run in CI; these local results do not claim coverage.

## Public-copy measurements

The release binary passes the following exact comparisons against the locked
oracle on copies of the public pm-vcs, pm-rl and pm-context trackers. Authoritative
item/history/configuration hashes remain unchanged in every copy.

| Tracker | Context | Relationship | Total |
| --- | ---: | ---: | ---: |
| pm-vcs | 62 | 37 | 99 |
| pm-rl | 62 | 39 | 101 |
| pm-context | 60 | 11 | 71 |

Median fresh-process latency in milliseconds, **native / oracle**. Each command
has five alternating samples per runtime; filesystem caches were not flushed.
The fixed clocks make packing deterministic. These shared-host observations are
not latency guarantees. The receipt includes every sample, p95 and binary hash.

| Tracker | Brief | Deep | Orient | Handoff | Relationship |
| --- | ---: | ---: | ---: | ---: | ---: |
| pm-vcs | 29.6 / 692.1 | 181.5 / 1169.9 | 169.0 / 882.5 | 186.6 / 955.1 | 21.5 / 601.6 |
| pm-rl | 31.3 / 883.7 | 222.0 / 1194.6 | 242.2 / 1471.0 | 258.0 / 1434.7 | 25.3 / 891.1 |
| pm-context | 16.8 / 865.4 | 61.0 / 863.5 | 97.5 / 1231.0 | 64.4 / 870.9 | 10.7 / 684.9 |

[Machine-readable receipt](benchmarks/native-context-intents-2026-11-06.json).

## Revert evidence

`just orientation-revert-proof` requires behavioral assertion failures, not
compilation failures. Mutations run in a disposable source copy and are restored
between checks. The completed run requires 15 assertion failures across 13
mutation commands and 12 distinct tests, covering all nine new tests.

| Reverted behavior | Test that fails |
| --- | --- |
| Core intent preparation | `core_intent_boundaries_and_budget_receipts` |
| Core intent preparation | `context_intents_and_sections_match_current_oracle` |
| Core intent preparation | `context_intents_empty_tracker_match_current_oracle` |
| Core intent preparation | `context_intents_wire_budget_match_current_oracle` |
| Core intent preparation | `context_reviewed_contracts_match_current_oracle` |
| Current compact table selection | `current_context_tables_keep_ambiguous_cells_lossless` |
| Public current-renderer selection | `current_context_tables_keep_ambiguous_cells_lossless` |
| Configured activity cap | `configured_sections_preserve_activity_errors_and_stale_order` |
| Activity timestamp comparison | `context_reviewed_contracts_match_current_oracle` |
| Empty activity timestamp guard | `section_activity_storage_and_ordering` |
| Omitted-intent estimates and selected wire costs | `context_reviewed_contracts_match_current_oracle` |
| ECMAScript option normalization | `context_control_whitespace_matches_current_oracle` |
| Producer memoization | `packet_retry_cache_bounds_work_and_storage` |
| Context/dependency diagnostic scope | `current_orientation_diagnostics_preserve_other_commands` |
| Current relationship packet implementation | `relationship_packets_match_oracle` |
