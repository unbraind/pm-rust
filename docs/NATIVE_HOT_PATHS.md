# Large tracker agent hot paths

Run `just bench` on Linux with Node 24+, Rust 1.90 and GNU time. The benchmark
installs its own lockfile-pinned pm-cli oracle from `tests/bench-oracle` (currently **2026.10.21**). Installed-version validation follows that manifest rather than a source literal. The executable-identity refusal precedes version validation, so independently updated parity and benchmark pins do not break that refusal contract.

The default run generates 1,000, 10,000 and 50,000 real canonical TOON items in
disposable plain directories initialized by `pm init`. Every item carries four
comments, file and test evidence, a description and a body; all but the first
have a dependency. The published history constructor generates a valid create
record followed by four comment records for each item. `GIT_CEILING_DIRECTORIES`
prevents discovery of a surrounding repository. No existing tracker is used.

Each command gets two warmups and twenty measured fresh processes per runtime.
Runtime order alternates; filesystem pages and oracle derived caches are warm.
The receipt retains every sample, nearest-rank wall p50/p95, maximum per-child
RSS in KiB from GNU time, stdout/stderr bytes, exit status and output hashes.
Each measured sample records its zero-based `round` and `runtime_order` (the
ordered runtime names actually launched in that round); warmups are excluded.
New receipts identify this as `runtime_order_source.kind: recorded-at-launch`.
The historical receipt linked below has order reconstructed from its exact
candidate scheduler and sample index, labeled `reconstructed-from-scheduler`,
with its original receipt SHA-256 retained. Its timings are unchanged; this
annotation is not independent observation of the historical launch order.
The external wall clock includes process startup, the GNU time wrapper and
output capture. The oracle driver pins Date before importing the real CLI and
pins the context packing watchdog for deterministic byte comparisons. It does
not load the SDK into CLI children. Driver overhead is included in oracle
measurements. Internal elapsed-time
telemetry is therefore unsuitable for comparing these runs.

For a paired before/after run, build main first, retain its executable under the
ignored `target` directory, then rebuild the candidate and run:

```sh
cargo +1.90.0 build --release --locked
mkdir -p target/before
cp target/release/pm-rust target/before/pm-rust
# Apply the candidate and rebuild through just bench.
just bench --baseline target/before/pm-rust --profile yes \
  --output target/native-bench.json
```

`--sizes`, `--samples` and `--warmups` allow shorter diagnostic runs. The
benchmark verifies executable fingerprints again after the run, compares
baseline and candidate stdout/stderr/status exactly, and compares their item
and history hashes after each write. Write targets and runtime state are reset
outside the timed interval, so every claim is a real first claim and every
comment/update starts from the same item. Only generated state is removed.
The paired executable filenames must match: CLI diagnostics expose the program
name, so changing that name would itself change observable error bytes. Every
paired run rejects identical executable fingerprints before creating measurements,
including local runs without CI commit metadata. CI builds baseline and candidate
in independent Cargo target directories, preventing a shared build cache from
silently replacing the candidate with the baseline.

The `Native hot-path benchmark` workflow runs this same twenty-sample matrix on
an ephemeral GitHub-hosted `ubuntu-latest` runner for the performance branch,
with an immutable main baseline and an exact candidate checkout. Its JSON and Markdown artifacts
contain synthetic measurements only; binaries, trackers and raw command output
are excluded. This avoids treating a heavily contended development host as a
stable performance reference; GitHub-hosted runner noise is still possible. It
can also be dispatched after the workflow is available on the default branch. Existing release gates remain independent.

The optional syscall profiles use `strace -c -f` outside the measurement
windows. They cover file opens, reads, metadata calls, directory enumeration,
writes and durability flushes. Hardware sampling with `perf` was unavailable
on the measured host because its kernel forbids unprivileged performance
events. No host setting is changed by this procedure.

Exact oracle equality is reported separately for output and persisted write
artifacts. A nonzero native exit or a false equality flag is a compatibility
gap, never a successful speed result. Main deliberately refuses context at
10,000 items and above, and does not expose the `next` or `list-open` command
names. The additional `list --status open --limit 20` case measures the
supported list path without disguising those missing commands. Optimizations
must preserve the baseline's observable behavior while those parity gaps
remain separately tracked.


## Current delivery evidence

The implementation measured here is `57845abdf2beb9087bfca25e674d56c32805a608`. The final documentation/PM receipt retains the exact source digests in [the delivery gate receipt](benchmarks/native-hot-path-final-gates-2026-10-22.json); it changes no runtime, harness or workflow source. The [PR delivery record](https://github.com/unbraind/pm-rust/pull/102) identifies the final pushed receipt head and its complete release-gate run. The [current-head checks](https://github.com/unbraind/pm-rust/pull/102/checks) remain the gate for that delivery head.

The [committed complete receipt](benchmarks/native-hot-path-final-2026-10-22.json) and [complete tables](benchmarks/native-hot-path-final-tables-2026-10-22.md) retain the new paired matrix from [this exact implementation run](https://github.com/unbraind/pm-rust/actions/runs/39860125987): 81 rows, 1,620 measured samples, distinct before/native executable hashes and launch-recorded order. This evidence is committed here and does not depend on artifact retention. The historical baseline remains `c0154dbd7cd8071ac52b891be13f956f7a0f21fd`; measurements below explicitly labeled `32c1479` are historical.

All 27 oracle cases succeed. Native and baseline each succeed in 19 of 27 cases; native output is byte-identical to the oracle in 2 of 27 cases, and none of the 9 native write cases has byte-identical target item/history artifacts. The baseline and candidate still match each other for all 27 observable command results and all 9 write states. These known compatibility gaps have the same counts as the historical matrix. This receipt establishes preservation of baseline behavior, not complete native/oracle parity.

Selected current 50,000-item results (fresh-process paired measurements):

| Command | Before p50 ms | Native p50 ms | Before peak RSS KiB | Native peak RSS KiB |
| --- | ---: | ---: | ---: | ---: |
| claim | 3142.2 | 2145.3 | 508388 | 25016 |
| comment | 611.6 | 59.6 | 11940 | 11932 |
| update | 608.6 | 50.9 | 11588 | 11428 |

### Refusal, locking and contention correction

Ownership preparation now occurs only for an eligible accepted attempt, under its item lock after journal recovery. The three refusal-contract cases are covered through the actual SDK and CLI:

- A held item with unrelated malformed TOON retains an exit-0 `--if-available` skip.
- The same held item without `--if-available` retains `already_claimed_by`, exit 4.
- A missing ID with an explicitly empty author or future timestamp retains its argument error, exit 2, for claim and release.

A busy lock also takes precedence over unrelated corruption, and a journal whose item half was lost rolls forward before lineage validation. Eligible corpus failures still leave the new target/history/session state unchanged; session-publication storage failures remain separately documented partial failures.

The [contended claim-next receipt](benchmarks/native-hot-path-contention-2026-10-22.json) exercises the public `claim_next` implementation used by `claim --next`. A test-only scheduling hook performs real locked competing mutations after ranking has captured its candidates; it never replaces SDK results and is absent from production builds. The regression covers 10 and 100 lost races with both hard refusals and `--if-available`, plus acceptance on attempt 100. Exhaustion performs exactly one corpus read for ranking, with zero lineage scans for rejected candidates; acceptance adds exactly one lineage read. Directory traversal and target lookup still occur on each attempt. This removes repeated payload decoding rather than claiming constant-time retries.

```sh
PM_RUST_CONTENTION_ITEMS=50000 cargo +1.90.0 test --locked --lib \
  selection::tests::contended_claim_next_does_not_rescan_rejected_candidates -- --nocapture
```

The diagnostic uses a synthetic non-git tracker. Its wall times include ranking and the competing writes themselves, run in a debug test process on a contended development host. They are single diagnostic observations, not fresh-process p50/p95 speed claims. The deterministic corpus-read assertion is the regression gate.

| Attempts | If available | Accepted | Corpus reads | Observed seconds |
| ---: | --- | --- | ---: | ---: |
| 10 | false | false | 1 | 91.396 |
| 10 | true | false | 1 | 50.430 |
| 100 | false | false | 1 | 240.773 |
| 100 | true | false | 1 | 118.762 |
| 100 | false | true | 2 | 87.044 |

The diagnostic was compiled at `527ab0e`; all Rust production and test sources are byte-identical at the measured implementation head above. The later change validates the benchmark profile option and adds its Node regression. Both source identities are retained in the receipt.

### Current revert proofs and gates

The [seven new behavioral controls](benchmarks/native-hot-path-followup-reverts-2026-10-22.json) retain exact commands and source digests. Moving preparation before refusal checks breaks the five new ownership contracts and the contention read-count assertion. Moving it before journal recovery breaks lost-item replay. Removing explicit-empty-author validation breaks the CLI contract. Restoring a literal oracle pin or validating it before executable identity breaks the independent-pin regressions. The additional profile parser control rejects `true`, `maybe` and `false` before binary reads or fixture creation; reverting only that validation fails its diagnostic assertion. Thread `PRRT_kwDOTwgMRs6vX_PV` is addressed by this regression and remains unresolved for the reviewer. Every negative control fails its intended test and every restored control passes; no compiler/tool error is accepted.

The [current optimization proof](benchmarks/native-hot-path-final-revert-2026-10-22.json) also retains both original assertion failures followed by both restored passes, with exact source restoration. The [implementation's full CI run](https://github.com/unbraind/pm-rust/actions/runs/39860125982) passes the unchanged `just release-check` on all platforms. Source digests tie this evidence to the final metadata-only delivery commit; final-head checks are linked above. Local targeted validation passes 9 ownership contracts, the five-variant contention regression, and 25 Node harness tests. Pins, source scope and thresholds remain unchanged; no TypeScript coverage percentage is claimed. Two local PM-linked harness attempts hit the existing nested-process timeout at the preceding correction head; the complete 25-test harness passed on retry at unchanged limits. The existing five-second `relationship_large_edge_limit_finishes_at_budget_floor` test also exceeded its native process deadline locally, including a targeted retry; that unchanged test passes in the full CI suites. The local full gate stopped at that deadline failure and is not counted as a pass. One PM launch overlapped oracle installation and failed before starting tests; it was rerun after setup completed. Superseded local gate attempts were intentionally stopped before subsequent source edits, rather than counted as passes. The earlier generic-callback head failed coverage and was corrected without exclusions.

| Platform | Ordinary tests | Instrumented tests | Lines | Functions | Regions | Branches |
| --- | ---: | ---: | --- | --- | --- | --- |
| Linux | 589 | 589 | 14106/14106 (100%) | 1476/1476 (100%) | 23545/23545 (100%) | 3310/3310 (100%) |
| macOS | 578 | 578 | 14021/14021 (100%) | 1463/1463 (100%) | 23395/23395 (100%) | 3272/3272 (100%) |
| Windows | 572 | 572 | 14017/14017 (100%) | 1462/1462 (100%) | 23393/23393 (100%) | 3274/3274 (100%) |

The existing allowed `RUSTSEC-2024-0449` advisory remains. The historical-branch workflow uses a fixed baseline and implementation-specific snippets; general source-PR workflow support is tracked separately by [pm-rust-bench-workflow](https://github.com/unbraind/pm-rust/blob/perf/native-hot-paths/.agents/pm/tasks/pm-rust-bench-workflow.toon). No general future-PR benchmark coverage is claimed here.

## Implementation

Mutation lookup uses directory-entry type information to skip unrelated regular
files before live path metadata checks. Targets, directories, symlinks and
unknown types retain the original live checks, recursive traversal, duplicate
handling and error behavior. A moved-target regression remains in place; the
new fallback test verifies that an unrelated corrupt file stays irrelevant
when cached type information is unavailable.

Ownership attribution builds an in-memory parent index in the same source
order as the full corpus reader. Each item still passes the complete strict
decoder, including unrelated payloads and duplicate detection. The index keeps
IDs, source paths and parent links while full payloads are dropped between
items. This preserves lineage and validation behavior while reducing peak RSS.
Preparation runs under the target item lock, after argument validation, durable transaction recovery and ownership eligibility checks. Held-item skips and refusals never build this index. An eligible request validates the corpus before its new write; a lost item is recovered before the snapshot. Session publication remains a separate phase whose storage failure preserves completed ownership metadata.

## Historical measurements (32c1479)

The [complete before/after tables](benchmarks/native-hot-path-tables-2026-10-21.md) include every command at all three sizes, p50/p95, maximum RSS, stdout/stderr bytes, exit status and exact oracle output/storage comparisons. The [raw receipt](benchmarks/native-hot-paths-2026-10-21.json) retains all 1,620 measured samples, 54 syscall profiles, load averages and executable hashes.

The [GitHub-hosted run](https://github.com/unbraind/pm-rust/actions/runs/39657720819) compares merged main `c0154dbd7cd8071ac52b891be13f956f7a0f21fd` (including PR #101) with candidate `32c14792b2c041fb4bf30a265af328bb3742ca44`. Each runtime/case has twenty measured launches after two warmups. All 27 oracle cases succeed. Every baseline/candidate output and target item/history comparison passes byte for byte, including known native refusals.

At 50,000 items:

| Operation | Before p50 / p95 ms | Native p50 / p95 ms | Before / native peak RSS KiB |
| --- | ---: | ---: | ---: |
| claim | 3609.2 / 3630.5 | 2522.3 / 2543.3 | 508516 / 24824 |
| comment | 682.0 / 693.4 | 64.3 / 66.8 | 11816 / 11496 |
| update | 681.6 / 686.3 | 64.1 / 67.7 | 11352 / 11424 |

Comment and update reduce `statx` calls from **300,030 to 30**, retaining **8 fsync calls** each and the same durable item/history bytes. Claim reduces `statx` from **400,050 to 100,050**, retains **11 fsync calls**, and reduces peak RSS by **95.1%**. Strict corpus validation still reads every item; payloads are discarded after decoding. On filesystems without cached directory-entry types, type lookup can itself require metadata calls. These syscall counts describe the measured runner.

The measurements do not establish a uniform native speed advantage. At 50k:

| Operation | Native p50 ms | Oracle p50 ms |
| --- | ---: | ---: |
| claim | 2522.3 | 495.1 |
| comment | 64.3 | 368.0 |
| update | 64.1 | 508.3 |
| search | 2600.4 | 1592.8 |
| list | 916.2 | 329.4 |
| get | 40.9 | 326.9 |

Exact current-oracle output equality holds in **2 of 27** fixture cases; the complete resource table identifies each one. The remaining format, behavior or command gaps are not parity claims. Storage equality is recorded separately for every mutation; all nine current-oracle artifact comparisons differ, so current-oracle mutation storage parity remains unestablished. Native context at 10k/50k and all next/list-open rows exit nonzero; those timings are refusal costs and are excluded from successful-operation speed claims. Strict validation and full scans remain substantial costs.

History streams are verified before timing, read commands retain all authoritative hashes, and mutations start from identical target bytes. These fixtures cover one dependency, four comments, one file and one test per item; they do not certify denser graphs, longer histories or every filesystem.

## Historical optimization proof (32c1479)

`just perf-revert-proof` temporarily restores full payload retention in claim lineage and removes the unrelated-file fast path. It preserves the test observations, requires the two specific performance assertions to fail, restores the original source bytes in a `finally` block, and requires both tests to pass. A compiler or tool error is not accepted as a regression proof.

At `32c1479`, both negative controls fail their intended assertion with exit 101 and one failed test each; both restored controls pass with one test each. The [receipt](benchmarks/native-hot-path-revert-2026-10-21.json) records exact source restoration and hashes for the combined implementation. Only the structured receipt is retained publicly.

## Historical validation (32c1479)

The [structured check receipt](benchmarks/native-hot-path-checks-2026-10-21.json) records ten linked PM commands, 26 passing test executions and zero skips. The direct concurrency suite passes both tests. Native writers run in a real `pm init` tracker with no Git executable on PATH; all fifty must succeed, all unique comments must be present, and the 51-record history hash chain must agree.

The first local linked run had three compiler-build or cargo-lock timeouts on a contended development host. A duplicate local library run was deliberately stopped and is not counted as a pass. After prebuilding the integration target, the linked rerun passes all commands. The existing evidence-recording policy remains `tracking_disabled`; public receipts omit private execution-context paths. Inherited commands were reviewed and acknowledged locally. `.mts` scripts use Node native type stripping and executable syntax/behavior checks; that historical run did not have a standalone TypeScript compiler. The current delivery also passed strict erasable no-emit checking with the available compiler.

The unchanged [full CI gate](https://github.com/unbraind/pm-rust/actions/runs/39657720684) passes at `32c1479` on all three platforms. Each aggregate runs ordinary and instrumented suites, formatting, strict Clippy, private-item rustdoc, dependency audit and changelog verification. The documented fresh-clone tracker preflight passes. The audit retains the existing allowed `RUSTSEC-2024-0449` advisory.

| Platform | Ordinary / instrumented tests | Lines | Functions | Regions | Branches |
| --- | ---: | ---: | ---: | ---: | ---: |
| Linux | 582 / 582 | 14062 / 14062 | 1472 / 1472 | 23500 / 23500 | 3302 / 3302 |
| macOS | 571 / 571 | 13977 / 13977 | 1459 / 1459 | 23350 / 23350 | 3264 / 3264 |
| Windows | 565 / 565 | 13973 / 13973 | 1458 / 1458 | 23348 / 23348 | 3266 / 3266 |

Every aggregate executes 44 test binaries with zero failures or ignored tests and 100% in all four required coverage dimensions. The [gate receipt](benchmarks/native-hot-path-gates-2026-10-21.json) retains source hashes, exact counts, job links and all 810 existing latency samples (270 per platform). All nine unchanged get/list/context p95 bounds pass. CodeQL also passes. Thresholds and exclusions are unchanged.

## Baseline transition

PR #101 merged during the original validation. This branch then incorporated the merged parity baseline and repeated the complete paired matrix, revert controls, linked tests and platform gates. The tables and receipts in the historical sections describe that combined implementation. Current delivery evidence is listed at the top of this document. The [earlier reproduction receipt](benchmarks/native-hot-path-reproduction-2026-10-21.json) is historical pre-parity evidence at `fd836ba`; it does not describe the current executable. Older full receipts remain available in Git history and their linked workflow artifacts.

## Historical review regressions

The [review receipt](benchmarks/native-hot-path-review-2026-10-21.json) records
five assertion-level negative controls: sample order, historical order provenance,
runner wording, aggregate latency counts and the README non-git boundary. Each
control fails one intended assertion with only its behavior or documentation
reverted, then passes after exact byte restoration. At that historical review snapshot, the Node harness
passed 13 tests and remains part of every platform release gate through
`harness_contract`. The scheduler tests cover both two and three runtimes, odd
and even warmup counts, and compare recorded order against actual callbacks.
