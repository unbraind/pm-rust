# Round-seven review evidence

The review read covers all eleven requested threads, including every comment and
AI-agent prompt (thirteen comments), in a complete 34-thread response. The latest
CodeRabbit review body was read in full; its additional Unix permission finding
is also addressed by probing actual readability before asserting a denied read. The branch
already contains the current main; no merge was necessary. The pinned published
oracle remains 2026.11.11. Existing exact differential comparators are unchanged.

## CI failures

The original macOS latency job failed list p95 at 200.514 ms against 150 ms.
The comparable main run at `5055925` passed at 46.285 ms with the same source and
threshold. The same-head retry at `5da6c06` passed get/list/context p95 at
9.381/64.781/102.864 ms, against 50/150/250 ms. No threshold or benchmark change
is justified. Evidence: [original run and retry](https://github.com/unbraind/pm-rust/actions/runs/43186365146),
[main run](https://github.com/unbraind/pm-rust/actions/runs/42916655253).

Windows aggregate tests passed before coverage failed. Its totals were lines
18660/18661, functions 1907/1907, regions 30383/30384 and branches 4215/4218.
The missing branches were blocker-directory symlink exclusion and parent-index
handling of invalid UTF-8. Windows now exercises a real directory junction,
invalid UTF-8 in ordinary and admission reads, and a real exclusive file handle
in both modes. The junction is checked as a symlink before traversal. Linux
runs the cross-platform encoding cases; Windows-specific execution and complete
coverage remain CI certification, at unchanged 100% thresholds.

## Review corrections

- Native cursor assertions always run without an oracle. Only published cursor
  assertions require the driver. Every native page must contain the exact
  remaining suffix, with all eight rows returned once in the frozen order.
- Claim and atomic start share one captured lifecycle registry loaded through the
  authoritative default-file discovery path. A configured default statuses file
  cannot be lost between terminal claim checking and atomic admission.
- Comment receipts discard a stored `index` before inserting the computed row
  position. Stored historical metadata remains unchanged.
- Only envelope-level continuation fields bypass string compaction. Nested item
  metadata using the same names is bounded like any other string. A full-item
  differential checks the complete published response and budget receipt.
- Public URL port/path text stays outside the following URL's userinfo, including
  whitespace, comma and semicolon separators. Actual nested password schemes
  continue to redact. Public URLs alone remain intact.
- Closed stdout readers are closed before spawning, including the existing
  history pipe test. A direct pre-spawn write proves `BrokenPipe` deterministically.
- Build-script discovery derives the profile from Cargo's `deps` or `build`
  layout, including custom and cross-target profiles, instead of profile names.
- The valid omission cursor is actually supplied to output finalisation; its
  receipt marker and omission policy are asserted alongside invalid input.

## Evidence-backed boundaries for the final review

Mixed open and invalid-encoding blockers produce exactly one published warning:
`start_blocker_unverifiable:sample-own:item_document_encoding_invalid`. Native
matches the full published bytes and durable state in warn and forced strict
modes. Adding the open-blocker warning would diverge from the pinned oracle.

The published atomic SDK permits a configured close target without a terminal
role, in both legacy `workflow.close_status` and `schema.workflow.close_status`
settings. Both published and native persisted items move into progress. Native matches that policy, with a persisted progress-status assertion.
Configured terminal-role targets refuse before writes, including force and a
stale ranked candidate. An unconditional refusal for the roleless close target
would change published policy; this remaining policy question is for the
orchestrator's existing follow-up issue, not a silent parity change.

A separate existing comment response-shape mismatch was measured while probing
stored indexes: published comment JSON exposes projected comment rows, whereas
native retains its mutation-result wrapper. The new index regression compares
complete comment rows and exact persisted item/history bytes, without changing
any pre-existing differential assertion. Broader comment-envelope parity is
reported for the orchestrator's existing follow-up issue.

## Validation

Final gate counts and isolated behavioral revert results are recorded on
`pm-rust-oracle-1111` and `pm-rust-long-intent-cursor`. No review thread was
answered, resolved or reacted to by this work.

## Reproduction commands

Every command is bounded to twenty minutes, with two build jobs and at most two
Rust test threads. Oracle-backed gates require the pinned CLI via
`PM_RUST_REQUIRE_PUBLISHED_CLI=1`.

```sh
cargo test --locked -j2 --lib --bin pm-rust
cargo test --locked -j2 --test cli_dispatch
cargo test --locked -j2 --test release_pipeline_contract
cargo test --locked -j2 --test claim_release_differential
cargo test --locked -j2 --test pagination_differential
cargo test --locked -j2 --test mutation_contract
cargo test --locked -j2 --test admission_contract
cargo test --locked -j2 --test list_differential
cargo test --locked -j2 --test orientation_differential
cargo test --locked -j2 --test close_differential
cargo test --locked -j2 --test conformance_differential
cargo test --locked -j2 --test output_parity_differential
cargo test --locked -j2 --test search_collections_differential
cargo clippy --locked -j2 --all-targets --all-features -- -D warnings
cargo check --locked -j2 --all-targets --all-features
cargo fmt --all -- --check
cargo test --locked -j2 --test identity_audit
node tests/oracle/launch.mts health --check-only --require-merge-drivers --strict-exit
just changelog-full
just changelog-check
```

The optional-oracle run sets `PM_NODE_CLI=missing-oracle`, unsets the required
oracle setting, and runs `round5_long_intent_cursor_remains_executable` in the
pagination target. The independent `round7_native_long_cursor_without_oracle`
case runs even when the rest of that target requires the oracle.

## Gate results

All 731 tests passed across the complete selected targets: library 432, binary
60, CLI dispatch 26, release pipeline 22, mutation 29, admission 12, plus the
150 differential cases below. Clippy with warnings denied, all-target/all-feature
`cargo check`, and formatting passed. The later expanded roleless policy and
stale-ranked probes, the Unix readability probe, and the explicitly absent-oracle
run also passed. Identity audit passed all seven cases.

| Differential target | Passed |
| --- | ---: |
| claim_release_differential | 46 |
| pagination_differential | 32 |
| list_differential | 8 |
| orientation_differential | 35 |
| close_differential | 8 |
| conformance_differential | 2 |
| output_parity_differential | 2 |
| search_collections_differential | 17 |

The exact strict health command passes with an isolated home and disabled global
Git configuration. In the ordinary host environment it refuses only because the
global pm-github extension expects CLI 2026.11.12 while the oracle is deliberately
pinned to 2026.11.11. No global extension or oracle pin was changed. Historical
advisories remain unchanged; no audit history was repaired. Complete aggregate
coverage, Windows execution, and the new head's latency jobs remain CI gates.

## Isolated revert results

All twelve negative checks failed by executed test assertions or returned test
errors (exit 101), then the same command passed after restoration: 24 verified
runs. No compile failure counts as a revert proof. The reviewed worktree was
never mutated for these experiments.

| Kind | Reverted function or sensitivity | Regression |
| --- | --- | --- |
| Function revert | `compact_strings` | `round7_cursor_metadata_compacts_below_the_envelope` |
| Function revert | `userinfos` | `round7_public_url_port_boundaries` |
| Function revert | `indexed_comment_receipt` | `round7_stored_comment_index_matches_published` |
| Function reverts together | `ownership_item_with_options`, `admit_lifecycle_transition` | `round7_terminal_source_contract` |
| Guard revert | Remove only the optional-oracle guard | `round7_native_long_cursor_without_oracle` |
| Function revert in real custom profile | `own_build_scripts` | `build_script_stack_contract` |
| Test sensitivity | Empty the resumed page | `round7_native_long_cursor_without_oracle` |
| Test sensitivity | Keep the pipe reader open | `round7_closed_pipe_precedes_child_creation` |
| Test sensitivity | Restore profile-name matching in the path helper | `round7_custom_cargo_profile_paths` |
| Test sensitivity | Reject every supplied valid output cursor | `round5_omission_cursor_policy` |
| Coverage branch sensitivity | Skip the wrong I/O error kind | `round7_parent_index_encoding_modes` |
| Coverage branch sensitivity on Unix | Traverse the symlink | `round5_admission_phases_and_warning_scope` |

The real custom-profile command was:

```sh
cargo test --locked -j2 --config 'profile.review-ci.inherits="dev"' --profile review-ci --test release_pipeline_contract build_script_stack_contract -- --test-threads=2
```

The Windows junction and exclusive-handle cases require Windows CI. Their Unix
or cross-platform equivalents prove branch sensitivity locally; they are not
reported as local Windows execution. The exclusive handle is independently
probed for Windows sharing violation 32, and both workspace modes must retain
that same raw I/O error and kind.

Final changelog generation and `just changelog-check` passed with no generated
content difference. Both PM items remain in progress with their claims released.
