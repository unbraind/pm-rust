# Audited history re-anchor prefix exception

PR #168 repairs the historical discontinuities in `pm-rust-bj7a` and
`pm-rust-unih`. The ordinary raw-byte prefix requirement rejected those repairs
even though `pm health` recommends `pm history-repair` for this drift.

## Exception rule

`scripts/crash-history-prefix.mts` preserves the existing byte-prefix rule for
ordinary history changes. A rewritten JSONL stream is accepted only if:

- Its only additional record is a final, sealed `history_repair` maintenance
  marker with an author and the published default re-anchor message. The
  message's rewritten-entry count must match the observed count.
- All original records remain in their original order. Each keeps every field
  except `before_hash`, `after_hash`, and `record_hash`.
- Each rewritten record appends exactly one `reanchor_evidence` entry, retaining
  the complete original record and its hash envelope. Any prior evidence stays
  unchanged. At least one item hash must change in each rewritten record.
- The repository's pinned published CLI accepts `history <id> --verify
  --strict-exit --limit 1 --json --no-extensions`, including full chain replay,
  retained-evidence integrity, and equality with the paired item. The invocation
  explicitly selects the tracker and removes inherited PM routing overrides.

The marker and evidence shapes come from `buildHistoryRepairMessage` and
`buildHistoryRepairEntries` in the installed package's `dist/sdk/history-repair.js`,
and `createHistoryRewriteEvidence` in `dist/core/history/history.js`.
Hashing and replay remain the published CLI's responsibility. Patch repairs,
other metadata rewrites, missing verifiers, and failed verifications retain the
`history_prefix_replaced` refusal.

## Validation

- `node --test scripts/crash-history-prefix.test.mts`: 26 tests pass, including
  the run through `pm test pm-rust-b4yl --run --progress`. Real CLI repairs of the
  original bj7a and unih records are accepted (14 and 17 re-anchored entries).
  Both streams reject dropped/reordered records, non-hash edits, missing or
  unsealed markers, invalid chains, unrelated messages, incorrect counts,
  missing/altered retained evidence, and mismatched paired items. Existing
  append, deletion, branch-only and missing-Git cases remain covered. An
  ordinary rewritten stream is rejected alongside valid repairs.
- `node scripts/crash-history-prefix.mts --base origin/main`: passes for both
  repairs and the appended tracking history.
- `cargo fmt --check`: passes. No Rust changes; clippy is not required.
- `npm run changelog:check`: passes.
- `tests/harness/node_modules/.bin/tsc -p tests/harness/tsconfig.json`: passes.
- `tests/harness/node_modules/.bin/eslint -c tests/harness/eslint.config.mjs scripts/crash-history-prefix.mts scripts/crash-history-prefix.test.mts`: passes.

## Full-health limit

`pm health --strict-exit --json` on a fresh clone exits 1 solely because
`merge_driver_configuration_missing` is gate-failing. History drift is clear.
It also reports the pre-existing advisory findings `agent_guidance_outdated`
and `provenance_value_domain_invalid` (19 legacy role values). These are already
owned by `pm-rust-vuwe`; they were not normalized as part of this change.
The package worktree instead reports inherited `merge_driver_configuration_drift`.
Consequently the requested condition of no findings other than missing clone
merge drivers is not met, even though neither repaired chain has drift.

Full release certification remains CI's responsibility. No review threads are
changed, workflows dispatched, or pull requests merged by this work.
