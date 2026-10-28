# Portable native adapter ABI v1

This is a serialized byte ABI, rather than a compiler-specific Rust object
layout or unsafe C pointer interface. `NativeAdapter` is the in-process Rust
integration point; JSON bytes are the versioned compatibility boundary for
native implementations in other languages. No executable, library or network
endpoint is auto-discovered. The embedding application chooses its adapter and
owns transport timeouts, cancellation, connection lifetime and authorization.

## Scope and overlap

Core owns item schema, storage contracts, validation, lifecycle, lock-scoped
mutations and receipts. pm-web and MCP own their transports and authorization.
The current published SDK provides `PmClient.run` and typed operation methods;
it does not publish a generic native storage plugin interface. This package
adds Rust interoperability over those authorized operations. It adds no backend,
HTTP client, authentication system or replacement pm-web client. Existing SDK
and Rust compatibility pins remain unchanged.

## Wire frames

All frames are single UTF-8 JSON objects. The exact ABI string is
`pm-native-adapter/1`; any different revision refuses. Unknown envelope and
operation fields refuse; backend output/details remain opaque JSON and retain
extension fields. Duplicate object keys refuse at every nesting depth, including
inside opaque output/details and objects in arrays. Keys are compared after JSON
unescaping, so `"a"` and `"\u0061"` conflict. Trailing non-whitespace and invalid
JSON also refuse. Frame lengths count encoded bytes, with a hard ceiling of 1,048,576.
The adapter may declare a smaller positive ceiling. Transport implementations
must stop reading once that ceiling is exceeded, before allocating an unbounded
response. The offline CLI envelope has a separate 16 MiB ceiling plus one
detection byte, allowing decimal byte-array expansion without reducing the 1 MiB frame ceiling.

The shared frame validator uses `strict_json::from_slice`: it reads one complete
recursive value, rejects duplicates before insertion, checks end-of-input, then
deserializes typed structs from that validated value. Typed field filtering
cannot hide ambiguous nested data. The transcript, example request decoder and
all tracker JSON readers use the same admission reader; see the
[decode audit](json-decode-audit.md). Size admission still precedes JSON parsing.

Manifest fields are `abi`, `capabilities`, and `max_frame_bytes`. Capability
values are `get`, `list`, `claim`, and `release`; absent capabilities refuse
before `exchange` is called. A manifest is immutable for a client instance.
Backend permission revocation must still be enforced at every operation.

A request has `abi`, `request_id`, and `operation`. Correlation IDs, item IDs and
asserted authors must be nonblank and at most 256 encoded bytes. Correlation is
not idempotency: request IDs never authorize retrying a write. Operations are:

| Action | Fields | Authoritative behavior |
| --- | --- | --- |
| `get` | `id` | Canonical SDK item read |
| `list` | `limit`, optional `after`, `output_cursor` | Canonical page, 1–100 requested rows; preserve continuation and omissions |
| `claim` | `id`, `author` | Atomic SDK claim, skipping foreign ownership with its reason |
| `release` | `id`, `author` | Atomic owner-checked SDK release; refuse or skip foreign ownership with its reason |

The backend binds tracker and authenticated principal itself. Request fields
cannot select paths, credentials, runtimes, force overrides or bypasses. An
asserted `author` is attribution, never authentication. Advertise claim/release
only when the backend can verify the authenticated caller and enforce these
ownership rules inside its authoritative lease or compare-and-set. For the
native Workspace SDK this requires strict ownership enforcement for release;
permissive release policies do not meet this capability contract. Decline that
capability rather than emulate it with a read followed by a write.

A response has `abi`, the matching `request_id`, and `outcome`:

```json
{"abi":"pm-native-adapter/1","request_id":"read-1","outcome":{"status":"success","output":{"items":[],"omission_receipt":{"has_omissions":false}}}}
```

`success.output` is the entire canonical backend envelope. `refused` instead
carries `code`, `message`, `remedy`, and `details`, retaining original structured
backend context and partial-operation receipts. `NativeAdapterClient.call`
returns backend refusal as an `Ok(AdapterReply)`, so a foreign record does not
become an ABI exception. Neither success nor ABI validity proves a write changed
anything, a claim belongs to the caller, or a read is complete. The embedding
application must interpret canonical receipts, including peer-success and skip
semantics. Complete reads require the core complete-list certificate, not a
locally invented completeness flag.

## Local refusals and uncertain mutations

Local refusal codes are `incompatible_abi`, `unsupported_capability`,
`invalid_frame`, `frame_limit`, and `outcome_unknown`. Every refusal carries a
message, remedy and `dispatched` indicator. Unsupported capability or malformed
request refuses before exchange, with `dispatched: false`. Once exchange starts,
a transport error, oversized response, incompatible reply or correlation failure
has `dispatched: true`; a mutation may already be committed. There are no retries,
fallback writes or client-side compensation. Read authoritative receipts before
deciding how to recover.

`adapter-check` validates an offline transcript containing `manifest`, `request`,
and optional `reply_bytes`, an array of integers from 0 through 255 containing
the exact recorded frame. Omit it or use null when no reply was recorded. The
previous decoded `reply` field is rejected: re-encoding a JSON object loses wire
size, whitespace, duplicate fields and encoding errors. Both offline checking
and live calls pass the original bytes to `check_adapter_reply`, which checks
size before UTF-8/JSON decoding, schema, ABI and correlation. Padding, escapes,
trailing data and invalid UTF-8 are never normalized before admission. Its small
compatibility receipt intentionally excludes backend bodies, secrets and large histories. A recorded backend refusal is valid
ABI data and exits successfully with `reply_status: refused`; malformed ABI input
exits nonzero with a typed local refusal. The CLI's `dispatched: false` describes
this offline check, not whether a recorded operation previously ran.

## Concurrency and evidence

The client owns no tracker files, locks, claims, leases, checkpoint, budget,
outbox or promotion state. Adapter methods receive `&mut self` and each call
performs exactly one exchange. Backend operations remain the sole shared-state
transitions. Adapters must read what they modify inside their authoritative
lease, release last, and delete only state belonging to the authenticated caller.
Foreign records are skipped or refused as canonical results with a reason.

`tests/native_adapter_contract.rs` runs synthetic protocol peers and a real SDK
adapter on one non-git tracker initialized by the pinned CLI. It injects a
competing claim or release-and-claim at all six observable adapter boundaries:
before/after manifest, before/after request decode, after authoritative operation,
and after reply encoding. Both mutation paths preserve peer ownership, no-op
claim receipts and historical success. The wrapper has no read-modify-write gap
between those boundaries. Malformed, oversized and disconnected peers exercise
unknown outcomes and prove no automatic retry.

Run targeted checks with:

```sh
cargo test --locked --test native_adapter_contract
cargo test --locked --bin pm-rust native_adapter
node scripts/native-adapter-revert.mts
cargo check --locked --all-targets
cargo clippy --locked --all-targets --all-features -- -D warnings
```

For public-data validation, select only a verified public tracked snapshot,
then run `node scripts/native-adapter-validate.mts --source <tracker-copy>
--example <native-example> --binary <native-cli>`. The tool creates its own
non-git copy, certifies all item identities with the pinned SDK, makes tracker
files read-only, exchanges all list pages and item reads through the byte ABI,
validates every transcript in both CLI formats, and checks source hashes.
Native children run with a PATH containing neither Git nor Node. Only aggregate
counts, sizes, duration and hashes are printed; source contents stay in scratch.

The npm and Bun CI release gates retain all existing checks and 100 percent
coverage thresholds. Transport-specific remote deployment acceptance remains
with the integrating backend; this feature's remote evidence uses synthetic
authorized peers, not hosted services or private data.

## Incremental change capability

The additive `changes` capability admits `AdapterOperation::Changes` with the
shared `ChangeRequest` schema. Old adapters remain compatible and refuse that
capability before exchange. `ChangeReader` uses one exchange per pull and retains
backend refusal details, with no checkpoint advancement on refusal. See
[native changes](native-changes.md) for per-stream resume, stale cursor, bounded
output and cancellation obligations.
