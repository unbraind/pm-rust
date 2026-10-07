# List output parity against the locked 2026.10.7 oracle

Implemented owner items:
[pm-rust-8hkb](https://github.com/unbraind/pm-rust/blob/main/.agents/pm/issues/pm-rust-8hkb.toon)
and [pm-rust-ba8f](https://github.com/unbraind/pm-rust/blob/main/.agents/pm/issues/pm-rust-ba8f.toon).
Implementation and local evidence are complete. The items remain open for
orchestrator verification.

## UTF-16 compaction and SDK serialization

The published CLI takes the first 240 UTF-16 code units of a long string and
appends an ellipsis. If unit 240 is a high surrogate, JSON preserves that unit
as an ASCII escape, such as `\ud83d…`. Replacing it with U+FFFD changes both
output bytes and measured token receipts.

`Workspace::list_page` now returns `ListOutput`. The envelope carries the
escaped string at its JSON pointer separately from its immutable, Unicode-safe
`serde_json::Value` view. The view uses U+FFFD for an unpaired unit. Serialize
the complete envelope with `stringify_json`, `write_pretty_json`, or serde's
JSON serializer to preserve the original code unit. Serializing a borrowed
subtree of the safe view cannot preserve it. Ordinary Rust strings and emitted
UTF-8 bytes never contain a literal surrogate.

Pretty and compact rendering share the JavaScript number formatter and lossless
string representation. Canonical budget receipts, triage estimates and row
size decisions therefore measure the emitted bytes. Snapshot hashes also retain
these escapes, and output continuations move their pointers to the retained
rows. Literal replacement characters and literal backslash-u strings stay
unchanged; no sentinel string or user metadata key is reserved.

## Stored extra metadata order

`ItemMetadata.extra` now uses insertion-ordered `serde_json::Map`. Full list
projections retain canonical positions for known fields and append unknown
fields in stored `.toon` order, including nested object order. The public
`canonical_metadata_pairs` history contract still sorts unknown keys, so
history hashing, patches and canonical write-back retain their existing order.
SDK callers constructing metadata should use `serde_json::Map` for extras.

## Generated differential evidence

`tests/output_parity_differential.rs` runs both real executables over a synthetic
tracker with a fixed clock and the committed lockfile oracle. It checks the
oracle package version is exactly 2026.10.7. Every comparison checks complete
stdout, stderr and exit code without removing or normalizing fields, then
checks SDK pretty/compact serialization against the same native bytes.

- Four astral characters (`😀`, `𐐷`, `𝄞`, `𠀀`) start at each UTF-16 offset
  from 0 through 241: 968 generated placements in titles, descriptions, extra
  scalars and nested arrays. Plain and triage reads preserve all 242 rows and
  verify escaped cuts and token estimates. Nested keys contain `/` and `~` to
  check JSON-pointer escaping.
- A binding canonical budget after triage compaction produces an output
  continuation. Replay compares both executables again, including a tail page
  starting exactly at the split surrogate, checking the escaped snapshot
  fingerprint and shifted row pointers. A two-row amount cap also
  compares complete bytes.
- All 24 permutations of four extra keys are tested under unbounded and
  nonbinding finite budgets, with nested nonalphabetical keys and numeric
  metadata.

```bash
just oracle-install
PM_NODE_CLI=tests/oracle/node_modules/@unbrained/pm-cli PM_RUST_REQUIRE_PUBLISHED_CLI=1 cargo +1.90.0 test --locked --test output_parity_differential -- --nocapture
```

## Independent native-only revert proofs

The native baseline is `3b21777`. For each proof, the test, published package
and lockfile stay unchanged. Only
the indicated native behavior is reverted; the other fix stays installed.

1. Remove the split-high-surrogate escape capture in `compact_strings` while
   keeping its previous U+FFFD safe string truncation. The generated UTF-16
   test fails with exit 101: native emits `�…`, while the oracle emits
   `\ud83d…`. Restore the capture before the next proof.
2. Restore original `BTreeMap` decoding/construction, the original history
   implementation, and the full list projection to `canonical_metadata_pairs`.
   The generated key-permutation test fails with exit 101:
   native emits alphabetical extras, while the oracle retains file order.
   Restore all native files byte-for-byte before running the aggregate gate.

```bash
PM_NODE_CLI=tests/oracle/node_modules/@unbrained/pm-cli PM_RUST_REQUIRE_PUBLISHED_CLI=1 cargo +1.90.0 test --locked --test output_parity_differential generated_utf16_cut_offsets_match_locked_oracle -- --exact --nocapture
PM_NODE_CLI=tests/oracle/node_modules/@unbrained/pm-cli PM_RUST_REQUIRE_PUBLISHED_CLI=1 cargo +1.90.0 test --locked --test output_parity_differential generated_extra_key_permutations_match_locked_oracle -- --exact --nocapture
```

The combined differential suite passes again after both fixes are restored.
Fault-injection tests also cover failing collection headers and entries,
malformed internal pointers and invalid raw scalar data. Pretty output shares
one writer type across sinks, so the same serializer error paths serve the
CLI, SDK buffers and failing sinks.

## Release receipt

The required aggregate command is `just release-check`: locked oracle install,
formatting, deny-warning Clippy, public/private rustdoc, ordinary and
instrumented tests, exact 100 percent lines/functions/regions/branches,
dependency audit and pinned changelog verification. `just changelog-full`
regenerates after tracker writes. Neither item is closed by this change.
This receipt covers local Linux execution; cross-platform CI and orchestrator
verification remain external evidence.

`just release-check` passed on Linux with 211 tests in each ordinary and
instrumented run. Measured native source coverage:

| Metric | Covered / total | Percent |
| --- | --- | --- |
| Lines | 3184 / 3184 | 100 |
| Functions | 290 / 290 | 100 |
| Regions | 4775 / 4775 | 100 |
| Branches | 626 / 626 | 100 |

Both PM-linked regression commands passed via `pm test --run --progress`.
Strict PM health and validation returned success with existing tracker
advisories; these are separate from source coverage and parity evidence.
