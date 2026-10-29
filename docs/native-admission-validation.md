# Native item admission validation

The declared contract is documented in [README](../README.md#preview-native-item-admission).
Rust is the product runtime; Node is used only by the published SDK oracle and validation tooling.

## Oracle and local evidence

- Published oracle: repository-pinned pm CLI 2026.10.21; current registry version checked as 2026.10.28 without changing the pin.
- 132 admitted/refused fixtures (including strict ownership across every scalar field, whitespace authors, configured close targets with active/draft roles and shared schema files): all eleven built-in create types, canonical custom active/draft statuses, scalar updates, tags, missing fields, missing targets and ordered batches. Twelve additional filename-alias fixtures compare native JSON/TOON refusals with published SDK lookup and preview refusals.
- 500 seeded state-preservation sequences replay older complete records with whitespace/CRLF variants, every touched native field, foreign metadata and relationships. State is read back through the Rust SDK and all source bytes are compared.
- 500 seeded sequences compare preview verdicts with actual native SDK apply results and SDK read-back. A further 500 adversarial governance sequences cover policy presets, persisted principals, every scalar field, annotations and closure readiness; registry read failures preserve item and history state.
- 500 seeded arbitrary-byte inputs, strict must-refuse/must-allow cases, exact integer limits and producer/validator round trips exercise both boundaries.
- Every read/candidate boundary injects a competing source transition. The final-read window remains explicitly unreserved.
- Behavioral revert in a disposable non-git copy replaces only `Workspace::preview_item_mutations` with its unsupported-capability refusal: the CLI regression fails at runtime, 0 passed / 1 failed. Six independent snapshot/identity guard reverts also fail behaviorally, with the locator rechecked after preserving ownership's refusal category. The original source stays fixed. All five linked commands pass through `pm test --run --progress`: 5 integration + 20 admission library + 1 CLI + 1 recovery + 18 existing read-contract tests, no skips. Pinned pm reports test-result tracking disabled, so measured results are retained in item comments. The directory-link regression also passes after adding the Windows junction fixture.
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
| Debug command wall time in this observation | 836 ms |

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

An initial neutral-home npm aggregate probe completed 24 Rust groups with
653 passed, 1 failed and 0 ignored. It exposed an ownership refusal-category
regression: a filename alias absent from the logical ID index must retain
`ItemNotFound`. The shared mutation locator now preserves that category, and all
18 existing read-contract tests pass without changes to their assertions.
All 32 orientation cases, including the previously intermittent packet deadline,
passed in this probe; `pm-rust-local-packet-deadline` remains an open historical
follow-up. No orientation source, assertion or limit changed.

Focused checks run locally to respect the shared host's resource constraint.
Exact-head CI must certify both full npm/Bun release gates and global coverage.
CI explicitly caps Cargo build jobs and Rust test threads at two; every release
check and all four 100% coverage floors remain unchanged.

Focused nightly branch instrumentation passed 32 tests (26 library, 1 binary,
5 integration). `src/admission.rs` has 100% of 576 lines, 58 functions, 786
regions and 134 branches covered; shared filename and recovery validation helper
regions are also exercised. The full mutation unit group passes 116 tests.
This scoped result does not certify global coverage.

Standalone `just crash-check` passed typecheck, lint and all 82 conformance tests,
with zero failures, skips, cancellations or todos. History-prefix and retained
commit checks passed (20 receipts, 36 commits). `cargo audit` passed with the
existing allowed `unsafe-libyaml` maintenance warning; dependencies are unchanged.
Final exact-head CI run links and aggregate counts are maintained on PR 132.

## Snapshot and target identity re-review

An observation must retain the first bytes used to derive every view of a source.
The bounded reader now reuses that image when schema roles share a file, including
settings used as a schema. A 500-sequence alias property checks all four roles,
unique byte accounting and the next observation. Actual filesystem replacements
between reads change active roles to terminal roles; shared-schema and shared-settings
interleavings must refuse with `admission_source_changed`.

A targeted item must resolve by the same physical filename and embedded ID through
the SDK. Admission, targeted native reads, every mutation through the shared locator,
and create/update journal recovery share filename identity validation. A 500-path
property covers root and nested directories, valid released records, aliases,
duplicate IDs, update/comment refusals and unchanged source/history bytes. Legacy
list/export decoding remains available to inspect earlier malformed records.
Non-git CLI tests exercise both preview renderers with an empty PATH and verify
ordinary writes refuse aliases. Preview leaves every tracker byte unchanged;
refused ordinary writes retain only the existing lease protocol's empty rendezvous
inode. Recovery tests cover missing, before and after images, wrong embedded IDs
and a wrong-ID before image with a matching saved hash.
Ownership calls retain their established `ItemNotFound` refusal for aliases that
are absent from the logical ID index; malformed document decoding retains its
existing error category.

Six disposable-copy behavioral reverts independently remove snapshot reuse,
the shared filename validator, admission's validation call, the mutation locator's
validation call, journal-image validation, and before-image identity validation.
Each compiles and fails its adversarial test with 0 passed / 1 failed. In particular,
the snapshot revert returns stale `ok: true`, and the admission-only revert admits
an unresolvable target. The fixed source is rebuilt after the copies are removed.

## Prior-head CI coverage follow-up

The previous governance head completed CI after its earlier queue cutoff. Its
release aggregates passed their tests but found four missed regions on the CLI's
stdin/discovery refusal lines. A real-process test now exercises both errors with
an empty PATH, complete limit receipts and unchanged tracker bytes. A targeted
nightly run records both shipped and test binaries executing those regions.
Windows additionally missed the shared history reader's directory-alias refusal;
a portable Unix symlink/Windows junction fixture now checks that SDK path and
preserves the original history and SDK-read item state.

These two focused tests supplement the 45 linked tests above. Removing either
CLI refusal mapping or the history-directory guard independently compiles and
fails its new regression (0 passed / 1 failed). Fixed-source retries pass.

The prior Windows latency job measured context p95 at 291.0006 ms against the
unchanged 250 ms ceiling; list p95 was 146.3533 ms against 150 ms. Its cause is
unproven and the observation is retained on existing performance item
`pm-rust-wdj4`. Fresh exact-head CI must certify coverage and timing; no limit or
existing assertion was relaxed.

## CI repair after head 7ca5196

All three failed aggregates in run 40889275383 completed their Rust tests and
stopped at the unchanged coverage gate. They missed the shipped CLI's propagation
of a preview failure after successful workspace discovery, plus the ownership
lineage helper's missing-target refusal. The real-process admission test now
includes an existing tracker with malformed settings and verifies the typed
receipt, complete limits, empty stdout and unchanged bytes. The lineage test
directly checks both absent and present targets: shared filename validation now
rejects missing identities before public ownership calls reach that helper.
Focused stable and nightly runs pass both tests and execute the missed regions.
Two behavioral mutations in disposable non-git copies remove the missing-target
guard and replace typed preview-error propagation with an untyped error. Each
compiles and fails its targeted regression (0 passed / 1 failed); the package
sources remain fixed and are rebuilt afterward.

Windows Rust exceeded its 20-minute job budget during passing differential tests;
the Windows aggregate exceeded 35 minutes during its second, instrumented pass.
Only those Windows job budgets increase, to 35 and 60 minutes. All tests, latency
ceilings and four coverage floors remain unchanged. The macOS latency job never
acquired a hosted runner; its annotations report capacity constraints, and it
contains no executed steps or available log blob. Fresh CI must establish that
job's result. There are no public review threads or CodeRabbit review bodies on
this PR; the CodeRabbit issue comment reports skipped automatic review.
