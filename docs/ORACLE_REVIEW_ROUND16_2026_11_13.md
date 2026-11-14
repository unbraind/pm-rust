# Username credential registration and Windows junction fixture

Starting head: `3af9d5cf8c2540154b2d59b790a18bbac94e4962`. The worktree was clean,
main was fully included and the remote head matched. The one CodeRabbit thread,
PRRT_kwDOTwgMRs63Ik26, was read in full, including its agent prompt.

## Shared credential boundary

`Credentials::new` previously selected only a nonempty password. URL userinfo
was hidden, but a token used as the username could survive in a copied message.
Both invocation recovery and stored diagnostic projection call this constructor.
The fix registers both nonempty components and reuses existing raw, POSIX,
Windows and JSON spelling generation. Complete usernames use the existing full
spelling projection; passwords retain their bounded fragment projection.

The complete username matters for compatibility: sampling `user.invalid` into
six-character fragments hid the unrelated `.invalid` suffix in `host.invalid`.
The unchanged round 12 slash-password test caught that candidate regression.
The final implementation keeps the old assertion, preserves the public host,
and still redacts a complete copied username. Short component boundaries remain
unchanged, including `git` and `u` inside the public words `github` and `legit`.

One shared corpus covers username tokens with nonempty, empty and absent
passwords, empty usernames, empty userinfo components, public host suffixes and
short-name controls. Unit tests exercise field projection, normalized arguments,
rebuilt commands and duplicate/parse/lock/history diagnostics. Four binary tests
separately cover invocation and stored duplicate diagnostics in JSON and human
modes. Each verifies output and unchanged authoritative item/history bytes.

The initial focused reproduction failed two unit tests and all four binary
tests. In a disposable source copy, restoring only the original
`Credentials::new` again failed those same six executed tests. Restoring the fix
passed all six. Every copied source/test file was then verified byte-for-byte
against the reviewed implementation. No reviewed source was mutated for reverts.

Independent published CLI probes also reach `already_claimed_by`, exit 4, in
both modes and retain the copied username. The intentional stronger native
privacy contract is recorded in `docs/NATIVE_PARITY_2026_11_11.md`; no existing
oracle assertion was changed, weakened or skipped.

## Windows fixture

Both failing Windows jobs (Rust and aggregate release gate) report only
`lifecycle_admission::tests::round7_windows_junction_is_not_a_blocker_source`.
Its `cmd /C mklink /J` setup failed before traversal was tested. The old assertion
discarded command stdout and stderr, so the operating-system error itself was
not retained. Mixed separators are the source-backed likely cause, not a locally
reproduced Windows diagnosis.

`disk()` is byte-identical between the round 7 commit `0d43438` and the prior
head; it did not acquire an existing junction or change its layout. However,
both its root and the junction path contained slash-separated joined strings.
The fixture now constructs `.agents`, `pm`, `tasks`, `ignored` and item filenames
as native path components. It explicitly verifies the parent/source directories
exist and the destination is absent, and reports mklink stdout and stderr on
failure.

The traversal property is stronger: a regular local open blocker is returned
while a valid closed blocker behind the junction is ignored. After removing the
local file, neither traversal nor blocker lookup may return the external file.
The existing junction boundary assertion and cleanup remain.

Windows execution is unavailable locally. Both Windows GNU and Apple ARM64
`cargo check --tests` compile the platform-specific test paths; CI must execute
the Windows junction case. No platform result is inferred from a Linux run.

## Gates

`cargo test --locked -j2 --all-targets --all-features` passed **902 tests,
0 failed, 0 ignored**, across 29 test-bearing targets and five zero-test examples,
in **1692.1 seconds (28.20 minutes)**. The pinned published CLI was required;
build jobs and test threads were capped at two, with a 2700-second process-group
deadline. All 171 differential tests passed without assertion changes.

| Target | Passed |
| --- | ---: |
| `src/lib.rs` | 451 |
| `src/main.rs` | 81 |
| `tests/admission_contract.rs` | 12 |
| `tests/branch_merge_contract.rs` | 1 |
| `tests/capability_contract.rs` | 6 |
| `tests/changes_contract.rs` | 1 |
| `tests/claim_release_differential.rs` | 57 |
| `tests/cli_dispatch.rs` | 37 |
| `tests/close_differential.rs` | 11 |
| `tests/concurrency_contract.rs` | 2 |
| `tests/conformance_differential.rs` | 2 |
| `tests/create_contract.rs` | 8 |
| `tests/harness_contract.rs` | 3 |
| `tests/identity_audit.rs` | 7 |
| `tests/list_differential.rs` | 11 |
| `tests/mutation_contract.rs` | 29 |
| `tests/native_adapter_contract.rs` | 13 |
| `tests/oracle_launcher.rs` | 2 |
| `tests/orientation_differential.rs` | 38 |
| `tests/output_parity_differential.rs` | 2 |
| `tests/ownership_contract.rs` | 11 |
| `tests/pagination_differential.rs` | 33 |
| `tests/parity_1021_contract.rs` | 14 |
| `tests/query_cost_contract.rs` | 2 |
| `tests/read_contract.rs` | 18 |
| `tests/read_session_contract.rs` | 7 |
| `tests/release_pipeline_contract.rs` | 23 |
| `tests/search_collections_differential.rs` | 17 |
| `tests/strict_json_contract.rs` | 3 |

The PM-linked command also passed all six new tests:
`cargo test --locked -j2 --bin pm-rust --test cli_dispatch round16 -- --test-threads=2`.
Its results were not written as package test-run artifacts.

The following gates passed with individual 1200-second caps:

- `cargo check --locked -j2 --tests --target x86_64-pc-windows-gnu`
- `cargo check --locked -j2 --tests --target aarch64-apple-darwin`
- `cargo clippy --locked -j2 --all-targets --all-features -- -D warnings`
- `cargo clippy --locked -j2 --all-targets --all-features --target x86_64-pc-windows-gnu -- -D warnings`
- `cargo fmt --all -- --check`

These were cross-checks, not Windows or Apple runtime execution. CI must run the
junction test and certify aggregate coverage. The full release gate repeats the
measured suite and adds instrumented coverage, so it remains for CI under the
host budget. Thresholds and coverage exclusions are unchanged.


`npm run changelog:full` and `npm run changelog:check` passed after the last
tracker write and own-claim release. Item `pm-rust-oracle-1111` remains in_progress
and unclaimed, never closed. Its history preserves the prior head as an exact
byte prefix; the item is introduced on this PR and is absent from main. Public
additions passed the local path/privacy inspection. The work uses the required
SteveBot commit identity. No review thread was replied to, resolved or reacted to.
