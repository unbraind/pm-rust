# Completed-authority decision and final remediation

This continues local checkpoint `5cd0cc32b04d123c9baef09ca5363ad37ce32c7c`. P1 (PRRT_kwDOTwgMRs626583) was already implemented there; no completed work was discarded. Its invocation consumers and five successful revert/restore pairs are recorded in `ORACLE_REVIEW_ROUND11_2026_11_13.md`.

The owner resolved P2 (PRRT_kwDOTwgMRs62658y): whitespace after a completed dotted hostname, IPv4 address, bracketed IPv6 address or localhost ends the URL, including valid ports. Otherwise unresolved text through `@` is private, including whitespace. Ambiguity resolves to redaction.

`src/refusal.rs` now applies that structural decision in `userinfos`, which is shared by credential registration, token projection and every refusal stream. The prose-word exception is removed. Subsequent credential URLs are still scanned, and existing nested/password protections remain covered. Empty or malformed authorities remain private. The classifier recognizes hostname label boundaries, ports, IP literals and the explicit localhost case; it contains no list of prose words.

The shared `tests/support/refusal_corpus.rs` exercises bare strings plus Gate/Contact decorations in registration, direct redaction, stored `claimed_by` displays and close retry advice. Both JSON and human process formats are tested. Required secret cases include the `stored and private-token`, stored-title, intranet-label and classic username/password forms. Required public cases include Contact/or/email, IPv4 with port, bracketed IPv6 and public URL/email combinations. Invalid labels, brackets and ports test the conservative branch. The corpus also includes dotted whitespace names reclassified by the owner's explicit decision. Earlier unqualified public prose controls use localhost to satisfy that decision; no differential assertion was changed.

P1 retains the actual bootstrap scope in attempted commands and close retries, preserves delimited reason text, and uses the shared parsed view for scope, output and authorship controls. Its exact oracle differential and execution against overlapping tracker IDs passed in the preceding checkpoint. Both fixes are to be pushed together; neither finding is deferred.

The bounded gate run passed. Every cargo command uses at most two workers and two test threads and has a 1,200-second cap. The aggregate and Windows runtime remain for CI, with unchanged thresholds.


## Targeted gates

Each test command below is `cargo test --locked -j2` followed by the listed target. `RUST_TEST_THREADS=2` applies throughout.

| Target | Passed |
| --- | ---: |
| `--bin pm-rust` | 72 |
| `--test cli_dispatch` | 31 |
| `--test admission_contract` | 12 |
| `--test claim_release_differential` | 51 |
| `--test close_differential` | 10 |
| `--test orientation_differential` | 36 |
| `--test list_differential` | 8 |
| `--test pagination_differential` | 33 |
| `--test conformance_differential` | 2 |
| `--test search_collections_differential` | 17 |
| `--test output_parity_differential` | 2 |

This is 274 unique tests, including all 159 differential tests and the additional 12 oracle-backed admission tests. Linux and `x86_64-pc-windows-gnu` clippy pass with `--locked -j2 --all-targets --all-features -- -D warnings`; `cargo fmt --all -- --check` passes. No command reached its 20-minute limit.

The normal differential targets alone took more than 35 minutes in this run; repeating the aggregate under coverage would exceed the local full-gate budget. CI certifies full aggregate coverage and Windows execution at unchanged thresholds. The branch contains the current main; no merge was needed.


## Revert proof and final verification

Four P2 regression commands were run in an isolated source copy with only `userinfos` replaced by its checkpoint implementation, then repeated after restoring the fixed function:

- `cargo test --locked -j2 --bin pm-rust round11_authority_registration_and_projection_corpus`
- `cargo test --locked -j2 --bin pm-rust round11_public_prose_preserves_retry`
- `cargo test --locked -j2 --test cli_dispatch round11_stored_holder_and_public_retry_boundaries`
- `cargo test --locked -j2 --test cli_dispatch round11_public_prose_process_keeps_retry`

All four negatives fail executed assertions with exit 101; all four restorations pass. The process corpus accumulates mismatches across the cases and both formats, so one early refusal does not prevent the other variants from executing. Every copied source/test file is byte-identical to the reviewed implementation after restoration. With P1's five documented pairs, there are nine distinct revert/restore pairs and 18 verified runs across the two fixes.

A final combined binary/CLI run passes 72 and 31 tests on the complete corpus. The pinned PM test runner also passes the public-retry regression with run-artifact recording disabled. Changelog generation/check follow the final tracker writes.


After the final PM results comment and claim release, `npm run changelog:full` and `npm run changelog:check` pass. The item remains in_progress and unclosed. History remains a byte-prefix extension of the checkpoint. All commits ahead of main use the required SteveBot email.
