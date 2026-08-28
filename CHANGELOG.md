# Changelog

## Unreleased

### Features

- Native update, comment, and close mutations ([pm-rust-27qz](https://github.com/unbraind/pm-rust/blob/main/.agents/pm/features/pm-rust-27qz.toon)) _type:Feature; status:closed; P2_
- Add daily release workflow guarded by PM_RELEASE_APPROVED variable ([pm-rust-mbdp](https://github.com/unbraind/pm-rust/blob/main/.agents/pm/tasks/pm-rust-mbdp.toon)) _type:Task; status:closed; P1_
- Absorb pm 2026.8.7 codec and workspace contracts ([pm-rust-yz76](https://github.com/unbraind/pm-rust/blob/main/.agents/pm/features/pm-rust-yz76.toon)) _type:Feature; status:closed; P1; release:0.1.0-alpha.3_
- Create canonical items with crash-safe native Rust transactions ([pm-rust-5tey](https://github.com/unbraind/pm-rust/blob/main/.agents/pm/features/pm-rust-5tey.toon)) _type:Feature; status:closed; P1; release:0.1.0-alpha.2_

### Changes

- Multi-process no-lost-update concurrency test ([pm-rust-nctu](https://github.com/unbraind/pm-rust/blob/main/.agents/pm/tasks/pm-rust-nctu.toon)) _type:Task; status:closed; P2_

### Bug Fixes

- Lock contention on Windows presents as access denied, and the native binary treated it as a fatal filesystem error ([pm-rust-rfuc](https://github.com/unbraind/pm-rust/blob/main/.agents/pm/issues/pm-rust-rfuc.toon)) _type:Issue; status:closed; P0_
- A float in item metadata hashes differently from the published CLI, and two doc contracts describe behaviour the code does not have ([pm-rust-27da](https://github.com/unbraind/pm-rust/blob/main/.agents/pm/issues/pm-rust-27da.toon)) _type:Issue; status:closed; P1_
- The concurrency contract cannot tell lock admission control apart from a real platform failure ([pm-rust-kgfh](https://github.com/unbraind/pm-rust/blob/main/.agents/pm/issues/pm-rust-kgfh.toon)) _type:Issue; status:closed; P1_
- Windows-only concurrency test failure: concurrent_comment_processes_preserve_every_accepted_mutation asserts host timing ([pm-rust-b6ey](https://github.com/unbraind/pm-rust/blob/main/.agents/pm/issues/pm-rust-b6ey.toon)) _type:Issue; status:closed; P2_
- release-check passes locally and fails in CI whenever CHANGELOG_DATE is overridden, because this repository has no git tags to date releases from ([pm-rust-1ps2](https://github.com/unbraind/pm-rust/blob/main/.agents/pm/issues/pm-rust-1ps2.toon)) _type:Issue; status:closed; P2_
- Release commit identity was not allowlisted and the dangling-object test proved nothing ([pm-rust-r9r2](https://github.com/unbraind/pm-rust/blob/main/.agents/pm/issues/pm-rust-r9r2.toon)) _type:Issue; status:closed; P1_
- Shallow CI checkouts made the identity audit pass without inspecting history and broke the changelog gate ([pm-rust-cu3d](https://github.com/unbraind/pm-rust/blob/main/.agents/pm/issues/pm-rust-cu3d.toon)) _type:Issue; status:closed; P1_
- Release workflow would have failed on a padded crate version and a missing gate toolchain ([pm-rust-wbvn](https://github.com/unbraind/pm-rust/blob/main/.agents/pm/issues/pm-rust-wbvn.toon)) _type:Issue; status:closed; P1_
- Release workflow pushed version bump straight to protected main, killing the job before the tag push ([pm-rust-b9yi](https://github.com/unbraind/pm-rust/blob/main/.agents/pm/issues/pm-rust-b9yi.toon)) _type:Issue; status:closed; P2_
- A green CI proves nothing about a working copy, because the toolchain is pinned only inside the workflows ([pm-rust-9yfr](https://github.com/unbraind/pm-rust/blob/main/.agents/pm/issues/pm-rust-9yfr.toon)) _type:Issue; status:closed; P2_
- Digest hex formatting blocked the sha2 0.11 upgrade across every platform ([pm-rust-y12t](https://github.com/unbraind/pm-rust/blob/main/.agents/pm/issues/pm-rust-y12t.toon)) _type:Issue; status:closed; P1_
- Converge changelog generation and verification on replace mode ([pm-rust-f2nv](https://github.com/unbraind/pm-rust/blob/main/.agents/pm/issues/pm-rust-f2nv.toon)) _type:Issue; status:closed; P2_
- Remove personal author identity from public Git history ([pm-rust-6q0n](https://github.com/unbraind/pm-rust/blob/main/.agents/pm/issues/pm-rust-6q0n.toon)) _type:Issue; status:closed; P0; release:0.1.0-alpha.2_

### Security

- Implement native Rust identity audit gate for reachable and unreachable objects ([pm-rust-g2w2](https://github.com/unbraind/pm-rust/blob/main/.agents/pm/tasks/pm-rust-g2w2.toon)) _type:Task; status:closed; P1_
- Add approved-git-identities file with maintainer decision header ([pm-rust-n7zv](https://github.com/unbraind/pm-rust/blob/main/.agents/pm/tasks/pm-rust-n7zv.toon)) _type:Task; status:closed; P1_

### Other

- Release two re-claimed tasks and assert legacy-journal recovery succeeds ([pm-rust-x1u8](https://github.com/unbraind/pm-rust/blob/main/.agents/pm/tasks/pm-rust-x1u8.toon)) _type:Task; status:closed; P2_
- Live differential conformance suite against the published Node pm CLI ([pm-rust-2di7](https://github.com/unbraind/pm-rust/blob/main/.agents/pm/tasks/pm-rust-2di7.toon)) _type:Task; status:closed; P2_
- Wire pm-changelog with timezone-stable scripts and release:check aggregate ([pm-rust-injz](https://github.com/unbraind/pm-rust/blob/main/.agents/pm/tasks/pm-rust-injz.toon)) _type:Task; status:closed; P1_

## 0.1.0-alpha.1 - 2026-08-07

### Features

- Read canonical pm workspaces without a JavaScript runtime ([pm-rust-o2yr](https://github.com/unbraind/pm-rust/blob/main/.agents/pm/features/pm-rust-o2yr.toon)) _type:Feature; status:closed; P1; release:0.1.0-alpha.1_
