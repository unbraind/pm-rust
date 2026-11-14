# Review round ten: decorated credentials and recovery boundaries

The round starts from orchestrator head `946bc2bd4248d5ad2136d8fcd5900b46e79cb72d`. All five requested threads and all five comments, including their complete AI-agent prompts, were read from the 46-thread GraphQL response without pagination omissions. The restored native stale-ranked terminal-policy test is retained. Cancelled checks on that head are not treated as failures.

| Thread | Disposition and evidence |
| --- | --- |
| PRRT_kwDOTwgMRs621TkU | Fixed in the shared `userinfos` scanner in `src/refusal.rs`. `round10_decorated_userinfo_corpus` uses the same credential corpus in bare URLs, titles, comments and close reasons, checking persisted producer fields and rebuilt recovery arguments. `round10_decorated_credentials_never_reach_refusal_streams` covers a real stored blocker title and positional comment/close input, with public URL/email controls. |
| PRRT_kwDOTwgMRs621Tj9 | Fixed against measured oracle behavior, which differs from the suggested patch. Missing workflow sidecars retain inline `working` in the published CLI. The live registry was correct; admission now overlays inline open/progress settings onto its scaffold defaults before optional sidecars. Published bulk-preview closure defaults remain intact, as the unchanged admission oracle requires. Runtime settings tolerate a missing configured workflow sidecar while preserving other storage and parsing errors. `round10_inline_workflow_registry_matches_live` compares registries with default, inline, configured-missing and present-file controls. `round10_missing_workflow_start_matches_oracle` compares real CLI status, complete stdout/stderr and item/history bytes. |
| PRRT_kwDOTwgMRs621TkC | Fixed in `preserve_refusal_scope` using the shared `scope_recovery_command` for executable next steps and string recovery advice, even without a recovery bundle. Existing explicit scope aliases remain intact. Optional `next_steps` is accessed with `get_mut`, so an absent field stays absent and an unrelated scalar is preserved. The first full pagination run caught four exact-byte failures from the intermediate mutable-index implementation adding `next_steps: null`; the focused failure then passed with the lookup correction, and the complete target was rerun. `round10_executable_steps_keep_tracker_scope` and `round10_recovery_scope_and_alias_controls` cover both branches. `round10_scoped_next_step_cannot_mutate_default_tracker` executes the exact asserted command tokens through the pinned CLI, then checks that the default tracker with overlapping IDs is unchanged. Native `update --unset` support is not claimed. |
| PRRT_kwDOTwgMRs621TkG | Fixed at argument capture: `published_read_arguments` preserves all tokens after `--`, including option-shaped reasons and alias spellings. Provided-field names normalize at `=`, matching published envelopes. `round10_close_retry_preserves_delimited_reason` and `round10_delimited_option_reasons_match_oracle` cover the original timestamp-shaped reason. The latter also pins published withdrawal when a delimited token resembles a missing evidence flag; emitting that ambiguous retry would break parity. |
| PRRT_kwDOTwgMRs621TkM | Deferred to [the authorized issue comment](https://github.com/unbraind/pm-rust/issues/167#issuecomment-6839941443). Adding the valid omission disclosure changes measured packet size and feasible recovery budgets, breaking the mandatory published-byte differential. No existing assertion was changed and no test was skipped. `round10_shortened_reference_receipt_matches_published_gap` explicitly pins the gap with complete output/status comparisons, plus complete-reference and summary controls. |

## Privacy boundary

The vulnerable path is persisted blocker title → lifecycle refusal details → shared credential-source collection → JSON/human refusal rendering. The old scanner used the URL's position in the surrounding text to reject whitespace-bearing, colonless userinfo, so a title prefix prevented both credential registration and URL replacement. The scanner now uses the same content boundary in bare and decorated strings. A public URL followed by explicit connective prose and a separate email remains public, including the existing unqualified-host control. Whitespace usernames, dotted username prefixes, ordinary colonless userinfo, passwords and nested password schemes are exercised at every corpus position. Copied registered credentials and recovery commands continue through the existing invocation-wide projection.

The process regression covers JSON and human claim/close refusals and the supported comment refusal stream. The native comment command has no `--json` option; its JSON producer-field projection is exercised directly through the shared refusal boundary. Legitimate public URL/email text stays verbatim. Refused operations leave the stored item unchanged.

## Oracle measurements and deferred receipt contract

The lifecycle oracle was run before selecting the fix. It starts in `working` when inline `schema.workflow.in_progress_status` is `working` and the sidecar is absent. This holds with both the implicit default path and an explicitly configured missing path. A present workflow sidecar still overrides inline values. The old storage-fault tests now use a directory at the workflow path to retain a real I/O refusal; the missing-file control explicitly asserts the newly measured allowed fallback.

The close oracle preserves `--timestamp=done` as a positional reason in both the attempted command and same-close retry. A reason spelling `--expected` makes the published CLI withdraw the retry because it could be mistaken for missing evidence. Native keeps the original positional bytes and matches the published conservative withdrawal, including canonical and legacy evidence aliases.

The proposed missing-reference omission disclosure reproduced an unchanged differential failure in `relationship_reference_limits_match_oracle`: `deps sample-root --format context --edge-limit 40 --token-budget 300` changed published feasible advice of 412 tokens to 434 tokens. The larger corrected receipt cannot safely advertise the old smaller budget. This requires coordinated receipt, measurement and recovery-envelope contract work; the isolated correction was withdrawn. A new differential independently observes successful shortened references with `meta.truncated=false` and `has_omissions=false` in both implementations. No source change in `orientation_deps.rs` remains.

Work, files, commands and append-only receipts are tracked by [pm-rust-oracle-1111](https://github.com/unbraind/pm-rust/blob/main/.agents/pm/tasks/pm-rust-oracle-1111.toon). Validation and isolated reverts are recorded below after completion.

## Validation

Every command was capped at 1,200 seconds, with at most two build workers and two test threads. The published CLI was required for the oracle-backed runs. Counts below describe complete targets; repeated focused checks are additional evidence, not extra unique tests.

| Command (`cargo test --locked -j2` prefix) | Passed |
| --- | --- |
| `--lib --bin pm-rust` | 436 + 68 |
| `--test cli_dispatch` | 28 |
| `--test admission_contract` | 12 |
| `--test claim_release_differential` | 51 |
| `--test close_differential` | 9 |
| `--test orientation_differential` | 36 |
| `--test list_differential` | 8 |
| `--test pagination_differential` | 33 |
| `--test conformance_differential` | 2 |
| `--test search_collections_differential` | 17 |
| `--test output_parity_differential` | 2 |
| `--test identity_audit` | 7 |

The eight differential targets passed 158 tests; CLI dispatch, admission contract and identity audit passed 47. The complete library/binary rerun on the final source is recorded below. Linux clippy and the `x86_64-pc-windows-gnu` cross-target clippy passed with all targets/features and denied warnings. Formatting passed. No threshold was lowered, and no new code was excluded.

The full aggregate remains for CI: the measured test subset already takes about 40 minutes, and `just release-check` repeats all targets under branch coverage in addition to the normal test run. Windows runtime behavior and aggregate 100% coverage are not claimed from cross-compilation or these targeted gates.

## Final source and revert proofs

The final source passed all 436 library and 68 binary tests again with global/system Git configuration disabled. The scoped credential-refusal and scoped history-retry differentials each passed again. The linked `pm test pm-rust-oracle-1111 --run --progress --only-last --timeout 1200` ran the decorated-userinfo unit successfully with result tracking disabled; no test-run artefacts were written to the package tracker.

Eleven isolated negative/restore pairs verified 22 executed outcomes. Nine pairs literally restore the starting head's changed function; the other two check optional-field insertion and the deferred receipt proposal. Every negative has exit 101 and an executed `test result: FAILED`; compilation failures do not count. Every restore passes. All copied source and test files were byte-identical to the reviewed tree afterward.

| Isolated change | Negative | Restored |
| --- | --- | --- |
| `decorated-userinfo-unit` | Assertion failed (101) | Passed (0) |
| `decorated-userinfo-process` | Assertion failed (101) | Passed (0) |
| `inline-workflow-admission` | Assertion failed (101) | Passed (0) |
| `missing-configured-workflow` | Assertion failed (101) | Passed (0) |
| `next-step-scope-unit` | Assertion failed (101) | Passed (0) |
| `next-step-scope-process` | Assertion failed (101) | Passed (0) |
| `absent-next-step-wire-control` | Assertion failed (101) | Passed (0) |
| `lossless-delimited-reasons` | Assertion failed (101) | Passed (0) |
| `provided-field-spellings` | Assertion failed (101) | Passed (0) |
| `ambiguous-retry-withdrawal` | Assertion failed (101) | Passed (0) |
| `deferred-receipt-parity-sensitivity` | Assertion failed (101) | Passed (0) |

The missing-reference pair is a parity sensitivity, not a claimed production fix: adding the proposed disclosure fails the new exact comparison, and restoring the accepted source passes. The absence-control pair reinstates mutable indexing solely in the new next-step lookup; it fails the absent-field assertion and passes after restoring `get_mut`. The remaining pairs cover decorated credential fields and real process streams, admission start resolution, configured missing workflow files, scoped advice and its execution against overlapping trackers, lossless close arguments, provided-field spellings, and conservative retry withdrawal.

## Tracker and changelog gates

`node tests/oracle/launch.mts health --check-only --require-merge-drivers --strict-exit` exits 1 in the reviewed tracker solely for `extension_host_pm_cli_version_skew:2026.11.11:2026.11.12:pm-github`. The pinned CLI discovers external pm-github metadata requiring the newer host. A clean, non-git tracker snapshot from the starting head reproduces that exact extension failure and all four warnings; its health check does not require Git merge drivers. A second probe within an ignored scratch directory also reproduces the version skew, with additional merge-artifact/fence warnings caused by fixture placement; those extra warnings are not attributed to the reviewed tracker. The native crate and tracked extension settings are unchanged by this diagnosis. The accepted oracle pin and all thresholds remain intact. Fresh CI must certify the full aggregate and its clean-environment health gate.

`npm run changelog:full` and `npm run changelog:check` passed and were refreshed after the final tracker writes. The item remains `in_progress`, released and unclosed. Legacy history is an append-only extension of the starting head.
