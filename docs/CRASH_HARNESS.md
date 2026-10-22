# Crash corpus SDK and examples

The corpus requires Node 24 or later, Linux, strace with syscall injection and
`--kill-on-exit`, the locked oracle (`just oracle-install`), and a compatible
native executable. `just crash-check` typechecks erasable strict TypeScript,
lints it and runs targeted regression tests. The existing CI `just release-check`
also runs these checks after its Rust tests. Rust's four 100% coverage thresholds
remain unchanged; those percentages describe Rust source, not the TypeScript
harness.

On Linux, missing, unreadable, malformed or mismatched oracle metadata refuses
with `oracle_version` and the `just oracle-install` remedy. Malformed source
settings refuse with `unsafe_source`; malformed mutation acknowledgements
refuse with `baseline_failed`. These refusals preserve the input tracker and
do not retain failure artifacts.

```typescript
import { runCrashCorpus, replayCrashFailure } from '../scripts/crash-harness.mts';
import type { CrashOptions, CrashReport } from '../scripts/crash-harness.mts';

const options: CrashOptions = {
  native: 'target/debug/pm-rust',
  artifacts: '.crash-failures',
  scenarios: ['update', 'comment'],
  maxBoundaries: 64,
  timeoutMs: 15000,
};
const receipt: CrashReport = runCrashCorpus(options);
console.log(receipt.ok, receipt.killed, receipt.parity);
// After a failure, select its retained case.json; this executes only the
// whitelisted scenario/implementation/syscall/ordinal, never stored commands.
// replayCrashFailure(options, '.crash-failures/failure-example/case.json');
```

Each fixture has one synthetic Task and its acknowledged create history.
Update changes its description; comment appends a synthetic comment. Restart
re-enters update with the acknowledged priority, or comment with a distinct
sentinel. Both uninterrupted alternatives (before/committed) run that identical
restart command. No history hash, patch, receipt or existing stream is normalized
or repaired. Item and history bytes must match one complete reference state;
each implementation's raw restart receipt must match its corresponding reference.
The cross-runtime acknowledgement projection is explicitly update `id/status`
or comment `id/comments`, because native mutation commands emit full legacy JSON
while the published CLI emits flat mutation or collection receipts. Durable
item/history comparisons remain exact bytes.

The declared boundaries are all observed task-local entry ordinals of `rename`
and `fsync` for each runtime/scenario, up to the configured maximum. A zero
syscall count is reported, and a missing requested boundary or truncated matrix
refuses. A kill is counted only when the trace shows an interrupted selected
syscall and a fatal SIGKILL. strace kills remaining tracees on controller death;
commands also have time and output bounds. The corpus does not cover every
syscall, torn kernel writes, filesystem/power failures, network backends or
cross-runtime journal recovery. Socket/event-loop `write` calls are excluded:
they do not provide stable storage boundaries in Node's worker scheduler.

Public tracker validation takes a stable regular-file copy of a publicly
published `.agents/pm` tree. The harness never runs a command in that input tree:

```bash
just crash --native target/debug/pm-rust --source public-copy/.agents/pm \
  --artifacts .crash-failures --json --full
just crash --native target/debug/pm-rust --artifacts .crash-failures \
  --replay .crash-failures/failure-example/case.json --json
```

Root caches (`runtime`, `locks`, `transactions`, `checkpoints`, `search`) are
excluded from the authoritative copy. Nested names remain authoritative. Regular
config/schema files, items and history streams are copied byte-for-byte; symlinks
and special files refuse. Output inside the input tracker refuses, including
symlink aliases. Source fingerprints are checked again after execution. No raw
public tracker content is retained in failure artifacts: an independent minimal
synthetic run supplies the retained reproduction. A source-specific failure that
does not reproduce synthetically remains a failure with zero reproductions.

The corpus is observational and never invokes Git. All children receive an
isolated environment and a Git discovery ceiling. Test telemetry isolation uses
the existing oracle environment contract; workspace telemetry configuration is
untouched. Inputs, original histories and retained evidence are never overwritten.

The [read-only public-copy receipt](CRASH_PUBLIC_COPY_2026_10_22.json) uses public
pm-rust main `c0154dbd7cd8071ac52b891be13f956f7a0f21fd`: 74 items and 154
authoritative files. All 20 real kills preserve acknowledged item bytes and
history prefixes, both baseline byte/acknowledgement comparisons pass, and 18
restart state/receipt comparisons pass. Two native first-fsync lock-initialization
kills refuse restart with exit 4; both reproduce on minimal synthetic fixtures.
The source fingerprint remains unchanged. The safe recovery policy is tracked
separately in `pm-rust-crash-lock-restart`; complete runtime crash conformance
remains unproven. Four [behavioral negative controls](CRASH_REVERT_PROOF_2026_10_22.json)
fail their regression with the production invariant disabled and pass restored.

Exact-head CI evidence is recorded in the PR.
Full-gate success tests the harness implementation and existing package contracts;
it does not turn a failed crash conformance report into runtime approval.
