# Credential URL boundaries and runtime status precedence

Starting head: `1a4b505bfba7fa93d819f447e92e818f14ab9240`. The worktree was clean,
main was fully included, and the remote branch matched. All three requested
threads and their complete agent prompts were read.

## Credential host boundary

PRRT_kwDOTwgMRs63K1MS is valid. The old `userinfos` fallback could select an email
address after a credential URL as its final separator. That removed public text
and enrolled the wrong password, leaving a later copied `abc` visible.

The common scanner now ends a credential URL at whitespace following a completed
authority. It calls the existing `completed_authority` function from round 11b;
no second hostname parser was introduced. Host extraction retains path/query
boundaries, while unresolved userinfo may still contain whitespace or embedded
at-signs. The scan tracks the current candidate separator, so it visits the
candidate once and tests each host span only at its first whitespace boundary.

The shared corpus covers the exact reported email/copy, the `and` email variant,
IPv4 with port, bracketed IPv6 with port, localhost, slash-bearing passwords,
whitespace-bearing passwords, embedded at-signs and a public URL/email control.
The unit test checks invocation and stored source registration, payload fields
and normalized arguments. Four actual binary tests separately exercise JSON and
human invocation recovery and stored duplicate diagnostics, preserving public
email text and unchanged authoritative item/history bytes.

Restoring only the original `userinfos` in an isolated source copy fails the
unit test and all four process tests. Restoring the fix passes all five. Earlier
authority, registration and redaction corpora also pass unchanged.

## Runtime schema precedence

PRRT_kwDOTwgMRs63K1MN is valid. `runtime_settings` replaced the first normalized
inline definition, allowing a later case-variant definition to defeat an external
override. It now searches from the last definition, matching `Lifecycle::load`.

A real-file unit test compares captured and live registries for inline active
`open`, then terminal `OPEN`, followed by an external active or terminal `open`.
The active override is the successful control; the terminal override preserves
refusal/no-write behavior. An exact published `claim --next` differential compares
stdout, stderr, exit status and persisted item/history/session bytes for both.
Restoring only the original `runtime_settings` fails both tests; restoration
passes both. No per-command status workaround was introduced.

## Absent-marker finding: retain measured published policy

PRRT_kwDOTwgMRs63K1L8 is declined on oracle evidence under the standing review
rule for invalid findings. Direct pinned pm-cli 2026.11.11 probes with stored
`blocked_by: "none"` refuse both `start-task` and `claim --start` with exit 4,
`start_blocked_by_open_dependency`, and unchanged item bytes. Native already
matches. The lifecycle collector in `dist/core/item/actionability.js` uses
`normalizeItemId` and excludes `no-active-blocker`; the separate packet/reference
normalizer cited in the review does not define this lifecycle behavior.

The new differential covers `none`, `NULL`, `n/a` and `Na` across explicit and
atomic starts. It independently asserts the published refusal code and exit,
then compares complete native/published output and durable bytes and asserts
unchanged authoritative snapshots on both sides. No production marker filter
was changed. Applying the proposed filter only in an isolated copy makes this
comparison fail; restoring the existing policy passes. This is a counterfactual
policy sensitivity control, not a production-function revert.

Seven regressions therefore fail on literal reverts of the two fixed functions;
one additional parity test rejects the proposed marker-policy change. All copied
source/test files were restored and compared byte-for-byte after the controls.
No existing differential assertion was altered, loosened, skipped or conditioned
on an oracle version. No upstream issue was filed and no review thread action
was taken.

## Gates

`cargo test --locked -j2 --all-targets --all-features` passed **910 tests,
0 failed, 0 ignored**, across 29 test-bearing targets plus five zero-test example
targets, in **1857.7 seconds (30.96 minutes)**. The pinned published CLI was
required, test threads and build jobs were capped at two, and the command had a
2700-second process-group deadline. All **173 differential tests** passed.

| Target | Passed |
| --- | ---: |
| `src/lib.rs` | 452 |
| `src/main.rs` | 82 |
| `tests/admission_contract.rs` | 12 |
| `tests/branch_merge_contract.rs` | 1 |
| `tests/capability_contract.rs` | 6 |
| `tests/changes_contract.rs` | 1 |
| `tests/claim_release_differential.rs` | 59 |
| `tests/cli_dispatch.rs` | 41 |
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

The following gates also passed with individual 1200-second caps:

- `cargo check --locked -j2 --tests --target x86_64-pc-windows-gnu`
- `cargo check --locked -j2 --tests --target aarch64-apple-darwin`
- `cargo clippy --locked -j2 --all-targets --all-features -- -D warnings`
- `cargo clippy --locked -j2 --all-targets --all-features --target x86_64-pc-windows-gnu -- -D warnings`
- `cargo fmt --all -- --check`

Cross-checks compile the platform-specific tests; they are not Windows or Apple
runtime execution. Thresholds and coverage exclusions are unchanged. The full
release gate repeats the measured suite and adds instrumented coverage, exceeding
the host budget; platform runtime and aggregate coverage remain for fresh CI.


The PM-linked focused command passed all eight round 17 tests:
`cargo test --locked -j2 --lib --bin pm-rust --test cli_dispatch --test claim_release_differential round17 -- --test-threads=2`.
Execution receipts confirm package result tracking stayed disabled; no test-run
artifacts were added to the tracker.

`npm run changelog:full` and `npm run changelog:check` passed after the last PM
write and own-claim release. Item `pm-rust-oracle-1111` remains in_progress and
unclaimed, never closed. History preserves the previous head as an exact byte
prefix; this PR introduces the item, which is absent from main. Public additions
passed the local path/privacy inspection. No review thread was replied to,
resolved or reacted to.
