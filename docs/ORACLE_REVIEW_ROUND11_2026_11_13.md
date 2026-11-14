# Review round eleven: invocation boundaries and credential spans

Starting head: `fd95dd688bbb883f15752638c175f51813acb0a7`. Both requested Greptile threads, their complete comments and AI prompts were read from the 48-thread response without pagination omissions. Main is contained in the branch. The restored native stale-ranked terminal-policy test is unchanged.

## Parsed invocation boundary

`src/invocation.rs` captures physical option/value ranges using the existing CLI definition's arity, aliases and hyphen-value rules. Its scope selector only considers bootstrap options before the first command. The `--` delimiter ends all option interpretation. Attached values and consumed separate values cannot become flags. Argument filtering uses a linear exclusion mask and retains all remaining tokens verbatim.

The shared view is used by these inspection sites:

- `native_arguments`: locate the command before normalizing native item namespaces.
- `main` and `rejected_orientation_argument`: identify an orientation command and fallback rejected options without searching inside values.
- `published_read_arguments`: remove actual bootstrap discovery and native clock option ranges and normalize actual close option aliases; preserve values and delimited positionals.
- `invocation_scope`: capture the genuine bootstrap workspace and its aliases.
- `preserve_refusal_scope`: determine whether recovery arrays already contain a genuine bootstrap scope, then rebuild attempted and retry commands. Real scope is first in the published `provided_fields` order even when positional data spells the same flag.
- `scope_recovery_command`: check the leading bootstrap option in renderer-owned command text before adding scope to executable string advice. Command data cannot provide that leading option.
- `strip_bootstrap_scope`, called by `project_close_receipt_scope` and `apply_close_error_recovery`: remove only bootstrap scope before applying the invocation's scope.
- `json_requested`: resolve actual output options without treating their values or post-delimiter text as controls.
- `command_refusal_detail` and `close_error_refusal`: exclude actual author/output control flags from rejected-value receipts, while retaining literal values that resemble those flags.

Live author selection (`ownership_request`, `scalar_identity`, `mutation_identity`) and successful output selection already consume the typed `Cli` parser results. The receipt's legacy `provided_fields` string inventory remains the published diagnostic projection; it is not used to choose tracker scope, encoding or identity.

`round11_bootstrap_scope_ignores_command_data` checks aliases, delimited reasons, option values, output/author controls, literal dash paths, and missing namespace data. `round11_scoped_close_reason_matches_oracle` compares the complete refusal streams, status and item/history artifacts for the exact reported command shape. `round11_scoped_close_retry_executes_only_selected_tracker` follows the emitted retry after replacing evidence placeholders, closes only the selected overlapping ID, preserves the reason and leaves the default item's bytes and history untouched. These fixtures are ordinary non-git trackers.

Two old native-only argument-stream controls expected unsupported post-command workspace tokens to disappear. Their exact expected streams now retain those tokens, as the new bootstrap-only contract requires; no differential comparison was changed.

The initial new oracle comparison caught a receipt-order mismatch: a positional `--pm-path=literal` put `--pm-path` last in the diagnostic field inventory. The real bootstrap field now wins that order, matching the oracle without changing any comparison. The complete close target passed 10 tests; admission passed 12 and claim/release passed 51. Further gate and revert results follow below.

## Credential boundary decision

The new shared `tests/support/refusal_corpus.rs` supplies identical bare/decorated cases to direct registration/projection and real stored-holder/close-refusal tests. The current word exception is independently reproduced: the registration test fails on `https://stored and private-token@host.invalid`, and the process test shows the stored holder in the human refusal. These are executed assertion failures, not compile failures.

A structural policy clarification is pending. Under the requested delimiter rule, the public example `https://host.invalid or user@example.invalid` has `host.invalid or user` between `//` and `@`, with no `/`, `?` or `#` interrupting it. The same bytes therefore qualify as whitespace-bearing userinfo under that rule, yet the requested control requires them verbatim. The existing must-redact `https://stored.name user@host.invalid` also prevents treating every apparent domain followed by whitespace as a public URL boundary. No substitute word list or domain list has been introduced. P2 is reproduced and remains unfinished pending that decision; it is not being deferred to an issue.

Work is recorded on [pm-rust-oracle-1111](https://github.com/unbraind/pm-rust/blob/main/.agents/pm/tasks/pm-rust-oracle-1111.toon).

## Checkpoint verification

This checkpoint does not claim P2 remediation and is not ready to push. Its newly added privacy assertions remain enabled and fail against the unchanged production word exception. No finding is moved to a follow-up issue. The pending decision is whether ambiguous spans are conservatively credentials (public prose then needs an explicit URL terminator), or whether a completed authority followed by whitespace ends the URL (which requires revising ambiguous whitespace-userinfo expectations). Both choices were presented for clarification; neither was assumed from elapsed time.

Completed differential targets (`cargo test --locked -j2 --test <target>`, required published CLI, at most two test threads, 1,200-second cap per command):

| Target | Passed |
| --- | --- |
| claim_release_differential | 51 |
| close_differential | 10 |
| orientation_differential | 36 |
| list_differential | 8 |
| pagination_differential | 33 |
| conformance_differential | 2 |
| search_collections_differential | 17 |
| output_parity_differential | 2 |

Admission contract passed 12. The final binary target has 70 passing tests and the two new privacy reproductions failing. CLI dispatch has 29 passing tests and the two new privacy reproductions failing. The process corpus records 36 mismatches across claimed-by display, close projection and retry availability in both formats. Existing assertions in all differential targets are intact.

Linux and Windows GNU cross-target clippy passed with all targets/features and denied warnings. Formatting passed. Test fixtures were extracted into a named helper and the scope/value contracts split into separate tests to meet the unchanged function-length lint; no lint was suppressed. Ordinary native read commands retain the fast namespace passthrough when there is no `item` candidate, avoiding an unnecessary second parser definition.

Five literal function-revert pairs verified ten runs: `preserve_refusal_scope` fails the scope unit, the real retry execution and the exact oracle differential; `json_requested` and `published_read_arguments` each fail the option-value contract. Each negative is an executed assertion failure with exit 101, every restore passes with exit 0, and source/tests in the disposable copy match the reviewed tree afterward. The linked P1 recovery-execution command passed through `pm test --run --progress --match round11_scoped_close_retry_executes_only_selected_tracker --timeout 1200` with `recorded=false`, `tracking_disabled`; no run artefacts were added to the package tracker.

The PM item is released and remains in progress, unclosed. `npm run changelog:full` and `npm run changelog:check` passed after the final tracker writes. History remains a byte-prefix extension of the starting head, and the public-path scan passed. This is a local checkpoint while the required boundary question is pending; no GitHub review reply, resolution, issue deferral or push was performed.
