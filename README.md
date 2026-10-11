# pm-rust

`pm-rust` is an independent, Rust-native implementation of the public
[`pm`](https://github.com/unbraind/pm-cli) workspace contracts for agents and
native applications.

The project is pre-release. Its current delivery slice reads workspaces and
creates, updates, comments on, and closes canonical items against the
published `pm` 2026.10.9 on-disk contract. Production code, tests, benchmarks,
and build tooling are Rust; the distributed binary does not require Node.js,
Bun, JavaScript, or TypeScript.

See the [command and storage parity matrix](docs/CLI_PARITY_2026_10_2.md) and
[2026.10.7 pagination contract](docs/LIST_PAGINATION_2026_10_5.md) for supported
read flags and remaining gaps.

| Read surface | Native parity |
| --- | --- |
| JSON list / `Workspace::list_page` | Producer pages, offsets, query fingerprints, continuation envelopes, empty/final pages |
| Bounded JSON list | Output limits, budget compaction and omission receipts, snapshot continuations and stale refusal |
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
String compaction that splits a Unicode surrogate pair remains a known gap,
tracked in [pm-rust-8hkb](https://github.com/unbraind/pm-rust/blob/main/.agents/pm/issues/pm-rust-8hkb.toon).

## Current native slice

```bash
cargo run -- --workspace /path/to/project list --status open
cargo run -- --workspace /path/to/project list --type Feature
cargo run -- list --json --limit 20 --output-budget unbounded
cargo run -- list --json --for triage --token-budget 1000
cargo run -- --workspace /path/to/project list --json --output-budget unbounded --output-limit unbounded --all
cargo run -- --workspace /path/to/project get pm-example
cargo run -- --workspace /path/to/project create --id pm-example --title "Native item" --type Task --author agent
cargo run -- --workspace /path/to/project update pm-example --title "Renamed" --priority 1 --author agent
cargo run -- --workspace /path/to/project comment pm-example "Status note" --author agent
cargo run -- --workspace /path/to/project close pm-example --reason "Done: shipped" --author agent
```

The library discovers `.agents/pm` from a workspace, nested directory, file, or
tracker root; strictly decodes TOON item documents; retains unknown package
fields; rejects merge markers and duplicate IDs; and returns deterministic JSON
projections. Create uses explicit IDs, per-item ownership locks, synced
same-directory atomic writes, a durable recovery journal, canonical history,
and exact post-document hashes. See
[the compatibility contract](docs/COMPATIBILITY.md).

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

The published parity oracle is pinned to exactly 2026.10.9 in
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
