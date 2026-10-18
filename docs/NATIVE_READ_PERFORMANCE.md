# Native reads and the item-file boundary

Tracked by [pm-rust-wdj4](../.agents/pm/tasks/pm-rust-wdj4.toon), under
[pm-rust-u4al](../.agents/pm/epics/pm-rust-u4al.toon). Prerequisite PR #92
is merged. Publishing remains guarded and has not been enabled.

`get` traverses actual directory entries in the tracker, including custom type
folders and nested folders. The filename identifies the item; no pluralized type
folder or content-derived path is guessed. Directory-entry types avoid a metadata
system call for each unrelated item. Duplicate matching filenames refuse before
opening either item. A successful lookup opens and strictly decodes exactly one
file, checks its stored identity against its filename, and reads no history.
Missing identities open no item files. Runtime and other reserved directories,
symlinked files and symlinked directories remain outside discovery.

List selection retains the fields needed for sorting, filtering, hierarchy and
blocking. Brief context additionally retains agenda and ranking inputs; canonical
tabular activity collections become count-only arrays after checking their row
counts. This avoids decoding comment text, bodies and unrelated collection
payloads. Quoted root keys fall back to full decoding. Merge markers anywhere in
the source refuse. Selected fields still pass through the strict TOON decoder.
A projection does not claim validation of omitted collection payloads; complete
reads and `get` continue to validate complete authoritative documents.

Items are opened one at a time. Default list ordering depends on lifecycle,
priority and update time; context also needs cross-item statistics, relationships
and substantive history recency. Both therefore scan compact metadata rather
than stopping after arbitrary directory entries. An exact-ID list filter uses
the single-item lookup when tag warnings, dependency blocking and hierarchy
selection do not require the corpus. Search filters metadata before decoding
searchable payloads, scores one item at a time, and retains only the best producer
page plus the total match count. Continuation requests retain compact hit rows
for identity-based cursor lookup. No persistent native index or cache is added.

## Portable regression and latency gate

```sh
cargo test --locked --lib workspace::tests::single_item_get
cargo build --release --locked
node scripts/latency-gate.mts
```

The lookup regression observes authoritative file opens in the shared reader on
Linux, macOS and Windows. Its 3,000-item tracker includes an unrelated damaged
item. A successful lookup must open exactly one item; a miss and a duplicate
filename must open zero. The generated latency fixture has 3,000 items in a
custom nested folder, mixed lifecycle states and priorities, comments and bodies.
Thirty fresh release processes run per command without native cache priming.
p50 is the median and p95 is nearest rank (sample 29 of 30). CI asserts p95 below
50 ms for `get`, 150 ms for `list --limit 20`, and 250 ms for
`context --limit 10`, independently on all three operating systems. Dedicated
jobs isolate these latency samples from the release coverage job.

## Revert proof

Replacing only `Workspace::get` with the old `read_items()` lookup and rebuilding
made both new lookup regressions fail: exit 101, zero passed and two failed.
The large fixture encountered unrelated merge markers, and the duplicate test
received a decode refusal instead of a directory-derived duplicate refusal.
Restoring the lookup made both pass. The exact command and receipt are in the
tracker item's append-only comments and linked test runs.

## Public-tracker measurement

The measurement uses a temporary copy of the 2,991-item public tracker. Neither
runtime is given the original tracker path. SHA-256 manifests before and after
verify regular authoritative bytes in the original and copy; generated runtime
caches, locks and search caches are excluded. The published oracle is the pinned
2026.10.14 package. All runs use isolated home directories, a synthetic author
and the test telemetry boundary. Raw samples contain no local paths or item data.
