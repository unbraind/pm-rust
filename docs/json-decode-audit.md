# Strict JSON decode audit

All 34 JSON value/typed decodes across 16 production source/example files now use
`strict_json::from_slice` or `strict_json::from_str`. The text entry point calls
the byte entry point. It builds a recursive JSON value with unique decoded
object keys, retains serde's recursion limit, requires complete consumption,
and only then calls `serde_json::from_value` for the requested type. Unknown
fields cannot bypass admission even when the final type deliberately ignores them.
Errors omit the repeated key so receipt/config data is not echoed in diagnostics.

| Source | Decode sites | Inputs |
| --- | ---: | --- |
| `src/native_adapter.rs` | 2 | Shared reply admission and transcript envelope |
| `examples/native_adapter.rs` | 2 | Stdin request and backend request |
| `src/main.rs` | 2 | CLI settings |
| `src/mutation.rs` | 10 | Settings, lock owner records, mutation journals, history records, warning and role configuration |
| `src/ownership.rs` | 2 | Session state, settings and schema objects |
| `src/read_session.rs` | 1 | Frozen settings snapshot |
| `src/orientation.rs` | 1 | Authoritative history JSONL |
| `src/orientation_context.rs` | 5 | Session state, history, configuration and checkpoints |
| `src/selection.rs` | 1 | History-derived selection receipts |
| `src/identity_probes.rs` | 1 | Provider provenance JSONL rows |
| `src/assurance.rs` | 1 | Assurance registry and nested declarations |
| `src/list_registry.rs` | 1 | External schema definitions |
| `src/pagination.rs` | 1 | Decoded opaque cursor bytes |
| `src/list_intent.rs` | 1 | Cursor classification |
| `src/read_output.rs` | 1 | Output continuation cursor |
| `src/collections.rs` | 2 | Quoted collection strings and internally generated numeric text |

Callers retain their existing invalid-JSON behavior: authoritative inputs refuse,
while optional hints may be ignored or replaced by the existing fallback. None
consumes a last-key-wins value. This change does not make optional hints authoritative.

Two other serde JSON operations are not raw input decoders. The existing
`serde_json::from_value` in orientation constructs item metadata from a value
already in memory. `RawValue::from_string` in `json_output::serialize_raw` is
an output-only serializer for numeric text and explicit lossless raw overrides,
including the existing lone-surrogate escape contract. It validates raw syntax
without decoding an object or collapsing any key; it never admits backend
frames or tracker documents. JSON value/typed input parsing exists only in
`strict_json`.

Regression evidence:

- `strict_json_duplicate_corpus_matches_live_client`: 392 frames, 196 accepted
  and 196 refused; direct validation, offline checking and live client verdicts
  match independently expected results. Envelope depths 0 and 1, payload depths
  0 through 5 (wire depths 2 through 7), literal/escaped-equal ASCII and Unicode
  keys, arrays of objects, both success output and refusal details, and late
  duplicates after 16,384 unrelated fields. Accepted replies retain their whole
  value. Refused dispatched mutations retain reconciliation guidance and never retry.
- `strict_json_transcripts_and_requests_reject_escaped_duplicates`: four malformed
  transcript/manifest/request/operation inputs reject through the SDK and CLI.
- `strict_json_preserves_values_and_rejects_invalid_documents`: primitive values,
  insertion order, full unsigned integer precision, separate-object keys, malformed
  syntax/UTF-8, trailing data, excessive nesting and typed-map admission.
- `strict_json_validates_before_typed_fields_are_ignored`: duplicate keys inside
  a forward-compatible type's ignored field still refuse.
- `strict_json_tracker_decodes_refuse_duplicates_without_git`: real non-git
  settings, history and assurance reads refuse duplicates and retain source bytes.

`node scripts/native-adapter-revert.mts` replaces only the strict byte reader with
serde's default raw decoder for three independent tests: recursive wire corpus,
ignored typed fields, and tracker input admission. All fail by assertion after
rebuilding. It restores exact source bytes in `finally`. The six earlier
behavioral revert cases also fail as required, for nine proofs total.
