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

Default extension diagnostics cover minimal manifests with `name`, `version`,
`entry` and optional descriptive `description`/`author` members. Configured
extension enable/disable policy and other manifest controls refuse with
`unsupported_native_context`; `--no-extension-health` explicitly omits that
diagnostic. Native reads never execute extension code or custom scorer hooks.
Declared `context_relevance` service overrides refuse even when extension health
is omitted; core ranking is the claimed scorer model.
Discovery skips ordinary files and symlinked extension directories, matching
the oracle's directory-entry policy.

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
Replay applies the oracle's prototype-path prohibition to both destinations and
copy/move sources, including `constructor/prototype`. A sealed-record regression
failed against the preceding verifier and passes with the matching guard.

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

The fourth review isolates context unit fixtures from ambient global extensions.
The public SDK captures the global root once, and an internal explicit-root entry
passes the same root to the scorer boundary and extension diagnostics. Fixtures
pass `None` or a synthetic root without mutating the process environment. A
synthetic global scorer is still refused with extension health enabled or omitted;
a minimal static manifest contributes diagnostics only when its root is supplied.
Restoring only the boundary's ambient-root lookup fails the new regression under
a synthetic global scorer. All 238 library tests pass with that scorer present.

The mixed-case dependency review retains the pinned oracle's behavior. Summary
counts normalize dangling identities, while full graph/tree counts preserve
separate display spellings from different holders. A fixture with two spellings
reports one missing node in summary and two in full output. Its 32 comparisons
cover tree/graph, summary/full, JSON/TOON, depth and node/edge limits; stored bytes
stay unchanged. Applying the suggested normalized graph keys fails this fixture
against the real oracle. Normalizing the projection here would break the parity
contract, so that suggestion is declined with the differential evidence.

List/search and orientation row compaction now share collection selection, path
receipts and prefix binary search. Both existing finalizers and their numeric
semantics are retained. This is a behavior-preserving extraction: reverting it
preserves output but restores duplication. At jscpd 4.0.5's unchanged defaults
(5 lines, 50 tokens), `src/read_output.rs` has zero clones with `--threshold 0`;
the preceding file fails that check with two clones and 26 duplicated lines.
Existing bounded-output unit and oracle comparisons validate both callers.
The clean fourth-review Linux gate passes 464 normal and 464 instrumented tests
across 20 suites each, with no failures or ignored tests. Coverage is
11,701/11,701 lines, 19,167/19,167 regions, 1,180/1,180 functions and
2,826/2,826 branches, all 100% at unchanged thresholds. The five PM-linked
commands separately pass 312 tests. Final-head platform and fresh-clone receipts
are recorded in the PR; the inherited full-health boundary below remains.


The second full review adds parser refusal checks for unknown short options,
unknown long options with values, and extra positional arguments after allowed
flags, across all three commands and both encodings. The rejected token comes
from Clap's error context; the fallback excludes supported options. History
validates the normalized stream filename before decoding the corpus or checking
the stream path. Twelve real-CLI comparisons match the oracle's complete invalid
filename refusal bytes for traversal, colon and backslash inputs, with an escaped
file both absent and present. Direct SDK acceptance also covers NUL and dot
identities and proves a malformed item cannot preempt invalid-ID validation.

Dependency index construction collects normalized corpus identities once and
reuses each scalar target key. A real 256-item tracker test counts normalization
calls in the test build and enforces at most 16 calls per item; restoring only
the repeated corpus scan violates this deterministic budget. The existing
dangling-parent and sentinel oracle comparisons continue to establish output
parity. This change is limited to the review's identity-set request; the separate
selective-read and latency follow-up remains deferred. Each new finding has a
single-fix behavioral revert proof, recorded with its command in PM comments.

The second-round PM-linked runner passes all five commands: orientation 17,
CLI dispatch 14, library 237, workflow contracts 18 and binary units 24, for
310 tests with zero failures or ignored tests. Automatic result recording is
disabled by project policy; manual receipts are committed in PM comments.

Tracker health has a separate boundary: the documented CI health command uses
`--check-only --require-merge-drivers --strict-exit`. A full
`health --strict-exit` also checks historical drift and reports inherited chain
mismatches on `pm-rust-bj7a` and `pm-rust-unih`. A fresh clone of the preceding
head reproduces both, and their item/history bytes are unchanged by this round.
They remain tracked in [pm-rust-b4yl](../.agents/pm/tasks/pm-rust-b4yl.toon);
no history was rewritten. Passing the CI health command does not certify those
older chains. The orientation feature's own append-only stream is separately
verified against the oracle.

The PR review round adds regressions for unrelated command arguments named
`context`, `history` or `deps`, non-Unicode Unix environment names and values,
and configured progress statuses without the active role. Command detection
uses the actual subcommand position; context resolves lossy OS strings without
panicking. Matching-item progress counts are restricted to the active set,
matching the oracle even when that set is smaller than the configured progress
set. Custom progress identities retain the oracle's `focus_row_states` receipt
when their classification cannot be inferred from the projected status, and
that receipt follows delivered rows through output compaction.

[Reviewer issue #93](https://github.com/unbraind/pm-rust/issues/93) adds ten
dangling-parent controls: open and terminal holders, existing and missing
parents, equivalent structured edges, missing blockers, absent references,
trimmed/mixed-case identities and both scalar-reference orders. Each runs tree,
graph and summary in JSON and TOON, with default bounds and depth zero: 120
real native/oracle comparisons. Missing parents are appended in oracle order
without duplicating an equivalent structured edge; existing scalar parents do
not introduce traversal edges. Authoritative item and history bytes remain
unchanged throughout.

An additional twelve real-CLI comparisons retain the pinned dependency
projection's `no-active-blocker` scalar as a missing `blocked_by` edge. This is
the oracle's traversal behavior, even though actionability treats that sentinel
as no active blocker. Tree, graph, summary, both encodings and depth zero agree;
the previous native suppression fails the new regression. Direct SDK context
acceptance separately verifies custom progress receipts and unchanged bytes.

Each of the four finding-specific regression tests fails when only its fix is
reverted. The additional custom-classification compaction test fails when only
the previous finalizer is restored. All five controls produce behavioral test
failures after rebuilding, and all fixed source bytes are restored exactly;
commands and negative/positive receipts are recorded in the feature's PM
comments. The original latency measurement remains the receipt for its recorded
binary; this review round does not implement the separate read-performance
follow-up.

The clean second-review `just release-check` passes with 462 normal and 462
instrumented tests across 20 suites each, with no failures or ignored tests.
Its complete crate source scope reaches 11,743/11,743 lines, 19,246/19,246
regions, 1,179/1,179 functions and 2,834/2,834 branches: 100% in all four
dimensions at unchanged thresholds. Fmt, pedantic deny-warnings Clippy,
private rustdoc, identity, audit and changelog checks pass. Audit retains the
existing allowed unmaintained `unsafe-libyaml` advisory. Source and test digests
remain identical throughout both passes. Final-head platform CI and fresh-clone
health receipts are recorded in the PR, with the full-health boundary above.

The initial review-head CI completed all normal OS jobs and the Windows
aggregate, but Linux/macOS aggregates hit their former 20-minute job limit
during the second full test pass. The aggregate budget is now 35 minutes on
every runner. Test scope, coverage thresholds, failure behavior and disabled
release automation are unchanged; the final-head rerun supplies the platform
acceptance evidence.

`tests/orientation_differential.rs` executes both real CLIs against disposable
trackers with a shared fixed clock and isolated agent environments. The oracle's
monotonic test clock is also fixed: its 25 ms packing guard otherwise changes
selected rows when the shared host deschedules the Node process. Native identity
packing is deterministic and completes the selection; scheduler-dependent oracle
packing timeout is outside the byte-parity claim. It compares
complete stdout, stderr and exit status. Private unit acceptance exercises
storage failures and the integrity, selection and traversal boundaries.
Extension discovery has a real oracle fixture for the minimal manifest and
explicit refusal/omission coverage for richer manifests.
Real-process history acceptance also covers strict verification success and
closed output pipes without changing the stored history.
Parser refusals and help preserve Clap's output streams and 0/2 statuses while
returning through `main` for normal process teardown. Six before/after parser
responses retain identical bytes. The preceding Windows aggregate exercised
the immediate-exit refusal successfully but could not retain its coverage;
the normal-return path permits finalization without conditional coverage code.

The complete 2,991-item public tracker copy also passed a clock-pinned context
comparison: 11,839 JSON bytes and 7,219 default TOON bytes matched exactly,
with empty stderr and successful exits. Both clocks use the same fixture
convention described above; an isolated synthetic author prevents inherited
agent feedback from changing the comparison. No tracker rows are filtered.

The concurrency fixture starts three independent reader threads, each issuing
four context/history/dependency reads, alongside four native updates to an unrelated
item. The target TOON and append-only JSONL bytes are identical before and after;
serial oracle reads then match both native results. Context captures one
authoritative corpus per invocation, verified with the existing real filesystem
scan observer. These readers acquire no writer lock, execute no recovery
journal, and do not claim a transactionally consistent snapshot across an
item being concurrently modified. Dependency mutation remains deferred.

Windows CI exposed the existing publisher's delete-before-rename gap: an
unrelated update briefly removed an item while an orientation reader captured
the corpus. Publication now calls `std::fs::rename` without unlinking the old
destination; Rust documents replacement of existing files on both platforms
([rename contract](https://doc.rust-lang.org/std/fs/fn.rename.html)). Writer
locks and the recovery journal are retained. A real-filesystem regression also
requires a failed publication to preserve the old destination. Reinstating the
previous Windows delete/rename sequence behavior makes that regression fail.

## Large-tracker latency receipt

Measured 2026-10-18 against a complete temporary copy of the public pm CLI
tracker: 2,991 items and 6,004 authoritative files. SHA-256 checks after the
run found zero changed authoritative files in either the original or its copy.
Only the copy was passed to either CLI.

Each cell below is cold-process wall time in milliseconds. There are 30 fresh
processes for every command/runtime pair (240 samples), alternating runtime
order each round. Filesystem caches and the oracle's derived indexes are warm
after one untimed validation of every command. p50 is the median; p95 is nearest
rank, the 29th sorted sample of 30. The search term is `history`; get uses one
existing task. Timed context uses real clocks; the separate byte comparison
above pins clocks.

| Command | Native p50 | Native p95 | Oracle p50 | Oracle p95 |
| --- | ---: | ---: | ---: | ---: |
| `context --limit 10` | 2592.8 | 5831.6 | 3355.3 | 8429.7 |
| `list --status open --limit 20` | 2116.0 | 4653.3 | 1269.9 | 3516.6 |
| `search history --limit 10` | 2200.0 | 4775.4 | 4177.7 | 8293.2 |
| `get <id>` | 1864.8 | 3713.8 | 1118.5 | 2862.6 |

Hardware: four logical CPUs, Intel Core i5-7400, shared Linux host. The harness
ran under `nice -n 0`, retaining the inherited effective nice 15. Concurrent
package validation remained active. Start load averages (1/5/15 minutes):
18.65/12.19/14.54; end: 21.37/18.95/16.86. The sampled one-minute range was
13.84–25.64. Measurement ran from 07:56:21 to 08:06:32 UTC.
Native used `cargo build --locked --release` with Rust 1.90.0, source
`309df2e95c201a9804e903572301fbecee0e4908`. The oracle was the locked
2026.10.14 package on Node v24.19.0. Agent home, global state and author were
isolated. The binary fingerprint stayed constant throughout the run.

Native context and search have lower measured p50/p95. List and get are slower
than the oracle's warm indexed reads; selective lookup/index work is tracked in
[pm-rust-wdj4](../.agents/pm/tasks/pm-rust-wdj4.toon). These observations include
shared-host contention. The complete [sample CSV](benchmarks/native-orientation-2026-10-18.csv)
and [summary, load and integrity receipt](benchmarks/native-orientation-2026-10-18.json)
retain the underlying measurements.


## Acceptance receipt

At implementation head `309df2e95c201a9804e903572301fbecee0e4908`, the isolated
Linux `RUST_TEST_THREADS=8 just release-check` passed: 449 normal tests and
449 branch-instrumented tests, 20 suites per run, zero failures and zero ignored
tests. The updated CLI boundary suite also passed 11 tests independently.
Fmt, pedantic Clippy with warnings denied, private-item rustdoc, identity audit,
security audit and the changelog check passed at unchanged thresholds.

| Coverage dimension | Covered / measured | Percent |
| --- | ---: | ---: |
| Lines | 11,656 / 11,656 | 100% |
| Regions | 19,104 / 19,104 | 100% |
| Functions | 1,170 / 1,170 | 100% |
| Branches | 2,830 / 2,830 | 100% |

The documented CI health scope (`--check-only --require-merge-drivers
--strict-exit`) passed all eight checks with four advisory
findings retained. The linked `pm test` execution passed all 12 orientation
differentials. Project policy disables automatic test-result tracking, so its
persistent result receipt is an item comment rather than an automatic run row.
CI normal test counts are Linux 449, macOS 445 and Windows 441, with zero
failures or ignored tests. All three platform aggregates passed the same four
100% coverage dimensions; security checks passed. Platform counts differ with
conditional filesystem and process tests.

The [machine-readable gate receipt](benchmarks/native-orientation-gates-2026-10-18.json)
records the measured source scope, platform counts and the preceding Windows
parser-exit regression. Six behavioral revert controls are recorded in the
feature comments. Release automation remains disabled.
