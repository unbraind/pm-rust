# Native query cost explanations

`Workspace::explain_list(&QueryCostRequest)` and `pm-rust explain list` expose the
native default list scan without executing selection, sorting or rendering items.
The protocol is `pm-rust/query-cost/v1`. Node, Git, extensions and network access
are absent from its execution path. The TypeScript contracts remain the semantic
oracle for the underlying reader; they are not a native runtime dependency.

```sh
pm-rust explain list
pm-rust explain list --full --json
pm-rust explain list --max-entries 20000 --max-bytes 33554432 --json
cargo run --example query_cost -- path/to/project
```

Default TOON and compact single-line JSON carry the same fixed-size envelope
(under 2 KB in the acceptance fixtures), regardless of tracker size. No item text,
identifiers, local paths or timestamps appear in the result.

| Field | Meaning |
| --- | --- |
| `projection` | `brief` scalar selection/sort fields or `full` complete decoding |
| `fields` | Actual brief output fields from the reader, or null for full metadata |
| `observed_candidate_files` | Regular item files encountered, not certified unique items |
| `candidate_rows` | Zero through observed file count, before lifecycle selection |
| `item_read_bytes` | Zero through observed item-file bytes; brief still scans full files |
| `decoder_input_bytes` | Zero through the sum of actual shared projector input lengths |
| `explain_read_bytes` | Bytes inspected by this explanation itself |
| `auxiliary_read_bytes` | Unknown settings/schema/registry and other auxiliary I/O |
| `output_bytes`, `heap_bytes`, `elapsed_ms` | Unknown (`null`); no fabricated cost model |
| `condition` | Bounds require unchanged sources; the scan is not a snapshot or reservation |
| `alternatives` | Bounded list delivery and an admitted immutable read session |

The item-read bound excludes settings and registry I/O, which remains unknown.
Decoder-input bytes measure planned materialization at the text/decoder boundary,
not the size of allocated Rust objects. Full list decoding includes bodies even
though its result contains metadata. The brief projector can fall back to complete
decoding for quoted root keys or conflict markers; the estimate includes this
fallback. The planner uses the exact projector called by execution. Invalid item
contents may therefore produce a plan; planning does not certify item validity.
The existing reader still refuses invalid documents when actually executed.

Output limits and pagination do not reduce corpus scanning or global sorting.
The `read-session` alternative bounds accounted memory and page delivery, copies
a private immutable snapshot and performs full validation; its admission can
refuse. Neither alternative guarantees smaller source I/O or faster execution.

## Admission and refusals

The SDK request is one strict schema: `projection`, `max_entries`, `max_bytes`.
Use `pm_rust::strict_json::from_slice::<QueryCostRequest>` for JSON input so duplicate
and unknown keys, non-integers and invalid variants are refused. Typed callers use
the same struct. `explain_list` validates numeric domains once before scanning:
entries 1 through 1,000,000; bytes 1 through 1,073,741,824. Defaults are 100,000
entries and 64 MiB. Arrays and unbounded input strings are not part of this schema.
Only integers bounded well below the JSON safe-integer limit can be returned.

The scanner streams directories to depth 32 and inspects at most `max_bytes + 1`
bytes (one byte to detect overflow). Ignored entries count toward the entry cap;
ignored directory contents do not. Symlinks and non-item entries are skipped under
the same item-discovery rules as the reader. No corpus-sized index is retained;
one bounded item source and its projected text are held at a time. Directory
order does not affect successful totals. A concurrent edit can change what is
observed or cause a refusal; use a stable copy to compare plans reproducibly.

| SDK refusal / CLI code | Remedy |
| --- | --- |
| `InvalidRequest` / `query_cost_invalid_request` | Use admitted numeric domains |
| `ScanLimit` / `query_cost_scan_limit` | Raise controls within their domains or choose a smaller, shallower tracker |
| `Source` / `query_cost_source` | Use readable regular UTF-8 files and retry on a stable tracker |

Runtime refusals emit JSON on stderr, exit 2 and no partial plan. CLI syntax errors
use the existing argument parser's usage guidance. The planner does no writes,
journal recovery, import/export/sync, idempotency-key creation, claims or status-role
classification. Consequently it adds no replayable mutation or shared state path.
Custom lifecycle registries affect execution through the existing SDK reader;
planning conservatively includes all candidate files without interpreting names.

## Evidence

`tests/query_cost_contract.rs` initializes a genuine tracker with the pinned core
CLI in a non-git temp directory. Runtime CLI tests remove Git and Node from PATH,
compare all source filenames/bytes before and after successful and refused plans,
and assert compact output. A fixed seed runs 500 model-based sequences with older
input replays, whitespace/CRLF variants, relationships, custom statuses and all
native fields touched by the shared projector; complete state is read back through
`Workspace::get` and compared with a simple last-input-wins model. There is no
test-side normalization. No import/export/sync surface is introduced or modified.

The 10,000-item fixture verifies lower planned brief materialization than full
body decoding, identical source-read work and unchanged SDK state. Domain edges,
extreme numeric JSON, ignored files, symlinks, traversal depth and unavailable
sources exercise refusals. Existing projection/differential suites remain the
semantic oracle for the refactored shared projection function. Public tracker
measurements and unchanged npm/Bun CI aggregate results are reported in the PR.
