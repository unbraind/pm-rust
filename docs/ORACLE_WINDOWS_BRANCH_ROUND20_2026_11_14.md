# Concurrent blocker removal closes the last Windows branch

Starting head: `603203d26232272d6c79867b17c5df37010f9409`. The worktree was
clean, the remote branch matched and main was fully included. Round 19's macOS
release gate completed within 45 minutes; Windows reached 100% regions, lines
and functions, with only one branch remaining. No review threads are open.

## Real filesystem scenario

`round20_listed_blocker_disappearance_matches_absence` removes an actual
`sample-gate.toon` after `read_directory` has captured its entry and before live
symlink/directory/file checks. The path now has no metadata, so the existing
`path.is_file()` condition takes its false side on every platform.

The per-entry hook is a separate thread-local scheduler, independent of the
existing directory-read scheduler. Its call site is `cfg(test)` and its helper
lives inside the existing test-only module; neither exists in production.
The callback performs a real deletion rather than substituting any metadata or
`is_file()` result. A drop guard clears the scheduler during normal return or
unwinding. The production traversal and regular-file check are unchanged.

The single ungated regression exercises both direct `blocker_paths` and admission
while the removal occurs. It requires the matching entry to be skipped, verifies
that the callback really removed a listed regular file, and compares the complete
admission context with an already-absent blocker. Strict admission retains the
`start_blocked_by_open_dependency` refusal with a null title/status unresolved
reference. All remaining settings/item bytes and directory contents stay
unchanged; fixture restoration/deletion is the only filesystem change. The
existing Unix-only `round12_fifo_blockers_are_not_items` remains unchanged.

The existing `start_declared_blocker_variants_match_published_bytes` differential
includes an absent local blocker and compares complete published/native streams,
exit codes and durable bytes. It remains in the full required-oracle coverage
pass. There is no product-policy change to missing-blocker handling.

## Negative/restored proof

In an isolated source copy, remove only `path.is_file() &&` from the matching-entry
condition, thereby treating the vanished matching name as found. The new test
executes and fails its empty-found assertion: exit 101, one failed test. Restoring
the condition yields exit 0, one passed test. Every copied tracked source and
test file is restored byte-for-byte. This is a behavioral mutation control,
not a compile/setup failure.

Focused validation and `cargo test --locked -j2 --lib lifecycle_admission` pass:
**15 passed, 0 failed, 0 ignored**, including the new regression and the old FIFO
case. Complete Linux coverage and cross-target results follow after completion.

## Full Linux coverage

`cargo +nightly-2026-08-06 llvm-cov --locked --branch --all-targets --all-features`
with JSON reporting outside the repository passes in **2,127.3 seconds
(35.46 minutes)**: **916 passed, 0 failed, 0 ignored across 34 targets**.
The pinned published oracle is required with two build jobs and test threads.
The existing missing-blocker differential passes, confirming the absent-target
policy against the published CLI without changing any differential assertion.

| Metric | Covered / total | Percent |
| --- | --- | --- |
| Regions | 31,897 / 31,897 | 100% |
| Branches | 4,434 / 4,434 | 100% |
| Lines | 19,483 / 19,483 | 100% |
| Functions | 1,994 / 1,994 | 100% |

Each covered count equals its total exactly; no rounded incomplete percentage
is accepted. Thresholds and coverage scope remain unchanged. Actual Windows
branch certification remains for fresh CI on the pushed head.

## Cross-target and static checks

All commands pass within their individual 20-minute caps:

- `cargo check --locked -j2 --tests --target x86_64-pc-windows-gnu` (4.3 s).
- `cargo check --locked -j2 --tests --target aarch64-apple-darwin` (4.5 s).
- `cargo clippy --locked -j2 --all-targets --all-features -- -D warnings` (10.9 s).
- `cargo clippy --locked -j2 --all-targets --all-features --target x86_64-pc-windows-gnu -- -D warnings` (10.9 s).
- `cargo fmt --all -- --check` (1.7 s).

The new regression has no platform cfg gate, so Windows compiles the same real
removal callback and missing-blocker assertions. The per-entry hook itself is
test-only. A fresh remote read confirms main is already included.

## Tracker and final checks

The pinned PM-linked lifecycle test run passes all 15 tests, with run-artifact
recording disabled. Files, this receipt, the command, coverage totals and
negative/restored proof are recorded on `pm-rust-oracle-1111`. Its claim is
released; it remains `in_progress` and unclosed. After the final tracker write,
`npm run changelog:full` and `just changelog-check` pass, with no generated
changelog content diff. Item history remains a byte-prefix extension of the
starting head. New public evidence contains no absolute local paths or host
identities.
