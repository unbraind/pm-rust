# Native adapter ABI validation — 2026-10-24

Source: the tracked public pm-rust tracker at
`ec39af2b06d3884c466b5770b41e1cdb745f1cd6` (origin/main), exported with
`git archive` and copied into an exclusively owned non-git scratch directory.
No hosted data or private tracker was accessed. The pinned published SDK
2026.10.21 certified the complete source list; the current published CLI version
was independently resolved as 2026.10.24. Existing pins were retained.

The validation program made source files and directories read-only for native
reads, compared the SDK's complete identity set and each page's rows/omission
receipt, compared identity/title/status on every get, and checked authoritative
source hashes before and after. All native children had a PATH containing
neither Git nor Node and a Git discovery ceiling above the scratch project.

| Measurement | Result |
| --- | ---: |
| Certified source items | 83 |
| Authoritative files | 172 |
| Authoritative bytes | 4,234,972 |
| List pages, requested 10 rows/page | 9 |
| Adapter calls: list + every item get | 92 |
| CLI transcript checks: JSON + TOON | 184 |
| Total adapter response bytes | 880,680 |
| Largest response frame | 88,935 bytes |
| Largest CLI compatibility receipt | 193 bytes |
| Validation elapsed time | 4,117 ms |
| Source changed | No |

Unchanged authoritative source SHA-256:
`4efdd31a9fd67caefced2b9852a38c6709f4452e2ecb519433be00bbd61d3607`.
Timing is one debug-build validation sample on the development host, not a
latency SLO or a remote transport benchmark. No network backend deployment is
claimed: remote adapter protocol/refusal behavior uses synthetic authorized peers.

Reproduction after building the native example and CLI:

```sh
node scripts/native-adapter-validate.mts --source <public-tracker-copy> \
  --example <native-adapter-example> --binary <native-cli>
```

The targeted ABI suite passes nine tests. Its shared-tracker test executes
12 deterministic interleavings: both ownership mutation paths at all six adapter
boundaries, plus a foreign-owner release guard. It delegates mutations to the
real native SDK on a tracker initialized by the pinned CLI. The eight other
protocol tests exercise outcomes, admission, ABI versions, field/frame limits,
uncertain outcomes/no retries, offline transcript validation and the CLI without
Git, Node or tracker prerequisites. Two additional CLI unit tests cover all
19 missing-tracker dispatch paths, successful and refused full command dispatch
with caller-owned streams in JSON and TOON, plus injected stdin, broken-pipe,
trailing newline, flush and TOON codec failures. The complete CLI unit target
passes 37 tests, and black-box CLI dispatch passes 17 tests. A separate CLI
process matrix checks all 19 missing-tracker commands without Git or Node;
instrumentation confirms every discovery error propagation point executes in
both process and unit-test dispatcher instances. Constructor/preflight refusal tests also
prove an incompatible manifest never reaches the backend.

`node scripts/native-adapter-revert.mts` independently replaces only the relevant
behavior for each proof, requires a real failed test assertion, and restores the
exact source bytes in a finally block:

| Reverted behavior | Regression required to fail | Result |
| --- | --- | --- |
| Pre-dispatch capability admission | `admission_refuses_before_transport_or_writes` | Failed as required |
| Exact ABI compatibility | `versions_fields_and_limits_are_fail_closed` | Failed as required |
| Unmodified canonical backend outcome | `canonical_outcomes_survive_the_byte_boundary` | Failed as required |
| Tracker-free CLI entry point | `cli_validates_without_tracker_git_node_or_backend_execution` | Failed as required |
| CLI flush-error propagation | `tests::native_adapter_stdio_refusals` | Failed as required |

The targeted instrumented run covers the ABI module at 100 percent in every
required dimension: 146 lines, 17 functions, 211 regions and 20 branches, all
covered. New CLI helper regions and missing-tracker propagation paths were
checked separately; this scoped run does not claim full-repository coverage.

Local validation also includes `cargo check --locked --all-targets`,
`cargo clippy --locked --all-targets --all-features -- -D warnings`, formatting,
and the existing strict TypeScript and ESLint harness checks. Full npm and Bun
release evidence is provided by the PR's CI run at unchanged thresholds.

## Raw-byte review regression

The offline transcript now records `reply_bytes` as a byte array, rather than a
decoded `reply` object. Offline checks and live client calls invoke the same
`check_adapter_reply` function on the exact recorded bytes. Size admission
precedes UTF-8/JSON parsing, schema, ABI and correlation checks. The CLI's
separate bounded 16 MiB transcript envelope accommodates decimal byte-array
expansion without reducing the 1 MiB response-frame ceiling. The public-data
validation harness supplies the actual response stdout bytes.

`raw_frame_seeded_corpus_matches_live_client` generates **768 frames**, seed
**0x05eed113**, using a fixed wrapping LCG. Every frame goes through the offline
checker and a `NativeAdapterClient` transport exchange; both verdict and refusal
code must agree, and each verdict is also checked against its expected result.
There are **240 accepted and 528 refused** frames. Categories include 4,096-byte
limit minus one/exact/plus one, the 5,094-byte whitespace-padded reproduction,
escapes, Unicode, invalid UTF-8, truncation, oversized nested values, trailing
data, ABI mismatch, correlation mismatch, duplicate fields, unknown fields,
excess recursion and backend refusals. Live failures retain `dispatched: true`;
offline compatibility receipts remain `dispatched: false`.

`cli_transcript_preserves_full_frame_limit_and_raw_errors` separately checks
that an exact 1 MiB recorded reply fits the CLI envelope, that an oversized
reply returns `FrameLimit`, invalid UTF-8 returns `InvalidFrame`, and the decoded
legacy `reply` field refuses rather than silently certifying normalized data.

The sixth case in `node scripts/native-adapter-revert.mts` (listed first)
restores only offline decode/re-encode behavior, retaining the new transcript
format and every live check. The differential test fails by assertion on frame
2: 4,097 recorded bytes with a 4,096-byte limit, offline accepted versus live
`FrameLimit`. The script restores exact source bytes in `finally`. All **6/6**
behavioral revert cases fail as required; after restoration the ABI suite passes
**11/11** and targeted native CLI unit checks pass **2/2**. Deny-warning clippy,
formatting, strict TypeScript and ESLint pass locally. Full release-gate evidence
for this revision belongs to its CI run, rather than the earlier run above.

## Recursive strict JSON review regression

The shared frame validator now uses the same strict JSON reader as all other
production raw decoders: 34 sites across 16 source/example files. It rejects
duplicate decoded keys recursively before constructing typed structs from the
validated value. The [decode audit](../json-decode-audit.md) lists each site and
documents optional-input fallback behavior and the output-only scalar serializer.

`strict_json_duplicate_corpus_matches_live_client` adds **392 frames** with
**196 accepted and 196 refused**, checking direct admission, offline admission
and live client results against independent expectations. It covers wire depths
0 through 7 (including payload depths 0 through 5), escaped-equal ASCII and
Unicode keys, arrays of objects, both output and refusal details, and 16,384-field
objects with late duplicates. Accepted values retain the complete backend
envelope. Dispatched failures retain uncertainty and never retry. The earlier
768 seeded frames remain unchanged, for **1,160 differential frames total**
(436 accepted, 724 refused).

`strict_json_transcripts_and_requests_reject_escaped_duplicates` also checks four
raw transcript/manifest/request/operation duplicates through SDK and CLI entry
points. The three tests in `strict_json_contract` cover primitive preservation,
invalid syntax/UTF-8/trailing data/recursion, typed-field filtering and typed maps,
and real non-git settings/history/registry reads that retain source bytes on refusal.

The revert script now has **9 independent proofs**. Its three new cases replace
only the shared reader with serde's default decoder, retain every routed call
site and test, rebuild and require assertion failures. The recursive corpus,
ignored-field and tracker-input regressions each fail as required; exact source
bytes are restored in `finally`. Revert evidence is recorded in the PM item.

The targeted strict-reader instrumented run passes six selected tests and covers
`src/strict_json.rs` at **100% lines/functions/regions/branches**: **50 lines,
12 functions, 79 regions, 6 branches**. This scoped result does not claim
whole-repository coverage; unchanged full-release thresholds run in CI.
