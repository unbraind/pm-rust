# Bounded native read sessions

The v1 embedding surface is `Workspace::read_session`, `ReadSessionLimits`,
`ReadSession::next_page`, `ReadSession::receipt`, `ReadSession::cancel`,
`SessionCancellation`, `SessionPage`, `SessionReceipt` and `SessionRefusal`.
The [runnable Rust host](../examples/read_session.rs) prints each canonical page
and then drops it. Applications retain ownership of returned pages and cancellation
handles; accumulating pages consumes application memory outside session accounting.

A session captures settings, the configured status/workflow schema files, and
regular TOON items in a private temporary directory. It validates each **complete**
item with the existing strict decoder and rejects duplicate IDs before delivery.
It uses `Workspace::list_page` unchanged with all statuses, brief projection,
caller page size and the existing output token budget. Producer and output
continuations retain the original tracker query identity. JSON overrides preserve
unpaired UTF-16 compaction cuts; the existing TOON codec can refuse those cuts.
Canonical list recovery flags refer to the ordinary `list` command; session
controls are documented separately. The original tracker is never written. Git, extensions, JavaScript and telemetry
are absent from the native path. The package's pinned oracle is unchanged.

The snapshot is an immutable capture of individually read files, not a database
transaction or an atomic point-in-time view across concurrent source writers.
Its SHA-256 commits to the deterministic sequence of relative names, byte lengths
and admitted bytes. It certifies validated source items, including terminal items;
`complete` becomes true only on the final delivered page. Field compaction and
brief-field omissions remain explicit in each canonical page. This is not the
SDK's unbounded full-field `complete_list` certificate. An interrupted or refused
session must never be promoted to a complete context read.

The memory ceiling is an **admission accounting contract**, not an operating-system
RSS limiter. Admission reserves 8 MiB plus 256 bytes per output token, then 128
bytes per source byte and caller clock byte, 8 KiB plus 16 bytes per relative filename byte per copied
file, and 1 KiB per enumerated directory entry. The cumulative ledger conservatively
covers source decoding, indexes and rendering; it refuses before reading a file
that cannot fit. Source reads are limited to the admitted length plus one byte
and refuse length drift. Directory enumeration charges before retaining entries;
depth is limited to 32. The private disk snapshot occupies at most the admitted
source bytes, apart from filesystem metadata. No full source corpus is retained
between pulls. Each pull still scans the admitted snapshot through the existing
reader; v1 adds bounded admission and embedding ownership, not a new query engine.

Allocator overhead, stack size, codec implementation, operating system and host
application allocations affect actual RSS. The validation harness separately
measures child peak RSS against the declared ceiling on large synthetic trackers.
Hosts requiring a hard process-wide physical limit must additionally use their
operating system's process/container limits. Accounting coefficients and the
measured platform envelope are explicit so a reservation cannot be mistaken for
an allocator-enforced universal bound.

Cancellation is cooperative: admission checks between directory entries and file
reads, and delivery checks before and after each canonical page query. It cannot
interrupt a synchronous decoder/query already running. A shared token wakes no
background task: call `next_page` after cancellation, call `cancel` while idle,
or drop the session to remove private storage. A successful final page also
removes storage. Process termination that bypasses Rust destructors is outside
this lifecycle guarantee. The CLI's cancel marker is polled before admission and
at page boundaries; it is not an OS signal handler.

| Refusal code | Caller remedy |
| --- | --- |
| `session_cancelled` | Reopen with a fresh cancellation token |
| `session_memory_limit` | Increase the admitted byte ceiling or use a smaller tracker |
| `session_invalid_limits` | Use positive controls; row/token controls are at most 1,000,000 |
| `session_unsupported_source` | Use regular files, at most 32 directory levels, tracker-relative schema files |
| `session_source_refused` | Repair tracker integrity or increase the per-page token budget |
| `session_storage_failed` | Check temporary storage permissions and free space |

All failures terminate delivery and release admitted snapshot resources. Refusals
are typed in Rust and emitted as compact structured JSON on CLI stderr with exit
code 2. An output writer failure drops the session through ordinary Rust unwinding.
The source tracker remains unchanged after cancellation and failed admission.

Reproduce the non-git public-copy and synthetic measurements with
`node scripts/read-session-validate.mts --source path/to/public-tracker-copy`.
The harness uses the real SDK's complete-list certificate and bounded list calls
as independent oracles, initializes its synthetic tracker through the SDK, checks
all delivered identities and compact JSON payloads (normalizing oracle indentation
only), and measures fresh native child peak RSS with
GNU time. Only aggregate numbers and hashes are reported; source contents and
scratch paths are excluded.

## Acceptance evidence

Measured with Rust 1.90.0, the unoptimized native binary and the pinned
2026.10.21 oracle on a contended development host. Every source is a plain,
non-git directory initialized through the real SDK or copied from public Git
objects. Files and directories were read-only during native validation. Each
native child is measured once with GNU time; wall times are observations rather
than performance guarantees. Caller controls: 512 MiB admission ceiling,
1,000 rows per page and 100,000 output tokens per page.

| Dataset | Items | Pages | Source bytes | Admission bytes | Peak RSS (KiB) | Wall (ms) | Output bytes |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| SDK-generated tracker | 10,000 | 10 | 1,959,611 | 381,185,760 | 43,688 | 28,001 | 988,986 |
| Public pm-rust tracker copy | 78 | 1 | 757,075 | 131,681,424 | 13,440 | 437 | 12,549 |

Both trackers passed the SDK's whole-corpus certificate. All identities matched;
all 11 session pages preserved the published CLI's canonical JSON payloads,
including cursor envelopes and omissions. Only indentation is normalized when
comparing the CLI's pretty continuations with compact embedded frames. The
existing differential suites independently retain exact CLI byte comparisons.
Before/after authoritative source fingerprints matched for both trackers.

The public copy came from commit
`5862034d9618aba23ee909d7a9f9bbdd1b78e252` of
[`unbraind/pm-rust`](https://github.com/unbraind/pm-rust/tree/5862034d9618aba23ee909d7a9f9bbdd1b78e252/.agents/pm).
Its unchanged source SHA-256 is
`f498f5527c1203481d01f42a473082756e48a173bc5b8dea54b222e8dfb536c2`;
its admitted snapshot SHA-256 is
`f3401ddf796ef684cebaeac8ac130e4a277e90b7132c1ed963cf1655bf74d189`.
The measured native binary SHA-256, checked before and after validation, is
`2f66a145e0d038ab66731965e98b12c5eadd960babc5e05d9f1f854caf05751d`.
Only text evidence is tracked; generated trackers, executable files and raw
outputs remain disposable.

Seven isolated behavioral reverts compiled and failed their targeted regression:
admission charging, snapshot binding, registry isolation, cancellation cleanup,
omission-only output refusal, clock-storage accounting and duplicate-file
remedies. All functions were restored before successful validation. Tests also
exercise real no-git initialization, a native PATH without Git or Node, empty
trackers, output-budget continuations, peer cancellation before and after a real
query, source length drift, schema symlink refusal, real query storage loss,
explicit cancellation, drop and successful terminal cleanup.
