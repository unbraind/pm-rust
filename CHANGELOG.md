# Changelog

## Unreleased

### Features

- Add daily release workflow guarded by PM_RELEASE_APPROVED variable ([pm-rust-mbdp](https://github.com/unbraind/pm-rust/blob/main/.agents/pm/tasks/pm-rust-mbdp.toon)) _type:Task; status:closed; P1_
- Absorb pm 2026.8.7 codec and workspace contracts ([pm-rust-yz76](https://github.com/unbraind/pm-rust/blob/main/.agents/pm/features/pm-rust-yz76.toon)) _type:Feature; status:closed; P1; release:0.1.0-alpha.3_
- Create canonical items with crash-safe native Rust transactions ([pm-rust-5tey](https://github.com/unbraind/pm-rust/blob/main/.agents/pm/features/pm-rust-5tey.toon)) _type:Feature; status:closed; P1; release:0.1.0-alpha.2_

### Bug Fixes

- A green CI proves nothing about a working copy, because the toolchain is pinned only inside the workflows ([pm-rust-9yfr](https://github.com/unbraind/pm-rust/blob/main/.agents/pm/issues/pm-rust-9yfr.toon)) _type:Issue; status:closed; P2_
- Digest hex formatting blocked the sha2 0.11 upgrade across every platform ([pm-rust-y12t](https://github.com/unbraind/pm-rust/blob/main/.agents/pm/issues/pm-rust-y12t.toon)) _type:Issue; status:closed; P1_
- Converge changelog generation and verification on replace mode ([pm-rust-f2nv](https://github.com/unbraind/pm-rust/blob/main/.agents/pm/issues/pm-rust-f2nv.toon)) _type:Issue; status:closed; P2_
- Remove personal author identity from public Git history ([pm-rust-6q0n](https://github.com/unbraind/pm-rust/blob/main/.agents/pm/issues/pm-rust-6q0n.toon)) _type:Issue; status:closed; P0; release:0.1.0-alpha.2_

### Security

- Implement native Rust identity audit gate for reachable and unreachable objects ([pm-rust-g2w2](https://github.com/unbraind/pm-rust/blob/main/.agents/pm/tasks/pm-rust-g2w2.toon)) _type:Task; status:closed; P1_
- Add approved-git-identities file with maintainer decision header ([pm-rust-n7zv](https://github.com/unbraind/pm-rust/blob/main/.agents/pm/tasks/pm-rust-n7zv.toon)) _type:Task; status:closed; P1_

### Other

- Wire pm-changelog with timezone-stable scripts and release:check aggregate ([pm-rust-injz](https://github.com/unbraind/pm-rust/blob/main/.agents/pm/tasks/pm-rust-injz.toon)) _type:Task; status:closed; P1_

## 0.1.0-alpha.1 - 2026-08-07

### Features

- Read canonical pm workspaces without a JavaScript runtime ([pm-rust-o2yr](https://github.com/unbraind/pm-rust/blob/main/.agents/pm/features/pm-rust-o2yr.toon)) _type:Feature; status:closed; P1; release:0.1.0-alpha.1_
