# Compatibility contract

The current `pm-rust` slice is a native read-and-create compatibility
implementation for the published `pm` 2026.8.7 workspace format.

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
- canonical tag ordering, TOON bytes, create-history patch ordering, and
  SHA-256 document hashes compatible with `pm` 2026.8.7;
- exclusive per-item locks with stale-lock cleanup allowed only by an explicit
  force request after the configured TTL;
- synced same-directory temporary writes and parent-directory syncs on Unix;
- a durable create journal that completes a missing item/history half after a
  crash, removes a transaction that committed both halves, rolls back a
  transaction that committed neither, and refuses to overwrite foreign bytes.

## Deliberately unsupported

Update/delete mutations, general history replay, field-aware merge, package
activation, semantic search, automatic ID allocation, custom item types, and
non-JSON rendering are not exposed. Create currently fails fast on a live lock;
it does not yet implement the configured lock wait budget. Adding a command
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
