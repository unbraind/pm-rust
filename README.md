# pm-rust

`pm-rust` is an independent, Rust-native implementation of the public
[`pm`](https://github.com/unbraind/pm-cli) workspace contracts for agents and
native applications.

The project is pre-release. Its current delivery slice reads workspaces and
creates, updates, comments on, closes, claims, and releases canonical items
against the published `pm` 2026.10.19 on-disk contract. Production code, tests, benchmarks,
and build tooling are Rust; the distributed binary does not require Node.js,
Bun, JavaScript, or TypeScript.

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

List pagination uses the pinned 2026.10.19 oracle in both JSON and default TOON.
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

Since pm-rust has no root `package.json`, changelog and release-note targets are
exposed via a [`justfile`](justfile):

```bash
just changelog          # prepend new release section to CHANGELOG.md
just changelog-full     # rebuild full changelog from all release tags
just changelog-check    # verify committed changelog matches generated (CI gate)
just release-notes      # print release notes for the current release window
just release-check      # aggregate release gate (fmt, clippy, docs, tests, coverage, audit, changelog)
```

The changelog heading date is fixed (not clock-derived) so the gate produces
identical output across all timezones.

Work is managed in this repository with the latest `pm` CLI under
`.agents/pm/`. The ecosystem-level lineage is the private companion item
`pm-cli-website-204t`.

## Development

The published parity oracle is pinned to exactly 2026.10.19 in
`tests/oracle/package.json` and installed from its committed `package-lock.json`.
Both local runs and every CI matrix job use `npm ci`; the lock records the
registry tarball and integrity, including the bundled runtime closure. This
avoids the transitive range drift described in
[pm-cli#1417](https://github.com/unbraind/pm-cli/issues/1417).
Dependabot tracks this manifest; oracle upgrades must also update the native
compatibility version and tooling contracts. Changelog generation keeps its
separate exact pm-changelog/CLI pairing.

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
