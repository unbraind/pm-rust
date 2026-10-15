# Native closure and safety parity (2026.10.14)

The oracle is the published package locked in `tests/oracle/package-lock.json`.
The batch belongs to [pm-rust-u4al](../.agents/pm/epics/pm-rust-u4al.toon).

## Closure metadata

Feature: [pm-rust-cqk7](../.agents/pm/features/pm-rust-cqk7.toon).

`close` accepts inline `--resolution`, `--expected-result` / `--expected`, and
`--actual-result` / `--actual`. Nonblank strings are trimmed before storage;
blank options preserve existing evidence. Validation runs against that resulting
document before any item/history commit. The SDK exposes these options through
`CloseItem.fields: CloseFields` while retaining the canonical mutation result.
Its `close_receipt` contains the compact CLI receipt.
Without an explicit validation flag, minimal uses off, default uses warn and
strict uses strict. Custom governance uses its stored override or the default
warn policy, matching the oracle's effective settings.

The single-item command also supports positional reasons, `--reason` / `-r`,
`--close-reason`, `--message` / `-m`, `--completed-at`, `--duplicate-of` / `-d`,
`--validate-close`, `--force`, and `--release-assignment`. Completion time is
separate from tracker close time; a stored completion time survives an omitted
option. Duplicate closure validates the target and supplies only missing
resolution/outcome evidence. Explicit repeat closure requires force. JSON and
TOON receipts retain the oracle's counts, warning arrays and recovery fields.
Assignment release reports the union of changed fields. If its later session
write fails, the refusal identifies the already-durable closure and retains its
item/history evidence.
Relative-date overflow retains the pinned oracle's `unknown_error` diagnostic
and exit 1. [pm-rust-9bci](../.agents/pm/tasks/pm-rust-9bci.toon) tracks reassessment
after a future pin update; this batch does not update the oracle.

```bash
cargo run -- close sample-item "Delivered" --author fixture-agent \
  --resolution "Fixed" --expected-result "Expected outcome" \
  --actual-result "Observed outcome" --validate-close strict --json
```

`tests/close_differential.rs` initializes a real tracker with the oracle, saves
its before-image, and replays each CLI at the same disposable path. It compares
complete stdout, stderr and exit codes plus exact canonical item, history and
session bytes. Keeping the path identical also makes recovery command paths
comparable without normalization. Caches and transient transaction/lock state
are excluded from durable artifact comparisons.

## Lock, readiness, lineage and cursor boundaries

| Item | Behavior | Regression |
| --- | --- | --- |
| [pm-rust-2tb9](../.agents/pm/issues/pm-rust-2tb9.toon) | Shared lock acquisition rejects malformed identifiers before directory creation or lock publication | Kernel directory-event watcher and valid ownership differential |
| [pm-rust-sldg](../.agents/pm/issues/pm-rust-sldg.toon) | Expired session-state locks retain the oracle's refusal and leave already-committed steps durable | Exact SDK context and CLI JSON comparisons |
| [pm-rust-1ycz](../.agents/pm/issues/pm-rust-1ycz.toon) | A nonterminal node reached again through a parent cycle is an active descendant | Self, mixed, active and terminal cycles; bounded corpus scan regression |
| [pm-rust-lcap](../.agents/pm/issues/pm-rust-lcap.toon) | Semantic lineage includes at most 16 resolved items and stops before a missing ancestor | Deep, unresolved and cyclic lineage with exact session bytes |
| [pm-rust-poen](../.agents/pm/issues/pm-rust-poen.toon) | Legacy producer cursors without `after_index` expose `opaque_cursor` in the continuation contract | Complete plain and triage envelopes |
| [pm-rust-ax0o](../.agents/pm/issues/pm-rust-ax0o.toon) | Lock replacement fixture synchronizes after owner-token initialization | FIFO rendezvous, complete-token assertion and replacement preservation |

The replacement fixture opens a FIFO writer only when mutation recovery has
opened its reader. Recovery occurs after the lock token has been fully written
and synced. The writer remains open while the fixture replaces the lock owner,
then supplies invalid recovery input and closes the FIFO. No sleep or polling
interval participates in this ordering. Fifty runs of the complete create suite
with eight test threads passed: 400 tests, including 50 replacement checks and 50 delayed-owner regressions.

Readiness witnesses begin at parent edges. This preserves cyclic self inclusion
without making ordinary leaves containers, and retains the index's bounded
corpus passes. Lineage and cursor repairs change their contract boundary without
changing ordinary resolved shallow lineage or modern producer cursors.

## Verification receipts

Per-item tracker comments record behavioral revert commands and their failures.
The final gate comments record the full test count, coverage dimensions and
release gates. Linked tests use the locked oracle and require its presence;
a missing oracle cannot produce a conformance pass.

The final Linux `just release-check` passed 317 tests in each of its ordinary
and freshly instrumented runs, across 17 targets, with zero failures or ignored
tests. Measured production coverage is 5,646/5,646 lines, 8,630/8,630 regions,
522/522 functions and 1,194/1,194 branches: all four dimensions are 100%.
Formatting, deny-warnings Clippy, private-item rustdoc, identity audit, the locked
oracle suites, dependency audit and changelog verification passed. The dependency
audit retains the inherited allowed `RUSTSEC-2024-0449` unmaintained advisory.
Platform CI remains separate from this local Linux receipt.
