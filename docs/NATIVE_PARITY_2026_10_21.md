# Native parity with pm-cli 2026.10.21

The published oracle and native compatibility constant are pinned to 2026.10.21.
The lockfile retains the registry tarball and integrity. This receipt covers the
changes since the previous 2026.10.19 oracle; it does not expand every published
command into the native implementation.

## Applicable release changes

| Release change | Native behaviour |
| --- | --- |
| GH-1518 dead same-host lock owner | Linux captures the same boot/PID-namespace hash and original process-start field as the oracle. A positive missing-process or reused-PID observation permits recovery before TTL expiry. Recovery intent is persisted before removal; the existing byte-identity check and publication gate protect replacements. |
| Contention advice | Preserve the actual invocation, suggest a bounded `PM_LOCK_WAIT_MS` budget and read-only lock inspection, and never suggest a live-lock force override. The native admission loop honours a valid environment wait budget. |
| GH-1520 unknown SDK mutation keys | Serialized create, update, comment, ownership and close requests reject unknown fields during decoding, before workspace access. Rust struct literals also reject unknown option fields at compile time. |
| GH-1519 test option preflight | Test execution remains outside the native slice. Unsupported execution/context/timeout flags are refused by CLI parsing before any linked-test mutation. |
| Unknown assurance declaration reads | Read-only discovery and lookup return typed usage refusals with `assurance_evaluation_refused`, unchanged state and a bounded discovery command; snapshot cursors resume bounded pages and refuse changed registries. Gate evaluation, declaration writes and presets remain outside this slice. |
| GH-1507 default list ordering | Brief rows retain priority. Metadata explains nonterminal status, ascending priority, descending update time and ascending ID tie breakers. Complete brief TOON retains this sorting disclosure. |
| GH-1511 context cursor boundaries | Active blocked candidates enter packing only through the current focus page. Nonactive blocked fallback candidates retain their separate admission. |
| GH-1512 completion boundaries | Integer counts define half-up completion rounding, including 29/200 = 15 percent. |
| GH-1510 relationship cursor recovery | Recovery names the cursor flag used by the command rather than always naming `--after`. |
| Rendered relationship budget recovery | Oversized root/evidence packets refuse with a typed usage receipt. Candidate budgets replay the preserved query; bounded verification distinguishes a proven minimum, a feasible candidate and unavailable advice. Canonical retry arguments preserve scope, traversal controls and encoding, and are accepted by the native CLI. Packet cost excludes hidden row-discovery metadata. |
| GH-1509 legacy blockers | Scalar and relationship blocker resolution recognizes `depends_on` aliases, retains resolved fallback details and ignores absent-target markers. Differential coverage checks the published outputs. |
| GH-1514 injected SDK author | The native SDK has no injected extension client. Explicit identity/author precedence remains owned by the native request and scoped attribution contracts. |
| GH-1500/1501 JSON facts and brief output | Literal empty and whitespace closure/search arguments remain in recovery and rejected-value receipts. Brief list JSON uses compact wire bytes and matching budget estimates. TOON preserves explicit nulls and empty descendants inside data rows. Standard brief envelopes remove redundant declarations while retaining partial-page continuation, source completeness and effective sorting evidence. |

The remaining release notes concern upstream dependency advisories, CI scheduling,
review receipts, native SQLite startup, extension/MCP transport, generic item
mutation previews, and other recovery/packing surfaces outside the supported native
slice. They do not justify adding a JavaScript runtime to the distributed binary.

## Concurrency without Git

`tests/concurrency_contract.rs` initializes a plain-directory tracker with the
published `pm init`, bounds Git discovery at its parent, and invokes the native
binary with an empty executable search directory. Eight concurrent writers submit
three comments each. Every submission must succeed; every unique marker appears
exactly once, all 25 history records remain, and every before/after hash link is
checked. A lock-timeout rejection fails this acceptance test.

Local linked-test provenance ignores inherited Git repository-location variables
and resolves the branch from the selected workspace. A real two-repository hook
fixture verifies both provenance and test-process isolation; missing Git remains
an optional-provenance case.

`tests/parity_1021_contract.rs` includes a real process that acquires a published
lock and exits without releasing it. A native writer must recover that unexpired
lock and preserve its original receipt in the immutable recovery audit. Unsupported
hosts omit unverifiable coordinates and retain conservative TTL/force policy.

Recovery receipts are retained as audit evidence, matching the 2026.10.21 oracle.
Its `dist/core/lock/lock-gc.js` scans direct `locks/*.lock` entries; it does not
collect `locks/recoveries/*.json`, even when their timestamps and modification
times are old. `published_lock_gc_retains_recovery_receipts` verifies this with
an actual published `gc --scope locks` run. No automatic receipt expiry is added.

## Verification

The required aggregate is `just release-check`, including the unchanged 100 percent
line, function, region and branch gate. CI applies it on Linux, macOS and Windows.
The linked tracker item records measured results, behavioural revert proofs and
any remaining blockers. A version pin alone is not certification.

The MSVC native executable reserves an 8 MiB virtual stack so LLVM-instrumented
CLI mutations can run under the Windows aggregate. Stack pages remain committed
on demand. This does not change the coverage scope or test thresholds.

Independent test cases can be serialized locally with
`RUST_TEST_THREADS=1 just release-check`. Each scenario retains its internal
process concurrency and subprocess deadlines; the CI matrix runs the standard
recipe.

## PR 101 review follow-up

All three unresolved review findings in the October 21 review are valid:

| Thread | Verified behavior and change |
| --- | --- |
| `PRRT_kwDOTwgMRs6uxrCh` | Stale recovery reads receipt bytes and file identity from one open file, then checks both before unlinking. Unix compares device/inode; Windows compares native volume/file IDs captured through the open handle. The handle is closed before lock recreation. Normal release and failed initialization cleanup also preserve replacement files. |
| `PRRT_kwDOTwgMRs6uxrCr` | Inverse `blocks` comparisons trim both IDs. The locked oracle resolves ordinary, padded and padded uppercase dependency IDs to the same blocker. |
| `PRRT_kwDOTwgMRs6uxrDJ` | Oracle-dependent fixture setup uses the shared optional-or-required locator. Every dependent caller skips without an optional oracle; required-oracle mode still fails. |

The latest review body contains no outside-diff findings. The full production
PR diff was inspected for the same deletion, normalization and fixture-admission
classes. Review threads are left for the orchestrator to disposition.

Five independent behavioral reverts fail with exit 101 at runtime, restoring
sources after each run. Use `cargo +1.90.0 test --locked` with these selectors:

| Reverted behavior | Selector |
| --- | --- |
| Stale receipt identity guard | `--lib stale_lock_replacement_with_identical_bytes_is_preserved` |
| Normal release identity guard | `--lib released_lock_replacement_with_identical_bytes_is_preserved` |
| Failed initialization identity guard | `--lib failed_lock_replacement_with_identical_bytes_is_preserved` |
| Inverse ID trimming | `--test orientation_differential padded_inverse_blockers_match_oracle` |
| Optional oracle admission | `--test parity_1021_contract absent_oracle_obeys_optional_and_required_modes` |

The identity guard narrows pathname replacement exposure under the existing gate
lease; it does not claim an atomic compare-and-unlink against arbitrary external
writers that bypass that lease.

## PR 101 second review follow-up

All four unresolved threads in the next review are valid. The latest CodeRabbit
review body has no outside-diff findings; pagination of threads and comments is
complete. The production diff and fixture callers were checked for the same
publication, repeated-observation, optional-oracle and scheduling defects.

| Thread | Verified behavior and change |
| --- | --- |
| `PRRT_kwDOTwgMRs6uz2WV` | Recovery intent flushes the receipt's containing directory before stale-lock removal. It also flushes `locks/`, persisting a newly created `recoveries/` entry before the deletion. Both new and existing recovery directories are exercised with real file writes and directory flushes. |
| `PRRT_kwDOTwgMRs6uz2Wn` | One owner inspection supplies both diagnostic text and structured reason, matching `buildLockConflictError` in the locked 2026.10.21 oracle. Test-only observation counters leave the real kernel probes intact. |
| `PRRT_kwDOTwgMRs6uz2Ww` | The no-git concurrency fixture uses the shared optional-or-required oracle contract. A subprocess regression runs the actual concurrency test with an absent oracle in both modes. |
| `PRRT_kwDOTwgMRs6uz2W4` | The zero-wait regression attempts acquisition on the current thread while retaining the holder, then checks conflict and elapsed time below the configured two-second budget. It no longer depends on a contender starting within a sleep window. |

Five isolated behavioral reverts fail with exit 101. Each source was restored
immediately after the expected failure; no tracker history was rewritten.
Use `cargo +1.90.0 test --locked` with the following selectors:

| Reverted behavior | Selector and observed failure |
| --- | --- |
| Recovery intent function | `--lib recovery_intent_flushes_receipt_and_new_directory`: zero directory flushes instead of two. |
| New recovery-directory entry flush alone | Same selector: only `recoveries/` flushed, missing `locks/`. |
| Ownership refusal function | `--lib ownership_refusal_observes_owner_once`: two real owner observations instead of one. |
| Concurrency fixture and its caller | `--test concurrency_contract absent_concurrency_oracle_obeys_optional_and_required_modes`: optional absence fails instead of skipping. |
| Resolved acquisition budget | `--lib resolved_zero_wait_budget_is_used_without_rereading_settings`: using configured wait rather than the resolved zero budget violates the elapsed-time assertion. |

There is no requested oracle divergence and no refusal. Thread replies and
resolution remain with the orchestrator.

The first Linux aggregate passed all 576 stable and 576 instrumented tests but
correctly rejected one uncovered error-return region at the new recovery-directory
flush. The threshold remained unchanged. The durable-intent regression now also
renames the written audit directory immediately before the flush, using a
thread-local test hook. This forces a real `NotFound` from directory persistence;
the original lock, item and history must remain unchanged, and the audit remains
available under its retained directory name. The normal production path has no
hook. Run `cargo +1.90.0 test --locked --lib
dead_owner_recovery_requires_a_durable_intent -- --nocapture` for this failure case.
The tracker records the replacement aggregate results separately from the initial
Linux coverage failure.
