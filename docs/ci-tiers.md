# Fleet CI v2

The required names remain `Rust ubuntu-latest`, `Rust macos-latest`,
`Rust windows-latest` and `Release gate aggregate`. Branch protection is unchanged.
PR CI runs on pull requests and manual dispatch, with superseded PR runs cancelled.
Every work job has a 22-minute hard cap and a target of at most 15 minutes;
classification and required aggregation jobs have five-minute caps.

Each OS builds and tests two disjoint Cargo target shards. Shard 1 includes
library, binary, example and benchmark targets. Integration targets are discovered
from Cargo metadata and assigned exactly once, with measured heavy differential
targets balanced between shards. The non-required `Quick Rust <os> shard <n>`
jobs feed the required `Rust <os>` aggregates. Each aggregate uses `needs` and
`if: always()` and fails unless all shard and policy jobs succeeded. The matrix
result deliberately requires success across all three platforms.

`Release gate aggregate` similarly requires the platform shards, policy,
quality, coverage and crash jobs to succeed. It cannot pass on a cancelled,
failed or skipped dependency. This follows GitHub's documented
[dependency and always-run job behavior](https://docs.github.com/en/actions/reference/workflows-and-actions/workflow-syntax#jobsjob_idneeds).
Coverage runs in two non-required `Quick coverage shard <n>` jobs using the
same disjoint Cargo target partition. Each uploads every original per-process
LLVM raw profile plus a digest inventory and commit/toolchain identity. The
merge job rebuilds all instrumented targets with `-- --list` (no tests execute),
removes only its compilation-generated profiles, restores the two verified
profile multisets, then lets native LLVM merge counters and generate the report.
This computes the exact union of per-file hits; percentages are never averaged.
The source scope and **100% lines, functions, regions and branches** thresholds
are unchanged. Missing shards, digest/identity mismatches or corrupt profiles fail.

Two required merge fixtures prove the result: complementary Rust branch runs
match a combined run for every file/segment/branch/counter, and a tiny Cargo
crate repeats the actual separate-target build, artifact restore and mapping
rebuild flow at all four 100% thresholds. They also reject corrupt profiles and
wrong build identities. Ordinary crash jobs skip these two LLVM-only fixtures;
the coverage merge job explicitly requires and executes them.

| Original check | PR quick tier | Deep tier / runner |
| --- | --- | --- |
| Rust build and all Cargo test targets on Linux/macOS/Windows | Two shards per OS, hosted | Full Linux test job locally; all three OS complete aggregates |
| Rustfmt and clippy, deny warnings | Linux quality job | All four complete aggregates |
| Complete private-source documentation, deny missing docs | Linux quality job | All four complete aggregates |
| Locked published CLI differential oracle | Every Cargo shard and coverage; missing oracle fails | Every full Cargo gate; missing oracle fails |
| LLVM nightly branch coverage, all four dimensions at 100% | Two Linux coverage shards and exact LLVM union, full source scope | All four complete aggregates |
| Typed crash/adapter harness typecheck, lint and tests | Linux crash job, focused syscall cases | Full harness in every aggregate; live syscall corpus on Linux |
| Real SIGKILL rename/fsync/history-open recovery | Focused retained cases on Linux | Exhaustive ordinal corpus on Linux |
| Tracker history-prefix and retained evidence commit gates | Policy job on every PR | Complete aggregate crash-check |
| All-object Git identity audit, reachable and unreachable | Node policy audit; Rust audit remains in shards/coverage | Rust audit in full suites |
| README/orientation/crash/benchmark documentation contracts | Node policy contracts without Rust compilation | Original full suites and policy contracts in tests |
| Cargo security audit | Linux quality job | All four complete aggregates |
| Canonical pinned changelog consistency | Policy job on every PR | All four complete aggregates |
| Clone-local merge drivers and strict tracker health | Policy job on every PR | All four complete aggregates |
| npm release aggregate | Components in parallel on hosted Linux | One full Linux npm gate on local/fallback runner |
| Bun release aggregate | Shared native gate covered by npm/components | One full Linux Bun gate on local/fallback runner |
| macOS/Windows complete release aggregates | Platform build/tests remain above | One full npm gate per hosted OS |
| Native authoritative-open and 3,000-item latency budgets | Deep only | Linux local/fallback; macOS/Windows hosted |
| CodeQL Rust analysis and explicit build | Separate matching-path workflow | Matching main pushes, weekly Wednesday 04:17 UTC, dispatch |

There is no published npm/Bun package acceptance contract in this crate's private
package.json. Both package-manager aliases execute the same native `just
release-check` recipe. The full recipe, release workflows, native benchmark
workflow and release verifier scripts are preserved.

## Documentation and tracker changes

A NUL-delimited base-to-head diff disables rename folding. Tracker data, Markdown,
`docs/**`, changelog and license changes use the cheap path, with executable
tracker files re-included before the allowlist. The original JS/TS exceptions
also include Rust, shell and MTS files for this crate. Required checks still run
and require the policy job. Compilation, Cargo tools and binary tests are skipped.
The policy job installs only the locked Node oracle, installs merge drivers,
runs strict check-only tracker health, verifies the canonical changelog, audits
every Git commit object, runs existing docs/public-evidence contracts and checks
tracker history prefixes and retained evidence commits. Full-history checkout
and tags remain mandatory for identity and changelog checks.

CodeQL is not a required context. Ordered path filters ignore tracker data,
Markdown, docs and lockfiles, then re-include executable tracker files. Its weekly
schedule and 30-minute timeout remain; superseded PR analyses cancel.

## Deterministic depth

Unset, `full` and unrecognized `PM_TEST_DEPTH` values preserve full depth.
Quick uses the same seed prefix at one tenth depth, rounded up, with a minimum
of 25 capped by the original count. Explicit property suites use this policy
and honor a positive `PROPTEST_CASES` cap; quick CI sets 50. Thus 500 becomes 50,
96 and 64 become 25, and the 768-frame adapter corpus becomes 77. Unconfigured
proptest's original 256-case default becomes 26. CI fixes the otherwise-random
proptest seed; suites with an existing fixed seed retain it. Acceptance and
refusal counts derive from generated cases and the independent verdict model.

This covers admission replay/governance/schema/filename models, query costs,
discovery, mutation serialization, pagination and read filtering, and the native
adapter frame corpus. The two 32-combination capability matrices keep a 25-case prefix, covering
every capability both granted and absent. Large fixed capacity, memory, row-count, pagination and
product-budget fixtures are unchanged. Small boundary-kind matrices remain
exhaustive. Quick omits the single exhaustive syscall-ordinal corpus test;
retained physical fsync, rename and history-open injections, focused
cleanup-fsync crashes, both mutation
operations, restart/return recovery and the known published-oracle divergence
remain tested. Full keeps every original ordinal and assertion.

## Deep routing and setup

Deep runs on main pushes, manual dispatch and daily at **01:29 UTC**, away from
the release schedule. Concurrency is per ref with `cancel-in-progress: false`;
every job has a 120-minute timeout. Required PR names are never reused.
Each of Linux npm, Linux Bun, macOS npm and Windows npm runs exactly one full
release gate. A separate Linux job retains the full Cargo suite. Native latency
runs on all three platforms. Linux uses `[self-hosted, pm-local]`, falling back
to `ubuntu-latest` only when `PM_LOCAL_DEEP=off`; macOS and Windows stay hosted.

Self-hosted Rust uses persistent runner tool storage. Under one host `flock`,
bootstrap installs rustup only if absent, stable 1.90.0 with clippy/rustfmt,
nightly-2026-08-06 with LLVM tools when needed, cargo-llvm-cov 0.8.7,
cargo-audit 0.21.2 and just 1.43.0. Existing installs are reused. These tools'
declared minimum Rust versions fit 1.90.0. Build targets stay in the job workspace.
Every local setup logs rustc/cargo versions. Hosted setup retains the pinned
Rust actions and uses SHA-pinned rust-cache. Node's npm cache action is hosted
only; local npm storage persists. Bun installs only on hosted jobs; every full
aggregate reports `bun --version`.

The image has no Rust or strace and cannot use sudo. The syscall setup extracts
strace and libunwind from the distro's signed package indexes into tool storage,
under the same lock, and probes tracing a child. A wrapper supplies its private
library path even when the harness clears the child environment. It never edits
the image, controller or system packages. Cold downloads, tool compilation and
container ptrace remain unverified until a deep job runs. A ptrace refusal fails
the job visibly rather than skipping the crash corpus.

Latency budgets remain unscaled: get/list/context p95 ceilings are 50/150/250 ms,
and the authoritative item-open budget is unchanged. Local hardware has not yet
been measured for these contracts, so its ability to hold them is unproven.
`PM_TEST_TIME_SCALE=3` scales only crash-test parent/child watchdogs in deep.
The helper validates a finite positive scale before launch, rounds to integer
milliseconds and rejects unsafe timer values. Crash CLI timeout option bounds,
product deadlines and latency assertions remain unscaled.

## Measurements and validation

Baseline: [main run 43729555388](https://github.com/unbraind/pm-rust/actions/runs/43729555388).
All values below are completed hosted measurements, in seconds.

| Original job | Execution | Created to completed |
| --- | ---: | ---: |
| Rust Linux | 790 | 5701 |
| Rust macOS | 863 | 3119 |
| Rust Windows | 1364 | 4080 |
| Release Linux npm | 1610 | 5114 |
| Release Linux Bun | 2376 | 6539 |
| Release macOS | 2073 | 4364 |
| Release Windows | 3236 | 5125 |
| Native latency Linux / macOS / Windows | 136 / 141 / 300 | 5046 / 2615 / 2519 |

The slow Linux test targets were claim/release (180.71s), orientation (138.80s),
pagination (108.38s), search/collections (83.77s), close (55.06s), and library
unit tests (48.30s). The old Linux aggregate spent about 12 minutes in its
coverage pass after repeating the full suite separately. The first PR run at
`d204834` passed all required checks and all four 100% coverage thresholds, but
its unsharded coverage exceeded the 15-minute target. That measured result
triggered the exact raw-profile sharding above. The new shard/merge execution
times require its own hosted run; the first run is not proof of the new wiring.
Local proof is limited to cargo check, clippy, fmt and touched tests at quick
depth, plus strict YAML and shell parsing and the docs/identity fixtures.
Full depth runs only in deep CI.

Tracking: [pm-rust-ci-v2](https://github.com/unbraind/pm-rust/blob/main/.agents/pm/tasks/pm-rust-ci-v2.toon),
companion `pm-cli-website-dyhg`.

First PR run [43812108627](https://github.com/unbraind/pm-rust/actions/runs/43812108627)
at `d204834` completed successfully. These seconds are measured from each job's
API timestamps; aggregate jobs are created after their dependencies, so their
own wall times do not include the predecessor jobs.

| Job | Execution seconds | Created-to-completed seconds |
| --- | ---: | ---: |
| Policy | 28 | 30 |
| Changes | 6 | 8 |
| Quick quality | 69 | 70 |
| Quick crash | 145 | 147 |
| Linux Rust shards 1 / 2 | 420 / 499 | 422 / 500 |
| macOS Rust shards 1 / 2 | 605 / 388 | 607 / 390 |
| Windows Rust shards 1 / 2 | 852 / 709 | 854 / 711 |
| Unsharded coverage, replaced after measurement | 1100 | 1101 |
| Required Rust Linux / macOS / Windows aggregates | 3 / 5 / 4 | 10 / 102 / 16 |
| Required release aggregate | 5 | 134 |

The quick Linux target ranking was orientation 194.12s, claim/release 175.61s,
search/collections 120.87s, pagination 107.09s, close 75.90s and library 31.17s.
These are target-level timings from the first PR's logs. The slow differential
fixtures retain their semantic cases; the two-shard partition keeps each
platform job below the 15-minute target. Coverage's 18m20s execution caused the
raw-profile split; its new wiring needs a fresh hosted result.


Final implementation run [43814943607](https://github.com/unbraind/pm-rust/actions/runs/43814943607)
at `cce2f8f774ac03d6145655058dd098792fb805fd` passed every CI job and all four required
checks. Every quick work job finished below 15 minutes. The merge restored
4,226 original profiles, passed both required fixtures and retained the four
unchanged 100% thresholds. CodeQL also passed. Aggregate job wall times begin
when the job is created after its dependencies.

| Job | Execution seconds | Created-to-completed seconds |
| --- | ---: | ---: |
| Tracker, identity and documentation | 25 | 28 |
| changes | 5 | 7 |
| Quick release quality | 53 | 55 |
| Quick crash harness | 198 | 200 |
| Quick Rust ubuntu-latest shard 1 | 424 | 426 |
| Quick Rust windows-latest shard 1 | 844 | 846 |
| Quick coverage shard 2 | 478 | 480 |
| Quick Rust windows-latest shard 2 | 642 | 644 |
| Quick Rust macos-latest shard 2 | 337 | 339 |
| Quick Rust ubuntu-latest shard 2 | 370 | 373 |
| Quick Rust macos-latest shard 1 | 447 | 450 |
| Quick coverage shard 1 | 613 | 615 |
| Quick exact coverage union | 88 | 91 |
| Release gate aggregate | 4 | 6 |
| Rust windows-latest | 4 | 6 |
| Rust ubuntu-latest | 5 | 7 |
| Rust macos-latest | 2 | 5 |

Local final proof: cargo check/clippy/fmt; 25 depth/workflow contracts; changed
property/model tests; 13 discovery tests; 37 crash tests plus two focused cleanup
cases, with one intentional full-corpus skip; four install/identity/depth helper
fixtures; two real LLVM/Cargo merge fixtures; harness typecheck/lint; strict YAML
and shell parsing; changelog and isolated strict tracker health. The broad local
pagination attempt was stopped after roughly 20 minutes with 32/33 passing;
its changed property test passed separately. No local full-depth suite ran.

Remaining proof: deep dispatch returned 404 before the workflow exists on main.
Cold persistent tool bootstrap, container ptrace and local latency remain
unverified; the original product time contracts are unscaled.
