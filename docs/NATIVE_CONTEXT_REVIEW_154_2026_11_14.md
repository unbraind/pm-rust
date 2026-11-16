# PR 154 review evidence, 2026-11-14

The snapshot contains 127 threads, including 33 unresolved threads with all comments. The latest CodeRabbit review on df7ba61 has no outside-diff findings section. Threads remain open for the orchestrator.

Main was merged first in 8873d86. Shared tracker conflicts took main item/history pairs, followed by pinned CLI writes. Main lifecycle admission, scope handling, userinfo redaction, platform budgets and its current 2026.11.11 oracle remain the baseline. The orientation oracle now uses that same already-selected version.

## Thread decisions

| Thread | Decision | Regression or proof |
| --- | --- | --- |
| PRRT_kwDOTwgMRs61VSbR | Shared Invocation retains raw roles. The global-path case matches current oracle text consumption; the JSON expectation is not its published contract. | review154_lossless_retry_and_argument_roles |
| PRRT_kwDOTwgMRs61YRVc | Own children before registration writes, including preload failures. | registration; preload-registration |
| PRRT_kwDOTwgMRs61YRVn | Bound failed termination and detach handles while retaining inputs. | bounded-stop |
| PRRT_kwDOTwgMRs61YRV1 | Separate runner termination from external interruption. | runner-cause |
| PRRT_kwDOTwgMRs61YRV6 | Snapshot before the version probe. | version-snapshot |
| PRRT_kwDOTwgMRs61YRWO | Reject null statuses in every comparison. | signal-status |
| PRRT_kwDOTwgMRs61YRWX | Require the expected diagnostic and a named failed assertion. | mutation-diagnostic; legacy-mutation-diagnostic; inline-fixture-diagnostic; same-line-fixture-diagnostic |
| PRRT_kwDOTwgMRs61YRWb | Propagate symlink resolution failures. | link-errors |
| PRRT_kwDOTwgMRs61YRWf | Reject directory links to ancestors in proof copies. | directory-cycle |
| PRRT_kwDOTwgMRs61YRWl | Forward the configured interpreter. | interpreter |
| PRRT_kwDOTwgMRs61YRWn | Preflight both oracle installations before copying. | oracle-preflight |
| PRRT_kwDOTwgMRs61YRW1 | Retain original Cargo config locations and relative references. | cargo-configuration |
| PRRT_kwDOTwgMRs61YRXC | Preserve spawn errors and compiler stderr. | compiler-diagnostic |
| PRRT_kwDOTwgMRs61YRXM | Keep CommandRefusal directly; remove the wrapper enum variant. | sdk-classification |
| PRRT_kwDOTwgMRs61YRXY | Reject whitespace-only activity limits; keep empty fallback. | activity-whitespace |
| PRRT_kwDOTwgMRs61YRXq | Check preservation at the supplied generous budget. | generous-metadata |
| PRRT_kwDOTwgMRs61YS_r | Mark the receipt historical; identify receipt introduction and runner semantics, and disclose the unknown measured revision. | historical-receipt |
| PRRT_kwDOTwgMRs61YS_z | Add actual Windows child argv assertions and a Windows mutation proof. | windows-argv; Windows CI |
| PRRT_kwDOTwgMRs61YS_8 | Return partial context plus an unreadable-item warning. | partial-context |
| PRRT_kwDOTwgMRs61YTAA | Current initialization was already selected; align with main 2026.11.11 and assert current defaults. | context_intents_fixture |
| PRRT_kwDOTwgMRs61YTAG | Assert terminal receipts in both encodings without an oracle. | terminal-without-oracle |
| PRRT_kwDOTwgMRs61Z2QT | Use unique kernel-job identity and ignore stale PID files. | stale-windows-registration; kernel-job |
| PRRT_kwDOTwgMRs61Z2Qk | Find ps through standard executable locations. | ps-search |
| PRRT_kwDOTwgMRs61Z2Qz | Identify dangling-link proofs as symlink proofs. | dangling-kind |
| PRRT_kwDOTwgMRs61Z2RC | Refused native-only divergence: main oracle also rejects the first repeated section and keeps the invalid retry. | Pinned-contract follow-up |
| PRRT_kwDOTwgMRs61Z2RR | Keep explicit path argument spans lossless while sanitizing other diagnostics. | lossless-retry |
| PRRT_kwDOTwgMRs61Z2Rg | Refused native-only divergence: pinned summaries also discard packet bounds. | Pinned-contract follow-up |
| PRRT_kwDOTwgMRs61Z2Rz | Keep main wire measurements and bound terminal canonical omissions. | list-wire; terminal-without-oracle |
| PRRT_kwDOTwgMRs61Z2R8 | Run aliases outside any tracker and restore the human parser warning. | argument-selected-tracker |
| PRRT_kwDOTwgMRs61Z2SE | Require a receipt for every intent and encoding. | per-intent-receipt |
| PRRT_kwDOTwgMRs61aiCl | Update mutation anchors and the settings guard selector. | managed-definitions; whole-settings-fallback |
| PRRT_kwDOTwgMRs61cCgF | Keep 60/90-minute aggregate budgets; share bounded recovery producer work; compare every product/platform gate. | canonical-probe-cache; Final-head CI measurement |
| PRRT_kwDOTwgMRs61cCgN | Validate stored settings before merging external schemas. | stored-validation |

## Behavioral reverts

Run node scripts/context-round154-revert.mts, or just review154-revert-proof. Each case uses an owned disposable source/build, passing control, named mutation-specific assertion failure, and restored passing control. Compiler, setup, skipped, signal and timeout outcomes cannot certify a proof. Native commands use two build and test workers.

Thirty-nine Linux behavioral reverts have 117 verified states: 78 passing controls/restorations, 17 expected Rust assertion failures and 22 expected Node failures. The initial complete 37-case runner passed through pinned PM with an explicit 3600-second timeout. After strengthening legacy diagnostics, all 22 Node cases were rerun; the 17 native behavior proofs remain unchanged. The Windows argv proof runs in Windows aggregates after release checks and is not claimed locally.

Legacy runners accept only explicitly marked behavior assertions, including declared comparison helpers for wrapper tests. Rust assertions use a preceding `// proof-behavior` comment; Node and synthetic fixtures may put `/* proof-behavior */` immediately inside the assertion call. Inline fixture assertions, fixture helpers, other assertions on the same line, column-prefix matches and later TAP stack frames cannot certify a proof. All 101 audited legacy selections have a declared diagnostic. The two added guard reversions independently remove marker admission and column bounds, and each fails at its specific negative regression.

Additional path-only diagnostic and stored-full-depth regressions prevent drift against the current pin. Self-review reproduced a partial-warning estimate of 770 tokens against 790 delivered JSON tokens; warning attachment now stabilizes JSON and TOON intent estimates before output admission. All six imported settings enum sets match the pinned SDK. The growing compaction fixture targets the ordinary wire ceiling with an explicit row limit, so the intent ceiling cannot terminate the probe before wire compaction.

Additional generic-warning and shared-refusal regressions preserve main userinfo redaction and duplicate-detail suppression in every command family. Current intent and list receipts use the delivered encoding; canonical budgets also bound terminal omitted-result envelopes. A 9.309-second tiny missing-reference reproduction exposed repeated producer work across canonical recovery probes. The query now shares its bounded producer cache, and a deterministic regression compares cached/fresh JSON and TOON delivery across decimal widths and requires zero additional builds on repeated probes. The five-second process guard is unchanged.

## Pinned-contract refusals

For context --activity-limit --path --json, both the current oracle and native CLI exit 2 with identical text diagnostics. Commander consumes the JSON token as the global path argument. Raw argument-role preservation is tested independently; the original finding's JSON-output expectation is not supported by the published parser.

The 2026.11.11 oracle reports hierarchy as rejected for --section hierarchy --section invalid, and its example retains the invalid command. The byte-exact reviewed-context differential includes this case. A native-only correction would break the pinned contract.

The dependency SDK attaches bounds from result.context.meta.truncated, but summaries already removed context. The current oracle therefore omits bounds too. Both limitations are tracked on pm-rust-context-history-output and are not described as fixed.

## Validation

The continuation uses targeted local gates. Full release and coverage validation is delegated to CI. No new full-suite or full-coverage run was started after that instruction. The earlier full Cargo attempt reported 960 passing tests and no failures before interruption; it is incomplete evidence.

Fresh targeted Rust checks pass 18 tests across 14 commands, with two build jobs and two test workers. The affected Node files report 100 unique passing tests and two Windows-only skips: four fixture-readiness checks initially timed out under concurrent host load and passed alone at the unchanged timeout. Formatting, Linux clippy with denied warnings, TypeScript typecheck and lint pass. The legacy diagnostic audit covers 101 selections. The preceding complete harness gate also passed 157 tests with two Windows-only skips; that earlier result is separate from the final targeted checks.

| Targeted Rust command | Passed |
| --- | ---: |
| `cargo test -j 2 --locked --lib review154 -- --test-threads=2` | 4 |
| `cargo test -j 2 --locked --bin pm-rust review154 -- --test-threads=2` | 2 |
| `cargo test -j 2 --locked --bin pm-rust current_orientation_diagnostics_preserve_other_commands -- --test-threads=2` | 1 |
| `cargo test -j 2 --locked --lib context_stored_settings_fallback_precedes_overrides -- --test-threads=2` | 1 |
| `cargo test -j 2 --locked --lib context_settings_warning_survives_refusals -- --test-threads=2` | 1 |
| `cargo test -j 2 --locked --lib item_metadata_named_context_intent_compacts_under_budgets -- --test-threads=2` | 1 |
| `cargo test -j 2 --locked --lib current_context_tables_keep_ambiguous_cells_lossless -- --test-threads=2` | 1 |
| `cargo test -j 2 --locked --lib current_compact_tables_round_trip_through_pinned_sdk -- --test-threads=2` | 1 |
| `cargo test -j 2 --locked --lib section_activity_storage_and_ordering -- --test-threads=2` | 1 |
| `cargo test -j 2 --locked --test orientation_differential relationship_large_edge_limit_finishes_at_budget_floor -- --test-threads=2` | 1 |
| `cargo test -j 2 --locked --test orientation_differential context_growing_compaction_probes_match_current_oracle -- --test-threads=2` | 1 |
| `cargo test -j 2 --locked --test pagination_differential triage_omitted_intent_receipts_match_published -- --test-threads=2` | 1 |
| `cargo test -j 2 --locked --test pagination_differential legacy_after_without_after_index_omits_final_triage_page -- --test-threads=2` | 1 |
| `cargo test -j 2 --locked --test read_contract sdk_streaming_preserves_invalid_utf8_refusals -- --test-threads=2` | 1 |

The affected Node command is:

```sh
node --test --test-concurrency=2 scripts/context-intents-revert.test.mts scripts/context-proof-definitions.test.mts scripts/context-review-safety.test.mts scripts/harness.test.mts scripts/native-adapter-capabilities-revert.test.mts scripts/context-round154.test.mts
```

Baseline e5ec703, run 43541768359, has all 10 Rust, latency and aggregate product jobs green. Every corresponding final-head job must be compared, including macOS, Windows and Bun, regardless of required-check status. CI must still prove the full suite, all four 100-percent Linux branch-coverage totals, platform aggregates and the actual Windows argv mutation.

The final-head CI run URL and platform comparison will be recorded in the [PR validation receipt](https://github.com/unbraind/pm-rust/pull/154#review-round-2026-11-14) after push. This keeps the tested commit fixed while exposing the run and every platform result. Changelog generation and verification passed after the last tracker write and claim release. Clean-profile tracker health passes; every changed history preserves its main prefix.
