# Native item admission validation

The declared contract is documented in [README](../README.md#preview-native-item-admission).
Rust is the product runtime; Node is used only by the published SDK oracle and validation tooling.

## Oracle and local evidence

- Published oracle: repository-pinned pm CLI 2026.10.21; current registry version checked as 2026.10.28 without changing the pin.
- 49 admitted/refused fixtures: all eleven built-in create types, canonical custom active/draft statuses, scalar updates, tags, missing fields, missing targets and ordered batches.
- 500 seeded state-preservation sequences replay older complete records with whitespace/CRLF variants, every touched native field, foreign metadata and relationships. State is read back through the Rust SDK and all source bytes are compared.
- 500 seeded sequences compare preview verdicts with actual native SDK apply results and SDK read-back.
- 500 seeded arbitrary-byte inputs, strict must-refuse/must-allow cases, exact integer limits and producer/validator round trips exercise both boundaries.
- Every read/candidate boundary injects a competing source transition. The final-read window remains explicitly unreserved.
- Behavioral revert in a disposable non-git copy replaces only `Workspace::preview_item_mutations` with its unsupported-capability refusal: the CLI regression fails at runtime, 0 passed / 1 failed. The original source stays fixed. Afterwards, all three linked commands pass through `pm test --run --progress`: 3 integration + 14 library + 1 CLI test, no skips. The directory-link regression also passes after adding the Windows junction fixture.
- `cargo +1.90.0 clippy --locked --all-targets --all-features -j 2 -- -D warnings` and format checks pass. Full unchanged npm/Bun release gates and final-head coverage are recorded on the PR.

## Public data in a non-git directory

Command: `node scripts/admission-real.mjs` after `cargo +1.90.0 build --locked -j 2 --bin pm-rust`.
Corpus: public `unbraind/pm-rust` tracker at `1af599038dcbbb59b5d15092b30d03c30b52f86f`.
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
| Debug command wall time in this observation | 859 ms |

All frozen bytes and all SDK-read item state remain unchanged. The missing-target row fails alone. This measures bounded admission and preservation, not a later commit guarantee.

## Deliberate limits

The native contract requires explicit scalar replacements; core's metadata-only empty updates are outside it. Custom field/type schemas and complex lifecycle changes remain typed refusals tracked by `pm-rust-admission-schema`. Existing compensation-marked targets require explicit history review. There are no import/export/sync or journal/retry paths in this feature. Preview returns diagnostics rather than an executable saved plan; no idempotency key or lock-based isolation is claimed.
