# pm-rust

`pm-rust` is an independent, Rust-native implementation of the public
[`pm`](https://github.com/unbraind/pm-cli) workspace contracts for agents and
native applications.

The project is pre-release. Its current delivery slice reads workspaces and
creates, updates, comments on, closes, claims, and releases canonical items
against the published `pm` 2026.10.23 on-disk contract. Production code, tests, benchmarks,
and build tooling use Rust and typed test harnesses; the distributed binary does not require Node.js,
Bun, JavaScript, or TypeScript.

See [large tracker hot-path benchmarks](docs/NATIVE_HOT_PATHS.md) for `just bench`,
the generated 1k/10k/50k matrix, peak RSS, byte comparisons and known parity gaps.

See the [command and storage parity matrix](docs/CLI_PARITY_2026_10_2.md) and
[2026.10.7 pagination contract](docs/LIST_PAGINATION_2026_10_5.md) for supported
read flags and remaining gaps. The [explicit ownership contract](docs/OWNERSHIP_PARITY_2026_10_7.md)
details claim/release parity and its tested boundaries. The
[native selection and lifecycle contract](docs/NATIVE_SELECTION_LIFECYCLE_2026_10_13.md)
covers claim-next, start, pause and session principals. The
[2026.10.14 closure and safety contract](docs/NATIVE_PARITY_2026_10_14.md)
details closure fields, lock refusal, cyclic readiness, lineage and legacy cursors.

| Read surface | Native parity |
| --- | --- |
| JSON and TOON list / `Workspace::list_page` | Producer pages, offsets, query fingerprints, continuation envelopes, empty/final pages |
| Bounded JSON and TOON list | Output limits, budget compaction and omission receipts, snapshot continuations and stale refusal |
| Triage list | Built-in projection, token-budget ceilings, intent receipts and rebased cursors |
| Legacy SDK list methods | Existing native and unbounded envelopes retained |

Parity uses both real CLIs over shared fixtures. Every advertised producer cursor
resumes after the last emitted row, including when `--output-limit` trims a
`--limit` page ([pm-cli#1420](https://github.com/unbraind/pm-cli/issues/1420),
[pm-rust-8qbo](.agents/pm/tasks/pm-rust-8qbo.toon)). Explicit terminal output caps
still disclose withheld rows without inventing a cursor; lift that cap at the
same incoming boundary to finish the tail. `--after` accepts workspace
mutations, while `--output-cursor` refuses changed row snapshots. Combined
amount and budget ceilings bind the original uncapped snapshot, so unchanged
output-cursor replay works and deleted-identity fallback uses the emitted
producer position.
String compaction preserves split UTF-16 code units as escaped JSON, and full
list projections preserve stored extra metadata key order. Generated differential
checks and independent revert proofs are documented in
[the output parity receipt](docs/OUTPUT_PARITY_2026_10_7.md).

See the [October 21 parity contract](docs/NATIVE_PARITY_2026_10_21.md) for lock-owner
recovery, strict mutation admission, ordering and cursor changes.

Native item mutations recover pending journals across operation names before
applying later edits. See [journal recovery](docs/CROSS_OPERATION_JOURNAL_RECOVERY.md)
for crash-state regressions and operator conflict remedies.

## Explain native query cost

Run `pm-rust explain list` (TOON) or `pm-rust explain list --full --json` before
requesting a large result. The native SDK exposes `Workspace::explain_list` with
typed requests, bounds and refusals. Plans disclose candidate files, source I/O
and actual projected decoder-input work; output bytes, heap use and time remain
unknown. Planning itself scans bounded source bytes and performs zero mutations.
The bounds require unchanged files and do not certify valid items or reserve state.

See the [contract and remedies](docs/QUERY_COST.md) and
[SDK example](examples/query_cost.rs). Brief projection reduces decoder work on
body-heavy trackers; output/page limits alone do not reduce source scanning.

## Embeddable bounded read sessions

`Workspace::read_session` freezes a private disk-backed snapshot, strictly decodes
all items and checks unique identities before serving any context. Native hosts
pull `SessionPage` values with backpressure; no Node, Bun, Git or background worker
is required. The session reuses canonical list pagination, lifecycle registries,
output budgeting and lossless JSON/TOON codecs. Its receipt certifies the admitted
snapshot and completion of **brief projected context**, not full-field delivery.

```sh
cargo run -- read-session --memory-bytes 536870912 --page-items 20 --output-tokens 4000 --json
cargo run --example read_session -- path/to/project
```

JSON emits one compact `{output,receipt}` frame per line. Default TOON emits a
receipt and canonical page followed by `---`. An optional `--cancel-file` marker
requests cancellation before admission or between pages; removing it allows a new
CLI invocation. SDK callers can share `SessionCancellation` across threads or call
`ReadSession::cancel()` for immediate cleanup while idle. Drop and terminal pulls
attempt snapshot cleanup; explicit calls report storage failures. See the [embedding contract](docs/BOUNDED_READ_SESSIONS.md) for
accounting limits, cancellation latency, integrity boundaries and typed remedies.

## Native incremental changes

`pm-rust changes [--cursor <opaque>] [--limit 20] [--output-bytes 65536] [--json]`
returns a bounded page of canonical mutation facts and a continuation cursor.
`changes::ChangeReader` and `LocalChangeAdapter` provide the same pull interface
for Rust consumers over the existing authorized adapter ABI. Per-stream prefix
cursors retain equal-time/backdated appends and refuse stale history explicitly.
No tracker writes, Node, Git, shared checkpoints or background polling are needed.
See [cursor semantics and resource bounds](docs/native-changes.md) and the
[SDK example](examples/changes.rs). The [validation report](docs/native-changes-validation.md)
records SDK comparisons, seeded sequences, revert proofs and public-data numbers.

## Without git

Incremental change pulls work on a plain tracker with no Git or Node executable.
Adapter compatibility checks work without a tracker or Git. Query cost explanations, read sessions and
workspace discovery, list/get/search/context/history/dependencies, item mutations,
ownership and linked artifacts work in plain directories. Concurrent writers share
the same tracker and retain every admitted item edit and append-only history entry.
The multi-process write contract tests 50 writers against one such tracker without
a Git executable. Native `context` refuses trackers with 10,000 or more items;
see the [large tracker limits and benchmark protocol](docs/NATIVE_HOT_PATHS.md).
Linked-test branch provenance is optional and inspects Git only when a repository
marker exists. GitHub synchronization and Git history tooling require a Git
repository and are outside the distributed native command slice.
Evidence recorders and the [receipt commit gate](docs/EVIDENCE.md) require Git
and a reachable origin; the crash corpus itself still runs in plain trackers.

## Crash-consistency differential corpus

The TypeScript SDK in [`scripts/crash-harness.mts`](scripts/crash-harness.mts)
and `just crash` compare real native processes with the locked pm-cli 2026.10.23
oracle. On Linux, strace injects SIGKILL at every declared per-task `rename` and
`fsync` entry ordinal observed in uninterrupted update/comment reference runs.
Each case starts after an acknowledged synthetic create, restarts through a
different operation and returns to the original one, and checks exact item/history bytes, immutable history
prefixes and recovery receipts. These commands work in plain directories without
a Git executable. The source tracker is read only; all mutations use disposable
non-git clones.

```bash
just oracle-install
cargo build --locked --bin pm-rust
just crash --native target/debug/pm-rust --artifacts .crash-failures --json
```

Default TOON and compact JSON contain bounded counts and per-case outcomes;
`--full --json` adds durable-byte and receipt hashes. Exit codes are 0 for full
conformance, 1 for measured failures, and 2 for typed setup/boundary refusals with
remedies. Failures retain a minimal synthetic tracker, the acknowledged/recovered
probe bytes and a replay manifest under the ignored artifact directory. Original
histories are never repaired or rewritten. A read-copy failure is independently
replayed on a synthetic tracker before retaining evidence.

See the [SDK, read-copy and replay example](docs/CRASH_HARNESS.md) for the declared
boundary scope and measured results. Process-kill evidence does not establish
power-loss durability. Different syscall ordinals across runtimes do not denote
the same semantic phase; the receipt reports each runtime's observed counts,
including zero `fsync` calls in the published implementation.

## Current native slice

```bash
cargo run -- --workspace /path/to/project list --status open
cargo run -- --workspace /path/to/project list --type Feature
cargo run -- list --json --limit 20 --output-budget unbounded
cargo run -- list --limit 20 --tag rust --sort updated --order desc
cargo run -- list --cursor "$NEXT_CURSOR" --limit 20 --output-budget unbounded
cargo run -- list --json --for triage --token-budget 1000
cargo run -- context --limit 10
cargo run -- context --depth deep --limit 10
cargo run -- context --limit 10 --token-budget 4000 --json
cargo run -- history pm-example --limit 10
cargo run -- history pm-example --verify --strict-exit --json
cargo run -- deps pm-example --json
cargo run -- search "native search" --limit 20 --json
cargo run -- search "Unicode café" --type Feature --status open --tag rust
cargo run -- search "native" --after "$NEXT_CURSOR" --output-budget unbounded
cargo run -- files pm-example --add path=src/lib.rs,note=Implementation --author agent
cargo run -- docs pm-example --add path=README.md --author agent
cargo run -- test pm-example --add 'command=cargo test,path=tests/example.rs' --author agent
cargo run -- files pm-example
cargo run -- test pm-example --remove-index 1 --author agent --json
cargo run -- --workspace /path/to/project list --json --output-budget unbounded --output-limit unbounded --all
cargo run -- --workspace /path/to/project get pm-example
cargo run -- --workspace /path/to/project create --id pm-example --title "Native item" --type Task --author agent
cargo run -- --workspace /path/to/project update pm-example --title "Renamed" --priority 1 --author agent
cargo run -- --workspace /path/to/project comment pm-example "Status note" --author agent
cargo run -- claim pm-example --author agent --json
cargo run -- release pm-example --author agent --json
cargo run -- --workspace /path/to/project close pm-example --reason "Done: shipped" --author agent --resolution "Fixed" --expected-result "Expected" --actual-result "Observed" --validate-close strict --json
```

List pagination uses the pinned 2026.10.23 oracle in both JSON and default TOON.
Configured lifecycle roles and declarative custom hierarchy kinds are resolved
before selection; see [configured list registries](docs/CONFIGURED_LIST_REGISTRIES.md).
Parent and tag filters and all six published sort fields run before slicing;
`--order desc` also reverses the default tie-break ordering. `--cursor` is a
native alias for the oracle's `--after` spelling. Producer cursors tolerate row
edits; snapshot-bound `--output-cursor` rejects changed projected rows. TOON
budgets measure rendered UTF-8 bytes, including table compaction; defaults are
4,000 tokens for TOON and 6,000 for JSON. See
[the mixed-status pagination receipt](docs/LIST_PAGINATION_2026_10_15.md) for the 270-item
walks and negative controls.

The library discovers `.agents/pm` from a workspace, nested directory, file, or
tracker root; strictly decodes TOON item documents; retains unknown package
fields; rejects merge markers and duplicate IDs; and returns deterministic JSON
projections. Create uses explicit IDs, per-item ownership locks, synced
same-directory atomic writes, a durable recovery journal, canonical history,
and exact post-document hashes. See
[the compatibility contract](docs/COMPATIBILITY.md).

Keyword search and the item `files`, `docs`, and `test` collection commands are
native. Search ranks weighted literal occurrences across item text, including
comments, notes, learnings, reminders, events, dependencies and plans; exact IDs,
title words, phrases and all-term coverage carry the oracle's bonuses. Collection
edits read the current item under its writer lock, skip no-ops, and append
canonical history through the existing recovery journal. See the
[search and collections contract](docs/NATIVE_SEARCH_COLLECTIONS.md) for tested
flags, selectors, refusal differences and concurrency evidence.

`context` at brief, standard, deep and full depth, append-only `history`, and dependency
tree/graph/context `deps` reads are native. See the [orientation contract](docs/NATIVE_ORIENTATION.md) for
supported flags, integrity behavior, explicit refusals and measured latency.

Rust formatting, strict Clippy (deny warnings, pedantic), complete
private-item rustdoc coverage, a native Rust identity-audit gate, and tests
run on Linux, macOS, and Windows. Each platform also gates the dependency
audit, generated changelog, strict `pm health`, the `release:check` aggregate
gate, and 100 percent line, region, function, and branch coverage.

Publication and automated releases remain disabled until a maintainer
explicitly approves publishing. The daily release workflow is present and
correct but cannot fire without the `PM_RELEASE_APPROVED` repository variable
being set to `true`.

### Identity audit gate

A native Rust identity-audit gate (`tests/identity_audit.rs`) scans every object
**in the local object store** — via `git cat-file --batch-all-objects`, so both
objects reachable from refs and any unreachable (dangling or orphaned) ones that
store holds — for author and committer identities, and fails closed on any
identity not listed in
[`.github/approved-git-identities.txt`](.github/approved-git-identities.txt).

**What CI can and cannot prove.** Workflow jobs check out with `fetch-depth: 0`
and `fetch-tags: true`, which fetches the complete *reachable* history — so in
CI the gate covers every commit on every ref and tag. It does **not** cover
objects that are unreachable on the server: a clone never transfers dangling
objects, so they cannot be in the runner's object store to scan. The
unreachable-object capability therefore protects a run against a repository that
actually holds such objects — a maintainer's local clone, or a forensic mirror —
and CI's guarantee is the narrower reachable one. Treat a green CI run as
evidence about refs, not about the whole upstream object database.

### Changelog and release tooling

The root `package.json` exposes `npm run release:check` and
`bun run release:check` aliases for the unchanged native gate. Changelog and
release-note targets remain available through the [`justfile`](justfile):

```bash
just changelog          # prepend new release section to CHANGELOG.md
just changelog-full     # rebuild full changelog from all release tags
just changelog-check    # verify committed changelog matches generated (CI gate)
just release-notes      # print release notes for the current release window
just release-check      # aggregate release gate (fmt, clippy, docs, tests, coverage, audit, changelog)
```

The changelog heading date is fixed (not clock-derived) so the gate produces
identical output across all timezones.

Runners without `just` can use `npm run changelog:full --prefix tests/oracle`
and append `-- --check` to verify the result. Both entry points use
`scripts/changelog.mts`, which reads the pins, fixed date, and item URL base
from the justfile. Gated issue sync and closeout use this npm entry point.

#### Without git

Regular tracker reads and writes work in plain directories. Changelog generation
from release tags needs Git history and a Git executable; the wrapper identifies
this prerequisite before invoking the generator.

Work is managed in this repository with the latest `pm` CLI under
`.agents/pm/`. The ecosystem-level lineage is the private companion item
`pm-cli-website-204t`.

## Development

The published parity oracle is pinned to exactly 2026.10.23 in
`tests/oracle/package.json` and installed from its committed `package-lock.json`.
Both local runs and every CI matrix job use `npm ci`; the lock records the
registry tarball and integrity, including the bundled runtime closure. This
avoids the transitive range drift described in
[pm-cli#1417](https://github.com/unbraind/pm-cli/issues/1417).
Dependabot tracks this manifest; oracle upgrades must also update the native
compatibility version and tooling contracts. Changelog generation keeps its
separate exact pm-changelog/CLI pairing. See the
[2026.10.23 behavior classification](docs/oracle-2026-10-23.md) for root evidence
packing, producer bounds and matching-population test health.

```bash
npm ci --prefix tests/oracle --ignore-scripts
export PM_NODE_CLI="$PWD/tests/oracle/node_modules/@unbrained/pm-cli"
export PM_RUST_REQUIRE_PUBLISHED_CLI=1
cargo test --locked --test pagination_differential

# Run the aggregate release gate (fmt, clippy, docs, tests, coverage, audit, changelog)
just release-check  # installs the lockfile oracle and selects it by default

# Individual gates
cargo fmt --all -- --check
cargo clippy --locked --all-targets --all-features -- -D warnings
RUSTDOCFLAGS='--document-private-items -D missing-docs' cargo doc --locked --all-features --no-deps
cargo test --locked --all-targets --all-features
cargo +nightly-2026-08-06 llvm-cov --locked --branch --all-targets --all-features --json --output-path coverage-branch.json
jq -e '.data[0].totals.lines.percent == 100 and .data[0].totals.functions.percent == 100 and .data[0].totals.regions.percent == 100 and .data[0].totals.branches.percent == 100' coverage-branch.json
cargo audit
node tests/oracle/node_modules/@unbrained/pm-cli/dist/cli.js merge install
node tests/oracle/node_modules/@unbrained/pm-cli/dist/cli.js health --check-only --require-merge-drivers --strict-exit
```

## License

MIT. See [LICENSE](LICENSE).

## Portable native adapter ABI

Rust embeddings can use an explicitly supplied `NativeAdapter` through
`native_adapter::NativeAdapterClient`. The `pm-native-adapter/1` contract
exchanges bounded UTF-8 JSON bytes, with typed get, paginated list, claim and
release requests. It requires neither Git nor Node. The caller supplies an
existing authorized backend transport; core SDK semantics and pm-web/MCP
transports retain their ownership.

The client checks the exact ABI, declared capability, arguments and frame
ceiling before dispatch. It makes one exchange, preserves canonical backend
output (including refusal, omission and mutation receipts), and never retries.
A successful response can describe a skipped claim or a peer's success; inspect
the canonical receipt before proceeding. `AdapterRefusal.dispatched` distinguishes
preflight refusal from an uncertain dispatched outcome. Reconcile authoritative
receipts before retrying an interrupted mutation.

`pm-rust adapter-check [--json]` reads a transcript from stdin and emits a compact
TOON or JSON compatibility receipt. It works without a tracker. It validates
recorded responses supplied as exact `reply_bytes` arrays without replaying them; `dispatched: false` and
`receipt_scope: abi_compatibility_only` are explicit. It does not certify backend
authorization, durable writes or corpus completeness.

```sh
pm-rust adapter-check --json <<'JSON'
{"manifest":{"abi":"pm-native-adapter/1","capabilities":["get"],"max_frame_bytes":1048576},"request":{"abi":"pm-native-adapter/1","request_id":"read-1","operation":{"action":"get","id":"demo-1"}}}
JSON
```

See the [ABI and adapter obligations](docs/native-adapter-abi.md) and the
[compilable read adapter example](examples/native_adapter.rs). Build the example
with `cargo build --locked --example native_adapter`, then pass an explicitly
authorized tracker directory as its argument and one `AdapterRequest` on stdin.
Replace its `exchange` implementation with the existing authorized transport
when embedding against a remote backend. The example grants only get and list.

The expanded crash corpus includes history-path `openat` cuts and native runtime
journals. Historical same-operation receipts do not establish complete recovery
conformance; the locked published oracle still has measured history-open failures.

The repository check `node scripts/crash-history-prefix.mts` requires Git and
compares every changed history stream with `origin/main` as exact bytes. Main
history must remain a prefix; new item streams are permitted. Tracker commands
and `just crash` continue to work in plain directories without Git.
