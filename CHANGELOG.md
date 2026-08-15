# Changelog

## Unreleased

### Bug Fixes

- Release commit identity was not allowlisted and the dangling-object test proved nothing ([pm-rust-r9r2](https://github.com/unbraind/pm-rust/blob/main/.agents/pm/issues/pm-rust-r9r2.toon)) _type:Issue; status:closed; P1_
- Shallow CI checkouts made the identity audit pass without inspecting history and broke the changelog gate ([pm-rust-cu3d](https://github.com/unbraind/pm-rust/blob/main/.agents/pm/issues/pm-rust-cu3d.toon)) _type:Issue; status:closed; P1_
- Release workflow would have failed on a padded crate version and a missing gate toolchain ([pm-rust-wbvn](https://github.com/unbraind/pm-rust/blob/main/.agents/pm/issues/pm-rust-wbvn.toon)) _type:Issue; status:closed; P1_
- Release workflow pushed version bump straight to protected main, killing the job before the tag push ([pm-rust-b9yi](https://github.com/unbraind/pm-rust/blob/main/.agents/pm/issues/pm-rust-b9yi.toon)) _type:Issue; status:closed; P2_
- A green CI proves nothing about a working copy, because the toolchain is pinned only inside the workflows ([pm-rust-9yfr](https://github.com/unbraind/pm-rust/blob/main/.agents/pm/issues/pm-rust-9yfr.toon)) _type:Issue; status:closed; P2_

## 0.1.0-alpha.1 - 2026-08-15

### Features

- Read canonical pm workspaces without a JavaScript runtime ([pm-rust-o2yr](https://github.com/unbraind/pm-rust/blob/main/.agents/pm/features/pm-rust-o2yr.toon)) _type:Feature; status:closed; P1; release:0.1.0-alpha.1_
- Add daily release workflow guarded by PM_RELEASE_APPROVED variable ([pm-rust-mbdp](https://github.com/unbraind/pm-rust/blob/main/.agents/pm/tasks/pm-rust-mbdp.toon)) _type:Task; status:closed; P1_
- Absorb pm 2026.8.7 codec and workspace contracts ([pm-rust-yz76](https://github.com/unbraind/pm-rust/blob/main/.agents/pm/features/pm-rust-yz76.toon)) _type:Feature; status:closed; P1; release:0.1.0-alpha.3_
- Create canonical items with crash-safe native Rust transactions ([pm-rust-5tey](https://github.com/unbraind/pm-rust/blob/main/.agents/pm/features/pm-rust-5tey.toon)) _type:Feature; status:closed; P1; release:0.1.0-alpha.2_

### Bug Fixes

- Digest hex formatting blocked the sha2 0.11 upgrade across every platform ([pm-rust-y12t](https://github.com/unbraind/pm-rust/blob/main/.agents/pm/issues/pm-rust-y12t.toon)) _type:Issue; status:closed; P1_
- Converge changelog generation and verification on replace mode ([pm-rust-f2nv](https://github.com/unbraind/pm-rust/blob/main/.agents/pm/issues/pm-rust-f2nv.toon)) _type:Issue; status:closed; P2_
- Remove personal author identity from public Git history ([pm-rust-6q0n](https://github.com/unbraind/pm-rust/blob/main/.agents/pm/issues/pm-rust-6q0n.toon)) _type:Issue; status:closed; P0; release:0.1.0-alpha.2_

### Security

- Implement native Rust identity audit gate for reachable and unreachable objects ([pm-rust-g2w2](https://github.com/unbraind/pm-rust/blob/main/.agents/pm/tasks/pm-rust-g2w2.toon)) _type:Task; status:closed; P1_
- Add approved-git-identities file with maintainer decision header ([pm-rust-n7zv](https://github.com/unbraind/pm-rust/blob/main/.agents/pm/tasks/pm-rust-n7zv.toon)) _type:Task; status:closed; P1_

### Other

- Wire pm-changelog with timezone-stable scripts and release:check aggregate ([pm-rust-injz](https://github.com/unbraind/pm-rust/blob/main/.agents/pm/tasks/pm-rust-injz.toon)) _type:Task; status:closed; P1_
