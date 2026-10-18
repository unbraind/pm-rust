# Native agent orientation

The published oracle is locked by `tests/oracle/package.json` and its lockfile.
This slice belongs to [pm-rust-u4al](../.agents/pm/epics/pm-rust-u4al.toon), tracked
in [pm-rust-xb3s](../.agents/pm/features/pm-rust-xb3s.toon). Publication and
release automation remain disabled.

## Brief context

`context` emits the default brief depth in TOON or `--json`. `--limit` bounds
the ranked producer page; absent a limit, the oracle caps each focus group at
ten items. `--token-budget` controls ranked identity packing and the complete
rendered response. The result retains `high_level`, `low_level`,
`blocked_fallback`, matching-corpus counters and the ten omitted section groups.
Blocked fallback is used when no active-status candidates exist. Active work
with unresolved dependencies retains its blocking receipt.

Structural ordering precedes weighted relevance scoring. Recency comes from
substantive history, a dated release cohort, or creation time. Activity density,
graph degree, claim state, priority, risk, deadlines, knowledge and author
signals contribute to ranking. Identity packing uses marginal value per token
and the context profile's redundancy penalty. Focus projection and inherited
tag elision match the published read surface. Future deadlines, reminders and
nonrecurring and weekly UTC scheduled events participate in the agenda, including
compact references to focus items.

Supported controls: `--limit` / `--max-items`, `--token-budget` /
`--token_budget`, `--depth brief`, `--no-tags`, `--no-extension-health`, and
`--json`. A hidden native `--timestamp` supplies the deterministic read clock
used by conformance fixtures; it is excluded from published error recovery.

Deferred context flags: `--for`, `--date`, `--from`, `--to`, `--past`, `--type`,
`--tag`, `--priority`, `--assignee`, `--assignee-filter` / `--assignee_filter`,
`--sprint`, `--release`, `--parent`, `--after`, `--format`, `--fields`,
`--section`, `--activity-limit`, `--stale-threshold`, and
`--explain-ranking` / `--explain_ranking`. Depths `standard`, `deep` and `full`
are deferred. Workspace-memory rollups at 10,000 items, non-weekly recurrence and
non-UTC scheduled events are explicit native context refusals. Invalid or stale
derived ranking snapshots also refuse explicitly. Semantic claim affinity and
trusted delivered-and-touched feedback contribute to native ranking; required
claim identities enter packing before optional candidates. Plain authoritative
reads do not write cache or usage state.

## History

`history <id>` defaults to compact timeline rows: record index, time,
operation, author, patch count and changed fields. `--full` retains complete
immutable entries and their original member order. `--limit` selects the latest
records; zero selects none and fractional values use the oracle's floor rule.
`--json` changes encoding. Compacted streams retain version-addressing evidence.

The pinned oracle does not verify a default history read. `--verify` checks
immutable record hashes and sequential document replay, then compares the
current item with the latest history hash. `--verify --strict-exit` (also
`--fail-on-warn`) renders the same report and exits 1 when verification fails;
without the strict flag, a failed verification is a successful read containing
`verification.ok: false`. Tests cover changed immutable authors, broken hashes,
malformed patches, empty streams and current-state drift. History is never
repaired, re-anchored, truncated or rewritten by these commands.

Supported controls: `--limit`, `--compact`, `--full`, `--verify`,
`--strict-exit` / `--fail-on-warn`, and `--json`.
Deferred flags: `--provenance`, `--provenance-summary`, `--harness`,
`--agent-instance`, `--provenance-filter`, `--diff`, `--field`, and `--format`.
History maintenance subcommands remain outside this read surface.
If an item has no stream, the native read returns an empty timeline under the
default policy without creating a stream. The oracle may create one under its
`auto_create` policy. Both refuse under `history.missing_stream: strict_error`.

## Dependencies

`deps <id>` lists outgoing relationships as a bounded tree; `--format graph`
returns deduplicated nodes and directed edges. Identity lookup ignores case and
surrounding whitespace, while output preserves the oracle's requested root
spelling and stored target identities. Cycles and missing references remain
visible. `--collapse repeated`, `--max-depth`, `--node-limit`, `--edge-limit`
and `--token-budget` bound expansion and disclose truncation. `--summary`
returns graph counts; `--full` explicitly requests the complete projection.
Both default TOON and `--json` retain projection and omission receipts.

Deferred controls: `--format context`, `--cursor`, `--direction`, and `--kind`.
`--add` and `--remove` are deferred: the pinned oracle exposes relationship
mutations through its mutation surface rather than these dependency traversal
options. This PR adds no dependency writer or alternate journal path.

All three commands defer canonical read-output controls (`--output-include`,
`--output-limit`, `--output-budget`, `--output-format`, `--output-session`,
`--output-cursor`, `--output-row-contract`). Other deferred oracle global flags
are `--lean`, `--token-accounting`, `--quiet`, `--no-changed-fields`,
`--full-changed-fields`, `--id-only`, `--pm-path`, `--path`, `--no-extensions`,
`--no-pager`, `--explain`, `--all`, `--profile`, and `--author`. Use the existing
native `--workspace` for discovery. Claimed flags follow the canonical command;
the oracle's `ctx` and `item deps` spellings are deferred.
Numeric controls are restricted to the JavaScript safe integer range; larger
values refuse with `unsupported_native_value`. History still floors supported
fractional limits. Unsupported options fail closed using the published problem-envelope shape,
with code `unsupported_native_option`, exit 2, and a recovery instruction to
use the published CLI. This code distinguishes a native boundary from an
oracle-invalid value. Native-only data boundaries use
`unsupported_native_context`. Supported invalid values and missing identities
are compared against the oracle's complete refusal bytes.

Deferred work is tracked in
[pm-rust-s6j4](../.agents/pm/tasks/pm-rust-s6j4.toon).

## Evidence

`tests/orientation_differential.rs` executes both real CLIs against disposable
trackers with a shared fixed clock and isolated agent environments. The oracle's
monotonic test clock is also fixed: its 25 ms packing guard otherwise changes
selected rows when the shared host deschedules the Node process. Native identity
packing is deterministic and completes the selection; scheduler-dependent oracle
packing timeout is outside the byte-parity claim. It compares
complete stdout, stderr and exit status. Private unit acceptance exercises
storage failures and the integrity, selection and traversal boundaries.

The concurrency fixture starts three independent reader threads, each issuing
four history/dependency reads, alongside four native updates to an unrelated
item. The target TOON and append-only JSONL bytes are identical before and after;
serial oracle reads then match both native results. Context captures one
authoritative corpus per invocation, verified with the existing real filesystem
scan observer. These readers acquire no writer lock, execute no recovery
journal, and do not claim a transactionally consistent snapshot across an
item being concurrently modified. Dependency mutation remains deferred.

Latency and final gate receipts are recorded after the implementation passes
its conformance checks.
