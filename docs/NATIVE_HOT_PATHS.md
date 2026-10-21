# Large tracker agent hot paths

Run `just bench` on Linux with Node 24+, Rust 1.90 and GNU time. The benchmark
installs its own lockfile-pinned pm-cli **2026.10.21** oracle. The release gate
keeps its existing independent oracle until the parity prerequisite merges.

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

The [complete before/after tables](benchmarks/native-hot-path-tables-2026-10-21.md)
include every command at all three sizes, p50/p95, maximum RSS, stdout/stderr
bytes, exit status and exact current-oracle output/storage comparisons. The
[raw receipt](benchmarks/native-hot-paths-2026-10-21.json) retains all 1,620
measured samples, 54 syscall profiles, load averages and executable hashes.
The [dedicated run](https://github.com/unbraind/pm-rust/actions/runs/39644972620)
measured immutable main `8e32f85` against production source `124975a`, using
twenty launches per runtime/case after two warmups. All 27 oracle cases exited
successfully. Every native baseline/candidate output and target item/history
comparison passed without normalization, including the known refusals.

At 50,000 items:

| Operation | Before p50 / p95 ms | Native p50 / p95 ms | Before / native peak RSS KiB |
| --- | ---: | ---: | ---: |
| claim | 3836.3 / 3881.5 | 2585.6 / 2602.1 | 508528 / 25080 |
| comment | 762.4 / 766.0 | 62.9 / 63.8 | 11584 / 11976 |
| update | 760.5 / 770.1 | 62.8 / 64.0 | 11544 / 11468 |

Comment and update each reduce `statx` calls from **300,026 to 26**, retaining
all **eight fsync calls**, the same opens/reads and the same durable item/history
bytes. Claim reduces metadata calls from **400,042 to 100,042**, retains all
**eleven fsync calls**, and lowers peak RSS by about **95.1%**. Its strict corpus
validation retains the same 50,033 `openat` calls; it drops each full payload after decoding
instead of retaining the entire corpus. On filesystems without cached directory
entry types, type lookup can itself require metadata calls; the syscall counts
above describe the measured runner.

The evidence does not establish a uniform native speed advantage. At 50k, the
oracle's claim p50 is 490.6 ms versus native 2585.6 ms; search is 1691.8 versus
2592.5 ms; the supported list case is 327.0 versus 1022.9 ms. Strict validation
and full scans remain substantial costs. Get measures 41.5 versus 316.3 ms.
Current-oracle byte equality is achieved by the 1k search fixture; larger search
rows and other raw command surfaces record mismatches explicitly. The release
oracle remains independently pinned, and current-oracle certification is still
owned by the separate parity work. Output-format or behavior differences must
be resolved before claiming equivalent current-oracle performance.

Native context at 10k/50k and all native next/list-open rows exit 2. Their timings
are refusal costs and are excluded from successful-operation speed claims.
History streams are verified before timing, read commands retain all
authoritative hashes, and mutations start from identical target bytes. The
measurements cover one dependency, four comments, one file and one test per item;
they do not certify denser graphs, longer histories or every filesystem.

## Behavioral revert proof

`just perf-revert-proof` temporarily restores full payload retention in claim
lineage and removes the unrelated-file fast path. It preserves the test
observations, requires the two specific performance assertions to fail, restores
the original source bytes in a `finally` block, and requires both tests to pass.
This distinguishes an intended regression failure from a compiler or tool error.

At `124975a`, both negative controls failed their intended assertion with one
failed test each; both restored controls passed with one test each. The source
hashes in the [receipt](benchmarks/native-hot-path-revert-2026-10-21.json) match
the source that was benchmarked. The workflow retains only the structured
receipt, with no raw compiler output or tracker contents.

## Validation

The [structured check receipt](benchmarks/native-hot-path-checks-2026-10-21.json)
records local library/SDK checks, the strict 50-process no-git contract and the
linked PM runner. Native writers run in a real `pm init` tracker with no Git
executable on their PATH; all fifty must succeed, all unique markers must be
present, and the 51-record history hash chain must agree.

Inherited linked commands were reviewed and acknowledged locally, then run
through `pm test --run --progress --fail-on-skipped --fail-on-empty-test-run`.
The existing result-recording policy remains `tracking_disabled`; public
receipts omit the runner's private execution-context paths. `.mts` scripts use
Node's native type stripping and executable syntax/behavior checks; a standalone
TypeScript compiler is not installed locally.

The unchanged [full CI gate](https://github.com/unbraind/pm-rust/actions/runs/39650772379)
passed at `fd836ba` on all three platforms. Each aggregate runs ordinary and
branch-instrumented tests, formatting, strict Clippy, private-item rustdoc,
dependency audit and changelog verification. The documented fresh-clone tracker
preflight also passed. The dependency audit retains the repository's existing
allowed `RUSTSEC-2024-0449` advisory.

| Platform | Ordinary / instrumented tests | Lines | Functions | Regions | Branches |
| --- | ---: | ---: | ---: | ---: | ---: |
| Linux | 525 / 525 | 13218 / 13218 | 1370 / 1370 | 22077 / 22077 | 3160 / 3160 |
| macOS | 521 / 521 | 13265 / 13265 | 1382 / 1382 | 22164 / 22164 | 3162 / 3162 |
| Windows | 516 / 516 | 13264 / 13264 | 1383 / 1383 | 22165 / 22165 | 3164 / 3164 |

Every aggregate has 42 suite executions, zero failures/ignored tests, and 100%
in all four required dimensions. The [gate receipt](benchmarks/native-hot-path-gates-2026-10-21.json)
retains exact source hashes, counts, job links and all 270 existing latency-gate
samples. All nine unchanged get/list/context p95 bounds pass.

The first aggregate exposed the locator fallback filename check's missing false
arm. The added test-only case closes that branch without changing production
Rust, coverage thresholds or exclusions. Both behavioral revert controls still
fail at their intended assertions and pass restored.

The [repeat benchmark](https://github.com/unbraind/pm-rust/actions/runs/39650772378)
also passes at `fd836ba`, with another 1,620 measured samples. Its baseline and
native executable hashes are identical to the committed measurement run; the
[reproduction receipt](benchmarks/native-hot-path-reproduction-2026-10-21.json)
records that comparison. The final documentation and tracker handoff preserve
the verified implementation and test source.
