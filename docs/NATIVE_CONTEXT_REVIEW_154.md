# Native context review follow-up

PR: https://github.com/unbraind/pm-rust/pull/154

This follow-up preserves the existing native context and relationship implementation.
The package was audited by defect class across SDK and CLI entry points, context,
history and dependency commands, JSON and TOON, and synthetic non-git trackers.
The private root package exposes npm/Bun release-gate aliases; no committed `dist/`
or separately packed native consumer exists in this repository.

| Review thread | General rule | Change and adversary |
| --- | --- | --- |
| PRRT_kwDOTwgMRs60-taD | Preserve actionable subprocess diagnostics. | Both context revert runners and their source scans use checked Git execution. Real bad-hunk and non-repository probes require the original Git diagnostic. |
| PRRT_kwDOTwgMRs60-taE | Discovery must describe supported selection controls. | Add `sections` and `activity_limit` dimensions; check all core context controls through SDK discovery and both actual CLI encodings without Git, Node or a tracker. |
| PRRT_kwDOTwgMRs60-taH | Optional dependencies gate only their consumers. | History uses the baseline driver directly. Current reads skip a missing current oracle only in optional runs; required runs fail. The proof runs all four history contracts with only the baseline installation. |
| PRRT_kwDOTwgMRs60-taM | Refusal contracts include CLI spellings and encodings. | Check `--for unknown` and `--for=unknown` in text and JSON. Require typed native refusal and the separate oracle diagnostic. |
| PRRT_kwDOTwgMRs60-taP | Prove read-only behavior after each invocation. | Compare settings bytes immediately after oracle and native success, before another encoding or fixture rewrite; extend valid missing, empty and configured settings across both intents in a non-git tracker. |
| PRRT_kwDOTwgMRs60-taR | A successful budget receipt must bound the final selected wire. | Check exact wire cost against the requested ceiling, and independently check compact intent feasibility. The SDK corpus includes every integer budget from 256 through 2300, plus 3000 and 5000, for both intents and encodings: 8188 cases. |
| PRRT_kwDOTwgMRs60-taS | Byte caps apply cumulatively across insertions. | Distinct producer budgets yield individually admissible packets whose combined bytes exceed two MiB; independently sum retained entries and reject overflow admissions. |
| PRRT_kwDOTwgMRs60-taV | Memoization keys retain every cost-affecting dimension. | Compare fresh/shared JSON and TOON rendering across 999/1000/9999/10000 budgets and eight byte alignments; require a witnessed cost increase and exact final wire estimates. |
| PRRT_kwDOTwgMRs60-taX | Codec tests prove reconstruction, not metadata presence alone. | Decode actual native compact TOON with pinned SDK `decodePmTableRows`; check rows and first-occurrence column order against the pinned encoder. Cover empty lists, absent cells, ambiguous lists and three null columns. |
| Additional review body | Stabilize self-referential receipts before enforcing ceilings. | Recompute intent feasibility flags from final estimates and stabilize both receipts before orientation ceiling checks. Direct stale-receipt and decimal-boundary adversaries cover both encodings. A separate 280-case matrix checks final wire ceilings for list/triage, search, files, docs and test; their legacy receipt-estimate contract is preserved. Omitted intent estimates remain preserved. |

`node scripts/context-review-revert.mts` independently mutates each contract in a
disposable source copy. Every selected test must pass before the mutation, fail
behaviorally under the mutation (compiler failures are rejected), and pass after
restoration. Source and build products of mutation checks remain disposable.
An optional test-name substring selects a smaller proof set and refuses empty
selections. Existing tests and thresholds remain intact.

The settings adversary also exposed a separate pre-existing nested-value parity
boundary: configured `context.activity_limit: 0` triggers the oracle's invalid
settings warning/default fallback, while native context accepts it. CLI
`--activity-limit 0` is separately valid. The open parity item `pm-rust-qhem`
records that boundary; this follow-up does not change settings schema policy.

Validation counts, behavioral-revert receipts and aggregate-gate limitations are
recorded on `pm-rust-context-intents`. Tracker integrity remains separate from
strict check-only health; the retained-history baseline issue stays on
`pm-rust-b4yl` without rewriting any history.

## Validation

- `CARGO_BUILD_JOBS=2 RUST_TEST_THREADS=2 node scripts/context-review-revert.mts`,
  executed with `pm test --run`: 15 behavioral mutations, 30 baseline/restored
  controls and four baseline-only history contracts passed (49 commands).
- `CARGO_BUILD_JOBS=2 RUST_TEST_THREADS=2 just release-check`, with an isolated
  home and credential-free npm configuration: all 753 ordinary Rust tests across
  34 targets and all 90 Node harness tests passed. TypeScript, ESLint, formatting,
  warnings-denied Clippy, private-item rustdoc and the two standalone crash
  history/evidence checks passed. The 55-minute local cap interrupted the
  instrumented orientation suite: 593 instrumented tests across 18 completed
  targets had passed, with no assertion failures. Aggregate coverage is pending;
  no full aggregate or four-dimensional 100 percent result is claimed.
- `cargo audit`: passed under the unchanged allowance for RUSTSEC-2024-0449.
- `node tests/oracle/launch.mts health --check-only --require-merge-drivers --strict-exit`:
  passed with an isolated global environment; retained-history integrity is a
  separate baseline issue.
- `just changelog-full` and `just changelog-check`: regenerate/check after the
  final tracker writes.

Bun acceptance passed these three commands with `RUST_TEST_THREADS=2` and
`PM_RUST_REQUIRE_PUBLISHED_CLI=1`:

```sh
PM_NODE_INTERPRETER=bun cargo test -j 2 --locked --test orientation_differential context_unknown_intents_have_explicit_cli_boundaries
PM_NODE_INTERPRETER=bun cargo test -j 2 --locked --test orientation_differential context_successful_reads_preserve_settings_bytes
PM_NODE_INTERPRETER=bun cargo test -j 2 --locked --lib current_compact_tables_round_trip_through_pinned_sdk
```
