# Changelog

## 0.1.0-alpha.1 - 2026-08-07

### Features

- Read canonical pm workspaces without a JavaScript runtime ([pm-rust-o2yr](https://github.com/unbraind/pm-rust/blob/main/.agents/pm/features/pm-rust-o2yr.toon)) _type:Feature; status:closed; P1; release:0.1.0-alpha.1_
- Explain native list query work before materializing context ([pm-rust-query-cost](https://github.com/unbraind/pm-rust/blob/main/.agents/pm/features/pm-rust-query-cost.toon)) _type:Feature; status:closed; P2_
- Compare native and published crash recovery with a reusable kill corpus ([pm-rust-crash-harness](https://github.com/unbraind/pm-rust/blob/main/.agents/pm/features/pm-rust-crash-harness.toon)) _type:Feature; status:closed; P2_
- Use authorized remote backends through a portable native adapter ABI ([pm-rust-portable-native-adapter-abi](https://github.com/unbraind/pm-rust/blob/main/.agents/pm/features/pm-rust-portable-native-adapter-abi.toon)) _type:Feature; status:closed; P2_
- Stream certified native reads under caller resource limits ([pm-rust-bounded-read-sessions](https://github.com/unbraind/pm-rust/blob/main/.agents/pm/features/pm-rust-bounded-read-sessions.toon)) _type:Feature; status:closed; P2_
- Implement native keyword search with pinned oracle parity ([pm-rust-bj7a](https://github.com/unbraind/pm-rust/blob/main/.agents/pm/features/pm-rust-bj7a.toon)) _type:Feature; status:closed; P2_
- Implement native item file doc and test collections with oracle parity ([pm-rust-unih](https://github.com/unbraind/pm-rust/blob/main/.agents/pm/features/pm-rust-unih.toon)) _type:Feature; status:closed; P2_
- Implement native context history and dependency orientation with oracle parity ([pm-rust-xb3s](https://github.com/unbraind/pm-rust/blob/main/.agents/pm/features/pm-rust-xb3s.toon)) _type:Feature; status:closed; P2_
- Match unbounded list JSON to PM CLI 2026.10.2 ([pm-rust-p0no](https://github.com/unbraind/pm-rust/blob/main/.agents/pm/features/pm-rust-p0no.toon)) _type:Feature; status:closed; P1_
- Persist native close resolution and completion metadata with oracle parity ([pm-rust-cqk7](https://github.com/unbraind/pm-rust/blob/main/.agents/pm/features/pm-rust-cqk7.toon)) _type:Feature; status:closed; P2_
- Extend native ownership to selection and composed lifecycle commands ([pm-rust-rysu](https://github.com/unbraind/pm-rust/blob/main/.agents/pm/features/pm-rust-rysu.toon)) _type:Feature; status:closed; P2_
- Implement native claim and release with published CLI parity ([pm-rust-zukw](https://github.com/unbraind/pm-rust/blob/main/.agents/pm/features/pm-rust-zukw.toon)) _type:Feature; status:closed; P1_
- Write the hash_algorithm field on history entries, matching pm CLI 2026.9.11 and later ([pm-rust-hkk3](https://github.com/unbraind/pm-rust/blob/main/.agents/pm/features/pm-rust-hkk3.toon)) _type:Feature; status:closed; P1_
- Native update, comment, and close mutations ([pm-rust-27qz](https://github.com/unbraind/pm-rust/blob/main/.agents/pm/features/pm-rust-27qz.toon)) _type:Feature; status:closed; P2_
- Add daily release workflow guarded by PM_RELEASE_APPROVED variable ([pm-rust-mbdp](https://github.com/unbraind/pm-rust/blob/main/.agents/pm/tasks/pm-rust-mbdp.toon)) _type:Task; status:closed; P1_
- Absorb pm 2026.8.7 codec and workspace contracts ([pm-rust-yz76](https://github.com/unbraind/pm-rust/blob/main/.agents/pm/features/pm-rust-yz76.toon)) _type:Feature; status:closed; P1; release:0.1.0-alpha.3_
- Create canonical items with crash-safe native Rust transactions ([pm-rust-5tey](https://github.com/unbraind/pm-rust/blob/main/.agents/pm/features/pm-rust-5tey.toon)) _type:Feature; status:closed; P1; release:0.1.0-alpha.2_

### Changes

- Enable the gated pm-github issue sync ([pm-rust-zmqa](https://github.com/unbraind/pm-rust/blob/main/.agents/pm/chores/pm-rust-zmqa.toon)) _type:Chore; status:closed; P2_
- Multi-process no-lost-update concurrency test ([pm-rust-nctu](https://github.com/unbraind/pm-rust/blob/main/.agents/pm/tasks/pm-rust-nctu.toon)) _type:Task; status:closed; P2_

### Bug Fixes

- Windows release gate covers the history-directory link refusal ([pm-rust-2g7c](https://github.com/unbraind/pm-rust/blob/main/.agents/pm/issues/pm-rust-2g7c.toon)) _type:Issue; status:closed; P1_
- Certify only observed crash-oracle checks and durable evidence commits ([pm-rust-crash-oracle-recorder](https://github.com/unbraind/pm-rust/blob/main/.agents/pm/tasks/pm-rust-crash-oracle-recorder.toon)) _type:Task; status:closed; P2_
- Regenerate changelogs after gated sync and closeout ([pm-rust-closeout-changelog](https://github.com/unbraind/pm-rust/blob/main/.agents/pm/issues/pm-rust-closeout-changelog.toon)) _type:Issue; status:closed; P2_
- Recover pending item journals before every native mutation ([pm-rust-cross-operation-journal](https://github.com/unbraind/pm-rust/blob/main/.agents/pm/tasks/pm-rust-cross-operation-journal.toon)) _type:Task; status:closed; P1_
- Match published history integrity and certify CLI 2026.9.7 ([pm-rust-ing0](https://github.com/unbraind/pm-rust/blob/main/.agents/pm/issues/pm-rust-ing0.toon)) _type:Issue; status:closed; P1_
- Pin pm-rust CI and release tooling to current pm CLI and pm-changelog ([pm-rust-4ld8](https://github.com/unbraind/pm-rust/blob/main/.agents/pm/issues/pm-rust-4ld8.toon)) _type:Issue; status:closed; P1_
- Preserve canonical numeric-string quoting with toon-format 0.6.1 ([pm-rust-guou](https://github.com/unbraind/pm-rust/blob/main/.agents/pm/issues/pm-rust-guou.toon)) _type:Issue; status:closed; P1_
- Keep oracle test runs out of Sentry ([pm-rust-oqst](https://github.com/unbraind/pm-rust/blob/main/.agents/pm/issues/pm-rust-oqst.toon)) _type:Issue; status:closed; P1_
- Stabilize the create lock-replacement fixture under parallel load ([pm-rust-ax0o](https://github.com/unbraind/pm-rust/blob/main/.agents/pm/issues/pm-rust-ax0o.toon)) _type:Issue; status:closed; P2_
- Legacy list cursors without after_index publish the query fingerprint ([pm-rust-poen](https://github.com/unbraind/pm-rust/blob/main/.agents/pm/issues/pm-rust-poen.toon)) _type:Issue; status:closed; P2_
- Match oracle semantic lineage depth and unresolved ancestor handling ([pm-rust-lcap](https://github.com/unbraind/pm-rust/blob/main/.agents/pm/issues/pm-rust-lcap.toon)) _type:Issue; status:closed; P2_
- Match published container readiness for cyclic parent graphs ([pm-rust-1ycz](https://github.com/unbraind/pm-rust/blob/main/.agents/pm/issues/pm-rust-1ycz.toon)) _type:Issue; status:closed; P2_
- Match published stale session-lock refusal diagnostics ([pm-rust-sldg](https://github.com/unbraind/pm-rust/blob/main/.agents/pm/issues/pm-rust-sldg.toon)) _type:Issue; status:closed; P2_
- Validate mutation identifiers before acquiring locks ([pm-rust-2tb9](https://github.com/unbraind/pm-rust/blob/main/.agents/pm/issues/pm-rust-2tb9.toon)) _type:Issue; status:closed; P2_
- Match published punctuation-bearing tag normalization order ([pm-rust-mttg](https://github.com/unbraind/pm-rust/blob/main/.agents/pm/issues/pm-rust-mttg.toon)) _type:Issue; status:closed; P2_
- Match published comment ordering at equal timestamps ([pm-rust-2ucz](https://github.com/unbraind/pm-rust/blob/main/.agents/pm/issues/pm-rust-2ucz.toon)) _type:Issue; status:closed; P2_
- Preserve .toon extra metadata key order in list output ([pm-rust-ba8f](https://github.com/unbraind/pm-rust/blob/main/.agents/pm/issues/pm-rust-ba8f.toon)) _type:Issue; status:closed; P2_
- Preserve UTF-16 surrogate cuts in bounded JSON list output ([pm-rust-8hkb](https://github.com/unbraind/pm-rust/blob/main/.agents/pm/issues/pm-rust-8hkb.toon)) _type:Issue; status:closed; P2_
- Classify Windows sharing violations as lock contention ([pm-rust-wshr](https://github.com/unbraind/pm-rust/blob/main/.agents/pm/issues/pm-rust-wshr.toon)) _type:Issue; status:closed; P2_
- Windows changelog verification rejects text-identical generated output ([pm-rust-cwlf](https://github.com/unbraind/pm-rust/blob/main/.agents/pm/issues/pm-rust-cwlf.toon)) _type:Issue; status:closed; P1_
- The CodeQL Dependabot group matched no dependency because the pattern lacked a wildcard ([pm-rust-u3ej](https://github.com/unbraind/pm-rust/blob/main/.agents/pm/issues/pm-rust-u3ej.toon)) _type:Issue; status:closed; P2_
- Add the missing Rust CodeQL analysis ([pm-rust-f4sy](https://github.com/unbraind/pm-rust/blob/main/.agents/pm/issues/pm-rust-f4sy.toon)) _type:Issue; status:closed; P2_
- The differential conformance suite never ran in CI, and it was hiding that pm-rust stamps a superseded history epoch ([pm-rust-ap8k](https://github.com/unbraind/pm-rust/blob/main/.agents/pm/issues/pm-rust-ap8k.toon)) _type:Issue; status:closed; P2_
- The release workflow pins a pm CLI three weeks older than CI's, and its changelog reads are bounded ([pm-rust-yilr](https://github.com/unbraind/pm-rust/blob/main/.agents/pm/issues/pm-rust-yilr.toon)) _type:Issue; status:closed; P1_
- Align local, CI, and release pm tooling for PR 26 ([pm-rust-okjs](https://github.com/unbraind/pm-rust/blob/main/.agents/pm/issues/pm-rust-okjs.toon)) _type:Issue; status:closed; P2_
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

- Recover incomplete native lock initialization after process death ([pm-rust-crash-lock-restart](https://github.com/unbraind/pm-rust/blob/main/.agents/pm/tasks/pm-rust-crash-lock-restart.toon)) _type:Task; status:closed; P2_
- Extend native list selection to configured lifecycle and hierarchy registries ([pm-rust-cbaq](https://github.com/unbraind/pm-rust/blob/main/.agents/pm/tasks/pm-rust-cbaq.toon)) _type:Task; status:closed; P3_
- Give concurrent claim acceptance explicit writer wait budgets ([pm-rust-23ps](https://github.com/unbraind/pm-rust/blob/main/.agents/pm/chores/pm-rust-23ps.toon)) _type:Chore; status:closed; P3_
- Extend native list parity to pagination and bounded output contracts ([pm-rust-ufzy](https://github.com/unbraind/pm-rust/blob/main/.agents/pm/tasks/pm-rust-ufzy.toon)) _type:Task; status:closed; P2_
- Track pm-cli 2026.10.19 as the parity oracle ([pm-rust-owzm](https://github.com/unbraind/pm-rust/blob/main/.agents/pm/tasks/pm-rust-owzm.toon)) _type:Task; status:closed; P1_
- Resume native output-limit continuations after the last emitted row, matching pm-cli 2026.10.7 ([pm-rust-8qbo](https://github.com/unbraind/pm-rust/blob/main/.agents/pm/tasks/pm-rust-8qbo.toon)) _type:Task; status:closed; P1_
- Install the published-CLI oracle from a committed lockfile in CI ([pm-rust-rnex](https://github.com/unbraind/pm-rust/blob/main/.agents/pm/tasks/pm-rust-rnex.toon)) _type:Task; status:closed; P1_
- Auto-merge green Dependabot updates and group the pm toolchain into one daily PR ([pm-rust-p9bu](https://github.com/unbraind/pm-rust/blob/main/.agents/pm/tasks/pm-rust-p9bu.toon)) _type:Task; status:closed; P2_
- Certify pm CLI 2026.9.17 ([pm-rust-6bwi](https://github.com/unbraind/pm-rust/blob/main/.agents/pm/chores/pm-rust-6bwi.toon)) _type:Chore; status:closed; P1_
- Release two re-claimed tasks and assert legacy-journal recovery succeeds ([pm-rust-x1u8](https://github.com/unbraind/pm-rust/blob/main/.agents/pm/tasks/pm-rust-x1u8.toon)) _type:Task; status:closed; P2_
- Live differential conformance suite against the published Node pm CLI ([pm-rust-2di7](https://github.com/unbraind/pm-rust/blob/main/.agents/pm/tasks/pm-rust-2di7.toon)) _type:Task; status:closed; P2_
- Wire pm-changelog with timezone-stable scripts and release:check aggregate ([pm-rust-injz](https://github.com/unbraind/pm-rust/blob/main/.agents/pm/tasks/pm-rust-injz.toon)) _type:Task; status:closed; P1_
