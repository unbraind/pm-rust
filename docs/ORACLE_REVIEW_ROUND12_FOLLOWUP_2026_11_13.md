# Round 12 follow-up: complete refusal spans and read registries

This follows accepted head `162642aa2d805dc370a786f8a1df6460cd0d3115` and addresses
six fully read review threads, including their agent prompts. The branch already
contains current main. No accepted earlier implementation was discarded.

| Thread | Decision and evidence |
| --- | --- |
| PRRT_kwDOTwgMRs629MA_ | Apply the completed-authority rule at the actual whitespace position, before a later credential `@`. A credential `@` before that whitespace retains the existing slash-password handling and registers copied passwords. The shared corpus includes the exact copied-password spelling and a quoted-message variant; the ordinary message also compares exactly with the oracle, while the oracle over-redacts quoted decoration without leaking the password; registration, stored holders, recovery arguments and both process formats are covered. |
| PRRT_kwDOTwgMRs629hCv | `runtime_settings` now discovers default schema sidecars through the same path and optional default-file reader as `Lifecycle::load`. Default arrays and definition wrappers normalize alongside object forms; built-in roles remain additive. Explicit configured-file behavior is preserved. Ranked custom-active and terminal-prerequisite candidates compare output and persistent bytes with the oracle, with a built-in terminal control. |
| PRRT_kwDOTwgMRs629hC0 | The pinned oracle emits compact list/null columns. Preserve its bytes and add the public `parse_toon` reader, restoring list cells, JSON cells, nulls, absences and column order. Maintained list/read-command consumers use it, and framed session pages are covered. Complete synthetic envelopes round-trip exactly; a real full-list differential compares bytes and restored JSON rows. |
| PRRT_kwDOTwgMRs629hCm | An option allowing hyphen values consumes `--` as its value, so subsequent real output flags remain visible. Native numeric validation now returns JSON for the reported command. Commander instead reports `Unknown option --json` for this delimiter case; the regression pins that evidence and compares an ordinary allowed limit exactly. No assertion on an existing differential changed. |
| PRRT_kwDOTwgMRs629hCo | Traversal skips non-regular entries. The final blocker read reuses the existing nonblocking, no-follow file opener, then verifies the opened handle is regular. A real FIFO follows unresolved-blocker strict/force policy; a live FIFO producer verifies final reads cannot wait for EOF while holding an item lock. |
| PRRT_kwDOTwgMRs629hCs | Preserve oracle parity. With progress `doing` aliasing `in_progress`, the oracle reports canonical in-progress items in the open bucket and reports zero context in-progress items. Removed-selector fallback has the same read classification. The differential compares full stdout/stderr/status for both list buckets and context, with canonical-progress controls. This is an evidence-backed refusal to change classification, not a deferred implementation. |

## Registry-builder inventory

- `src/list_registry.rs::Lifecycle::load` reads inline settings, built-in defaults,
  and explicit/default statuses and workflows via `optional_schema` and shared
  `schema_path`. `from_values` is the common role/alias normalizer.
- `src/ownership.rs::runtime_settings` captures runtime schema for selection,
  ownership and context policies. It previously missed unlisted default files;
  this is the fixed builder. Default sidecars now share discovery/parsing with
  `Lifecycle::load`; explicit files retain their prior strict parsing contract.
- `src/selection.rs::CorpusIndex::new` calls `Lifecycle::from_values` on those
  captured runtime settings. It now receives the default-file additions. The
  test-only `selection::role` builder intentionally takes synthetic values and
  has no filesystem discovery contract.
- `src/admission.rs::Source::load` captures explicit/default status and
  workflow sidecars with guarded relative paths before `Lifecycle::from_values`.
  It already discovers default files. Published preview scaffold defaults remain
  deliberate and previously certified.
- `src/read_session.rs::Workspace::read_session` copies explicit/default sidecars
  into its guarded snapshot before normal lifecycle reads. This is source
  capture, rather than a separate normalizer, and already discovers defaults.
- List, search, dependency packet, context, lifecycle admission and mutation
  consumers call `Lifecycle::load`; none constructs an additional registry.
  The built-in-only `list::terminal` helper is a legacy classification helper,
  not a configured registry builder.

## TOON reader inventory

`parse_toon` restores compact pm table metadata after strict TOON decoding and
accepts a single terminal session frame marker. Pagination/list and CLI read
consumers use it. Source item readers, changes, discovery, mutation refusals and
crash-review reports retain their separate ordinary encodings. See
`native-changes-validation.md` for the maintained reader contract. Brief output
projection omissions are never invented by restoration.

## Validation

The bounded normal gate run passed. Coverage and final restoration evidence follow below.
All commands use at most two build jobs and two test threads with a 20-minute
per-command limit. No output threshold or coverage exclusion is changed. Full
aggregate coverage and Windows runtime are certified by CI.


### Normal test targets

Each target below runs with `cargo test --locked -j2` and `RUST_TEST_THREADS=2`.

| Target | Passed |
| --- | ---: |
| `--lib` | 442 |
| `--bin pm-rust` | 74 |
| `--test cli_dispatch` | 32 |
| `--test admission_contract` | 12 |
| `--test claim_release_differential` | 52 |
| `--test close_differential` | 10 |
| `--test orientation_differential` | 37 |
| `--test list_differential` | 10 |
| `--test pagination_differential` | 33 |
| `--test conformance_differential` | 2 |
| `--test search_collections_differential` | 17 |
| `--test output_parity_differential` | 2 |

These targets pass 723 unique tests, including all 163 differential tests (159
at the preceding head). The extra oracle-backed admission target passes 12 tests.
Linux and `x86_64-pc-windows-gnu` cross-clippy pass with
`--locked -j2 --all-targets --all-features -- -D warnings`; formatting passes with
`cargo fmt --all -- --check`. No command reached the 20-minute cap.

The full aggregate repeats all tests under coverage and includes additional
release checks, exceeding the local command budget. CI certifies that aggregate
and Windows execution without reducing any threshold.


### Coverage, restoration and final checks

`cargo +nightly-2026-08-06 llvm-cov --locked -j2 --branch --lib --bin pm-rust --json`
passes all 442 library and 74 binary tests. A follow-up `--no-clean --lib` run of
`round12` covers nested object/array error propagation. The new reader measures
101/101 lines, 11/11 functions, 193/193 regions and 16/16 branch outcomes. This is
file-scoped evidence, not a full aggregate coverage claim.

The final Linux and Windows GNU cross-clippy and fmt reruns pass. Identity audit
passes seven tests, bringing the normal unique test count to 730. Rustdoc passes
with `RUSTDOCFLAGS='--document-private-items -D missing-docs' cargo doc --locked
-j2 --all-features --no-deps`. The linked PM `--lib round12` command passes all six
tests with run-artifact recording disabled. There is no separate self-host recipe
in the package scripts or justfile; aggregate release certification remains in CI.

Thirteen distinct regression scenarios fail when the relevant behavior is
removed and pass restored. Initial controls comprise nine literal prior-function
reversions (three privacy, two registry, two delimiter, two FIFO) and three
controls disabling only the newly added restoration function. A final copied-
password oracle control adds a tenth literal prior-function reversion; the three
restoration scenarios are repeated against the final reader implementation.
All 32 runs are verified: negatives exit 101 at executed assertions, positives
exit zero. Compilation failures do not count. Every copied source/test file is
byte-identical after restoration. The FIFO open control releases its producer
and joins its reader even on a negative, leaving no blocked background reader.

| Regression scenario | Test command after `cargo test --locked -j2` |
| --- | --- |
| privacy-unit | `--bin pm-rust round12_copied_slash_passwords` |
| privacy-message | `--test cli_dispatch round12_quoted_message_copies_are_redacted` |
| privacy-stored | `--test cli_dispatch round11_stored_holder_and_public_retry_boundaries` |
| registry-unit | `--lib round12_runtime_default_schema_discovery` |
| registry-oracle | `--test claim_release_differential round12_selection_default_status_sidecar_matches_oracle` |
| delimiter-unit | `--bin pm-rust round12_hyphen_value_delimiter` |
| delimiter-oracle | `--test orientation_differential round12_hyphen_value_json_refusal_contract` |
| fifo-traversal | `--lib round12_fifo_blockers_are_not_items` |
| fifo-open | `--lib round12_fifo_blockers_are_not_items` |
| toon-unit | `--lib round12_list_page_round_trip` |
| toon-oracle | `--test list_differential round12_compact_list_round_trip_matches_oracle` |
| toon-session | `--lib round12_session_toon_restores_list_columns` |
| privacy-oracle | `--test claim_release_differential round8_nested_password_invocation_matches_published` |

The current public npm latest is 2026.11.13; the accepted review oracle remains
pinned to 2026.11.11 as requested for this round. No assertions, version-specific
test exceptions, coverage exclusions or thresholds were weakened. No review
thread was replied to or resolved. Neither implementation nor evidence work is
deferred from this round.


After the last tracker write, the claim was released and the item remains
in_progress and unclosed. Changelog generation and checking pass. A final fetch
confirms that main is already included and the remote PR head is unchanged.
Tracker history is append-only and is committed with the implementation.
