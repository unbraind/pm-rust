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
cp target/release/pm-rust target/pm-rust-before
# Apply the candidate and rebuild through just bench.
just bench --baseline target/pm-rust-before --profile yes \
  --output target/native-bench.json
```

`--sizes`, `--samples` and `--warmups` allow shorter diagnostic runs. The
benchmark verifies executable fingerprints again after the run, compares
baseline and candidate stdout/stderr/status exactly, and compares their item
and history hashes after each write. Write targets and runtime state are reset
outside the timed interval, so every claim is a real first claim and every
comment/update starts from the same item. Only generated state is removed.

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

## Measured results

The completed receipt and before/after tables are recorded below after the
paired run. Shared-host contention limits the interpretation of tail latency;
the complete samples and load averages remain part of the evidence.
