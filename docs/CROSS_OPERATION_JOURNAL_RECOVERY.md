# Recover journals before later item mutations

A native operation can stop after writing
`runtime/transactions/{operation}-{id}.json` but before replacing the item.
Previously, a different operation read the old item without recovering that
journal. Its successful write then matched neither journal image, so retries
of the original operation refused with `RecoveryConflict`.

Every native item mutation now drains pending journals for that exact ID while
holding the existing item lock, before reading or changing the item. Journal
filenames are sorted lexicographically; recovery stops at the first refusal.
Native operation names use underscores, so splitting at the first hyphen
preserves the complete item ID, including IDs that end in another item's ID.
Discovery does not maintain an operation allowlist: future operation names use
the same in-place journal recovery path. Create journals retain their separate
no-overwrite recovery rules.

A conflicting journal remains on disk. The typed `RecoveryConflict` identifies
its tracker-relative filename and asks the operator to reconcile the journal
with the item and history before retrying. Other refusal codes and the existing
before-image, after-image, missing-item and history-replay semantics are retained.
Recovery never deletes or changes another item's journal.

The regression fixtures reconstruct the state before the first item rename
using canonical after bytes and history records produced by the native mutator,
then restore the before images and write the native journal structure directly.
They use ordinary temporary trackers outside Git and require no tracing,
process interruption or timing race. Coverage includes update/comment retries,
other-item isolation, multiple journals in reverse creation order, unknown
operation names, interrupted create history, create attempts, conflict and I/O
refusals, and recovery only after obtaining the item lock.

## Behavioral revert proof

Command: `cargo +1.90.0 test --locked --lib cross_operation_journal -- --nocapture`.
The tests stay unchanged while only the production recovery changes are reverted;
`src/mutation.rs` then equals the baseline production source. The fixed functions
are restored immediately after the expected failing run.

| Regression | Fix present | Production behavior reverted |
| --- | --- | --- |
| `cross_operation_journal_update_comment_update` | pass | `RecoveryConflict` on final update |
| `cross_operation_journal_comment_update_comment` | pass | `RecoveryConflict` on final comment |
| `cross_operation_journal_leaves_other_items_untouched` | pass | `RecoveryConflict` on target retry |
| Six additional recovery/refusal/lock regressions | 6 pass | 6 fail |

Fixed run: **9 passed, 0 failed**. Behavioral revert: **0 passed, 9 failed**
(exit 101). The initial three-regression run also failed all three before the fix.

The nine regressions also pass with an empty `PATH`, a Git discovery ceiling,
and all fixtures below a plain temporary directory. Two additional CLI acceptance
runs initialize fresh non-Git trackers with the locked published
`pm init sample- --defaults --agent-guidance skip`, then reconstruct pending
update/comment journals and run both cross-operation retry sequences through the
native binary. Both pass without a Git executable, remove the recovered journal,
and retain a continuous four-entry history hash chain.

## Release validation

The isolated-HOME local `just release-check` passed formatting, deny-warnings
clippy, private-item rustdoc and the mutation suites. Its unchanged orientation
case `relationship_large_edge_limit_finishes_at_budget_floor` exceeded the
existing five-second child deadline; a direct filtered retry also failed with
no native exit code versus the expected refusal code 2. The same local condition
and passing CI are documented on main in `docs/NATIVE_HOT_PATHS.md`.
The local aggregate is not counted as passing. The focused follow-up is
`pm-rust-local-packet-deadline`; limits, floors and unrelated source stay unchanged.

Separate `cargo audit` passed for 136 dependencies, with the one existing allowed
`RUSTSEC-2024-0449` warning. `just changelog-full` and `just changelog-check` passed.
[Exact implementation CI](https://github.com/unbraind/pm-rust/actions/runs/39924710637)
at `bb7cd2a` passed all **9 CI jobs**, including the complete unchanged
`just release-check` on Linux, macOS and Windows.

| Platform | Ordinary | Instrumented | Lines | Functions | Regions | Branches |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Linux | 598 | 598 | 14139/14139 | 1477/1477 | 23602/23602 | 3318/3318 |
| macOS | 587 | 587 | 14054/14054 | 1464/1464 | 23452/23452 | 3280/3280 |
| Windows | 581 | 581 | 14050/14050 | 1463/1463 | 23450/23450 | 3282/3282 |

Every listed suite has zero failures and zero ignored tests. All four coverage
columns are exactly **100%**, with source scope and floors unchanged. The gates
also pass deny-warnings clippy, private-item documentation, cargo-audit (one
existing allowed `RUSTSEC-2024-0449` warning), changelog verification and strict
tracker health.

The Linux mutation-facing differential suites pass **27 ownership + 6 close +
2 conformance + 17 search/collection = 52 tests**; the current-oracle certification
suite passes another **13 tests**. All nine new recovery regressions pass on every
platform, ordinarily and under instrumentation.

The isolated-HOME local `just release-check` attempt remains **494 passed,
1 failed, 0 ignored**; the failure is the pre-existing five-second packet child
deadline, also reproduced on a direct unchanged retry. It passes every CI
platform, in both test passes. This separate local limitation is tracked by
`pm-rust-local-packet-deadline`; it is not represented as a local aggregate pass.

After the implementation was measured, only this evidence document and tracker
receipts changed. All other tracked inputs are checked against `bb7cd2a` before
handoff; the final tracker histories and changelog are verified again.
