# Large tracker agent hot paths

Run `just bench` on Linux with Node 24+, Rust 1.90 and GNU time. The benchmark
installs its own lockfile-pinned pm-cli **2026.10.21** oracle. The release gate uses the same version after integrating the merged parity prerequisite.

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
name, so changing that name would itself change observable error bytes. CI
builds baseline and candidate in independent Cargo target directories and
rejects identical paired executables, preventing a shared build cache from
silently replacing the candidate with the baseline.

The `Native hot-path benchmark` workflow runs this same twenty-sample matrix on
a dedicated Linux runner for the performance branch, with an immutable main
baseline and an exact candidate checkout. Its JSON and Markdown artifacts
contain synthetic measurements only; binaries, trackers and raw command output
are excluded. This avoids treating a heavily contended development host as a
stable performance reference. It can also be dispatched after the workflow is
available on the default branch. Existing release gates remain independent.

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
The existing per-item locks, transaction recovery and durable history boundary
continue to govern every mutation.

## Measured results

The [complete before/after tables](benchmarks/native-hot-path-tables-2026-10-21.md) include every command at all three sizes, p50/p95, maximum RSS, stdout/stderr bytes, exit status and exact oracle output/storage comparisons. The [raw receipt](benchmarks/native-hot-paths-2026-10-21.json) retains all 1,620 measured samples, 54 syscall profiles, load averages and executable hashes.

The [dedicated run](https://github.com/unbraind/pm-rust/actions/runs/39657720819) compares merged main `c0154dbd7cd8071ac52b891be13f956f7a0f21fd` (including PR #101) with candidate `32c14792b2c041fb4bf30a265af328bb3742ca44`. Each runtime/case has twenty measured launches after two warmups. All 27 oracle cases succeed. Every baseline/candidate output and target item/history comparison passes byte for byte, including known native refusals.

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

## Behavioral revert proof

`just perf-revert-proof` temporarily restores full payload retention in claim lineage and removes the unrelated-file fast path. It preserves the test observations, requires the two specific performance assertions to fail, restores the original source bytes in a `finally` block, and requires both tests to pass. A compiler or tool error is not accepted as a regression proof.

At `32c1479`, both negative controls fail their intended assertion with exit 101 and one failed test each; both restored controls pass with one test each. The [receipt](benchmarks/native-hot-path-revert-2026-10-21.json) records exact source restoration and hashes for the combined implementation. Only the structured receipt is retained publicly.

## Validation

The [structured check receipt](benchmarks/native-hot-path-checks-2026-10-21.json) records ten linked PM commands, 26 passing test executions and zero skips. The direct concurrency suite passes both tests. Native writers run in a real `pm init` tracker with no Git executable on PATH; all fifty must succeed, all unique comments must be present, and the 51-record history hash chain must agree.

The first local linked run had three compiler-build or cargo-lock timeouts on a contended development host. A duplicate local library run was deliberately stopped and is not counted as a pass. After prebuilding the integration target, the linked rerun passes all commands. The existing evidence-recording policy remains `tracking_disabled`; public receipts omit private execution-context paths. Inherited commands were reviewed and acknowledged locally. `.mts` scripts use Node native type stripping and executable syntax/behavior checks; a standalone TypeScript compiler is not installed locally.

The unchanged [full CI gate](https://github.com/unbraind/pm-rust/actions/runs/39657720684) passes at `32c1479` on all three platforms. Each aggregate runs ordinary and instrumented suites, formatting, strict Clippy, private-item rustdoc, dependency audit and changelog verification. The documented fresh-clone tracker preflight passes. The audit retains the existing allowed `RUSTSEC-2024-0449` advisory.

| Platform | Ordinary / instrumented tests | Lines | Functions | Regions | Branches |
| --- | ---: | ---: | ---: | ---: | ---: |
| Linux | 582 / 582 | 14062 / 14062 | 1472 / 1472 | 23500 / 23500 | 3302 / 3302 |
| macOS | 571 / 571 | 13977 / 13977 | 1459 / 1459 | 23350 / 23350 | 3264 / 3264 |
| Windows | 565 / 565 | 13973 / 13973 | 1458 / 1458 | 23348 / 23348 | 3266 / 3266 |

Every aggregate executes 44 test binaries with zero failures or ignored tests and 100% in all four required coverage dimensions. The [gate receipt](benchmarks/native-hot-path-gates-2026-10-21.json) retains source hashes, exact counts, job links and all 270 existing latency samples. All nine unchanged get/list/context p95 bounds pass. CodeQL also passes. Thresholds and exclusions are unchanged.

## Baseline transition

PR #101 merged during the original validation. This branch then incorporated the merged parity baseline and repeated the complete paired matrix, revert controls, linked tests and platform gates. Primary tables and receipts above describe that combined implementation. The [earlier reproduction receipt](benchmarks/native-hot-path-reproduction-2026-10-21.json) is historical pre-parity evidence at `fd836ba`; it does not describe the current executable. Older full receipts remain available in Git history and their linked workflow artifacts.
