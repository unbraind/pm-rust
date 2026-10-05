# List pagination and bounded output against PM CLI 2026.10.5

Owner: [pm-rust-ufzy](https://github.com/unbraind/pm-rust/blob/main/.agents/pm/tasks/pm-rust-ufzy.toon).

The reference is the real published `@unbrained/pm-cli@2026.10.5` executable.
`tests/pagination_differential.rs` runs both executables on the same synthetic
tracker with a fixed read clock and compares complete stdout and stderr bytes,
including key order, token estimates, fingerprints, cursors, and exit codes.
No response fields are normalized or removed. The original nine 2026.10.2
unbounded golden envelopes also pass unchanged against 2026.10.5; the existing
full-output diagnostic fixture now applies to the native executable too.

## Supported controls

| CLI control | SDK `ListOptions` field | Published behavior |
| --- | --- | --- |
| `--limit N` | `limit` | Producer row ceiling, including zero; echoes the supplied decimal string. A zero limit returns an empty, final page. |
| `--offset N` | `offset` | Skip matching rows before the producer ceiling; conflicts with `after`. |
| `--after CURSOR` | `after` | Continue from item identity and validate the semantic query fingerprint. Subsequent pages replace selection metadata with `continuation_contract`. |
| `--no-truncate` | `no_truncate` | Ignore the producer ceiling; explicit output budgets can still compact rows. |
| `--output-limit N\|unbounded` | `output_limit` | Independent delivered-row ceiling; disclose an `applied_bound` when it binds. This ceiling alone does not generate a continuation. |
| `--output-budget N\|unbounded` | `output_budget` | Enforce the canonical output ceiling with compaction, recovery or omission receipts. Infeasible budgets emit an omission receipt and exit 2. |
| `--output-cursor CURSOR` | `output_cursor` | Continue a budget-bounded row snapshot; reject a changed collection, stale offset, or different command. |
| `--for triage` | `intent` | Governance and ownership projection with a budget-derived producer ceiling and intent receipt. |
| `--token-budget N` | `token_budget` | Override the selected triage intent; minimum 256. Requires an intent. This is a separate intent policy from the canonical output budget. |
| `--full` / `--brief` | `full` / `brief` | Restore metadata or identity fields independently of the page selection. Full projection discloses output-policy diagnostics when canonical controls require an audit. |

`Workspace::list_page(&filters, &options, now)` exposes these contracts without
Node or JavaScript. `Workspace::list`, `list_unbounded`, and
`list_unbounded_full` retain their existing SDK envelopes. The CLI's JSON mode
now uses `list_page`, and no longer requires explicit unbounded flags.

Producer limits default to the whole matching selection below 10,000 rows and
20 rows at or above that threshold. JSON's default output ceiling is 6,000
estimated tokens. Explicit full projection and no-truncate express complete
read intent; an explicit canonical budget can still override it. Canonical
controls take precedence over aliases, with both requests recorded when a
receipt is necessary. Producer page sizes and projection flags are presentation
controls and do not change the query fingerprint.

Canonical output token estimates use the rendered pretty JSON plus its newline,
measured as `ceil(UTF-8 bytes / 4)`. Intent estimates use compact JSON. Estimates
stabilize after receipts are attached. Bounded output can compact long strings
and nested row arrays as well as the main item collection; only the declared
main item collection receives an output continuation. Its fingerprint binds
original projected row content, before string or row compaction.

## Two continuation contracts

The producer fingerprint binds the tracker root, filters and ordering. The
published producer cursor contains `version`, `fingerprint`, `after_id`, and
`after_index`. It does **not** bind a workspace snapshot. A changed item can
therefore remain readable with `--after`. If the cursor item disappears, the
published implementation falls back to its stored position. Stable-workspace
walks have no missing or repeated rows; producer walks across mutations do not
have that guarantee.

A budget continuation contains `v`, `c`, `p`, `o`, `n`, and `f`, binding command,
collection, offset, original row count and a snapshot fingerprint. The parity
suite mutates an item between two processes and requires byte-identical stale
refusal from both executables. It separately tests filter-mismatched producer
and output cursors. Snapshot checks are observational per invocation; they do
not lock a workspace across a page walk.

The suite also walks all output-budget pages, changes producer page size and
projection between pages, checks empty and final pages, and exercises compaction
on a final producer page. Ninety-six randomized SDK walks vary fixture sizes
and positive page limits, asserting the exact expected set and rejecting every
repeated ID. Source coverage remains gated at 100 percent for lines, functions,
regions and branches; formatting, Clippy, rustdoc, identity, dependency audit,
and release guards remain enforced.

## Boundaries

Selection retains the earlier native slice: the default lifecycle registry,
single status/type/ID filters, lowercase ASCII identifiers, and strict stored
item reads. Runtime schemas, configured intents, extension hooks, custom field
selectors, sorting flags, tree mode, output sessions, and other encodings remain
outside this slice. Numeric controls use nonnegative safe decimal integers for
producer limits/offsets and positive safe decimal integers for canonical bounds.
Invalid native controls fail closed; the wider TypeScript argument-recovery
catalog remains outside the slice.

A reproduced Unicode boundary remains tracked in
[pm-rust-8hkb](https://github.com/unbraind/pm-rust/blob/main/.agents/pm/issues/pm-rust-8hkb.toon):
when the 240-unit string ceiling splits a surrogate pair, the reference emits
a lone surrogate escape while the native JSON value replaces it with U+FFFD.
That case also changes token receipts and is not covered by the parity claim.
To reproduce, give a synthetic item a title consisting of 239 `x` characters,
one `😀`, and another 2,000 `x` characters, then compare both executables with
`list --json --full --output-budget 500`. The reference's compacted title ends
in `\ud83d…`; the native title ends in `�…`.

`just changelog-full` regenerates the changelog through the pinned pm-changelog
flow. It selects closed items, so this still-open task receives its generated
entry after orchestrator verification and closure. No item is closed by this
implementation change.

## Verification receipt

Windows run `37287014816` exposed a native query fingerprint bug: the three
failing differential cases first differed inside `next_cursor.fingerprint`
(including byte 43,225 in the triage case), after identical rows and counts.
Rust's canonical tracker path carries a Windows verbatim namespace prefix;
the published SDK's `resolvePmRoot` uses Node `path.resolve`/`path.join` and
hashes the ordinary drive or UNC spelling. Run `37289539548` then proved a
second difference: the canonical filesystem root had no short-name alias and
61 characters, while Node's resolved root retained an 8.3 alias and had 58
characters. Both had eight backslashes and zero forward slashes. Native
discovery now retains the resolved input spelling separately for query hashing,
removing its namespace prefix while keeping the canonical filesystem root intact.
Portable drive/UNC regression cases include a fingerprint measured with the
published SDK. Temporary path-shape diagnostics were removed after diagnosis;
the byte-comparison harness and Windows test selection remain unchanged.

Source implementation: `5ecc383`. Review: [draft PR #60](https://github.com/unbraind/pm-rust/pull/60).

`just release-check` passed 183 tests in both ordinary and instrumented runs,
formatting, deny-warning Clippy, public/private rustdoc, dependency audit, and
pinned changelog verification. Measured coverage:

| Metric | Covered / total |
| --- | --- |
| Lines | 2,892 / 2,892 |
| Functions | 251 / 251 |
| Regions | 4,383 / 4,383 |
| Branches | 604 / 604 |

`pm test pm-rust-ufzy --run --progress` passed the linked command
`PM_RUST_REQUIRE_PUBLISHED_CLI=1 cargo test --locked --test pagination_differential --test list_differential`:
eight existing list tests and seven pagination tests, including 96 randomized
walks. Strict tracker health and validation passed with existing guidance and
historical provenance advisories.

The unchanged lock-replacement fixture failed once during an aggregate run and
passed both an isolated PM-linked run and the final aggregate retry. Investigation
is tracked in [pm-rust-ax0o](https://github.com/unbraind/pm-rust/blob/main/.agents/pm/issues/pm-rust-ax0o.toon).
The draft remains open for the Unicode boundary above and orchestrator verification.
