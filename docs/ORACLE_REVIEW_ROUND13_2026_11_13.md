# Opaque item data in compact TOON pages

Thread PRRT_kwDOTwgMRs62_AJu was read in full, including its agent prompt. The
accepted starting head is `d29b3f3078bac19ce65d964226566ab16bbc16e8`; current main
is already included and the remote PR head matches. Earlier fixes are retained.

## Renderer positions and reservation

`src/toon_output.rs::render_output` compacts the read envelope, then invokes
`render` at its root. `render` follows object-member edges. At an array field it
chooses ordinary codec output or a table from `table`, and passes that entire
array to `encode`. It never calls `render` on an array element. `table` produces
`json_columns`, `absent`, optional `list_columns`, `null_columns` and
`null_column_index`; `render` writes these together as the array's sibling
`<field>_encoding` using one codec call. Nested JSON cells are data, not another
renderer traversal. The scalar/root-array path likewise uses the plain codec.

Thus an envelope path contains only object-member edges from the page root.
Crossing any array edge ends the instruction scope permanently. `/page/items`
can have table controls; `/items/0/labels` and paths inside decoded JSON cells
cannot. This applies equally to expanded rows and restored table rows.

The chosen policy is reservation at those producer-owned envelope positions,
matching the existing pre-encoded-table branch: an array's sibling
`<field>_encoding` is already reserved there and suppresses double encoding.
No item field names are reserved or escaped. All open-ended item metadata is
inside record arrays; the CLI's standalone get surface emits JSON. No new wire
marker or escaping is introduced, so published oracle bytes remain unchanged.

## Reader change and regressions

`src/toon_input.rs::restore` now follows exactly that object-only path rule. It
starts from array positions to look up their reserved control sibling instead
of suffix-scanning arbitrary keys. It never recurses into arrays, expanded rows,
restored rows or their decoded JSON cells. `restore_row` continues restoring the
producer's list/null/JSON/absence metadata once, retaining the earlier ordering
contract.

The shared `tests/support/toon_data_corpus.rs` covers every JSON value kind in
`labels_encoding`, `list_columns`, `null_columns`, `null_column_index` and
`x_encoding`. It includes nested user objects and a second corpus shaped like
valid table instructions that would remove fields, inject nulls and rewrite
strings under the old traversal. Minimal complete list envelopes round-trip as
whole JSON values with one and 32 rows. A real non-git tracker differential
compares complete published/native stdout, stderr and status, parses the full
page, and compares all restored item values with JSON output.

The prior sparse/list/framed-session round trips remain. The old invalid-table
test's root-array case now asserts exact opaque data preservation: the renderer
does not write instructions through that array boundary. Its malformed envelope
and nested object-envelope refusal assertions remain; no differential assertion
was weakened.

## Verification

The bounded gates and scoped coverage pass; final-source revert results are recorded below. Commands use at most two build jobs and test threads, with a
1,200-second limit per command. Full aggregate and Windows runtime remain for CI
at unchanged thresholds.


### Gate counts

Each test command uses `cargo test --locked -j2` with `RUST_TEST_THREADS=2`.

| Target | Passed |
| --- | ---: |
| `--lib toon_` | 18 |
| `--test cli_dispatch` | 32 |
| `--test admission_contract` | 12 |
| `--test claim_release_differential` | 52 |
| `--test close_differential` | 10 |
| `--test orientation_differential` | 37 |
| `--test list_differential` | 11 |
| `--test pagination_differential` | 33 |
| `--test conformance_differential` | 2 |
| `--test search_collections_differential` | 17 |
| `--test output_parity_differential` | 2 |
| `--test identity_audit` | 7 |

This is 233 unique selected tests, including all 164 differentials. The existing
list, sparse-table and framed-session checks remain in the selected TOON target.
The new full-list regression also passes with the oracle explicitly unavailable:
native success, complete page parsing and equality with JSON item values remain
mandatory; only the published comparison is conditional.

Linux and Windows GNU cross-clippy pass with
`--locked -j2 --all-targets --all-features -- -D warnings`, and
`cargo fmt --all -- --check` passes. Instrumented execution of the 18 selected
TOON tests measures the reader at 99/99 lines, 12/12 functions, 190/190 regions
and 18/18 branch outcomes. This is scoped evidence, not full aggregate coverage.
The linked PM command passes the same selected tests with recording disabled.
No threshold or coverage exclusion was changed.


### Final-source revert proof

Only `restore` is replaced with its literal implementation from the starting
head in a disposable source copy. Each command below fails at an executed
assertion with exit 101, then passes with exit zero after restoring the fix:

- `cargo test --locked -j2 --lib round13_user_field_names_round_trip`
- `cargo test --locked -j2 --lib round13_instruction_shaped_item_fields_remain_opaque`
- `cargo test --locked -j2 --test list_differential round13_user_encoding_fields_match_oracle`

All six final-source runs are verified; compilation failures do not count.
The controls were repeated after the empty-value corpus expansion and optional
oracle handling, and all copied source/test files are byte-identical after
restoration. The two unit controls expose both rejection and silent data
rewriting, while the differential pins complete published wire parity and
preservation of JSON item values. No version-specific test behavior is added.

The current public npm latest is 2026.11.13. The accepted review oracle remains
2026.11.11. No review thread was replied to or resolved; no part of this fix is
deferred. Changelog generation/check follow the final tracker writes.


After the final PM write the claim is released, with the item in_progress and
unclosed. Changelog generation and checking pass. A fresh fetch confirms that
main is included and the remote PR head is unchanged. Tracker history remains
append-only and is committed with the implementation.
