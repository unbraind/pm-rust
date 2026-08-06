# pm-rust

`pm-rust` is an independent, Rust-native implementation of the public
[`pm`](https://github.com/unbraind/pm-cli) workspace contracts for agents and
native applications.

The project is pre-release. Its first delivery slice is read-only and targets
the published `pm` 2026.8.6 on-disk contract. Production code, tests,
benchmarks, and build tooling are Rust; the distributed binary does not require
Node.js, Bun, JavaScript, or TypeScript.

## Current read-only slice

```bash
cargo run -- --workspace /path/to/project list --status open
cargo run -- --workspace /path/to/project list --type Feature
cargo run -- --workspace /path/to/project get pm-example
```

The library discovers `.agents/pm` from a workspace, nested directory, file, or
tracker root; strictly decodes TOON item documents; retains unknown package
fields; rejects merge markers and duplicate IDs; and returns deterministic JSON
projections. See [the compatibility contract](docs/COMPATIBILITY.md).

Quality gates run on Linux, macOS, and Windows and require formatting, strict
Clippy, tests, dependency audit, strict `pm health`, and 100 percent line,
region, function, and branch coverage.

Publication and automated releases remain disabled until the complete tracked
tree and raw Git history pass a privacy review and a maintainer explicitly
approves publishing.

Work is managed in this repository with the latest `pm` CLI under
`.agents/pm/`. The ecosystem-level lineage is the private companion item
`pm-cli-website-204t`.

## Development

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
cargo llvm-cov --all-targets --all-features
cargo audit
pm health --strict-exit
```

## License

MIT. See [LICENSE](LICENSE).
