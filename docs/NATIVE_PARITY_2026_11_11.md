# Published oracle parity: 2026.11.11

The locked oracle and native compatibility constant use the npm latest observed
for this change, `@unbrained/pm-cli@2026.11.11`. This replaces the dependency-only
update proposed in #164.

The initial full differential baseline was 59 passed and 61 failed across 120
tests. The claim/release target alone was 20 passed and 7 failed. No differential
comparison is skipped, normalized more permissively, or conditioned on a version.

## Refusal contracts

The shared native CLI refusal boundary renders recovery before either JSON or
human output, preserves explicit tracker scope and output arguments, and applies
credential projection to every field. URL userinfo and registered password
fragments are redacted; suggested retries are withheld when credentials must be
restored. Fixed contract identifiers remain intact, and producer detail keys are
redacted with collision-safe suffixes.

- `already_claimed_by` includes `claimed_by`, `claimed_at`, `claim_source`, and
  `item_id`. Its prose and provenance use the same latest assignee-patch evidence;
  stale or missing evidence reports an unknown source.
- `missing_required_argument` and `ambiguous_option_combination` retain the
  published target-selection guidance. Selected claim-start transitions happen
  in the same ownership transaction.
- `close_validation_missing_fields` recommends the original close invocation
  with missing evidence flags added, and supplies the completing update as its
  next-best command. Recovery preserves scope and JSON output.
- Human refusals suppress repeated detail when it equals the title, include the
  same ownership provenance as JSON, and bound the displayed legal domain while
  retaining complete structured evidence.
- Claim session-attribution failure after commit is a successful claim with
  `claim_semantic_attribution_unrecorded`. Release session-lock failure remains a
  refusal. Existing lock files, item state, history, and absence of session state
  are asserted independently.

## Other measured oracle changes

The complete suite also requires compact TOON string-list and null-column
encodings, compact complete omission receipts, actual-format intent estimates,
newest-first compact history with total/order echoes, context test-summary scope,
an exclusive activity-window upper bound, required-root-evidence-first
relationship ordering and bounded-field omission declarations, configured
lifecycle fallback without scaffold writes, and immutable close completion
provenance. Infeasible intent receipts now exit two and use the published recovery
budget margin. Explicit unbounded output retains final-page rows. Canonical
numeric output budgets take precedence over legacy intent budgets, and intent
projection reserves finalized-envelope overhead through the same delivery path.
Recovery probes cache exact rendered packet shapes while still checking every
candidate; scalar counter widths are checked against full rendering, including a boundary
where two fixed points exist and the original counter determines the published
answer.

The pagination harness supplies the published `--pm-path` equivalent of native
`--workspace`, so scope-preserving refusal comparisons receive equivalent
invocations. Unicode continuation cases follow the emitted producer cursor with
`--after` and independently retain output-snapshot replay and pair-preserving
compaction assertions. Frozen semantic expectations were updated to the measured contract;
complete stream and durable-state comparisons remain exact.

Potential published regression: a claim or composed start can complete while
semantic session attribution fails, reporting only
`claim_semantic_attribution_unrecorded`. The same stale session lock still refuses
a release. This can leave durable ownership without semantic session evidence;
the native implementation mirrors the published result. No upstream issue was
filed for this observation.

Published behavior is preserved independently of whether it is desirable. The
local validation record and behavioral revert proof are linked from
`pm-rust-oracle-1111`; platform and aggregate release certification belong to CI
when the aggregate gate cannot fit the requested local time budget.


## Validation commands and boundaries

Every differential target is run separately with the required published CLI,
`CARGO_BUILD_JOBS=2`, a 1,200-second timeout, and `--test-threads=2`:

```sh
PM_RUST_REQUIRE_PUBLISHED_CLI=1 CARGO_BUILD_JOBS=2 timeout 1200 cargo +1.90.0 test --locked -j 2 --test TARGET -- --test-threads=2
```

The targets are `claim_release_differential`, `close_differential`,
`conformance_differential`, `list_differential`, `orientation_differential`,
`output_parity_differential`, `pagination_differential`, and
`search_collections_differential`.

The envelope gate is:

```sh
PM_RUST_REQUIRE_PUBLISHED_CLI=1 CARGO_BUILD_JOBS=2 timeout 1200 cargo +1.90.0 test --locked -j 2 --no-fail-fast --lib --bin pm-rust --test cli_dispatch --test ownership_contract --test parity_1021_contract --test release_pipeline_contract -- --test-threads=2
CARGO_BUILD_JOBS=2 timeout 1200 cargo +1.90.0 clippy --locked -j 2 --all-targets --all-features -- -D warnings
timeout 1200 cargo +1.90.0 fmt --all -- --check
```

The aggregate `just release-check` runs the full suite twice, once normally and
once under branch coverage, plus additional crash, documentation, audit and
changelog checks. Measured differential durations put those two passes beyond
the requested 45-minute aggregate budget. The npm/Bun aliases invoke that same
aggregate; aggregate coverage and all-platform certification are deferred to CI.
No coverage threshold or release gate was changed.

## Behavioral revert evidence

Five independent disposable copies retained the current oracle and assertions:

| Reverted function | Differential that failed at an assertion |
| --- | --- |
| `refusal::redact` replaced with a no-op | `scoped_refusals_redact_credentials_without_changing_durable_bytes` |
| `ownership_error_json` restored from the base | `claim_refusal_uses_matching_latest_ownership_evidence` |
| `toon_output::table` restored from the base | `search_matches_oracle_small_toon` |
| `list_intent::prepare` restored from the base | `generated_utf16_cut_offsets_match_locked_oracle` |
| Cached counter initialization reset to a minimum width | `cached_render_measurements_match_exact_wire_bytes` |

The credential revert first fails on the synthetic credential-bearing URL,
following successful ordinary-message cases. Each scratch function was restored
in a `finally` block. The main worktree was never mutated for these proofs.

## Measured results

| Differential target | Before passed / failed | After passed / failed |
| --- | ---: | ---: |
| `claim_release_differential` | 20 / 7 | 29 / 0 |
| `close_differential` | 0 / 6 | 6 / 0 |
| `conformance_differential` | 1 / 1 | 2 / 0 |
| `list_differential` | 8 / 0 | 8 / 0 |
| `orientation_differential` | 9 / 23 | 32 / 0 |
| `output_parity_differential` | 1 / 1 | 2 / 0 |
| `pagination_differential` | 11 / 15 | 26 / 0 |
| `search_collections_differential` | 9 / 8 | 17 / 0 |

Total: **59 passed / 61 failed** before; **122 passed / 0 failed** after. The after
count includes two new claim/refusal regressions. Every run required the locked
published CLI and reported zero ignored tests. Orientation was refreshed after
the final counter-width correction.

The final envelope gate passed **505 tests**: 398 library, 44 binary, 18 CLI
dispatch, 11 ownership, 14 parity, and 20 release-pipeline contracts. Clippy, fmt,
and five independent behavioral revert proofs passed their expected gates.
