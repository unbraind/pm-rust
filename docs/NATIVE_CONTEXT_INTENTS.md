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

Deeper sections retain high-level and low-level focus, dependency blockers,
agenda summaries and matching-corpus counters. Current activity is bounded
strictly before the captured clock, including retained deleted-item streams.
Test-health summaries cover all matching active items independently of focus
packing, and disclose that scope. TOON automatic tables select the smallest
expanded, ordinary-table or compact list/null-column representation.

Dependency context packets use the current required-root-evidence-first ordering:
prerequisites and ancestors precede optional neighbors, node selection reserves
space for root edges, and edge selection gives those edges priority. Truncated
packets declare the omitted relationship bounds, and continuation fingerprints
identify this ordering contract. Tree and graph reads retain their own contracts.

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

Run `just orientation-check` for the generated current-oracle differential
corpus. `node scripts/context-intents-revert.mts` copies text source into a
scratch directory and requires assertion failures after independently reverting
intent preparation, compact table behavior and the previous relationship packet
implementation. All five new tests and the upgraded packet differential are
covered. It leaves the working implementation untouched.
The public-copy runner requires an opt-in marker on a scratch copy, compares stdout,
stderr and exit status, checks authoritative item/history hashes and records
alternating native/oracle cold-process latency samples. Installed packages and
rebuildable runtime caches are excluded from these core-tracker copies.
Both wall-clock time and the oracle's monotonic packing clock are fixed during
differentials, so its elapsed-time packing guard cannot vary with host scheduling.
Latency still measures real elapsed time outside each fresh process.

## Revert evidence

`just orientation-revert-proof` requires behavioral assertion failures, not
compilation failures. Mutations run in a disposable source copy and are restored
between checks.

| Reverted behavior | Test that fails |
| --- | --- |
| Core intent preparation | `core_intent_boundaries_and_budget_receipts` |
| Core intent preparation | `context_intents_and_sections_match_current_oracle` |
| Core intent preparation | `context_intents_empty_tracker_match_current_oracle` |
| Core intent preparation | `context_intents_wire_budget_match_current_oracle` |
| Current compact table selection | `current_context_tables_keep_ambiguous_cells_lossless` |
| Current relationship packet implementation | `relationship_packets_match_oracle` |
