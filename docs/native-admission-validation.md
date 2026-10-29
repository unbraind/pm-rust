# Native item admission validation

The declared contract is documented in [README](../README.md#preview-native-item-admission).
Rust is the product runtime; Node is used only by the published SDK oracle and validation tooling.

## Oracle and local evidence

- Published oracle: repository-pinned pm CLI 2026.10.21; current registry version checked as 2026.10.28 without changing the pin.
- 130 admitted/refused fixtures (including strict ownership across every scalar field, whitespace authors, and configured close targets with active/draft roles): all eleven built-in create types, canonical custom active/draft statuses, scalar updates, tags, missing fields, missing targets and ordered batches.
- 500 seeded state-preservation sequences replay older complete records with whitespace/CRLF variants, every touched native field, foreign metadata and relationships. State is read back through the Rust SDK and all source bytes are compared.
- 500 seeded sequences compare preview verdicts with actual native SDK apply results and SDK read-back. A further 500 adversarial governance sequences cover policy presets, persisted principals, every scalar field, annotations and closure readiness; registry read failures preserve item and history state.
- 500 seeded arbitrary-byte inputs, strict must-refuse/must-allow cases, exact integer limits and producer/validator round trips exercise both boundaries.
- Every read/candidate boundary injects a competing source transition. The final-read window remains explicitly unreserved.
- Behavioral revert in a disposable non-git copy replaces only `Workspace::preview_item_mutations` with its unsupported-capability refusal: the CLI regression fails at runtime, 0 passed / 1 failed. The original source stays fixed. Afterwards, all three linked commands pass through `pm test --run --progress`: 4 integration + 16 library + 1 CLI test, no skips. The directory-link regression also passes after adding the Windows junction fixture.
- `cargo +1.90.0 clippy --locked --all-targets --all-features -j 2 -- -D warnings` and format checks pass. The CLI invalid-tracker propagation regression subsequently closes the single missing region found by the first macOS aggregate; a focused nightly run records that region executed. Full unchanged npm/Bun release gates and final-head coverage are recorded on the PR.

## Public data in a non-git directory

Command: `node scripts/admission-real.mjs` after `cargo +1.90.0 build --locked -j 2 --bin pm-rust`.
Corpus revalidated after review: public `unbraind/pm-rust` tracker at `91ac8a985f4f32060b30ec38fd1c61d1b12ccc0f`.
The SDK's `listAllComplete({includeBody:true})` certifies the whole corpus before and after native execution. SDK reads prepare 8 disposable runtime cache/usage files; every archived source file remains byte-identical. The complete prepared copy is then made read-only. Native execution has an empty PATH and a Git ceiling outside the tracker. No Git repository, Node runtime, or Git executable is available on that native command path.

| Measurement | Observed |
| --- | ---: |
| Certified tracker items / native source rows | 89 / 89 |
| Archived public files | 184 |
| Archived public bytes | 4,698,201 |
| Frozen files including SDK preparation | 192 |
| Source bytes per native observation | 909,472 |
| Directory entries per observation | 99 |
| Intent rows | 4 |
| Admitted / refused | 3 / 1 |
| Input JSON bytes | 427 |
| Output JSON bytes including newline | 1,090 |
| Debug command wall time in this observation | 1,944 ms |

All frozen bytes and all SDK-read item state remain unchanged. The missing-target row fails alone. This measures bounded admission and preservation, not a later commit guarantee.

## Deliberate limits

The native contract requires explicit scalar replacements; core's metadata-only empty updates are outside it. Custom field/type schemas and complex lifecycle changes remain typed refusals tracked by `pm-rust-admission-schema`. Existing compensation-marked targets require explicit history review. There are no import/export/sync or journal/retry paths in this feature. Preview returns diagnostics rather than an executable saved plan; no idempotency key or lock-based isolation is claimed.

## Governance re-review

Preview and ordinary native update/comment writes share strict ownership validation.
Configured close identities retain their enabled governance reason requirement independently of roles;
create and update preparation share that guard. Captured previews and native reads
share the published default-registry fallback
when optional schema files are absent; embedded overrides alone do not replace
that effective workflow. The missing-file case is included in the SDK oracle.

Each guard was independently replaced with success in a disposable non-git source
copy. The adversarial property failed behaviorally in each case: 0 passed / 1
failed, respectively admitting a foreign update and admitting a reasonless close.
The original source stayed fixed and was rebuilt for the focused and full gates.

The pinned published transaction preview admits active-role configured closure
updates. Those complex transitions remain outside the native scalar parity slice;
24 configured targets are instead checked for native JSON/TOON refusals. The
follow-up `pm-rust-admission-schema` retains this SDK discrepancy. Create closure
refusals and supported scalar ownership cases do agree with the pinned oracle.

Local aggregate boundary: both neutral-home npm and Bun `release:check` runs
passed 576 Rust tests and failed one unchanged five-second packet child deadline,
with zero ignored tests across 18 completed groups in each run. A focused one-thread retry reproduced the same
missing-exit-code signature. This pre-existing local condition is tracked by
`pm-rust-local-packet-deadline`; no orientation source, assertion or limit changed.
Current exact-head CI must certify the full release and coverage gates.

Focused nightly branch instrumentation passed 26 tests (21 library, 1 binary,
4 integration). `src/admission.rs` has 100% of 571 lines, 57 functions, 775
regions and 132 branches covered; the new governance guard source regions are
also exercised. This scoped result does not certify global coverage.

Standalone `just crash-check` passed typecheck, lint and all 82 conformance tests,
with zero failures, skips, cancellations or todos. History-prefix and retained
commit checks passed (20 receipts, 36 commits). `cargo audit` passed with the
existing allowed `unsafe-libyaml` maintenance warning; dependencies are unchanged.
Final exact-head CI run links and aggregate counts are maintained on PR 132.
