# Safe executable advice and stored diagnostic redaction

Starting head: `9b5d1690126127ae0a9cedaa568bcd89e2ab407d`. All four round 15
threads were read in full, including their agent prompts. Main was already
included. No earlier assertions, thresholds or coverage exclusions were relaxed.

## Executable advice

Concrete commands now flow through `command_line::render_pm_command`, which
accepts an argument vector and calls `quote_command_arg` on every argument.
String-only legacy retries are decoded without evaluating shell expressions,
then scoped and rendered from the resulting vector. Malformed or executable
shell syntax is withheld with an explanation. The search SDK's fixed query
placeholder is instantiated once, before rendering; user text is never subjected
to a second placeholder substitution.

Construction-site inventory, including wrappers that delegate to the renderer:

| File | Sites |
| --- | --- |
| `src/command_line.rs` | `render_pm_command`; non-evaluating `parse_command_args` |
| `src/assurance.rs` | `Workspace::assurance_declaration` |
| `src/collections.rs` | `Workspace::collection` |
| `src/lifecycle_admission.rs` | `admit_with_registry`, `unblock` (inspect, scalar removal and dependency removal), `admit_blockers` |
| `src/list_intent.rs` | `omitted` |
| `src/main.rs` | `project_close_receipt_scope`, `validate_claim_target`, `orientation_error_json`, `search_error_json`, `instantiate_search_examples`, `render_recovery`, `scope_recovery_command`, `preserve_refusal_scope`, `cursor_error_json`, `apply_close_error_recovery`, `write_cursor_refusal_to` |
| `src/mutation.rs` | `close_evidence_tokens`, both `close_duplicate` examples, `close_item` completion hint, `collection_item`, `ownership_item_with_options` release suggestion |
| `src/orientation.rs` | `records` history inspection and repair advice |
| `src/orientation_deps/packet.rs` | `command` |
| `src/ownership.rs` | `partial_failure` inspection advice |
| `src/search.rs` | `types`: fixed SDK query template with rendered concrete filter arguments |
| `src/refusal.rs` | `redact`: rebuilds the projected attempted command through `render_recovery` |

Fixed help text and placeholders retain their published syntax. They contain no
interpolated stored data. Stored IDs, messages, paths and filter values are
arguments to the shared renderer. The source audit covered advice fields,
recovery fields, examples, hints and embedded inspection commands.

The shared corpus includes semicolons, conjunctions, pipes, substitutions,
backticks, newlines, quotes, backslashes, Unicode, spaces and empty arguments.
POSIX tests parse each emitted command with a real shell and compare the exact
argument boundaries, without invoking pm. Ordinary IDs remain unchanged.
Actual lifecycle, duplicate-close, partial-failure and scoped recovery producers
exercise that corpus. A four-case differential compares full native/oracle
refusal stdout, stderr, exit status and durable bytes for blocked atomic starts.
The published CLI already quotes blocker inspection commands.

## Stored diagnostics

Both structured refusal projection and fallback diagnostic projection register
credential-bearing source text before replacing any value. The fallback path
previously registered argv alone. Duplicate IDs, parse diagnostics, lock errors
and history errors now share the same registration and redaction corpus. Copies
of short, whitespace-bearing and slash-bearing passwords are covered, including
multiple URLs. Public URLs and email text remain unchanged.

A process test writes duplicate stored rows, requests JSON and human output,
checks copied secrets are absent, and verifies both files remain byte-identical.
No credential is passed in argv. The binary and in-process tests use the same
source corpus. The source-to-sink review checked both fallback diagnostic calls
and structured command, ownership and lock refusals.

## Portable FIFO regression and documentation

The Unix FIFO fixture uses the POSIX `mkfifo` utility with mode 600, avoiding the
Apple-unavailable rustix `mknodat` API and respecting the crate's unsafe-code ban.
The Linux and macOS test still keeps its nonblocking descriptor, timeout and
reader completion assertions. No dependency was added. Apple cross-compilation
checks all tests; platform runtime and aggregate coverage are certified by CI.

The earlier JSON partial-failure receipt now correctly says that its differential
requires omission of the explanation. The assertions themselves are unchanged.

## Verification

Isolated copies restored one original production function at a time. Each of
these ten controls failed its runtime assertion with exit 101 and passed after
restoration: blocker shell parsing, the blocker oracle differential, unblock
advice, terminal reopen advice, duplicate-close examples, partial-failure
inspection, scoped retries, search interpolation, stored diagnostic registration
in process, and stored diagnostic registration through the binary. The Apple
FIFO control failed compilation with E0425 for `mknodat`; the portable fixture
passed the same cross-check. All copied source and test files were compared
byte-for-byte with the reviewed implementation after restoration.

Focused coverage of the new `command_line` module measured 51/51 lines, 3/3
functions, 89/89 regions and 26/26 branches. This is a module measurement; it does
not claim full aggregate coverage. CI retains the unchanged aggregate thresholds.

The complete `cargo test --locked -j2 --all-targets --all-features` run passed
**896 tests, 0 failed, 0 ignored**, across 29 test-bearing targets and five
zero-test example targets, in **1700.4 seconds (28.34 minutes)**. The environment
required the pinned published CLI and limited test threads to two; the full
command had a 2700-second process-group deadline. All 171 tests in the eight
differential targets passed, as did the oracle-backed admission and SDK
contracts. The initial 2026.11.11 differential baseline remains 59 passed and
61 failed out of 120; no assertion was loosened.

| Target | Passed |
| --- | ---: |
| `src/lib.rs` | 451 |
| `src/main.rs` | 79 |
| `tests/admission_contract.rs` | 12 |
| `tests/branch_merge_contract.rs` | 1 |
| `tests/capability_contract.rs` | 6 |
| `tests/changes_contract.rs` | 1 |
| `tests/claim_release_differential.rs` | 57 |
| `tests/cli_dispatch.rs` | 33 |
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

The PM-linked focused command also passed all 12 round 15 tests:
`cargo test --locked -j2 --all-targets --all-features round15 -- --test-threads=2`.
Test-run artifacts were not written to the package tracker.

These final gates passed with individual 1200-second caps:

- `cargo clippy --locked -j2 --all-targets --all-features -- -D warnings`
- `cargo clippy --locked -j2 --all-targets --all-features --target x86_64-pc-windows-gnu -- -D warnings`
- `cargo check --locked -j2 --tests --target aarch64-apple-darwin`
- `cargo fmt --all -- --check`

Apple validation was a cross-check of every test, not a linked or executed Apple
test run. Linux executed the retained FIFO nonblocking assertions. Fresh CI
certifies macOS/Windows runtime and aggregate coverage; no threshold or exclusion
changed. The full release gate repeats the measured suite and adds instrumented
coverage, so it remains for CI under the host budget.

The shared runner hold was active after testing completed. The local commit is
prepared while waiting, and the hold file is re-read immediately before push.


`npm run changelog:full` and `npm run changelog:check` passed after the final PM
write and claim release. Item `pm-rust-oracle-1111` remains in_progress and
unclaimed, never closed. Its history preserves the previous head as an exact
byte prefix; the item is introduced on this PR branch and is absent on main.
Public additions passed the local path/privacy inspection. No review thread was
replied to, resolved or reacted to.
