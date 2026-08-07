# pm-rust

`pm-rust` is an independent, Rust-native implementation of the public
[`pm`](https://github.com/unbraind/pm-cli) workspace contracts for agents and
native applications.

The project is pre-release. Its current delivery slice reads workspaces and
creates canonical items against the published `pm` 2026.8.6 on-disk contract.
Production code, tests, benchmarks, and build tooling are Rust; the distributed
binary does not require Node.js, Bun, JavaScript, or TypeScript.

## Current native slice

```bash
cargo run -- --workspace /path/to/project list --status open
cargo run -- --workspace /path/to/project list --type Feature
cargo run -- --workspace /path/to/project get pm-example
cargo run -- --workspace /path/to/project create --id pm-example --title "Native item" --type Task --author agent
```

The library discovers `.agents/pm` from a workspace, nested directory, file, or
tracker root; strictly decodes TOON item documents; retains unknown package
fields; rejects merge markers and duplicate IDs; and returns deterministic JSON
projections. Create uses explicit IDs, per-item ownership locks, synced
same-directory atomic writes, a durable recovery journal, canonical history,
and exact post-document hashes. See
[the compatibility contract](docs/COMPATIBILITY.md).

Rust formatting, strict Clippy, and tests run on Linux, macOS, and Windows.
Ubuntu additionally gates the dependency audit, generated changelog, strict
`pm health`, and 100 percent line, region, function, and branch coverage.

Publication and automated releases remain disabled until the complete tracked
tree and raw Git history pass a privacy review and a maintainer explicitly
approves publishing.

Work is managed in this repository with the latest `pm` CLI under
`.agents/pm/`. The ecosystem-level lineage is the private companion item
`pm-cli-website-204t`.

## Development

```bash
cargo fmt --all -- --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-targets --all-features
cargo +nightly-2026-08-06 llvm-cov --locked --branch --all-targets --all-features --json --output-path coverage-branch.json
jq -e '.data[0].totals.lines.percent == 100 and .data[0].totals.functions.percent == 100 and .data[0].totals.regions.percent == 100 and .data[0].totals.branches.percent == 100' coverage-branch.json
cargo audit
npm exec --yes --package=@unbrained/pm-cli@2026.8.6 -- pm health --check-only --require-merge-drivers --strict-exit
```

## License

MIT. See [LICENSE](LICENSE).
