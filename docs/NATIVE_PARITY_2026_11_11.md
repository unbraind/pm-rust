# Published oracle parity: 2026.11.11

Round-two admission, crash-pin and review evidence is recorded in
[ORACLE_REVIEW_2026_11_11.md](ORACLE_REVIEW_2026_11_11.md). The counts below
are the initial implementation baseline; the follow-up includes every oracle-backed target.

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

## Round-three review boundaries

Credential projection now scans every URL in each invocation argument and refusal
field before splitting whitespace. Both password registration and projection use
the same global span finder. Comma- and semicolon-separated URLs, a later URL
with a whitespace-bearing password, copied password fragments and both output
streams are covered. Surrounding text remains intact.

There is an intentional divergence from the pinned oracle described in
[pm-cli #1707](https://github.com/unbraind/pm-cli/issues/1707): a public URL such as
`https://www.npmjs.com/package/@scope/x` remains unchanged, including when it
precedes a credential URL. A URL carries userinfo when `@` precedes the first
`/`, `?` or `#` after the scheme, or when `:` precedes the first `/`
and `@` occurs later. The latter conservative case still redacts unencoded slash
passwords such as `https://user:abc/def+gh==@host/x`. Username-only authorities
and passwords containing colons are also projected. Historical tracker data
continues to read; this changes emitted refusals rather than persisted identities.

For delimited close commands, evidence recovery flags are inserted immediately
after `close`, before the original target/options and `--` positional reason,
matching the published ordering. Human close-evidence refusals now include the
published `Run` template and recovery fields; JSON retains the published envelope.
Both emitted retry arrays were executed with real evidence in a non-git tracker:
they successfully closed the item with identical receipt, item and history bytes.

The remaining review requests were checked against the actual 2026.11.11 oracle:

- JSON partial-start refusals omit the completed-step `why`; human output keeps
  it. The exact existing differential tests retain that distinction. Adding the
  explanation to JSON is declined because it changes published bytes.
- Numeric `output_budget=3000` accepts legacy `token_budget` values `abc`, `255`,
  `256` and `1000`. The new exact JSON and TOON corpus preserves this measured
  precedence. Rejecting either invalid legacy value in this combination is
  declined because the oracle accepts it.
- An invalid context clock returns `unknown_error`, exit one, with detail
  `Invalid time value`. Native cutoff validation now emits those exact JSON and
  human bytes before reading context state. The requested argument-error
  classification is not used because the oracle reports a runtime error.
  Equivalent RFC 3339 offsets remain accepted and produce the same exclusive
  activity window as the oracle's UTC clock.

The review read covered all six requested threads in full. GitHub returned
fifteen threads, seven unresolved; the seventh unresolved thread is the outdated
configured-terminal finding already accepted in round two. Review threads remain
open for the orchestrator.

### Round-three measured validation

Every oracle-backed target uses the required published CLI, a 1,200-second cap
and two test threads via the command template documented above. All twenty-one
targets pass with zero failures or ignored tests:

| Target | Passed |
| --- | ---: |
| `admission_contract` | 12 |
| `branch_merge_contract` | 1 |
| `capability_contract` | 6 |
| `changes_contract` | 1 |
| `claim_release_differential` | 30 |
| `close_differential` | 7 |
| `concurrency_contract` | 2 |
| `conformance_differential` | 2 |
| `create_contract` | 8 |
| `harness_contract` | 3 |
| `list_differential` | 8 |
| `native_adapter_contract` | 13 |
| `oracle_launcher` | 2 |
| `orientation_differential` | 35 |
| `output_parity_differential` | 2 |
| `pagination_differential` | 27 |
| `parity_1021_contract` | 14 |
| `query_cost_contract` | 2 |
| `read_session_contract` | 7 |
| `release_pipeline_contract` | 20 |
| `search_collections_differential` | 17 |

Total: **219 passing oracle-backed target tests**, including **128 differential
tests**. Round two had 125 passing differentials; the original oracle-update
baseline was 59 passed and 61 failed across 120 tests. Existing exact stream and
state comparisons remain unchanged.

The envelope gate passes **514 tests**: 399 library, 49 binary (including six
credential-projection tests), 21 CLI dispatch, 11 ownership, 14 parity and 20
release-pipeline contracts. The complete `just crash-check` passes **90 tests**,
with zero failures or skips, plus TypeScript, ESLint, history-prefix and evidence
retention guards. Clippy and fmt pass. The aggregate normal/coverage release
check remains beyond the established 45-minute local budget and belongs to CI.

Six independent disposable-copy behavioral reverts all failed at assertions,
never compilation, and the exact same commands passed after restoring each
function. The main worktree was never mutated by these controls:

| Reverted boundary | Regression | Restored result |
| --- | --- | --- |
| Global URL span scan | `multiple_url_credentials_are_redacted_globally` | Passed |
| Registration of every password | `multiple_url_credentials_are_redacted_globally` | Passed |
| Close recovery flag placement | `delimited_close_recovery_matches_published_bytes` | Passed |
| Close human output mode | `delimited_close_recovery_matches_published_bytes` | Passed |
| Close human envelope rendering | `delimited_close_recovery_matches_published_bytes` | Passed |
| Context cutoff validation | `context_cutoff_validation_matches_published_clock_refusal` | Passed |

The JSON partial-explanation and dual-budget requests retain measured oracle
behavior, so they have no changed production functions to revert. Their exact
oracle tests pass. No upstream issue was filed during this round.

| Round-three thread | Result and regression |
| --- | --- |
| `PRRT_kwDOTwgMRs62L3gD` | Global registration and whitespace projection; `multiple_url_credentials_are_redacted_globally` |
| `PRRT_kwDOTwgMRs62MPEO` | Every URL in both streams; `multiple_url_credentials_never_reach_refusal_streams` |
| `PRRT_kwDOTwgMRs62FkY1` | Keep human explanation; decline JSON addition against `failed_start_preserves_claim_and_published_recovery_receipt` |
| `PRRT_kwDOTwgMRs62MPD5` | Correct retry placement and human receipt; `delimited_close_recovery_matches_published_bytes` |
| `PRRT_kwDOTwgMRs62MPDp` | Decline stricter legacy control; `dual_budget_validation_matches_published_precedence` |
| `PRRT_kwDOTwgMRs62MPEC` | Refuse invalid cutoff, preserve offsets; `context_cutoff_validation_matches_published_clock_refusal` |

## Round-four lifecycle admission

`lifecycle_admission::admit_lifecycle_transition` is the common admission and
status-resolution function for atomic claim/start, explicit start's claim
preflight and status update, and pause's status update. Callers invoke it while
holding the real item mutation lock and before committing item, history or
session changes. It resolves the configured registry, normalizes both stored
and requested statuses, checks terminal/transition policy at the published
commit boundary, and admits declared blockers under strict (the default), warn,
off or explicit-force policy. Explicit start deduplicates preflight warnings
when its second step rechecks the same dependency policy.

Progress resolution uses the registered configured progress status when it is
distinct from open, then registered `in_progress` (including aliases), then the
configured open status. Removed progress selectors and coincident open/progress
selectors follow the same function in both execution paths. Atomic starts retain
the terminal refusal; forced explicit starts retain the published update
semantics. Workflow refusal in explicit start's second step still preserves its
successful claim and partial-step recovery, matching the existing oracle test.

Ranking never authorizes a later write. A non-git SDK differential captures a
ready candidate, reopens its blocker through a real published mutation, then
compares the actual native and published ranked claim writers. Both refuse
without changing the candidate, history or session. Claim-next records refused
candidate IDs and returns the published exhausted-selection details.

The added differential cases are:

- `explicit_start_blocker_refusal_matches_oracle_without_writes`: JSON and human
  bytes, exit status, canonical files/history/session and direct atomic SDK
  refusal context.
- `ranked_start_blocker_refusal_matches_oracle_without_writes`: actual CLI
  selection refusal and independent direct atomic SDK admission.
- `explicit_start_progress_fallback_matches_published_bytes` and
  `ranked_start_progress_fallback_matches_published_bytes`: both starts persist
  the registered progress fallback, with exact receipt/history bytes.
- `start_blocker_enforcement_modes_match_published_bytes`: warning, off and
  forced strict policies with matching durable bytes and warning ordering.
- `start_declared_blocker_variants_match_published_bytes`: closed prerequisites,
  missing targets, external provenance and relationship aliases.
- `configured_progress_resolution_matches_published_registry`: removed and
  aliased workflow selectors for explicit and atomic ranked starts.
- `pause_progress_fallback_admission_matches_published_no_write`: pause and
  release/pause refuse before writing when open is also the effective progress
  fallback and an unresolved dependency exists.

The additional library differential is
`ranked_start_rechecks_reopened_blocker_against_oracle`. The SDK test driver is
shared by the integration and library tests; it loads the installed published
SDK through the existing launcher rather than copying its admission logic.

The composition audit covered all native call sites of `start_task`,
`pause_task`, `claim_with_options`, `lifecycle_status`, release and composed close.
Claim/start aliases share the explicit path; ranked starts share the atomic
writer. Pause/release-pause had the additional fallback admission bypass fixed
above. Close-plus-release already calls both guarded operations and retains
partial-step diagnostics. There is no native start-plus-close composition.

One source-audited difference remains: published
`sdk/lifecycle/task-composition.js::releaseRefusedStartClaim` conditionally
releases a newly acquired claim when a blocker appears between explicit start's
two commits. Native retains that successful claim and partial-step diagnostic.
This is tracked in `pm-rust-start-claim-compensation`. The race was not
independently reproduced in this round; automatic
compensation requires an ownership/history proof and is not added here. Ordinary
strict blocker refusals, including stale ranked candidates, are independently
verified without writes.

A newly arrived cursor-compaction review thread is tracked separately in
`pm-rust-long-intent-cursor`; no cursor fix is claimed by this lifecycle change.
Round four read all 16 review threads and their 16 comments without pagination.

The mutation acceptance target initially passed 27 and failed two frozen close
history comparisons inherited from the older oracle. Replaying the unchanged
five-operation recipe through the pinned published CLI independently confirmed
all four preceding history records byte for byte. The close record adds the
published `context.completion` timestamp/source and its corresponding record
hash. Only that measured expected literal and its provenance comment changed;
all equality assertions remain intact. This is the same completion context
already checked by the passing live conformance and close differentials.

The round-four full-target refresh passes **603 tests**, with zero failed or
ignored tests. It preserves each 1,200-second cap and two-thread bound:

| Target | Passed |
| --- | ---: |
| `claim_release_differential` | 38 |
| `mutation_contract` | 29 |
| `conformance_differential` | 2 |
| `close_differential` | 7 |
| `admission_contract` | 12 |
| Library (including stale-ranked oracle admission) | 400 |
| Binary refusal envelopes | 49 |
| `cli_dispatch` | 21 |
| `ownership_contract` | 11 |
| `parity_1021_contract` | 14 |
| `release_pipeline_contract` | 20 |

These commands use the same required-oracle, locked Cargo command and time/worker
limits documented above, with `--test mutation_contract` added to the linked
commands. The combined envelope command contributes 515 of the 603 tests.
Clippy and fmt pass. The accepted round-three all-target and crash receipts
remain earlier evidence; this refresh targets the lifecycle/mutation surface
requested for round four. The aggregate normal-plus-coverage release gate still
exceeds the established 45-minute local budget, so coverage and platform
certification remain with CI at unchanged thresholds.

Six disposable function reverts retain the current oracle and every assertion.
Each reverted run fails behaviorally (exit 101, no compilation failure), and the
exact same command passes after the fixed function is restored in `finally`:

| Test | Reverted production function | Negative / restored |
| --- | --- | --- |
| `explicit_start_blocker_refusal_matches_oracle_without_writes` | `ownership_item_with_options` | Failed / passed |
| `ranked_start_blocker_refusal_matches_oracle_without_writes` | `ownership_item_with_options` | Failed / passed |
| `ranked_start_rechecks_reopened_blocker_against_oracle` | `ownership_item_with_options` | Failed / passed |
| `explicit_start_progress_fallback_matches_published_bytes` | `lifecycle_status` | Failed / passed |
| `ranked_start_progress_fallback_matches_published_bytes` | `ownership_item_with_options` | Failed / passed |
| `pause_progress_fallback_admission_matches_published_no_write` | `lifecycle_status` | Failed / passed |

The integration filters run on `--test claim_release_differential`; the stale
ranking filter runs on `--lib`. Each uses the documented required-oracle Cargo
command with the exact test name before `-- --test-threads=2`. Only the named
function is restored from the accepted round-three source for each negative
control. The package worktree is never modified by these reverts.

Final tracker reads cover all 23 open items without pagination. The initial
orientation returned 10 focus items and the required search returned 10 of 58
keyword matches; the existing exact oracle item was reused. Both newly discovered
follow-ups remain open and unclaimed. The oracle item stays in progress with its
claim released for orchestrator verification.

## Round-five review boundaries

[Round-five review evidence](ORACLE_REVIEW_ROUND5_2026_11_12.md) records additive
lifecycle schemas, encoding-warning admission, complete blocker IDs, alias-only
fixtures, final receipt measurements and credential registration from stored
producer fields. Native also preserves long producer and output cursors during
intent string compaction. The pinned oracle clips the long producer cursor and
returns an unusable continuation; native intentionally keeps it executable. This
divergence is checked independently without weakening existing byte comparators.

The stored-producer privacy corpus also exposes a published omission: a blocker
title containing a nested-scheme credential URL and a copied password returns
`start_blocked_by_open_dependency` (exit four), redacts the URL, but leaves the
copied password visible. Native registers every credential-bearing producer
string and key before projection, so both copies are redacted. This intentional
privacy divergence follows the same userinfo rule as the URL-scanning correction.
No upstream issue was filed.

## Round-six CI boundaries

[Round-six CI evidence](ORACLE_CI_ROUND6_2026_11_12.md) records the Windows
SDK/native item-path spelling comparison and the argument-dependent unit coverage
gap. Native retains its canonical filesystem diagnostic. Only the compared
`context.item_path` uses shared filesystem canonicalisation; all other context
fields and complete CLI byte assertions remain exact. Bootstrap-scope coverage
now supplies its own inputs instead of relying on test-runner arguments.
