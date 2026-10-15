# Mixed-status native list pagination and TOON output

Owner: [pm-rust-ufzy](https://github.com/unbraind/pm-rust/blob/main/.agents/pm/tasks/pm-rust-ufzy.toon).
Configured-registry follow-up:
[pm-rust-cbaq](https://github.com/unbraind/pm-rust/blob/main/.agents/pm/tasks/pm-rust-cbaq.toon).

The oracle is exactly `@unbrained/pm-cli@2026.10.14`, installed through
`npm ci --prefix tests/oracle --ignore-scripts` from the committed lockfile.
The pin is unchanged. Both real CLIs read the same disposable tracker under a
fixed read clock. Tests compare complete stdout/stderr bytes and exit status;
no envelope fields, cursor paths, timestamps or budget receipts are stripped.

The 270-item fixture contains 45 items in each built-in lifecycle status,
Task/Issue types, priority ties, tag groups, scalar and dependency parents,
missing/present deadlines and unresolved prerequisites. Producer walks compare
the ordered union with an independent unbounded oracle read and reject every
repeated ID. JSON and TOON cover every status, combined status/type/parent/tag
selection and all six sort fields in both directions, plus timestamp aliases.
Unknown tag warnings and query fingerprints remain part of the comparison.

Selection and ordering happen before projection and slicing. Descending order
reverses the default tie-break order too. Parent filtering accepts scalar parents
and built-in hierarchy dependency edges, including their aliases. Blocked
selection includes explicit blocked status and active items with unresolved
prerequisites. Full read projections use the oracle's normalization key order,
which places linked collections before optional scalar metadata; canonical
storage/history ordering remains separate.

TOON uses compact empty-field handling and complete brief-list summaries,
quoted object scalars, and smaller union-column tables with JSON-column and
absent-cell receipts when these beat expanded rows. Numeric cells use JavaScript
number spelling. Read projections coerce unsafe integers through JavaScript
Number before hashing or rendering; storage and history hashing remain separate. Budget measurement uses the actual selected wire encoding,
including its newline: `ceil(UTF-8 bytes / 4)`. Defaults are 4,000 tokens for TOON
and 6,000 for JSON. Intent budgets still use their separate compact-JSON policy.
Tests cover amount caps, default/explicit/infeasible budgets, full projections,
triage and complete snapshot walks without missing or repeated rows.

## Cursor semantics and pinned-oracle gaps

The oracle's producer flag is `--after`; `--cursor` is a native alias. The pinned
oracle rejects `pm list --cursor` as `unknown_option`. Native alias output is
compared with the oracle's `--after` output using the same cursor.

Producer cursors bind the tracker root and query, including filters and ordering.
They contain `version`, `fingerprint`, `after_id` and `after_index`. They tolerate
row edits and use the original positional fallback when an identity disappears.
They do not promise a stable snapshot across edits. Subsequent pages retain the
oracle's compact `continuation_contract` reference envelope.

`--output-cursor` binds original projected rows and refuses changed snapshots.
Tests cover edits between invocations, another tracker, changed filters/sort,
malformed base64 and unsupported cursor versions, preserving exact diagnostics
and exit codes. Snapshot cursors with identical projected rows can be replayed
across trackers, as allowed by the pinned row-collection contract.

The pinned TOON codec refuses a string containing a split UTF-16 surrogate during
budget compaction. The native implementation preserves its raw diagnostic and
exit 1; JSON preserves the escaped code unit. This oracle limitation is tested
with valid source strings that split at the compaction boundary.

## Evidence

The focused scenarios are in `tests/pagination_differential.rs`:

- `mixed_status_filtered_walks_match_both_wire_encodings`
- `toon_budget_envelopes_and_snapshot_walks_match`
- `mixed_tracker_cursor_refusals_and_native_cursor_alias`

Unit contracts exercise invalid ordering, inverse/dangling hierarchy edges,
sparse-table and empty-value handling, quoted number-like strings and writer
failures. Existing pagination, UTF-16 JSON and unknown-key permutation tests
continue to run. Native production code requires no Node runtime.

Seven independent native-only negative controls each executed its test and
failed behaviorally with Cargo exit 101 against the unchanged oracle. They
restore storage ordering, JSON-only budget measurement, the missing cursor
alias, unchecked ordering, scalar-only hierarchy indexing, redundant brief
metadata and ignored flush failures respectively. All mutations were restored
in the disposable source copy. The package worktree stayed unchanged during
these controls. Exact target/test names and final gate counts are recorded in
the owner item's comments.
The task remains in progress for orchestrator verification. Configured schemas,
custom hierarchy registries, status/type CSV selectors, tree mode, output
sessions and extension hooks remain outside this slice. Legacy SDK `list` and
unbounded envelopes retain their previous interfaces.
