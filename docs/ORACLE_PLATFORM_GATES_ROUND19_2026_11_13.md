# Platform release-gate closure

Starting head: `87718b655ab4b65677da6dbcb4abf48572f94d4f`. The worktree was
clean, the remote branch matched and main was already included. This round
addresses only the two platform release gates; no review threads are open.

## Measured CI causes

[Run 43512040037](https://github.com/unbraind/pm-rust/actions/runs/43512040037)
shows macOS reaching 100% regions, branches, lines and functions before
cancellation at the job budget during final changelog checking. The workflow
now gives macOS 45 minutes, retaining Windows at 60 and Linux at 35. A one-line
comment explains the macOS exception. Every step and threshold remains intact.

The Windows aggregate misses exactly three regions, four branches and eight
lines. Its affected paths and portable regression coverage are below.

## Windows test map

| Case | Test and production path | Sensitivity control |
| --- | --- | --- |
| B1 | `command_line::tests::round19_windows_decoder_edges` is `cfg(windows)`. Expected vectors are written from the documented Windows rules: an even backslash run before a quote halves the run and toggles quotation; odd runs retain a literal quote; other and trailing runs stay literal. Cases exercise opening and closing even runs, odd runs inside and outside quotes, ordinary and trailing runs, and unclosed quotations. | Replacing only the even-run quote toggle with always-unquoted fails the opening case; replacing it with always-quoted fails the closing case. Both restored tests pass. |
| B2 | `lifecycle_admission::tests::round19_opened_nonregular_blocker_is_read_only` is ungated. It supplies an actual opened directory to the same `read_blocker_handle` used by production, asserts `io::ErrorKind::InvalidInput` and the exact nonregular-file diagnostic, and checks all tracker bytes and directory contents. A regular blocker handle still returns its original bytes. | Removing only the regular-file guard fails the exact typed-error assertion; restoration passes. |
| B3 | `tests::round19_unclosed_recovery_advice_is_not_executable` is ungated. The unclosed double quote requires the exact nonexecutable explanation; its well-formed twin retains selected-tracker scope. The real POSIX shell round trip stays Unix-only. | Returning `None` instead of the decode-refusal explanation fails the assertion; restoration passes. |

For B2, Windows requires `FILE_FLAG_BACKUP_SEMANTICS` to open a directory.
Only the test fixture supplies that flag. The production opener and its no-follow
flags remain unchanged; the existing post-open kind/read sequence is factored
into a shared function so every platform can test that exact guard with a real
handle. Neither a pathname-type check nor a synthetic metadata value substitutes
for the owned-handle check.

## Local proof boundary

Windows cannot execute locally. In the isolated source copy only, the
platform-independent character decoder and its new test select the Windows arms
instead of the POSIX arms. This executes the precise decoder logic and literal
expected vectors on Linux for the two sensitivity controls; it is not a Windows
runtime or coverage result. No cfg selector in the reviewed worktree is changed
for this exercise. The other two controls run their ungated tests directly.
All copied source and tests are restored byte-for-byte after the controls.

Windows GNU test cross-check/clippy and Apple ARM64 test cross-check validate
platform compilation. Fresh CI certifies Windows execution and coverage. Linux
must retain exact 100% counts in all four gate metrics in the full instrumented
suite; those final totals and targeted-gate results follow below.

## Touched targets

`cargo test --locked -j2 --lib --bin pm-rust --test release_pipeline_contract`
passes **456 library + 83 binary + 23 release-pipeline contract tests = 562**,
with zero failures or ignored tests, in **112.4 seconds**. Focused execution also
passes the two ungated regressions. The B1 decoder test has its Windows cfg gate
and its literal expected vectors checked by the isolated controls described above.

## Full Linux coverage

`cargo +nightly-2026-08-06 llvm-cov --locked --branch --all-targets --all-features`
with JSON reporting outside the repository completes in **1,745.7 seconds
(29.10 minutes)**: **915 passed, 0 failed, 0 ignored across 34 targets**.
The pinned published oracle is required, with two build jobs and test threads.

| Metric | Covered / total | Percent |
| --- | --- | --- |
| Regions | 31,895 / 31,895 | 100% |
| Branches | 4,434 / 4,434 | 100% |
| Lines | 19,482 / 19,482 | 100% |
| Functions | 1,994 / 1,994 | 100% |

The JSON totals are checked for exact covered/total equality for every metric;
no rounded incomplete percentage is accepted. No threshold, coverage exclusion,
coverage-specific source gate, differential assertion or production guard changes.

## Cross-target and static gates

Each command passes within its 20-minute cap:

- `cargo check --locked -j2 --tests --target x86_64-pc-windows-gnu` (4.0 s).
- `cargo check --locked -j2 --tests --target aarch64-apple-darwin` (4.1 s).
- `cargo clippy --locked -j2 --all-targets --all-features -- -D warnings` (11.7 s).
- `cargo clippy --locked -j2 --all-targets --all-features --target x86_64-pc-windows-gnu -- -D warnings` (11.7 s).
- `cargo fmt --all -- --check` (1.8 s).

The Windows cross-check compiles the `cfg(windows)` decoder corpus and Windows
directory-handle fixture along with the ungated refusal test. The PM-linked
`cargo test --locked -j2 --lib --bin pm-rust round19` passes both portable tests,
with run-artifact recording disabled. Main remains fully included after a fresh
remote read. The full platform aggregate and actual Windows coverage remain for
fresh CI on the pushed head.

## Final tracker and changelog checks

Evidence is recorded on `pm-rust-oracle-1111` with linked files, this document,
the test command and negative/restored control receipts. Its claim is released;
it remains `in_progress` and unclosed. After the final tracker write,
`npm run changelog:full` and `just changelog-check` pass; the generated changelog
has no content diff. Item history remains a byte-prefix extension of the starting
head. New public evidence contains no absolute local paths or host identities.
