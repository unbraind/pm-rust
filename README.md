# pm-rust

`pm-rust` is an independent, Rust-native implementation of the public
[`pm`](https://github.com/unbraind/pm-cli) workspace contracts for agents and
native applications.

The project is pre-release. Its first delivery slice is read-only and targets
the published `pm` 2026.8.6 on-disk contract. Production code, tests,
benchmarks, and build tooling are Rust; the distributed binary will not require
Node.js, Bun, JavaScript, or TypeScript.

Publication and automated releases remain disabled until the complete tracked
tree and raw Git history pass a privacy review and a maintainer explicitly
approves publishing.

Work is managed in this repository with the latest `pm` CLI under
`.agents/pm/`. The ecosystem-level lineage is the private companion item
`pm-cli-website-204t`.

## License

MIT. See [LICENSE](LICENSE).
