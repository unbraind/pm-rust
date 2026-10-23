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
Full unchanged release-gate and coverage results will be recorded from CI.
