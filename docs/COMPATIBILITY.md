# Compatibility contract

The first `pm-rust` slice is a read-only compatibility implementation for the
published `pm` 2026.8.6 workspace format.

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

## Deliberately unsupported

Mutation, locking, history append/replay, transactions, recovery, field-aware
merge, package activation, semantic search, and non-JSON rendering are not
exposed. Adding a command before its safety and differential fixtures exist is
treated as a compatibility failure, not progress.

## Conformance evidence

Repository tests use real filesystem layouts, nested folders, symlink escape
attempts, invalid documents, duplicate IDs, extension fields, CLI black-box
execution, and property-generated exact-ID filters. Manual acceptance runs the
compiled binary against the private mature companion tracker without copying
its contents into this public repository. Only aggregate counts and
contract-level results may be recorded publicly.

The Rust `toon-format` crate serializes an empty array as `field[0]:`, while the
canonical JavaScript encoder used by `pm` emits `field: []`. `pm-rust`
normalizes only that exact syntax before strict decoding; scalar strings that
contain the same characters are untouched.
