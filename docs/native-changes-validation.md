# Native incremental changes validation

The final local validation used a read-only, non-git copy of the public pm-rust
tracker from commit `fe42e9547a79f4f32e711cc23543c924d822af7d`. The source checkout
was never modified. The pinned 2026.10.21 SDK supplied the complete item count and
canonical mutation-event reference before the copy was made read-only. SDK-derived
reference caches are included in the subsequent unchanged-copy digest, while the
source file/byte counts below describe the original copied tracker.

| Measurement | Result |
| --- | ---: |
| Source items | 86 |
| Source regular files | 178 |
| Source bytes | 4,519,252 |
| Canonical events matched through the SDK | 2,266 |
| Native pages, maximum 20 records each | 114 |
| TOON checks | 2 |
| Idle-poll events / cursor changes | 0 / 0 |
| Total JSON delivery bytes | 1,334,728 |
| Largest JSON page including newline | 17,141 bytes |
| Largest cursor | 13,994 bytes |
| Validation wall time | 126,281 ms |

Every `(item_id, version)` appeared once. Canonical event projections matched the
SDK exactly. Native subprocesses had an empty executable search path: neither Git
nor Node was available. The source-copy digest was unchanged:
`aa6e1f0100c31730710dd602b843e47a6dd9e18e9eab46c8ec9284b9f22b7726`.
The validated debug executable digest was
`499df544539ae7613830358b7411d49c87ab079030e3e115a2d8fc9f176869da`.
No executable or generated cache is committed.

The wall time covers 114 separate native CLI processes, two extra TOON reads and
an idle poll on a heavily loaded shared host. It is validation evidence rather
than a latency SLO. The local adapter revalidates bounded history prefixes; it
provides incremental delivery without claiming O(delta) source I/O.

Reproduce with an explicitly selected public tracker:

```sh
cargo +1.90.0 build --locked --bin pm-rust --example changes -j 2
node scripts/native-adapter-changes-validate.mts \
  --source public-tracker/.agents/pm --binary target/debug/pm-rust
```

The deterministic model runs 500 random operation sequences, including older
input replay, random append bursts, equal/backdated timestamps, foreign authors,
custom operations and statuses, relationships, whitespace/CRLF variants, cursor
restarts and older-cursor retries. It compares canonical fields read back through
`Workspace::history` with an independent per-stream delivery model. Model JSON and TOON pages round-trip through their decoders and share the producer's page validator. No import/export/sync, item mutation, status-role decision, ambient
scope default or journaled write plan is introduced by this feature.

Additional cases inject peer appends at every native processing boundary and
around transport/admission, cancellation at each source boundary, rare OS errors,
partial tails, foreign records, pruning, same-size rewrites, invalid cursor
ordinals, extreme input/output numbers, incompatible adapters and transport frame
limits. Cursor state is private to each reader; no shared checkpoint is written.

Focused coverage of `src/changes.rs` reached 100% lines (384), functions (37),
regions (574) and branches (84). Full repository acceptance is delegated to the
unchanged npm and Bun CI release gates. Their exact-head run and counts are
reported in the PR.

Six behavioral reverts run in a disposable copy and fail their regression tests:
exclusive resume, prefix integrity, cancellation before checkpoint admission and
canonical field projection, duplicate-event admission and delayed-reply correlation. The fixed checkout is never modified by that harness.

```sh
cargo +1.90.0 test --locked --lib changes::tests -- --test-threads=2
cargo +1.90.0 test --locked --test changes_contract -- --test-threads=2
cargo +1.90.0 test --locked --bin pm-rust changes_writer_refusals -- --test-threads=2
node scripts/native-adapter-changes-revert.mts
```
