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
// After a failure, select its retained case.json. Boundary failures replay only
// the whitelisted scenario/implementation/syscall/ordinal, never stored commands.
// Exception: kind: 'baseline_parity' replays the scenario across both implementations
// and all their observed boundaries, rather than selecting a single case.
// A recovery_parity manifest likewise replays its complete paired scenario.
// replayCrashFailure(options, '.crash-failures/failure-example/case.json');
```

Each fixture has one synthetic Task and its acknowledged create history.
Update changes its description; comment appends a synthetic comment. Restart
uses a different operation: update is followed by a comment sentinel; comment
is followed by a description update. It then returns to the original operation
to detect a stranded journal that a successful different operation concealed. Both uninterrupted alternatives (before/committed) run that identical
restart command. No history hash, patch, receipt or existing stream is normalized
or repaired. Item and history bytes must match one complete reference state;
each implementation's raw restart receipt must match its corresponding reference.
The cross-runtime acknowledgement projection is explicitly update `id/status`
or comment `id/comments`, because native mutation commands emit full legacy JSON
while the published CLI emits flat mutation or collection receipts. Durable
item/history comparisons remain exact bytes.

The declared boundaries are all observed task-local entry ordinals of
`rename` and `fsync`, up to the configured maximum. Child processes set
`UV_THREADPOOL_SIZE=1` so successive published publications use one libuv worker
and their observed ordinals remain reproducible. Cache renames remain in scope;
this configuration does not certify other worker-pool sizes. An additional
`openat` ordinal 1 cut is traced with strace `-P` scoped to the item history
stream, including published execution after item publication but before history
append. strace ordinals count per thread; libuv worker assignment does not make
a rename cut equivalent to a history-open cut. A zero
syscall count is reported, and a missing requested boundary or truncated matrix
refuses. A kill is counted only when the trace shows an interrupted selected
syscall and a fatal SIGKILL. strace kills remaining tracees on controller death;
commands also have time and output bounds. The corpus does not cover every
syscall, torn kernel writes, filesystem/power failures, network backends or
cross-runtime replay of another implementation's journal format. Socket/event-loop `write` calls are excluded:
they do not provide stable storage boundaries in Node's worker scheduler.

Public tracker validation takes a stable regular-file copy of a publicly
published `.agents/pm` tree. The harness never runs a command in that input tree:

```bash
just crash --native target/debug/pm-rust --source public-copy/.agents/pm \
  --artifacts .crash-failures --json --full
just crash --native target/debug/pm-rust --artifacts .crash-failures \
  --replay .crash-failures/failure-example/case.json --json
```

Root caches (`locks`, `transactions`, `checkpoints`, `search`) are
excluded from the authoritative copy. The complete `runtime/` tree is captured,
including native `runtime/transactions/` recovery intent. Derived runtime indexes
may change; pending transaction bytes are retained at kill, after restart and
after returning to the original operation. Any pending journal fails the case. Nested names remain authoritative. Regular
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
The source fingerprint remains unchanged. The native lock recovery fix is now integrated from main; the receipt above
remains historical. Complete cross-runtime crash conformance remains unproven. Four [behavioral negative controls](CRASH_REVERT_PROOF_2026_10_22.json)
fail their regression with the production invariant disabled and pass restored.

Exact-head CI evidence is recorded in the PR.
Full-gate success tests the harness implementation and existing package contracts;
it does not turn a failed crash conformance report into runtime approval.

## Orchestrator review correction

The historical receipts above excluded runtime journals and used same-operation
restart. They remain immutable and do not establish cross-operation conformance.
The current corpus records pending journals and tests a different operation plus
return to the original one. Priority-zero `pm-rust-cross-operation-journal` blocks
PR 104: the real native rename-1 kill leaves a journal, the different operation
exits zero, then the original operation refuses with RecoveryConflict. PR 105's
first-fsync gate fix does not repair this separate defect.

Recovered item/history bytes, shared receipt projections, states and pending
journals are compared across runtimes for every common measured cut. A mismatch
fails the report, even when both local reference checks passed. Unpaired native
fsync ordinals are not represented as cross-runtime parity evidence.

The metadata constructor and Date.now() are both frozen at CRASH_CLOCK. This also
freezes published lock TTL ages, Date.now-based wait budgets and stale-gate age
calculations (which may be negative against real file timestamps). performance.now()
and the harness's external process deadline remain real. The clock policy is
included in every report. No elapsed-time, stale-gate or TTL parity is certified.

Permanent independent update/comment rename manifests and a published history-open
manifest live in `tests/fixtures/crash-review/`. Their tests always run on Linux;
they never depend on the full corpus finding another failure. Fixing first-fsync
recovery therefore cannot silently remove replay coverage. The locked oracle is
2026.10.21. Reported empty-lock recovery in 2026.10.22 is outside that pin; the
first-fsync item must not imply current published-release parity without a
separate versioned measurement. Dependency pins are unchanged.

A synthetic reduction refusal is secondary evidence. It is attached to the
already measured failure; the CLI retains exit 1, the original cases and the
unchanged-source result, rather than replacing them with a setup refusal exit 2.

Reproduce review evidence with `node scripts/crash-review-proof.mts` after the
recorded source commit is checked out, the locked oracle/tools installed and the
native executable rebuilt. This Git-dependent evidence recorder explicitly
refuses a missing repository; `just crash` still requires no Git. The new receipt
`docs/CRASH_REVIEW_2026_10_23.json` records the code commit, public tracker commit,
source hashes, exact toolchain versions, build/reproduction commands and exact
negative-control patches under `docs/controls/crash-review/`. Each patch is
applied independently, must fail its regression, and is restored byte-for-byte
before the passing control. Original receipts are not rewritten.

## Integrated native recovery

The recovery fix permits an unexpired empty publication gate to be reclaimed only
while holding its kernel lease and after confirming the lock's original process
is dead on the same Linux host/namespace. The lock payload is already complete at
the first fsync entry. Reclamation checks the gate's identity and emptiness;
nonempty, replaced, live-owner and unverified gates retain their refusal policy.
The existing durable recovery-intent receipt is written before removing the dead
owner's lock. Item/history serialization and journal recovery are unchanged.

Both minimized reproductions are permanent, independent replay cases:

```bash
just crash --native target/debug/pm-rust --artifacts .crash-failures \
  --replay tests/fixtures/crash-restart/update.json --json
just crash --native target/debug/pm-rust --artifacts .crash-failures \
  --replay tests/fixtures/crash-restart/comment.json --json
```

The fixtures contain only the whitelisted selection and minimal synthetic
settings. The locked oracle creates their acknowledged seed, and each real
injected kill must restart successfully with exact reference item/history bytes
and the corresponding raw restart receipt. The permanent native first-fsync regressions require successful recovery.
The expanded corpus separately reports published history-open divergences. Separate ownership JSON refusal comparisons on the shared gate path
retain the oracle's complete error receipt and authoritative bytes. The no-git
contention contract runs 50 simultaneous
writers, three comments each, and verifies all 150 comments and the history chain.

The [post-fix synthetic receipt](CRASH_RESTART_RECOVERY_2026_10_22.json) records
20/20 measured kills with exact reference states, preserved acknowledged history
and matching raw restart receipt hashes, including both first-fsync cases.
Both cross-runtime baseline byte and acknowledgement comparisons pass. A 60000 ms
per-process timeout was used for this local run; the SDK default remains 15000 ms.
The [rebuild-based revert proof](CRASH_RESTART_REVERT_PROOF_2026_10_22.json)
replaces only the recovery function with its original implementation: both
permanent replays fail with restart exit 4, then both pass after restoration.

