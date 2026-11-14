# Oracle review round five

This final fix round follows the accepted shared lifecycle-admission implementation
in `75dc318`. All 22 review threads were enumerated; all 12 unresolved threads and
their 12 comments were read completely, with no remaining comment or thread page.
The latest CodeRabbit review body has no outside-diff findings. Threads remain
open for the orchestrator.

## Lifecycle admission

`admit_lifecycle_transition` remains the shared pre-write admission boundary.
Runtime schemas now merge additive status entries onto embedded or built-in
statuses, replacing equal normalized IDs in place. Selection captures the same
normalized registry once per corpus. A file defining only `done` retains the
built-in open, progress and terminal roles; explicit and atomic blocked starts
refuse without writes, while a closed prerequisite permits progress. Existing
external role overrides and status aliases remain effective.

Complete stored blocker identities resolve before current-prefix shorthand.
Invalid UTF-8 has the published typed encoding diagnostic: strict admission
refuses, warn and force retain the warning and proceed, and off bypasses blocker
inspection. Only an already-admitted start may omit malformed or ambiguous
unrelated lineage entries. Ordinary claims retain strict corpus validation, and
filesystem access failures remain fatal. This prevents the later parent-index
read from silently undoing an admitted warning or force bypass.

The historical `depends_on` fixture now contains only that edge, with neither a
scalar nor a canonical edge masking admission. Public SDK writes canonicalize
that alias, so the fixture changes only the persisted kind after setup to model
previously stored documents. Inspection advice quotes the comma-bearing field
list exactly like the published CLI.

## Refusals and bounded reads

The shared userinfo scanner retains nested scheme text inside an unresolved
password, registers the entire password, and projects copied fragments across
all refusal fields before either output stream renders. Producer strings and
keys are registered before projection, so stored blocker titles receive the same
protection as invocation arguments and claim holders. Public scoped-package URLs,
prose addresses and email addresses remain readable. The previously documented
public-URL divergence from the oracle remains intentional. An independent
non-git oracle probe also returned exit four and the correct blocker refusal,
redacted the stored title URL, but retained its copied password. Native deliberately
redacts that copied fragment too, following the requested userinfo rule.

Producer `next_cursor` and output-snapshot `output_cursor` are excluded from
string clipping. Native continuation is executed with long persisted IDs. The
2026.11.11 oracle independently emits a clipped, unusable producer cursor for this
fixture. Keeping native continuations executable is an intentional safety
divergence; existing exact differential comparators remain unchanged.

Output cursors are validated before any budget-omission return. Intent receipt
stabilization can downgrade a previously feasible declaration when final overhead
exceeds the declaration budget; it cannot upgrade a declaration already proven
infeasible before dimensional clipping. This preserves both the new boundary
fixture and the existing generated exact-output corpus.

## Thread disposition

| Thread | Disposition and regression |
| --- | --- |
| PRRT_kwDOTwgMRs62FkY1 | Human completed-step explanation was fixed in `17cb2da`; `failed_start_preserves_claim_and_published_recovery_receipt` retains exact JSON and human comparisons. Adding `why` to JSON is declined: the oracle omits it. |
| PRRT_kwDOTwgMRs62FkY8 | Configured terminal admission was fixed in `58e811b`; `atomic_start_rejects_configured_terminal_status_like_oracle` remains exact. Additive-schema coverage now also protects its shared registry. |
| PRRT_kwDOTwgMRs62MPDp | Declined: the oracle accepts legacy `abc`, `255`, `256`, and `1000` with canonical numeric budget 3000. `dual_budget_validation_matches_published_precedence` asserts JSON and TOON exactly. |
| PRRT_kwDOTwgMRs62MPEC | Invalid cutoff was fixed in `17cb2da`; `context_cutoff_validation_matches_published_clock_refusal` matches the oracle's runtime refusal. Additional mixed historical timestamps cover valid activity preservation. |
| PRRT_kwDOTwgMRs62MPEO | Global URL scanning from `17cb2da` is retained and strengthened for nested passwords and stored producer fragments; binary and CLI-stream credential corpora. |
| PRRT_kwDOTwgMRs62QgiY | Preserve producer cursors in `read_output::compact_strings`; `round5_long_intent_cursor_remains_executable` independently checks native replay and documents the oracle defect. |
| PRRT_kwDOTwgMRs62TbPO | Merge additive status definitions in `ownership::runtime_settings`; `round5_additive_status_admission_matches_published_bytes`. |
| PRRT_kwDOTwgMRs62T9wn | Full blocker identity precedes shorthand in `lifecycle_admission::blocker`; `round5_complete_blocker_identity_matches_published_bytes`. |
| PRRT_kwDOTwgMRs62T9wq | Stabilized feasibility stays monotone in `list_intent::stabilize`; the exact oracle comparison is supplemented by the discriminating `round5_final_receipt_feasibility_downgrades_only`. |
| PRRT_kwDOTwgMRs62T9wu | Validate cursors before omission in `read_output::apply_command`; `round5_omitted_intent_validates_output_cursor`. |
| PRRT_kwDOTwgMRs62T9w0 | Nested schemes remain inside the password in `refusal::userinfos`; `multiple_url_credentials_are_redacted_globally` plus stored producer registration. |
| PRRT_kwDOTwgMRs62T9w4 | Isolate historical alias-only fixture; `start_declared_blocker_variants_match_published_bytes` fails when alias admission is removed and separately when exact inspection advice is reverted. |

## Class self-review

The complete PR diff was reviewed for schema replacement versus additive merging,
normalization of both sides, full IDs versus shorthand, policy bypasses followed
by a second strict read, all-field credential registration, cursor clipping and
validation order, and finalized receipt measurements. This found and corrected
the selection registry mismatch, admitted lineage reread, quoted alias advice,
and stored blocker-title credential registration described above. The final coverage corpus also schedules real directory moves immediately before
recursive and fallback reads to reproduce concurrent disappearance; both return
fatal I/O refusals even under force. The actual compiled build script is also executed with GNU and MSVC target
settings, asserting the exact linker directive across stable and nightly Cargo
artifact layouts. This covers the local report's build-script branch without any
coverage exclusion. Typed comment/list iterators avoid redundant
fallible guards after classification, while historical scalar comments remain
readable. The performance
revert harness source matcher follows the new parent-index signature and remains
covered by the signal-restoration harness tests. Its TypeScript file is now
included in the existing typecheck and lint commands.

The existing credential unit corpus now requires redaction of the copied password
from a second producer-key URL as well as its URL key. This strengthens its privacy
assertion to cover the expanded producer registry; machine contract identifiers
remain unchanged.

No differential assertion was weakened, skipped, or made oracle-version-specific.
No coverage exclusion or threshold changed. The aggregate still requires 100%
lines, functions, regions and branches. The oracle inventory includes all 21 oracle-backed integration targets and the
library target: `selection::tests::ranked_start_rechecks_reopened_blocker_against_oracle`
also executes the published CLI. All 27 integration targets and both unit targets
are exercised, so the inventory does not depend on a differential filename.
Instrumented targets run individually
with a 1200-second limit, two build jobs and two test threads; the aggregate's two
complete suite passes plus other checks exceed the permitted local aggregate
window, so CI certifies the complete release gate.

## Behavioral revert evidence

Thirteen independent negative controls ran in disposable copies: eleven restore
baseline-equivalent function or source behavior, and two deliberately disable
existing alias admission and normalized role handling to verify fixture sensitivity.
Each negative command
failed at a behavioral assertion, and the same focused command passed after the
function was restored. Compilation failures do not count as negative evidence.

| Reverted behavior | Failing regression |
| --- | --- |
| Additive status merge | `round5_additive_status_admission_matches_published_bytes` |
| Typed blocker encoding error | `round5_invalid_utf8_blocker_policy_matches_published_bytes` |
| Admitted lineage handling | `round5_invalid_utf8_blocker_policy_matches_published_bytes` |
| Complete blocker ID lookup | `round5_complete_blocker_identity_matches_published_bytes` |
| Nested URL credential span | `multiple_url_credentials_are_redacted_globally` |
| Producer credential registration | `round5_stored_blocker_title_registers_copied_credentials` |
| Cursor exclusion from clipping | `round5_long_intent_cursor_remains_executable` |
| Validation before omission | `round5_omitted_intent_validates_output_cursor` |
| Final feasibility downgrade | `round5_final_receipt_feasibility_downgrades_only` |
| Quoted relationship inspection | `start_declared_blocker_variants_match_published_bytes` |
| `depends_on` recognition | `start_declared_blocker_variants_match_published_bytes` |
| Normalized ranked status roles | `round5_status_registry_normalizes_aliases` |
| Performance source matcher | Signal-restoration cases in `scripts/harness.test.mts` |

The first feasibility differential alone passed on revert, so it was retained as
an oracle comparison and supplemented with a direct finalized-receipt boundary
corpus that discriminates the reverted function. Both small must-allow and large
must-downgrade pages are asserted in JSON and TOON. A frozen TOON page additionally
requires omission at 3236 tokens and delivery at 3237, including validation of a
snapshot cursor that becomes stale only after the second projection.

The tracker self-host health check initially detected merge-driver configuration
drift. Reinstalling the pinned merge drivers restored strict health to exit zero.
Existing guidance, stale-status and historical provenance advisories remain
unchanged; no history repair or rewrite was performed.

## Validation receipts

The published-oracle inventory is 22 targets, including the library target. Each
listed target passed with no failures or ignored tests:

| Oracle-backed target | Passed |
| --- | ---: |
| Library (`--lib`) | 429 |
| `admission_contract` | 12 |
| `branch_merge_contract` | 1 |
| `capability_contract` | 6 |
| `changes_contract` | 1 |
| `claim_release_differential` | 41 |
| `close_differential` | 8 |
| `concurrency_contract` | 2 |
| `conformance_differential` | 2 |
| `create_contract` | 8 |
| `harness_contract` | 3 |
| `list_differential` | 8 |
| `native_adapter_contract` | 13 |
| `oracle_launcher` | 2 |
| `orientation_differential` | 35 |
| `output_parity_differential` | 2 |
| `pagination_differential` | 30 |
| `parity_1021_contract` | 14 |
| `query_cost_contract` | 2 |
| `read_session_contract` | 7 |
| `release_pipeline_contract` | 21 |
| `search_collections_differential` | 17 |

The 21 integration targets contain 235 passing tests. Together with the six
native-only integration targets (93 tests), library (429) and binary (58), the
complete inventory is **815 passing Rust tests**. Five examples also compile and
execute their zero-test harnesses; they are not counted as regression tests.
The original eight differential targets move from **59 passed / 61 failed** at
the original version bump to **143 passed / zero failed**.

Exact test command forms, with each individual invocation capped by
`timeout 1200`, two build jobs and two test threads:

```sh
PM_RUST_REQUIRE_PUBLISHED_CLI=1 cargo +1.90.0 test --locked -j 2 --test TARGET -- --test-threads=2
PM_RUST_REQUIRE_PUBLISHED_CLI=1 cargo +nightly-2026-08-06 llvm-cov --branch --no-clean --json --output-path target/round5-final-coverage.json --locked -j 2 --test TARGET -- --test-threads=2
PM_RUST_REQUIRE_PUBLISHED_CLI=1 cargo +nightly-2026-08-06 llvm-cov --branch --no-clean --json --output-path target/round5-final-coverage.json --locked -j 2 --lib --bin pm-rust -- --test-threads=2
```

`TARGET` ranges over all 27 integration targets, including every target above.
These are actual test executions; an auxiliary coverage-binary refresh using
`--list` is not counted as a test pass. The complete normal and instrumented
suite durations already total more than the permitted local aggregate window,
so `just release-check` is left to CI at the unchanged thresholds.


The final instrumented report reaches all four unchanged release thresholds:

| Dimension | Covered / total | Percent |
| --- | ---: | ---: |
| Lines | 18745 / 18745 | 100% |
| Functions | 1922 / 1922 | 100% |
| Regions | 30539 / 30539 | 100% |
| Branches | 4256 / 4256 | 100% |

The final binary unit target passes 58 tests; CLI dispatch passes 25. Additional
process tests exercise cancelled change reads, both dependency output formats,
strict history refusals, and a closed stdout pipe across create, update, comment,
close, files, docs and context. Successful mutations remain committed after the
output failure. The comment-indexing boundary receives a fallible serialized
value, so its error path and historical scalar rows share the same non-generic
implementation. This avoids an untestable error branch on a value-only generic
instantiation while preserving serialization errors.

Final static and harness gates pass, each with a 1200-second cap:

```sh
cargo +1.90.0 check --locked -j 2 --all-targets --all-features
cargo +1.90.0 clippy --locked -j 2 --all-targets --all-features -- -D warnings
cargo +1.90.0 fmt --all -- --check
RUSTDOCFLAGS='--document-private-items -D missing-docs' cargo +1.90.0 doc --locked -j 2 --all-features --no-deps
just crash-check
```

`just crash-check` passes typecheck, lint and **90 Node tests**, with zero failures,
skips or cancellations. Its evidence gate verifies 21 receipts and 37 commit
references in a fresh origin clone. The full integration inventory includes the
seven-test identity audit. Dependency audit passes for 136 dependencies under the
existing advisory policy; the pre-existing allowed unmaintained `unsafe-libyaml`
advisory remains. No dependency or advisory-policy change is introduced.

All thirteen negative controls were refreshed against the final source. Every
negative command failed at an assertion, and every restored command passed. The
restored performance harness passes **25 tests**, with zero failures, skips or
cancellations. All mutations and restoration occurred in disposable copies;
the review worktree never contained a reverted implementation.

## Tracker health boundary

The exact CI self-host command
`node tests/oracle/launch.mts health --check-only --require-merge-drivers --strict-exit`
passes, as does the seven-test identity target. Changelog generation and its
verification run after the last tracker write.

A supplementary full probe,
`node tests/oracle/launch.mts health --strict-exit --json`, exits one for the two
historical chain mismatches already tracked by `pm-rust-b4yl`: `pm-rust-bj7a` and
`pm-rust-unih`. Both item/history pairs are byte-identical to `origin/main` and the
pre-round head. Evidence was appended to that existing follow-up, which remains
open and unclaimed. No history was repaired, rewritten or re-anchored. The
check-only CI pass does not certify those historical chains. The main parity and
cursor items remain in progress with their agent claims released.
