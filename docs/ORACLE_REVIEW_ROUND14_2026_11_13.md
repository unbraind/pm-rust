# Missing workflow sidecars and review regressions

Starting head: `f6958818aec3d3baf71af1b5b2aba7f242a5a107`. All four review threads
were read in full, including their agent prompts. Main was already included;
earlier accepted fixes remain.

## Workflow oracle decision

The pinned published pm-cli 2026.11.11 accepts both an explicitly mapped missing
workflow file and an absent default `schema/workflows.json`. Independent CLI
probes in fresh non-git trackers tested all eight combinations. Pause leaves the
item open, start and atomic claim-start set in_progress, and close sets closed.

| Operation | Explicit missing mapping | Absent default |
| --- | --- | --- |
| pause-task | exit 0, open | exit 0, open |
| start-task | exit 0, in_progress | exit 0, in_progress |
| claim --start | exit 0, in_progress | exit 0, in_progress |
| close (validation off) | exit 0, closed | exit 0, closed |

The exact minimal settings from the failing contract also succeed; the published
CLI additionally emits its existing invalid-settings-schema warning before
normalizing those partial settings. Missing sidecars do not produce an I/O
refusal.

Four new claim/release differentials cover pause, start, atomic claim-start and
close. Each compares complete CLI stdout, stderr and exit status plus persisted
item/history/session bytes using valid full settings. A second pass uses the
exact minimal settings (and `{}` for default absence), public published SDK
mutations, and the same native methods, comparing persisted bytes. The SDK
receipt is checked explicitly. No field normalization or assertion loosening
was added.

The production fallback is retained. The ownership contract now distinguishes
absence from a genuine storage fault by creating a directory at the explicitly
mapped workflow path. That still returns `PmRustError::Io` without changing the
item. The subsequent history-write failure still verifies committed ownership,
in_progress status and the pending recovery journal.

## Review threads

- PRRT_kwDOTwgMRs63A9qW: the refusal writer selects the last parsed output-format,
  matching clap override semantics. Both flag orders have a unit test and a
  complete oracle differential using the review command. The differential also
  exposed a dependency-format descriptor mismatch: the SDK now supplies its
  operation surface explicitly, and the envelope writer honors that structured
  surface instead of inferring a rejected flag from prose. Numeric and collapse
  validation retain their published flag descriptors.
- PRRT_kwDOTwgMRs63A9qa: deferred with byte-level oracle evidence in
  [issue 167](https://github.com/unbraind/pm-rust/issues/167#issuecomment-6849115850).
  Both implementations withdraw a retry for the exact reported command and
  offer an evidence update first. The new close differential pins this policy.
  Moving flags in advice emitted for other delimiter cases would change the
  published bytes. Native update also currently refuses the dedicated evidence
  flags offered in that advice; this separate compatibility gap is recorded in
  the same follow-up comment. The dedicated update probe exits 0 in the oracle
  and 2 in native. After searches, future work is recorded on the open, unclaimed
  Task `pm-rust-oracle-recovery-167`, tagged `gh:unbraind/pm-rust#167`. No thread
  was replied to or resolved.
- PRRT_kwDOTwgMRs63A9qc: column_order is an ordering hint. Listed cells are emitted
  first and unlisted delivered cells follow, with duplicates removed naturally
  by extraction. Empty and partial hints retain decoded JSON cells, restored
  null columns and ordinary cells while honoring absent-cell metadata.
- PRRT_kwDOTwgMRs63A9qv: explicit-target build-script discovery also searches the
  host profile directory. Adjacent Cargo cache markers bound the search, directories are
  deduplicated, and an absent target build directory is harmless. Synthetic
  custom-profile layouts cover default and explicit targets; an actual explicit
  host-target invocation checks the compiled build script.

## Verification

Commands use at most two build jobs and test threads. The full test command has
an explicit 2,700-second cap; other gates have 1,200-second caps. Full aggregate
coverage and Windows execution remain CI responsibilities at unchanged thresholds.

### Revert controls

In a disposable source copy, literal starting-head functions were substituted
one at a time. Each negative run reached an executed failing test (exit 101),
then passed after the fixed function was restored:

| Function or expectation reverted | Regression |
| --- | --- |
| `json_requested` | `--bin pm-rust round14_last_output_format` |
| `json_requested` | `--test orientation_differential round14_repeated_output_formats` |
| `orientation_error_json` | the same repeated-format differential |
| `Workspace::deps` | the same repeated-format differential |
| `restore_row` | `--lib round14_column_order` |
| ownership storage-fault test expectation | `--test ownership_contract lifecycle_storage_faults_preserve_claim_and_recovery_journal` |
| `own_build_scripts` | `--target x86_64-unknown-linux-gnu --test release_pipeline_contract build_script_stack_contract` |

The ownership row proves the previous test expectation reproduces the CI failure;
it is an expectation correction, not a production behavior change. A separate
negative control changes only the optional-schema loader to reject absence:
all four new lifecycle differentials fail, then all four pass after restoration.
That proves the new oracle assertions protect the accepted absence policy.
The compiled explicit-target control caught an overly restrictive first cache
marker guard: Cargo marks both the root and triple directory. The final helper
handles both markers and passes the real target invocation and synthetic layouts.
No unsuccessful first attempt is counted as a successful revert proof.

An initial full-suite process was stopped after this test-helper correction was
needed. The final full-suite command starts again from the final source; its
results below are not assembled from the stopped process or focused runs.

All eight negative controls above failed in executed tests, and all eight
restored controls passed (the absence control executes four tests in each run).
The synthetic discovery test also passes. The final scratch copies are
byte-identical to the reviewed source and tests after restoration.

Linux and Windows GNU cross-clippy pass for all targets/features with denied
warnings. All-target cargo check and formatting checks pass. The first final-source full command reached its 2,700-second cap in
search_collections_differential without an executed test failure. The remaining
full targets are retried, followed by a complete full-suite retry with the other
gates finished. Final tracker/changelog and full-suite receipts follow below.

### Complete target coverage before the full-command retry

The first final-source full command completed 27 targets with 864 passing tests,
then reached its 45-minute cap during search, without a test failure. An unfiltered
retry of both unfinished targets passed all 17 search/collection tests and all
three strict-JSON tests in 254.4 seconds. Thus all 29 test-bearing targets have passed: 884 unique
tests, zero failures and zero ignored tests. This is the complete test-bearing target inventory,
not a claim that the timed full command exited successfully. The single full
command is retried with the other gates finished; its separate receipt follows.

| Target | Passed |
| --- | ---: |
| `src/lib.rs` | 445 |
| `src/main.rs` | 75 |
| `tests/admission_contract.rs` | 12 |
| `tests/branch_merge_contract.rs` | 1 |
| `tests/capability_contract.rs` | 6 |
| `tests/changes_contract.rs` | 1 |
| `tests/claim_release_differential.rs` | 56 |
| `tests/cli_dispatch.rs` | 32 |
| `tests/close_differential.rs` | 11 |
| `tests/concurrency_contract.rs` | 2 |
| `tests/conformance_differential.rs` | 2 |
| `tests/create_contract.rs` | 8 |
| `tests/harness_contract.rs` | 3 |
| `tests/identity_audit.rs` | 7 |
| `tests/list_differential.rs` | 11 |
| `tests/mutation_contract.rs` | 29 |
| `tests/native_adapter_contract.rs` | 13 |
| `tests/oracle_launcher.rs` | 2 |
| `tests/orientation_differential.rs` | 38 |
| `tests/output_parity_differential.rs` | 2 |
| `tests/ownership_contract.rs` | 11 |
| `tests/pagination_differential.rs` | 33 |
| `tests/parity_1021_contract.rs` | 14 |
| `tests/query_cost_contract.rs` | 2 |
| `tests/read_contract.rs` | 18 |
| `tests/read_session_contract.rs` | 7 |
| `tests/release_pipeline_contract.rs` | 23 |
| `tests/search_collections_differential.rs` | 17 |
| `tests/strict_json_contract.rs` | 3 |

The eight `*_differential` targets account for 170 passing tests. Their initial
2026.11.11 baseline was 59 passed and 61 failed out of 120; the expanded cases
retain exact assertions and all intentional privacy differences already accepted
in the earlier receipts. Admission and the library's oracle-backed delivery test
also run in this full inventory. Doc-tests separately report zero tests and pass.

### Completed full-command retry

`cargo test --locked -j2 --all-targets --all-features` completed successfully in
2057.2 seconds (34.29 minutes), with `RUST_TEST_THREADS=2` and a
2,700-second cap. It reports 884 passed, zero failed and zero ignored across all
29 test-bearing targets plus five zero-test example targets. Every per-target count equals the complete inventory above; this
receipt comes from one completed full command. No source change or relaxed
assertion separates the capped attempt and this successful retry. Other gate
processes had finished before the retry. The separate doc-test command also
passes with zero doc-tests.

Linux and Windows GNU cross-clippy, all-target check and fmt pass. All eight
negative controls fail as required and pass on restoration; the synthetic host
layout check also passes. The PM-linked four workflow tests pass with run-artifact
recording disabled. Full aggregate coverage and Windows runtime remain for CI at
unchanged thresholds. Tracker release and final changelog generation/check are
performed after these results, before commit and push.

The primary claim is released; `pm-rust-oracle-1111` remains in_progress and
unclosed. The follow-up `pm-rust-oracle-recovery-167` remains open and unclaimed.
Changelog generation and checking pass after release and are repeated after the
final tracker receipt. Paired item/history files are included with the code;
previous history bytes remain an exact prefix. Public evidence uses repository
paths and synthetic fixture data. No coverage threshold or exclusion changed.
