# CLI parity: original 2026.10.2 slice and 2026.10.7 pagination

Owner: [pm-rust-p0no](https://github.com/unbraind/pm-rust/blob/main/.agents/pm/features/pm-rust-p0no.toon).

The oracle is the exact published `@unbrained/pm-cli@2026.10.2`, installed in
an isolated temporary directory. Inventory was captured with
`npx -y @unbrained/pm-cli@2026.10.2 help --all --json`. This is a vertical
conformance slice; the version pin does not claim whole-CLI parity.

The later [2026.10.7 pagination slice](LIST_PAGINATION_2026_10_5.md) adds
`Workspace::list_page`, producer and output continuations, budgets, and triage.
The original nine golden envelopes remain unchanged. The native CLI now includes
published full-projection diagnostics and accepts JSON reads with default bounds.
The table below records the original legacy SDK boundaries.

## Implemented and divergent native commands

| Native command / SDK operation | Implemented | Remaining differences |
| --- | --- | --- |
| `list --json --output-budget unbounded --output-limit unbounded` / `Workspace::list_unbounded` | Complete published JSON envelope; default terminal exclusion; terminal-last/priority ascending/update descending/ID ordering; brief projection; full metadata for `--full`, `--all` or `--status all`; single status/type/ID filters; matching-count total; omission receipt; unknown metadata retained | Canonical default lifecycle registry and lowercase ASCII IDs only. Extra (unknown) metadata keys are emitted in alphabetical order, while the published CLI keeps the `.toon` file order ([pm-rust-ba8f](../.agents/pm/issues/pm-rust-ba8f.toon)). Legacy unbounded SDK methods omit output-policy diagnostics; the newer `list_page` API supplies them. No custom field selection, hooks, runtime schemas, CSV filters, or partial-result recovery |
| Bare `list` / `Workspace::list` | Existing deterministic native projection, ID order, exact filters | Native priority/parent fields and pre-filter total; includes terminal items; different envelope/order from published defaults |
| `get` / `Workspace::get` | Validated complete item by ID; unknown fields retained | Flat native document; published entity envelope includes linked data, claim state, child summary, projections and omission receipts |
| `create` / `Workspace::create` | Explicit ID; TOON and append-only history differential | Native argv/receipt subset; automatic IDs, runtime types, governance, hooks and other fields omitted |
| `update` / `Workspace::update` | Whole-field title/description/status/priority/tags/body replacement; stored-byte differential | Limited flags and receipt; no general lifecycle governance/field operations |
| `comment` / `Workspace::comment` | Comment append with stored-byte differential | Limited native argv and receipt; published canonical `item comment` family absent |
| `close` / `Workspace::close` | Immutable closing summary with stored-byte differential | Limited native argv/receipt and lifecycle/governance behavior |
| Native clap help | Native argument documentation | Published structured `help --all --json` contract absent |

Examples (the explicit unbounded flags opt out of default bounds):

```bash
pm-rust --workspace /path/to/project list --json --output-budget unbounded --output-limit unbounded
pm-rust --workspace /path/to/project list --json --output-budget unbounded --output-limit unbounded --all
pm-rust --workspace /path/to/project list --json --output-budget unbounded --output-limit unbounded --status open --type Task --ids demo-a
```

`--ids` remains a native alias for `--id`. `--all` conflicts with `--status`.
`--full` restores complete metadata without changing the selected statuses;
`Workspace::list_unbounded_full` exposes the same projection in the SDK.
CSV selectors fail with a typed read error. `--timestamp` supplies a validated
UTC RFC 3339 read clock for fixtures; ordinary invocations use current UTC time.
The SDK accepts the caller's clock string. Timestamp ordering parses RFC 3339 instants
and uses spelling to break equal-instant ties; malformed stored timestamps
sort after valid timestamps within the same priority, with lexical ordering
between malformed values; timestamps are parsed once per selected item and
cached with the complete ordering key; alternate formats accepted by
JavaScript `Date.parse` are outside the slice. Native invalid/duplicate item
reads fail closed rather than reporting TypeScript's partial completeness.

## Missing published surface

The captured help advertises these 39 roots:

```text
activity aggregate assurance claim close close-task context contracts create
extension get graph help history history-attest history-compact history-redact
history-repair install item list list-all list-blocked list-canceled list-closed
list-draft list-in-progress list-open ops package pause-task plan release restore
search start-task update upgrade workspace
```

The create/update/comment/close/list/get native commands have implementation slices;
help differs and the other roots remain missing. Nested operations do not
become implemented because one related native alias exists. In particular,
agent `context`, `search`, `next` (nested/alias task selection), claim/release,
configured context intents, and native history merge/replay/repair remain follow-up work.
The published root inventory includes deprecated aliases and command families,
so its count is an inventory measure rather than a readiness percentage.

## Storage and multi-agent history

Items use canonical TOON with history JSONL and `item_hash_version: 3`.
Canonical metadata ordering, exact post-document hashes, append-only mutations,
locks, journals and crash recovery already have published-CLI differential
contracts. This slice does not change stored bytes or the hash epoch.

`tests/branch_merge_contract.rs` drives real multi-branch history merges through
the published history merge driver and verifies native-produced histories.
That proves interoperability of the tested records; it does not provide a
native merge command, general item field merging, or full history replay.

## Golden evidence and gates

`tests/list_differential.rs` creates one synthetic tracker and runs both real
CLIs against that same tracker under a fixed UTC clock. Nine complete JSON
envelopes are checked against `tests/fixtures/list-2026-10-2.json`, recorded
from the exact published oracle. Cases cover defaults, all statuses, explicit
terminal status, case-insensitive type, ID, combined filters and an empty
result; fixtures cover priority/update/ID ties and unknown metadata. No fields
are removed from either output during the nine-envelope comparison. Native and SDK golden comparisons run even when the published CLI or Node
interpreter is absent; only the published comparison skips. Required-tool mode
fails instead of skipping. Regression tests also cover invalid CLI clocks, the
SDK all/status conflict, empty trackers, malformed documents and CSV refusal.
Additional full-projection regressions compare complete native and published
envelopes with their respective expected diagnostics, and mixed-validity
ordering regressions cover comparator transitivity. Fixtures contain synthetic data only. Production remains entirely Rust.

CI requires the published CLI, runs tests on Linux/macOS/Windows, and keeps the
100 percent line/function/region/branch gate. Rustdoc remains enforced by
Cargo's `missing_docs = "deny"` and the complete documentation gate. Publication
remains disabled by Cargo `publish = false` and the unchanged maintainer
approval guard; updating a static tool pin does not authorize release execution.
