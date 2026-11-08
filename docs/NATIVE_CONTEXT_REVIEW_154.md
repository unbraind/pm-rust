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

## Auto round 2

The existing branch and PR are reused; current main is already included. The two
resolved findings above (Git diagnostics and compact-table reconstruction) remain
covered. Six remaining findings are addressed by their general contract:

| Review thread | Rule and package scope | Regression and behavioral revert |
| --- | --- | --- |
| PRRT_kwDOTwgMRs60_iKa | Standalone gates install every imported dependency. `crash-check` and `orientation-revert-proof` depend on `oracle-install`, which installs both oracles before their consumers. | Inspect Just's parsed dependency graph; removing the prerequisite fails the named Node test. |
| PRRT_kwDOTwgMRs60_iKb | A failing control is never mutation evidence. Context intent proofs require the published oracle, pass the same tests before each mutation, restore exact source and rerun the controls. Native adapter and performance proof runners receive the same baseline requirement. Native adapter mutations now use a disposable copy. | An actual runner with a failed setup cannot emit a successful mutation receipt. A workflow adversary requires the order pass/mutate/fail/restore/pass and refuses mutation after baseline failure. |
| PRRT_kwDOTwgMRs60_iKe | Proofs identify the specific behavioral test. Cargo and explicit TAP reporters share exact named result validation across context, packet and adapter runners. | The outcome corpus rejects unrelated names, skips, TODOs, empty selections, compiler errors, spawn errors and signals. Disabling named validation fails the corpus. |
| PRRT_kwDOTwgMRs60_iKg | Feasibility describes retained output after temporary metadata is removed. Current context stabilization recomputes the retained receipt. Legacy list retains its separately pinned pre-projection feasibility contract. Omitted-result estimates remain preserved. | 240 current-context cases cross their declared ceilings in JSON/TOON options. Another 100 cases guard the legacy list contract; applying current-context recalculation there fails the compatibility property. Reverting the context attachment correction fails its projection test. |
| PRRT_kwDOTwgMRs60_iKi | Node admission preserves reserved root evidence at tight budgets. The existing generous-budget cost and stability checks remain. | Four neighboring tight budgets admit exactly the first node and preserve every root edge for it. Removing reservation enforcement fails the existing named packet test. |
| PRRT_kwDOTwgMRs60_iKj | Tests inspect the producer's return before any repair. The receipt matrix checks the raw `apply_orientation` result and requires both receipts in each encoding. | Removing producer stabilization fails the raw-return test; the test no longer calls stabilization before assertions. |

The audit covers shared SDK and CLI output paths, list/search/collection reads,
context/history/dependencies, legacy and current TOON contracts, and real non-git
fixtures. The root npm package is private and has no separate published native
artifact or committed `dist/`. Node/Bun acceptance uses the actual pinned decoder
and CLI drivers. All prior tests, timing guards and coverage thresholds remain.

Round-2 focused evidence: the managed context proof passed 21 mutations, 42
before/after controls and four baseline-only history tests. Both final recipe
prerequisites passed an additional independent revert check. The ordinary Node
harness passed 93 tests. Node and Bun each passed the 72-comparison reviewed
context matrix; Bun also decoded the compact-table fixtures, and list triage
still matched the pinned CLI. Full unit and aggregate claims are tracked
separately on the item.

The previous aggregate exhausted its 55-minute local cap with instrumented
coverage unfinished. Under the task's approximately-one-hour runtime exception,
this round runs the ordinary Rust suite and individual required checks; full
instrumented coverage remains an exact-head CI requirement. No full aggregate or
four-dimensional 100-percent result is inferred from these focused checks.

The full ordinary run caught the performance interruption fixture's former
in-place-mutation assumption. Its six existing signal/delivery tests now observe
both sandbox source files restored before cleanup, require both passing controls
before mutation, and assert that checkout source/build artifacts stay unchanged.
The same signal limits remain. Three independent reverts (baseline omission,
restore omission and checkout build output) fail the selected test. Literal test
names now share escaping for selection and TAP validation, including parentheses.
All 25 general harness tests pass through the managed pm runner; the three Rust
harness contracts and Bun's actual general-harness invocation pass too.

The package-wide audit also verified the legacy list boundary against the pinned
2026.10.21 CLI. Its unchanged `legacy_cursor_fingerprint_matches_published` test
uses 75 synthetic rows and `--limit 69 --for triage --token-budget 100000`:
the published receipt requires `declaration_feasible: false`. Applying the new
context rule emits `true` and breaks byte parity. The current context contract
uses its independent oracle and is corrected; list preserves its supported older
serialization contract. The original cursor comparison remains unchanged and
passes. A 100-case compatibility property witnesses both feasibility outcomes
and the metadata-removal boundary, and fails if context recalculation leaks into
legacy list. This is a versioned contract boundary, not a skipped finding.
