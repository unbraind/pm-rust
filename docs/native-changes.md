# Native incremental change readers

`pm-rust changes` pulls one page from retained canonical mutation history. It
uses the public SDK's compact `MutationEvent` fields (`item_id`, `version`, `ts`,
`author`, `type`, optional `message`, `patch_count`). History remains authoritative;
this transport neither ranks context nor interprets statuses or relationships.
It does not import, export or sync item state and never changes a tracker.

```sh
pm-rust --workspace project changes --limit 20 --output-bytes 65536 --json
pm-rust --workspace project changes --cursor "$cursor" --json
# Default encoding is TOON; JSON is one compact page plus a newline.
pm-rust --workspace project changes --cursor "$cursor"
```

Consume **the whole page**, including `skipped`, before persisting `next_cursor`.
Repeated empty pulls retain the same cursor. Delivery is exactly once relative to
the last admitted cursor: two independent consumers intentionally receive their
own copies. A crash after external processing but before saving the cursor can
replay that page; applications must make their external side effects idempotent.
There is no shared checkpoint file, ownership lease, automatic retry, outbox,
background worker, prefetch or remote write.

The local adapter tracks a byte boundary, ordinal and SHA-256 of the admitted
prefix of each stream. It validates every retained prefix before returning any
page. Equal timestamps, backdated appends, new streams and concurrent appends
remain eligible. Ordering is lexical stream identity, then physical nonblank
record ordinal; it deliberately makes no global timestamp-order promise. A file
length observed when opened bounds each read. Only newline-terminated records
are admitted: an incomplete tail waits for a later pull. Blank lines advance the
byte prefix but not the event ordinal. A malformed or foreign record produces a
`foreign_or_invalid_history_record` omission, counted against the page limit;
neighboring records remain consumable. Unknown history fields and all original
bytes are preserved. Legacy timestamps and author strings remain readable.

Cursors are opaque, unsigned, caller-controlled read positions, **not authority**.
They bind to the selected canonical tracker location. Compaction, pruning,
truncation, prefix rewrites or another tracker produce `changes_stale_cursor`:
reorient with a full read and explicitly start a fresh cursor. Never reset silently.
These native per-stream cursors are distinct from the core SDK's timestamp-ordered
mutation cursors and pm-context's ranked delta cursors.

`ChangeReader<A: NativeAdapter>` uses the existing `pm-native-adapter/1` byte ABI.
An adapter must explicitly declare the additive `changes` capability. Existing
adapters lacking it refuse before dispatch. A request uses:

```json
{"abi":"pm-native-adapter/1","request_id":"changes","operation":{"action":"changes","request":{"cursor":null,"limit":20,"output_bytes":65536}}}
```

Remote adapters must implement equivalent immutable-prefix/exclusive-resume
semantics through their backend's authorized SDK, validate stale positions, and
preserve refusal details. The native client performs one bounded exchange and
validates the complete page before advancing its private cursor. The local
adapter and CLI use that same producer/validator path. This package supplies the
local adapter; it does not supply a new network transport or remote backend.

Admission is strict and happens before source access: `limit` is 1–100 admitted
records, `output_bytes` is 1024–1048576 and bounds **both** complete JSON and TOON
encodings, including the cursor and newline. A whole record is never truncated.
If a record or cursor cannot fit, `changes_output_limit` retains the previous
cursor; raise the ceiling or reduce the row limit. Input and output frames retain
the ABI's 1 MiB ceiling; its envelope can require additional headroom. The local
adapter visits at most 1024 history directory entries and reads at most 64 MiB
plus one detection byte per pull. It revalidates history prefixes, so this is
incremental event delivery rather than a persistent index or an O(delta) scan.
Cursor space grows with admitted streams. The working set includes at most one
bounded history file, directory paths, cursor state and a bounded output page;
this is an admission bound, not a measured RSS guarantee.

`SessionCancellation` is checked before dispatch, between streams and records,
and before page admission. Cancellation never advances the reader cursor. It is
cooperative: it cannot preempt a filesystem read, codec call or an embedding
adapter's synchronous exchange; transports must supply their own bounded timeout.
The CLI's `--cancel-file` checks before its single pull. Consumers control polling
and backpressure by deciding when to request the next page.

Other typed refusals identify invalid requests, source or scan limits, malformed
adapter pages, ABI/capability failures and authoritative backend refusals, with
remedies. No refusal commits a cursor. See the [compilable SDK example](../examples/changes.rs)
and `scripts/native-adapter-changes-validate.mts` for read-only public-tracker
validation against SDK history read-back with no Node/Git on the native path.
