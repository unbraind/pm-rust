# Compatibility contract

The current `pm-rust` slice is a native read-and-mutate compatibility
implementation for the published `pm` 2026.8.21 workspace format.

## Supported

- ancestor discovery through the `.agents/pm/settings.json` marker;
- direct tracker-root discovery;
- strict TOON item decoding;
- the JavaScript TOON encoder's canonical empty-array spelling (`field: []`);
- required core metadata and the priority range `0..=4`;
- arbitrary runtime item types and statuses;
- lossless retention of unknown core and package metadata;
- recursive item folders, stable ID sorting, and exact ID/status/type filters;
- conflict-marker, duplicate-ID, malformed-document, and missing-item failures;
- deterministic JSON for full-item and list projections.
- explicit-ID creation for every canonical built-in item type;
- in-place field updates, comment appends, and closes that write the same
  canonical TOON item bytes and `item_hash_version: 2` history records as
  the published CLI, including argv-derived `agent_provenance` roles;
- canonical metadata ordering for storage, diffs, and hashes, matching the
  published `ITEM_METADATA_KEY_ORDER` contract;
- a lock wait budget (`locks.wait_ms`) retried by every mutation before a
  lock-conflict failure;
- exclusive per-item locks with stale-lock cleanup allowed only by an explicit
  force request after the configured TTL;
- synced same-directory temporary writes and parent-directory syncs on Unix;
- a durable create journal that completes a missing item/history half after a
  crash, removes a transaction that committed both halves, rolls back a
  transaction that committed neither, and refuses to overwrite foreign bytes.

## Deliberately unsupported

Item deletion, item moves between type folders, lifecycle workflow validation
beyond terminal-status refusal on close, harness/model identity detection
(the native slice records asserted authors and argv-derived roles only), and
general history replay are not exposed. Close requires a non-empty closing
summary because the published default governance policy does. Update accepts
only whole-field replacements of title, description, status, priority, tags,
and body. Adding a command
before its safety and differential fixtures exist is treated as a compatibility
failure, not progress.

## Conformance evidence

Repository tests use real filesystem layouts, nested folders, symlink escape
attempts, invalid documents, duplicate IDs, extension fields, CLI black-box
execution, and property-generated exact-ID filters. Manual acceptance runs the
compiled binary against the private mature companion tracker without copying
its contents into this public repository. Only aggregate counts and
contract-level results may be recorded publicly. Create conformance additionally
uses an exact official-SDK 2026.8.7 differential fixture generated under its
reproducible workspace clock, simultaneous process writers,
every recovery presence state, stale-lock ownership races, closed output pipes,
and real filesystem failures.

The Rust `toon-format` crate serializes an empty array as `field[0]:`, while the
canonical JavaScript encoder used by `pm` emits `field: []`. `pm-rust`
normalizes only that exact syntax before strict decoding; scalar strings that
contain the same characters are untouched.
