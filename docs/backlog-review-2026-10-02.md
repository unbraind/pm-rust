# PR backlog review, 2026-10-02

Package owner: [pm-rust-4ld8](https://github.com/unbraind/pm-rust/blob/main/.agents/pm/issues/pm-rust-4ld8.toon).

## Toolchain branch

`fix/pm-rust-ci-release-toolchain-2026-9-26` contains SteveBot commit
`823513ec5870758d82732bdca2b1acc59be12f72`, already squash-merged by
[PR #52](https://github.com/unbraind/pm-rust/pull/52) as
`a39a52b629cd138ccf7b6429a855a884919ede9e`. The branch and main have identical
tracked files. Opening another PR would duplicate shipped work. The old branch
is retained. The canonical checkout was restored to main and fast-forwarded.

## Dependabot

| PR | Exact head reviewed | Scope | Verdict |
| --- | --- | --- | --- |
| [#53](https://github.com/unbraind/pm-rust/pull/53) | `e6fda8f927c7b189827cb5e1b62a4f1e2000d71b` | thiserror and thiserror-impl 2.0.20 to 2.0.21 in Cargo.lock | Merge-ready after rebase: exact-head Linux, macOS, Windows, aggregate gate and CodeQL successful; GitHub CLEAN |
| [#54](https://github.com/unbraind/pm-rust/pull/54) | `7fa2a348f5f4b2ea828f389d9aa208cd9e6fc253` | Both CodeQL sub-actions 4.38.1 to 4.38.2 | Merge-ready: exact-head Linux, macOS, Windows, aggregate gate and CodeQL successful; GitHub CLEAN |
| [#57](https://github.com/unbraind/pm-rust/pull/57) | `4cc2ac68258759951bf33da4125b46dc65db7cda` | taiki-e/install-action 2.87.18 to 2.87.22 | Merge-ready: exact-head Linux, macOS, Windows, aggregate gate and CodeQL successful; GitHub CLEAN |

The former #53 head `917543114ace9ad38cb841c705f1dd804ed705cd` had successful
CI but was behind main. An authorized `@dependabot rebase` comment caused the
new head above. Prior-head success does not approve the new head. Final review confirmed the rebased head passed CodeQL too. Main requires
all three Rust platform jobs and the aggregate gate with strict base currency.
Cross-platform evidence comes from hosted CI; the local host is Linux.

These verdicts cover dependency PRs against their declared 2026.9.26 CI target.
The parity PR separately measures PM CLI 2026.10.2. No merge, publication,
release-workflow execution, or approval-guard change is part of this review.

## Local baseline

`PM_NODE_CLI` pointed at a disposable npm installation of the exact published
`@unbrained/pm-cli@2026.10.2`; `PM_RUST_REQUIRE_PUBLISHED_CLI=1` was set.
`cargo test --locked --all-targets --all-features` passed all 146 tests in 11
suites, including the real
published-CLI mutation differential and append-only multi-branch merge contract.
This is existing-code baseline evidence, not a claim that all read commands
already match the TypeScript CLI.

`pm test pm-rust-4ld8 --run --only-index 4 --progress` passed the linked
`cargo clippy --locked --all-targets --all-features -- -D warnings` command.

The rebased #53 head was also checked in a detached local worktree with
`cargo test --locked --all-targets --all-features` and the required published
PM CLI 2026.10.2: 146 tests passed. macOS and Windows results are hosted-CI
evidence, rather than local emulation.
