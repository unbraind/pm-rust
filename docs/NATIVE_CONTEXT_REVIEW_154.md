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

## Final local validation for this round

- All 34 ordinary Rust targets were exercised: 755 distinct tests passed across
  the full ordinary run and the corrected/remaining target reruns. The final
  correction reran all 409 library and 42 binary tests, all 26 pagination tests,
  the seven previously unexecuted integration targets and all examples: 558
  tests across 15 targets passed. Earlier unchanged targets account for the
  remaining 197 tests. This is not a passing single aggregate invocation.
- Formatting, warnings-denied Clippy, private-item rustdoc, TypeScript/ESLint,
  all 93 crash/context/adapter Node tests, crash history/evidence checks,
  dependency audit, isolated strict self-host health and changelog verification
  passed. The separate managed general Node harness passed all 25 tests.
- Node/Bun each passed the 72-comparison reviewed context matrix. Bun also passed
  compact-table decoding and the corrected general interruption harness.
- Behavioral proofs cover the current context correction, the legacy list
  compatibility boundary, both oracle prerequisites, exact named results,
  passing baselines, raw returned receipts, tight root-edge reservations, codec
  reconstruction and interrupted-copy restoration. The existing pm item records
  the managed run and focused reruns with exact commands and counts.
- The complete single-invocation release/coverage gate and final-head external
  reviews remain CI requirements under the documented runtime exception.


## Review round 3: remaining non-blocking findings

The existing branch includes main and retains all earlier tests. No review thread
was answered or resolved by this round.

| Thread | General rule and fix | Regression |
| --- | --- | --- |
| PRRT_kwDOTwgMRs61Bm1L | Copy live working-tree state, rather than assuming every indexed path exists. All three Git-enumerated proof runners share `copyWorkingTree`, which skips absent paths before creating directories. | A real Git fixture has unstaged file and directory deletions, retained tracked files and untracked files. It exercises the shared helper and every actual runner. |
| PRRT_kwDOTwgMRs61Bm1P | Read-only certification must fingerprint every authoritative persisted format. The existing shared snapshot reader already includes JSON; its property now mutates both root settings JSON and nested JSON alongside the original TOON and JSONL corpus. | Every authoritative mutation changes its hash; root cache mutations remain excluded and nested cache-named directories remain authoritative. |
| PRRT_kwDOTwgMRs61Bm1S | Scratch ownership outlives every subprocess using its files. On Unix, Cargo runs in a dedicated process group; interruption signals the group, escalates when necessary, waits for workers to stop and only then permits exit cleanup. Failed drains retain the tree. | Eight signal/delivery cases include parent-only signals, direct Cargo signals, terminal groups and descendants that ignore graceful termination. Original source-restoration and checkout-isolation assertions remain. |
| PRRT_kwDOTwgMRs61Bm1W | Validate stored settings before request overrides and use the published whole-settings fallback. Activity/staleness limits, default depth, ranking policy and section switches share validation. Valid numeric spellings such as `1.0` are normalized in memory. Successful CLI warnings preserve the JSON and TOON contracts; SDK warnings remain separate from serialized output. | A 162-case SDK matrix and 66 exact CLI comparisons cover fallback, valid stored values and explicit zero/nonzero overrides in non-git trackers. Settings bytes never change. |

Package searches covered every Git copy loop, exit-time scratch removal,
authoritative snapshot consumer and stored context-field reader. Fixed-list
native proof copies do not enumerate absent Git entries. Crash snapshots already
include arbitrary file types; the separate changes proof owns timeout process
groups without an exit-time deletion handler. The Rust crate is the product; the
private npm development package has no separately packed native consumer or
committed `dist/`. Both output encodings use the same native SDK implementation.

Behavioral proofs use disposable copies and require named assertions, with a
passing control before mutation and after restoration. Ten independent mutations
cover the copy guard, JSON inclusion, whole-process signaling, drain ordering,
forced termination, original restoration/baseline/build isolation, SDK settings
fallback and exact CLI fallback diagnostics. No test or threshold was removed.


Round-3 final local validation:

- `cargo +1.90.0 test --locked --all-targets --all-features`, with two workers,
  passed all 757 tests across 34 targets in one aggregate invocation.
- `just crash-check` passed TypeScript, ESLint, all 94 Node tests (zero skipped),
  append-only history verification and all retained crash-evidence receipts.
  The final managed focused command passed 33 Node tests in a scratch tracker.
- `bun test scripts/harness.test.mts` passed all 27 tests at the unchanged
  timeout on retry. The first attempt timed out under host load.
- Formatting, warnings-denied Clippy, private-item rustdoc, identity audit,
  dependency audit and changelog verification passed. The dependency audit
  retained its existing allowed unmaintained-dependency warning. Strict self-host
  health passed with an isolated global extension root; the shared global install
  has an unrelated extension-host version mismatch and was left unchanged.
- `timeout --kill-after=10s 3300s just release-check` exited 124 during the
  instrumented orientation suite, after completing the entire ordinary suite
  and crash checks. No assertion failure was reported before the cap. The full
  coverage gate and final-head external reviews remain CI requirements; no
  complete coverage percentage or aggregate pass is claimed.

## Review round 4 continuation

This continuation retains the existing branch, PR and item
`pm-rust-context-intents`. Main was already included. No inline thread is
answered, resolved or reacted to by this continuation.

| Finding | General rule and package-wide fix | Regression / independent revert |
| --- | --- | --- |
| PRRT_kwDOTwgMRs61Bm1P (already resolved) | Read-only receipts include every authoritative persisted format. Preserve the earlier root/nested JSON assertions and cover insertion, removal and modification in custom directories. | `JSON snapshot additions removals and changes are authoritative in every custom directory`; removing JSON inclusion must fail. |
| PRRT_kwDOTwgMRs61Cfjp | Scratch ownership lasts until owned subprocesses and descendants stop. All nine context, adapter, changes and performance proof entry points (including this round's safety proof) share asynchronous process ownership, interruption, escalation and drain-before-cleanup. Every nested proof command runs in its scratch source. | Both signals through every entry point; preserve all eight earlier performance signal/delivery cases. Replacing group signaling with direct-child signaling must fail. |
| PRRT_kwDOTwgMRs61Cfjt | Measurement provenance must be observed through the actual driver. Run its `--version` before tracker/cache work, require the pinned release, and bind receipts to the driver's unchanged SHA-256. | Real driver processes with wrong, absent, failed or noisy versions refuse before output or cache changes. Removing the version comparison must fail. |
| PRRT_kwDOTwgMRs61Cfjw | Disposable mutation destinations cannot alias live source. Every copy materializes symlinks as independent files. Git-enumerated copies explicitly exclude dependency junctions, including links not excluded by directory-only Git ignore patterns. | Absolute, relative and directory symlinks; every actual copy callback writes only its independent copy. Revert each of the six distinct copying implementations independently. |
| PRRT_kwDOTwgMRs61Cfj2 | Baseline selection must respect platform support. Default Windows selection reports skipped signal proofs and retains all ordinary proofs; explicit signal selection directs the operator to Unix. | Exercise both platform branches and explicit/default selection on every host. Removing the Windows guard must fail. |
| PRRT_kwDOTwgMRs61Cfj_ | Diagnostics survive every refusal after settings fallback. SDK errors retain the typed original cause plus a warning accessor. JSON refusals append published warning fields to the error envelope; text prints the warning before the refusal. | Seven SDK refusal types, valid/invalid stored settings, both encodings, unchanged non-git tracker bytes; expanded exact pinned-oracle matrix. Independently remove SDK propagation and CLI warning attachment. |
| Latest review body: Windows performance descendants | Termination must cover the complete owned tree. Shared Windows interruption uses `taskkill /T /F`; inability to prove termination retains scratch inputs and fails explicitly. | Simulate the Windows process boundary on every host, checking complete-tree arguments and retained scratch on termination failure. Removing `/T` must fail. Native Windows execution remains a CI responsibility. |

The class search covers every proof subprocess/copy/removal path, all authoritative
snapshot consumers, settings-warning producers, SDK context evaluation and CLI
error encodings. Runtime oracle junctions remain explicitly shared dependencies,
separate from source mutation destinations. The crate is the shipped product;
the private npm development package has no published native npm/Bun artifact or
committed `dist/`. Existing real non-git CLI/SDK differential consumers remain
mandatory; no tests, budgets, timeouts or coverage thresholds are relaxed.

`node scripts/context-safety-revert.mts` requires the named assertion for each
mutation, with a passing baseline and a passing restored control. All mutation
and compiler output is confined to disposable copies. The earlier aggregate
release gate exhausted its 3300-second cap during instrumented tests. Under the
original task's runtime exception, ordinary tests and individual required gates
run locally; full instrumented coverage remains a final-head CI requirement.

Round-4 behavioral evidence: the managed PM test passed its original 13 mutations
and 26 passing before/after controls (39 commands). The added Windows selection
call and dependency-junction exclusion each passed an independent three-command
control. `node scripts/context-review-revert.mts SIGTERM` passed six retained
performance safety mutations with twelve passing controls (18 commands).
Together these are 21 mutations, 42 passing controls and 63 commands. The
canonical safety script includes all 15 new controls; the two late controls were
executed separately rather than claiming a second full invocation.

Round-4 final local validation:

- `cargo test -j 2 --locked --lib --bins --test identity_audit --test release_pipeline_contract`:
  411 library, 43 binary, 7 identity and 20 pipeline tests passed (481 total).
- `cargo test -j 2 --locked --test orientation_differential context_invalid_stored_activity_limits_match_current_oracle`:
  132 exact comparisons passed. The same command with
  `PM_NODE_INTERPRETER=bun` independently passed all 132 comparisons.
- `just crash-check`: TypeScript, ESLint, 116 Node tests (zero skipped), history
  prefix verification and retained evidence-commit verification passed. After
  adding the safety proof's own entry point to the shared property, the expanded
  safety file independently passed all 24 tests. The separate
  `node --test --test-concurrency=2 scripts/harness.test.mts` passed 27 tests.
- Both new linked SDK/renderer tests passed via `pm test --run --match warning`.
  Project run-artifact tracking remains disabled; PM comments retain the results.
- Formatting, all-target warnings-denied Clippy, private-item rustdoc, dependency
  audit and final TypeScript/ESLint checks passed. The dependency audit retains
  its existing allowed unmaintained-dependency warning.
- Strict self-host health passed with an isolated global environment. Existing
  guidance, stale-item and historical provenance-domain advisories remain under
  their existing tracker work; append-only history was not repaired or rewritten.
- The earlier 55-minute aggregate timeout remains the runtime evidence for the
  task's exception. This continuation does not claim a full local release gate,
  full instrumented coverage, or native Windows interruption execution. Those
  remain final-head CI/review requirements.

The claim is released while the item remains in progress for orchestrator
verification. `just changelog-full` and `just changelog-check` pass after the last
tracker write; the generated changelog is unchanged.

## Review round 5 continuation

The eight unresolved threads were read with every comment; the latest review
body adds no outside-diff findings. The existing clean branch matches the remote
head and already contains main. This continuation reuses
`pm-rust-context-intents` and leaves thread replies and resolution to the owner.

| Thread | General rule, scope and regression |
| --- | --- |
| PRRT_kwDOTwgMRs61Cfjp | The existing async runner already stops Cargo descendants before scratch removal. Retain both signals through all nine proof entry points; independently revert group signaling to require the named descendant assertion failure. |
| PRRT_kwDOTwgMRs61E7rv, PRRT_kwDOTwgMRs61FGhA | Platform selection precedes every baseline. Share the selector across both review runners; mark all seven symlink mutations Unix-only, report each skip and reject an explicitly selected unsupported control. Supported later controls remain selected. The property exercises default and explicit selection on both platforms and invokes the actual safety entry point. |
| PRRT_kwDOTwgMRs61FGg1 | Ownership includes nested detached runner groups. Inherit a scoped registration directory, adopt nested registrations on interruption and drain, rescan while draining, and tolerate concurrent registration removal without skipping siblings. Both direct-parent exit and parent-only SIGTERM must stop the nested worker while scratch still exists. |
| PRRT_kwDOTwgMRs61FGg6 | A direct child's exit does not establish descendant termination. Windows commands start suspended inside a kernel job before resuming; kill-on-close retains orphan ownership and an empty-job receipt precedes release. Keep the timeout armed across exit and inherited pipe closure. Cross-host adversarial controls cover the exit/close gap and failure retention; a native Windows integration test checks actual orphan termination. |
| PRRT_kwDOTwgMRs61FGg- | Successful parity and feasible timing are separate claims. Only an identical structured dependency-budget refusal may produce an unavailable timing receipt. Status, output and unrelated runtime failures still abort; every sampled success also retains byte parity. The actual receipt entry point is tested with all four outcomes and unchanged synthetic non-git data. |
| PRRT_kwDOTwgMRs61FGhE | Independent diagnostics survive generic item failures as well as typed refusals. JSON embeds the original item error and settings warning in one valid document; text retains its existing warning/error output. Preserve the warning in ownership/lock envelopes too. Exercise malformed/conflicted items through default/orient/handoff in both encodings and unchanged non-git trackers, plus writer failures. |
| PRRT_kwDOTwgMRs61FGhM | Declaration metadata is immutable under payload compaction. Exclude the complete intent declaration from both collection and string compaction in the shared read-output layer. A generated corpus varies metadata cardinality and encoding, verifies nested/future declaration values, and still requires actual activity rows to shrink. |

The class search includes every proof runner, all platform-filter consumers,
shared snapshot and benchmark consumers, native SDK context failures and CLI
error dispatch, and both shared JSON/TOON compaction walkers. The crate remains
the shipped product: the private npm/Bun development wrappers have no packed
native consumer or committed dist artifact. All new fixture data is synthetic.
The existing whole-package release timeout exception applies; full instrumented
coverage and native Windows job execution remain final-head CI requirements.

The older drain-bypass mutation now bypasses the complete shared drain function;
clearing only its in-memory set is no longer a behavioral revert because scoped
registrations repopulate that set. Its original named SIGTERM assertion remains
unchanged. No existing test, coverage threshold or timeout is weakened.

Windows job assignment uses the documented
[process-creation job-list attribute](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-updateprocthreadattribute)
so ownership exists during creation, including interruption before resumption.
The stream-control bridge decodes the same serialized invocation, preserving
original argv and diagnostics in isolated controls on either platform. Isolated
fixture copies include the complete helper module dependency.

Round-5 behavioral evidence: fifteen class mutations require fifteen named
failures and thirty passing controls. The six retained SIGTERM mutations
still require six named failures and twelve passing controls. CI-log inspection
also found a logical/physical temp-path mismatch in the restoration observer;
all eight original Unix signal tests now execute through a directory symlink,
and reverting only canonical comparison fails the original named assertion.
The combined evidence is twenty-two distinct mutations and forty-four passing
before/after controls, sixty-six required executions, all in disposable copies.
The original group deadlines and every existing assertion remain unchanged.

The unchanged intent matrices exposed one compatibility detail while validating
the metadata fix: a completely omitted packet retains the published historical
compaction measurement in `omitted_result_estimated_tokens`. Delivery protects
the declaration. If delivery is impossible, the same compactor also measures a
private historical projection and retains the conservative estimate floor;
that projection is never delivered. The property now covers seventy-two budget
cases through both shared output paths as well as the collection walker.
The new four-comparison `context_omitted_intent_receipt_matches_current_oracle`
regression independently verifies the pinned JSON and TOON terminal receipts.
Removing either delivery protection or the receipt floor fails a named test.

Final code validation includes 125 passing Node tests and one explicit native
Windows integration skip through `just --no-deps crash-check`, with TypeScript,
ESLint, history-prefix and evidence retention checks. The locked oracle packages
were installed first; skipping prerequisite installs kept those packages stable
while parity processes ran. All recipe checks ran unchanged. Fourteen shared
read-output tests, all forty-three binary tests, the three intent matrices
(342 comparisons), reviewed contracts (72 comparisons), and four new terminal
receipt comparisons pass. The malformed-item boundary passes twelve cases on
Node and twelve with the oracle launched through Bun. The real-copy harness
accepts matching refusals, writes its receipt and retains authoritative bytes.

The initial ordinary all-target attempt completed eighteen targets and 599
passing tests before exposing the terminal-receipt differences. That failing
attempt was stopped; all four affected intent/reviewed tests pass after the fix.
The missing-runtime check also passes on a stable executable after the earlier
concurrent rebuild. This is not a passing final-head aggregate run. The full
release/coverage gate exceeds the original local runtime allowance and remains
required in CI, together with native Windows execution. Thresholds are unchanged.


The class search includes the legacy list-triage declaration. Its canonical
output-budget path uses the same protected delivery policy and historical
terminal-receipt measurement as context. Four additional pinned-CLI JSON/TOON
comparisons and seven list SDK tests pass; both list delivery and receipt-floor
controls independently fail on behavioral revert. The context and list terminal
receipts together add eight comparisons to the 414 existing intent/reviewed
comparisons, 422 total. No schema, ordering, test threshold or timeout changes.


Final stable-code Rust gate: `cargo +1.90.0 test -j 2 --locked --all-features
--lib --bins --test identity_audit --test release_pipeline_contract` passes all
482 tests: 412 library, 43 binary, 7 identity and 20 release pipeline contracts.
The final shared-output and list SDK subsets pass 14 and 7 tests. The pinned pm
runner also passes the safety command in a disposable tracker, with strict
isolated health passing. Full package release coverage and native Windows
execution remain required in CI; no whole-package aggregate pass is claimed.

## Round six: ownership, proof admission and command contracts

All twelve findings are accepted. The shared rules and adversaries are:

| Threads | General rule and package-wide fix | Regression |
| --- | --- | --- |
| PRRT_kwDOTwgMRs61Cfjp | Every proof runner owns and drains workers before deleting scratch. All nine context, adapter and performance runners use the shared async owner. | Every runner under SIGINT and SIGTERM; stubborn descendants in the performance harness. |
| PRRT_kwDOTwgMRs61GlW4 | Restoration requires positive worker-termination evidence. Shared async mutations and manual changes/performance restoration await `settled`; failed reap retains mutated inputs. | `failed worker reap retains mutated inputs until termination is confirmed` |
| PRRT_kwDOTwgMRs61GlW_, PRRT_kwDOTwgMRs61GlXH | Detached children must be registered synchronously at spawn, before module startup. Owned Node children preload the spawn registry; cleanup preserves restoring runners through worker escalation. | Both delayed-constructor exit and SIGTERM tests; existing nested-owner tests retained. |
| PRRT_kwDOTwgMRs61GlXF | Scratch copies materialize usable links and skip dangling targets. The policy covers git-enumerated and fixed-list snapshots. | Dangling tracked, untracked, directory and nested links through every copy callback. |
| PRRT_kwDOTwgMRs61GaS4, PRRT_kwDOTwgMRs61GaS5 | Mutation definitions must target live, unique behavior. Updated all stale selectors, including neighboring renderer and intent-policy cases. | `context-proof-definitions.test.mts` admits every managed target before expensive compilation; real proofs still require named failures and passing controls. |
| PRRT_kwDOTwgMRs61GlXM | Diagnostics survive parser refusals, without changing settings on reads. The context parser attaches the same warning used by SDK/CLI execution refusals. | `context_parser_refusals_preserve_settings_warnings`, invalid and valid settings in non-git trackers, text/JSON/TOON and path spellings. |
| PRRT_kwDOTwgMRs61GlXS | Intent protection belongs to the envelope declaration only. Nested item fields compact under the shared list/search/collection policy. | `item_metadata_named_context_intent_compacts_under_budgets`, both encodings, omission/compaction/full-delivery budgets. |
| PRRT_kwDOTwgMRs61GlXU | Serialization contracts come from the command, not document content. An out-of-band `ListOutput` flag selects current context/deps tables, including measurement and omission paths. | Legacy assurance-shaped envelopes with both spoofed marker fields retain legacy tables. |
| PRRT_kwDOTwgMRs61GlXX | Null-column positions use JavaScript key order. The shared table union sorts canonical array indices before insertion-order keys. | Pinned SDK decoding checks values and `Object.keys`, including zero, maximum array index, overflow, negative, float and noncanonical keys. |
| PRRT_kwDOTwgMRs61GlXZ | Whole-settings fallback discards lifecycle settings too. | Invalid/valid settings matrix asserts default/custom lifecycle classification and unchanged settings bytes. |

The previous macOS ordinary Rust job failed the SIGTERM stubborn-descendant
case after 1294.747 ms; the aggregate macOS job instead failed SIGINT after
1341.503 ms. Both lacked the restoration receipt. These logs rule out the
15-second test bound and a SIGTERM-only defect. Subprocess stderr and exit status
are now checked before reading that receipt, so cleanup errors retain their
cause. All signal cases and timeout bounds remain in place. Exact-head macOS
verification is required before claiming that platform is certified.
