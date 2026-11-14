# Oracle review follow-up: 2026.11.11

This extends the initial parity evidence in `NATIVE_PARITY_2026_11_11.md`.
The admission baseline at the reviewed head was nine passed and three failed.
Native scalar refusals again use their published structured default, while
captured admission previews use published scaffold defaults without making
observational reads write schema files. Admission assertions remain unchanged.

## Complete oracle-backed target inventory

The inventory includes direct published CLI consumers, SDK/Node harness consumers,
and launcher/pin/release metadata guards. It was checked from all `tests/*.rs`
references to `published_cli`, `PM_RUST_REQUIRE_PUBLISHED_CLI`, `tests/oracle`,
and `support/node_harness`, plus transitive support-module imports.
All targets use this command through linked pm tests, sequentially:

```sh
PM_RUST_REQUIRE_PUBLISHED_CLI=1 CARGO_BUILD_JOBS=2 timeout 1200 cargo +1.90.0 test --locked -j 2 --test TARGET -- --test-threads=2
```

- `admission_contract`
- `branch_merge_contract`
- `capability_contract`
- `changes_contract`
- `claim_release_differential`
- `close_differential`
- `concurrency_contract`
- `conformance_differential`
- `create_contract`
- `harness_contract`
- `list_differential`
- `native_adapter_contract`
- `oracle_launcher`
- `orientation_differential`
- `output_parity_differential`
- `pagination_differential`
- `parity_1021_contract`
- `query_cost_contract`
- `read_session_contract`
- `release_pipeline_contract`
- `search_collections_differential`

The separate envelope gate runs library, binary, CLI dispatch, ownership,
parity and release-pipeline contracts. Clippy and fmt retain their original
thresholds. TypeScript crash checks also run the real pinned oracle and the
new cross-platform pin guard. The aggregate normal/coverage release check
exceeds the requested 45-minute budget and remains for CI.

## Review fixes and compatibility boundaries

- Refusal projection enrolls persisted holder/detail credentials before rendering
  JSON or human output. Whitespace-bearing userinfo is registered as a complete
  span, with shell and JSON spellings. Must-allow controls include prose handles,
  email outside URLs, and URL path/query/fragment authentication separators.
- Recovery reads scope only from parsed bootstrap positions. A positional close
  reason cannot redirect completion advice. String-only history retries retain
  explicit scope without inventing argument arrays absent from the oracle.
- Human partial failures retain completed-step explanations. Published JSON
  omits that explanation, and the JSON differential requires the omission.
- Atomic claim/start checks configured lifecycle terminal roles under the same
  lock as the ownership transition, including forced starts. Tests call the
  actual published atomic-start implementation and assert unchanged durable bytes.
- Packet measurement retention is capped at eight shapes. A 512-reference prefix
  test asserts both retained key bytes and entry count, rather than elapsed time.
- Recent activity compares parsed instants for its exclusive upper bound;
  published lexical ordering is preserved even across mixed timestamp precision.
- Crash validation, remedies and report provenance share the current oracle
  constant; a normal Cargo harness test also executes the manifest pin guard.
- The TOON 256-token triage refusal is now an actual exact comparison case.

## Published behavior observed during review

The pinned 2026.11.11 CLI still discloses a whitespace-bearing stored holder
password on a later foreign-claim refusal. It also treats path/query/fragment
`@` text as userinfo in its permissive helper. Native emission is hardened for
both cases as explicitly requested; ordinary durable input remains readable.
These observations use synthetic credentials only. No upstream issue was filed.

The published parser routes positional `--pm-path=other` as scope even after
`--`, refusing the probe before closure. Native parsing treats it as close text;
its recovery must consequently reference the workspace actually mutated. A
native process regression verifies that contract without weakening an existing
differential assertion. Published recent activity sorts timestamp strings
lexically, so mixed precision can display out of chronological order; that
ordering is retained for exact compatibility. Published partial-failure JSON
omits the successful-step explanation while human output includes it.

Validation counts and independent disposable-copy behavioral revert results
are recorded below after the complete run.

## Review-thread evidence map

| Thread | Production boundary | Regression |
| --- | --- | --- |
| `PRRT_kwDOTwgMRs62FQ-6` | `src/refusal.rs` | `persisted_credentials_redact_every_refusal_copy` |
| `PRRT_kwDOTwgMRs62FQ_D` | `src/orientation_context/sections.rs` | `activity_mixed_fractional_precision_matches_published_bytes` |
| `PRRT_kwDOTwgMRs62FkYl` | `src/main.rs` | `positional_close_text_keeps_actual_recovery_scope` |
| `PRRT_kwDOTwgMRs62FkYv` | `src/main.rs` | `scoped_history_retry_matches_published_bytes` |
| `PRRT_kwDOTwgMRs62FkY1` | `src/main.rs` | `failed_start_preserves_claim_and_published_recovery_receipt` |
| `PRRT_kwDOTwgMRs62FkY8` | `src/mutation.rs` | `atomic_start_rejects_configured_terminal_status_like_oracle` |
| `PRRT_kwDOTwgMRs62FkZB` | `src/orientation_deps/packet.rs` | `measurement_cache_retention_is_linear` |
| `PRRT_kwDOTwgMRs62FkZG` | `src/refusal.rs` | `ordinary_invocations_preserve_refusals` |
| `PRRT_kwDOTwgMRs62FkZL` | `src/refusal.rs` | `persisted_holder_credentials_never_reach_refusal_streams` |
| `PRRT_kwDOTwgMRs62FkZP` | `scripts/crash-harness.mts; scripts/crash-oracle-pin.test.mts` | `crash harness and differential oracle pins never diverge` |
| `PRRT_kwDOTwgMRs62FkZU` | `tests/pagination_differential.rs` | `toon_budget_envelopes_and_snapshot_walks_match` |

## Full Rust results

| Oracle-backed target | Passed | Failed / ignored |
| --- | ---: | ---: |
| `admission_contract` | 12 | 0 / 0 |
| `branch_merge_contract` | 1 | 0 / 0 |
| `capability_contract` | 6 | 0 / 0 |
| `changes_contract` | 1 | 0 / 0 |
| `claim_release_differential` | 30 | 0 / 0 |
| `close_differential` | 6 | 0 / 0 |
| `concurrency_contract` | 2 | 0 / 0 |
| `conformance_differential` | 2 | 0 / 0 |
| `create_contract` | 8 | 0 / 0 |
| `harness_contract` | 3 | 0 / 0 |
| `list_differential` | 8 | 0 / 0 |
| `native_adapter_contract` | 13 | 0 / 0 |
| `oracle_launcher` | 2 | 0 / 0 |
| `orientation_differential` | 34 | 0 / 0 |
| `output_parity_differential` | 2 | 0 / 0 |
| `pagination_differential` | 26 | 0 / 0 |
| `parity_1021_contract` | 14 | 0 / 0 |
| `query_cost_contract` | 2 | 0 / 0 |
| `read_session_contract` | 7 | 0 / 0 |
| `release_pipeline_contract` | 20 | 0 / 0 |
| `search_collections_differential` | 17 | 0 / 0 |

The conservative inventory totals **216 passing target tests**, including
**125 differential tests**. The initial differential baseline was 59 passed and
61 failed across 120 tests; the initial admission baseline was nine passed and
three failed. The current differential suite includes three additional cases
relative to the reviewed head, while all twelve unchanged admission tests pass.

The envelope gate passes **512 tests**: 399 library, 48 binary, 20 CLI dispatch,
11 ownership, 14 parity and 20 release-pipeline contracts. The parity and
release-pipeline substring selectors also matched the two linked combined
envelope commands, so those envelope tests were repeated; these totals count
unique target tests rather than repeated executions. Clippy and fmt pass.

## Independent behavioral reverts

Each revert changed only its production function or validation constant in a
disposable copy. Every negative run failed at assertions (not compilation),
and every exact same command passed again after restoring the fixed source.
The main worktree was never mutated by these checks.

| Reverted boundary | Negative result | Restored result |
| --- | --- | --- |
| `persisted-credentials` | Assertion failure | Passed |
| `authority-boundary` | Assertion failure | Passed |
| `linear-cache` | Assertion failure | Passed |
| `configured-terminal` | Assertion failure | Passed |
| `positional-scope` | Assertion failure | Passed |
| `string-retry-scope` | Assertion failure | Passed |
| `partial-failure-why` | Assertion failure | Passed |
| `activity-instant` | Assertion failure | Passed |
| `scalar-admission` | Assertion failure | Passed |
| `preview-defaults` | Assertion failure | Passed |
| `crash-pin` | Assertion failure | Passed |

The TOON 256-token review change adds coverage to an existing production
contract, so it has no changed production function to revert. Its exact oracle
exit-status and stream comparison passed in the full pagination target.

Focused revert commands use `CARGO_BUILD_JOBS=2`, the required oracle,
`timeout 1200 cargo +1.90.0 test --locked -j 2`, the target and test filter,
and `-- --test-threads=2`. The pin revert uses
`timeout 1200 node --test --test-concurrency=2 scripts/crash-oracle-pin.test.mts`.

## Crash-suite acknowledgement drift

After accepting the new oracle pin, the first complete crash suite passed 87
and failed three tests. Durable bytes matched, but published CLI comment
acknowledgements now add a one-based `index` to each comment. Native CLI comment
receipts now add the same indexes through `comment_payload`; stored item and
history bytes are untouched. Existing crash equality assertions remain exact.

The indexed comments are a CLI receipt projection; the native mutation result
hash continues to describe stored metadata without display indexes.

The refreshed complete crash gate passes **90 tests**, with zero failures or
skips, plus TypeScript, ESLint, append-only history and retained-evidence guards.
The three initially failing cases also pass focused checks. After the receipt
change, the complete 512-test envelope gate, clippy, fmt and all other contract
targets were refreshed successfully. The final claim/release refresh passes 30.

A twelfth independent revert restores only `comment_payload` from the reviewed
head: `real pm corpus preserves acknowledged history and independently replays
retained failures without git` then fails its exact acknowledgement assertion.
The same test passes after restoration. This uses a disposable copy and the
unchanged original crash assertion. All twelve negative checks are assertion
failures; all twelve restored checks pass.

The tracker task remains in progress and unclaimed for orchestrator verification.
Platform and aggregate coverage checks remain with CI; no coverage threshold or
release recipe was reduced.
