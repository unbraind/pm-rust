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

List selection retains the required scalar fields for sorting and filtering.
Relationship payloads and dependency/parent indexes are used only when the
requested filters require them. First-page limits discard unreturned documents
before JSON projection while preserving the full matching total and cursors.
Brief context additionally retains agenda and ranking inputs; canonical
tabular activity collections become count-only arrays after checking their row
counts. This avoids decoding comment text, bodies and unrelated collection
payloads. Quoted root keys fall back to full decoding. Merge markers anywhere in
the source refuse. Selected fields still pass through the strict TOON decoder.
A projection does not claim validation of omitted collection payloads; complete
reads and `get` continue to validate complete authoritative documents.

Each worker decodes one item at a time. macOS and Windows overlap filesystem reads with
four scoped workers and at most 256 documents per batch. List/context workers
read and decode projections; the caller retains listing order for duplicate checks
and refusals. Search workers prefetch raw contents before serial scoring. Direct `get` bypasses this prefetch. Context caches update/deadline/history-recency
sort keys and parses its captured read clock once, avoiding timestamp parsing
inside sort comparisons. A timestamp-count regression enforces linear work on
the default brief-context fixture. Default list ordering depends on lifecycle,
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
runtime is given the original tracker path. The copy matched a SHA-256 manifest
at capture and its regular authoritative
bytes remain unchanged through measurement; generated runtime caches, locks
and search caches are excluded. The live original advanced independently during
later measurements. Neither CLI received its path, and all original access was
read-only copying and hashing. The published oracle is the pinned
2026.10.14 package. All runs use isolated home directories, a synthetic author
and the test telemetry boundary. Raw samples contain no local paths or item data.

Each cell is p50 / p95 in milliseconds (30 fresh processes per cell).
The baseline uses the preceding 30-run phase on the same captured copy. Native
and oracle samples use the final comparison phase, alternate runtime order each
round and rotate command order. Filesystem caches are warm.

| Command | Before | Native | Pinned Node oracle |
| --- | ---: | ---: | ---: |
| `get pm-02c4` | 1779.3 / 3062.6 | 22.9 / 54.9 | 1662.3 / 4461.0 |
| `list --limit 20` | 1296.7 / 2573.5 | 585.2 / 1449.0 | 2083.9 / 3853.0 |
| `context --limit 10` | 2217.2 / 4137.2 | 1847.5 / 4816.2 | 4788.9 / 11232.3 |
| `search history --limit 10` | 2715.2 / 9344.4 | 2307.5 / 5318.2 | 4667.9 / 8703.9 |

Native source: `3c7e7010955e07d3bc68997aa8b9d2ef0d1146d8`; baseline: `368697864187e347ed17c5a1e7dfc00284cf4e26`.
Comparison window: 2026-10-18T20:22:53Z to 2026-10-18T20:33:06Z.
Inherited nice: 15; sampled one-minute load: 27.35–37.69.
Focused coverage compilation overlapped the early comparison rounds. The
real-copy run includes stored history and shared-host scheduling, so individual
p95 results can regress against the separately captured baseline. CI
independently measures the generated fixture on each OS.

[360 raw samples](benchmarks/native-read-performance-2026-10-18.csv) and the
[binary fingerprints, copy integrity and timing receipt](benchmarks/native-read-performance-2026-10-18.json)
contain the complete measurements. The captured copy retained all 6,005
authoritative file hashes. The live original advanced independently; all original
access remained read-only.

Linux `strace` on the captured copy measured **2,991 → 1 item-file opens**,
**3,009 → 19 total `openat` calls**, and **zero history files** in either build.

## Generated-fixture CI latency receipt

Measured at `3c7e7010955e07d3bc68997aa8b9d2ef0d1146d8` in [CI](https://github.com/unbraind/pm-rust/actions/runs/39163033065).
All three OS jobs also passed both portable lookup/open-count regressions.

| OS | get p95 | list p95 | context p95 |
| --- | ---: | ---: | ---: |
| Linux | 5.7 | 59.1 | 107.6 |
| macOS | 17.0 | 81.5 | 110.2 |
| Windows | 22.0 | 128.0 | 191.7 |

Required ceilings remain **50 / 150 / 250 ms**, respectively. Each cell has
30 fresh release processes. The [CI sample receipt](benchmarks/native-read-ci-2026-10-18.json)
retains every sample. Earlier Windows failures were fixed through bounded
bounded read-and-project workers and query-work reductions; no ceiling or fixture was relaxed.

Reverting only the two context ordering/scoring functions to their pre-cache
implementation made `context_timestamp_parse_work_is_linear` fail: 346 timestamp
parses exceeded its 282-parse bound for 50 items. Restoring the implementation
passed. The single-item lookup revert separately failed both lookup regressions.

macOS and Windows use bounded read-and-project workers; duplicate checks and
refusals follow source order on the caller. Linux remains serial. The binary
fingerprint and source identity in each receipt identify its measurement.
