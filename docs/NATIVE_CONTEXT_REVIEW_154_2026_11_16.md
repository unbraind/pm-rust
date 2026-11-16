# PR 154 continuation evidence, 2026-11-16

## Main integration

Fast-forward pull reported current. Merge `7ac2477` incorporates main `77c2ff2718019cd1e94def16b5e0f7065550e122` without conflicts and retains both sides. Main updates only the pm-github issue-sync and closeout pins. PR 173 remains open; its Fleet CI v2 layout is not merged and was not copied into this branch. No force push, PR merge, review-thread interaction or workflow dispatch was performed.

## Named unresolved threads

The six requested threads remain open. The fixes below already exist in `a53dd5fbb2bad58020f1619ede96c92519838530`; this continuation verifies current behavior and strengthens cross-platform regressions.

| Thread | Current decision and evidence |
| --- | --- |
| `PRRT_kwDOTwgMRs61VSbR` | Refuse the requested native-only JSON behavior: for `context --activity-limit --path --json`, the pinned 2026.11.11 oracle consumes `--json` as the global path value and returns the same status-2 text refusal as native. `context_refusal_value_roles_match_current_oracle` compares all five numeric controls, five flag-shaped values and both encodings, including this exact invocation. `review154_lossless_retry_and_argument_roles` separately asserts raw role preservation. |
| `PRRT_kwDOTwgMRs61YRVc` (outdated) | Already fixed in a53dd5f: `groups.add(pid)` and registration ownership precede the registration write. `review154 process ownership registration` and `preload-registration` inject registration failures and assert worker termination before cleanup. |
| `PRRT_kwDOTwgMRs61YRVn` | Already fixed in a53dd5f: the bounded abandonment timer settles the invocation, destroys pipes and unreferences the child; cleanup refuses to remove scratch when termination is unproven. `review154 process ownership windows-failed-stop` verifies handle detachment and retained inputs after failed job termination. |
| `PRRT_kwDOTwgMRs61YRV1` (outdated) | Already fixed in a53dd5f: `runnerStopped` distinguishes timeout/output-limit termination from an external interrupt. `review154 process ownership timeout` asserts the original timeout error and `wasInterrupted === false`. |
| `PRRT_kwDOTwgMRs61YRV6` | Already fixed in a53dd5f: the authoritative baseline precedes `verifiedOracleVersion`. `review154 real-copy refuses version-write` mutates during the version probe and requires the authoritative-copy refusal. |
| `PRRT_kwDOTwgMRs61YRWO` (outdated) | Already fixed in a53dd5f: every comparison rejects either null status. `review154 real-copy refuses signal-intent` injects matching null statuses and requires the specific intent-parity refusal. |

## Original failing checks and main comparison

Latest completed main CI is [43729555388](https://github.com/unbraind/pm-rust/actions/runs/43729555388) at 77c2ff2. All ten Rust, latency and aggregate jobs pass; each aggregate reports 100% lines, functions, regions and branches. These failures are not already present on that main run.

| PR job in run 43587230331 | Cause | Correction or boundary |
| --- | --- | --- |
| Release gate aggregate, job 132506468672 | Tests passed; exact coverage gate found two uncovered regions and five branches. | Add intent feasibility transitions, non-object intent measurement, unsupported-context warning, empty activity timestamp and mixed diagnostic/path CRLF regressions. |
| Release gate aggregate bun, job 132506468729 | Same two-region/five-branch coverage failure. | Same regression additions; preserve all four 100% thresholds. |
| Release gate aggregate windows-latest, job 132506468682 | Same gaps plus an uncovered malformed-command return: the fixture used a single quote, which is ordinary data in Windows command parsing. | Use an unterminated double quote, invalid on both platforms, plus the shared regressions. |
| Release gate aggregate macos-latest, job 132506468773 | `review154 Cargo options reach every independent proof runner` expected the lexical temporary alias, while ESM canonicalized the source location. | Canonicalize the expected configuration directory. An explicit directory alias reproduces this on Linux: the former expectation fails; the corrected assertion passes. Original configuration file and relative-reference checks remain strict. |
| Native latency windows-latest, job 132506468684 | `list` median-of-window-p95 160.3372 ms exceeds the unchanged 150 ms ceiling. | No budget change. Windows causality remains subject to fresh CI; cross-platform paired measurements are supplementary evidence only. |

Windows latency numbers (ms), using the existing five warmups and three 30-sample fresh-process windows:

| Command | PR p50 | PR p95 | Main p50 | Main p95 | Ceiling |
| --- | ---: | ---: | ---: | ---: | ---: |
| get | 17.5412 | 21.6787 | 14.5248 | 17.1245 | 50 |
| list | 101.1058 | 160.3372 | 94.2109 | 108.6863 | 150 |
| context | 193.6876 | 205.6215 | 174.7540 | 188.9128 | 250 |

List window p95 values were 160.3372 / 221.1699 / 134.6614 on the PR and 108.6863 / 110.1272 / 104.6220 on main. Variance is visible, but separate hosted-runner measurements alone cannot prove that Windows runner noise is the entire cause. Latency budgets and CI timeouts are unchanged by this continuation.

## Local validation

The changed script and the five other affected Node test files pass: 100 tests and two Windows-only skips. The exact command is:

```sh
node --test --test-concurrency=2 scripts/context-intents-revert.test.mts scripts/context-proof-definitions.test.mts scripts/context-review-safety.test.mts scripts/harness.test.mts scripts/native-adapter-capabilities-revert.test.mts scripts/context-round154.test.mts
```

The changed file also passes alone and through `pm test --run --only-last --progress` (18 tests). The new native review tests pass through the same PM runner (six tests). The directory-alias regression fails at its expected assertion with the former path expectation and passes when restored.

Formatting, all-target clippy with denied warnings, TypeScript typechecking and lint pass. History-prefix validation preserves every existing main prefix (three shared histories checked; three branch-only streams); retained evidence commits also pass. Tracker health with isolated global registrations passes `health --check-only --require-merge-drivers --strict-exit`. The ordinary profile reports an inherited extension version skew between the pinned 2026.11.11 host and a globally registered 2026.11.16 pm-github; registrations were not modified. Check-only health does not certify the previously documented full-history discontinuities.

The complete CLI unit target passes 89 tests:

```sh
CARGO_BUILD_JOBS=2 RUST_TEST_THREADS=2 cargo test -j 2 --locked --bin pm-rust -- --test-threads=2
PM_RUST_REQUIRE_PUBLISHED_CLI=1 CARGO_BUILD_JOBS=2 RUST_TEST_THREADS=2 cargo test -j 2 --locked --test orientation_differential context_refusal_value_roles_match_current_oracle -- --test-threads=2
```

The refusal differential passes all 50 numeric-control/value/encoding comparisons. The required focused library review and activity regressions pass:

```sh
CARGO_BUILD_JOBS=2 RUST_TEST_THREADS=2 cargo test -j 2 --locked --lib review154 -- --test-threads=2
CARGO_BUILD_JOBS=2 RUST_TEST_THREADS=2 cargo test -j 2 --locked --lib section_activity_storage_and_ordering -- --test-threads=2
```

The supplementary whole-library attempt ran all 489 tests: 488 passed and the untouched `read_session::tests::snapshot_temp_creation_failure_is_typed` returned `ENOENT`. Concurrent Cargo invocations replaced the library executable after that run started; the fixture respawns `current_exe`. The isolated exact fixture passes both directly and via Cargo after the overlap ends. This broad run is explicitly **not** reported as a clean full-library pass. All touched-module tests in it, including the long exhaustive intent-budget test, passed. No runtime code or unrelated test was changed to conceal the failure.

The unchanged relationship budget-floor differential also passes (12 bounded native cases plus oracle comparisons), using:

```sh
PM_RUST_REQUIRE_PUBLISHED_CLI=1 CARGO_BUILD_JOBS=2 RUST_TEST_THREADS=2 cargo test -j 2 --locked --test orientation_differential relationship_large_edge_limit_finishes_at_budget_floor -- --test-threads=2
```

A supplementary Linux measurement compares release builds of merged runtime 7ac2477 and main 77c2ff2 on one host, alternating runtime order for every sample. Each command has five warmups per runtime and three windows of 30 fresh processes per runtime. The fixture is the latency gate's 3,000-item synthetic tracker; context uses a fixed timestamp to compare identical output. All captured stdout and stderr match. These results show no local p95 regression and do not certify Windows causality:

| Command | PR p50 (ms) | Main p50 (ms) | PR p95 (ms) | Main p95 (ms) |
| --- | ---: | ---: | ---: | ---: |
| get | 6.157 | 5.905 | 7.521 | 7.736 |
| list | 78.109 | 75.536 | 111.187 | 118.780 |
| context | 154.042 | 151.493 | 182.597 | 184.605 |

Release binary SHA-256:

- pr: `79df272e82c96b20cd78fef675021cf8cc234c28dae35361e98925b17eb910b1`
- main: `9f38a5753ed5a5d2e7e4a28689cb3022c736b51bb47849a08f5c3213e1eacec0`

Raw timing samples remain local; no binaries or temporary artifacts are tracked. `npm run changelog:full` and `npm run changelog:check` pass after the final PM release; generated content is unchanged. The PM item remains in progress and unclaimed for orchestrator verification. Full release coverage and platform certification remain the responsibility of fresh CI. Monitoring ends no later than 30 minutes after the last push.
