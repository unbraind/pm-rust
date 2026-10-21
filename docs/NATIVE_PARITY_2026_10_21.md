# Native parity with pm-cli 2026.10.21

The published oracle and native compatibility constant are pinned to 2026.10.21.
The lockfile retains the registry tarball and integrity. This receipt covers the
changes since the previous 2026.10.19 oracle; it does not expand every published
command into the native implementation.

## Applicable release changes

| Release change | Native behaviour |
| --- | --- |
| GH-1518 dead same-host lock owner | Linux captures the same boot/PID-namespace hash and original process-start field as the oracle. A positive missing-process or reused-PID observation permits recovery before TTL expiry. Recovery intent is persisted before removal; the existing byte-identity check and publication gate protect replacements. |
| Contention advice | Preserve the actual invocation, suggest a bounded `PM_LOCK_WAIT_MS` budget and read-only lock inspection, and never suggest a live-lock force override. The native admission loop honours a valid environment wait budget. |
| GH-1520 unknown SDK mutation keys | Serialized create, update, comment, ownership and close requests reject unknown fields during decoding, before workspace access. Rust struct literals also reject unknown option fields at compile time. |
| GH-1519 test option preflight | Test execution remains outside the native slice. Unsupported execution/context/timeout flags are refused by CLI parsing before any linked-test mutation. |
| Unknown assurance declaration reads | Read-only discovery and lookup return typed usage refusals with `assurance_evaluation_refused`, unchanged state and a bounded discovery command; snapshot cursors resume bounded pages and refuse changed registries. Gate evaluation, declaration writes and presets remain outside this slice. |
| GH-1507 default list ordering | Brief rows retain priority. Metadata explains nonterminal status, ascending priority, descending update time and ascending ID tie breakers. Complete brief TOON retains this sorting disclosure. |
| GH-1511 context cursor boundaries | Active blocked candidates enter packing only through the current focus page. Nonactive blocked fallback candidates retain their separate admission. |
| GH-1512 completion boundaries | Integer counts define half-up completion rounding, including 29/200 = 15 percent. |
| GH-1510 relationship cursor recovery | Recovery names the cursor flag used by the command rather than always naming `--after`. |
| Rendered relationship budget recovery | Oversized root/evidence packets refuse with a typed usage receipt. Candidate budgets replay the preserved query; bounded verification distinguishes a proven minimum, a feasible candidate and unavailable advice. Canonical retry arguments preserve scope, traversal controls and encoding, and are accepted by the native CLI. Packet cost excludes hidden row-discovery metadata. |
| GH-1509 legacy blockers | Scalar and relationship blocker resolution recognizes `depends_on` aliases, retains resolved fallback details and ignores absent-target markers. Differential coverage checks the published outputs. |
| GH-1514 injected SDK author | The native SDK has no injected extension client. Explicit identity/author precedence remains owned by the native request and scoped attribution contracts. |
| GH-1500/1501 JSON facts and brief output | Literal empty and whitespace closure/search arguments remain in recovery and rejected-value receipts. Brief list JSON uses compact wire bytes and matching budget estimates. TOON preserves explicit nulls and empty descendants inside data rows. Standard brief envelopes remove redundant declarations while retaining partial-page continuation, source completeness and effective sorting evidence. |

The remaining release notes concern upstream dependency advisories, CI scheduling,
review receipts, native SQLite startup, extension/MCP transport, generic item
mutation previews, and other recovery/packing surfaces outside the supported native
slice. They do not justify adding a JavaScript runtime to the distributed binary.

## Concurrency without Git

`tests/concurrency_contract.rs` initializes a plain-directory tracker with the
published `pm init`, bounds Git discovery at its parent, and invokes the native
binary with an empty executable search directory. Eight concurrent writers submit
three comments each. Every submission must succeed; every unique marker appears
exactly once, all 25 history records remain, and every before/after hash link is
checked. A lock-timeout rejection fails this acceptance test.

`tests/parity_1021_contract.rs` includes a real process that acquires a published
lock and exits without releasing it. A native writer must recover that unexpired
lock and preserve its original receipt in the immutable recovery audit. Unsupported
hosts omit unverifiable coordinates and retain conservative TTL/force policy.

## Verification

The required aggregate is `just release-check`, including the unchanged 100 percent
line, function, region and branch gate. CI applies it on Linux, macOS and Windows.
The linked tracker item records measured results, behavioural revert proofs and
any remaining blockers. A version pin alone is not certification.

The MSVC native executable reserves an 8 MiB virtual stack so LLVM-instrumented
CLI mutations can run under the Windows aggregate. Stack pages remain committed
on demand. This does not change the coverage scope or test thresholds.
