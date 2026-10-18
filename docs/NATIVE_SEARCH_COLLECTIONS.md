# Native keyword search and item collections

The oracle remains the published package locked by `tests/oracle/package.json`
and `tests/oracle/package-lock.json`. Features
[pm-rust-bj7a](../.agents/pm/features/pm-rust-bj7a.toon) and
[pm-rust-unih](../.agents/pm/features/pm-rust-unih.toon) belong to
[pm-rust-u4al](../.agents/pm/epics/pm-rust-u4al.toon).

## Keyword search

`search <query...>` joins positional arguments with spaces and emits compact
hits in default TOON or `--json`. Selection supports `--type`, `--status` and
`--tag`; `--limit` bounds the ranked producer page. Search includes terminal
items unless a status filter excludes them. Status CSV values share one trimmed,
nonempty token sequence for validation and filtering. A sole case-insensitive
`all` token, including `all,`, selects every status and is echoed as `all`;
cursor fingerprints retain the raw option. It searches title, description,
tags, status, body, comment/note/learning text, reminder/event text, dependency
identities and kinds, and plan text. Linked file and document paths are outside
the default keyword corpus.

Queries retain punctuation. Whitespace-separated literal tokens match
substrings, while exact title words use the oracle's ASCII alphanumeric word
tokenizer. Each corpus field is lowercased once per document and reused for token and
phrase scoring. Repeated occurrences contribute repeatedly. Multi-token queries add
the all-term coverage and normalized phrase bonuses. Exact full IDs and short
IDs use their dedicated scores. The oracle's configured search weights and
score threshold apply before ranking. Ties resolve by terminal status, priority,
newest update instant and locale-aware ID order.

`--after` resumes a producer page and discloses reference metadata;
`--output-budget`, `--output-limit` and `--output-cursor` share the existing native
read-output engine. Snapshot continuations reject changed rows, while producer
continuations retain the oracle's identity/index boundary behavior. Full,
semantic/hybrid, linked-content and execution-oriented search options are outside
this compact keyword command slice.

Both `--type value` and `--type=value` refusals preserve the oracle's exact
rejected value and flag names. The pinned oracle gives a corrected retry only
for the separate-token spelling; equals-form refusals keep generic help examples.

## Item collections

The SDK exposes `Workspace::collection` with `CollectionOptions`; the CLI exposes
`files <id>`, `docs <id>` and `test <id>`. Without mutation flags they list the
stored collection. Repeated `--add` inputs accept bare values or structured
`path=...` / `command=...` pairs. Structured CSV supports quoted and escaped
commas and Markdown key/value lines. Files/docs accept scope and note metadata;
tests also accept a path alongside the executable command. A bare path does not
create an executable test. Documentation references also accept Markdown links.

Files/docs deduplicate by path and scope, update supplied notes, and sort by path
and scope. Files additionally support `--append-stable`. Test identity includes
command, path, scope and context modes; duplicate additions preserve the original
entry. Test additions carry author, clock and available branch provenance, resolved
once for the transaction.

CLI create, update, comment, claim, release, close and collection mutations carry
resolved agent attribution and probe outcomes before computing record hashes,
including the release step of `close --release-assignment`. `CollectionOptions::identity` supplies SDK attribution; the CLI reuses
native identity resolution and session principals. Same-session detected claims
are accepted and foreign sessions receive the oracle's refusal.
`AuthorIdentity::for_collections` also resolves optional local session metadata:
Codex discovery visits at most 512 entries across four directory levels and
reads at most four MiB; Claude discovery checks at most 512 project directories
and reads at most one MiB. These probes use local files and the `AI_AGENT`
version label. `AuthorIdentity::with` scopes synchronous SDK mutations and restores
the previous identity afterward. Resolver outcomes and environment overrides match history bytes
without network or subprocess access.

| Command | Exact value removal | Index removal | Missing selector |
| --- | --- | --- | --- |
| files / docs | Bare path or `path=...`, across scopes | Not an oracle artifact selector | Successful unchanged receipt |
| test | Bare command/path or explicit `command=...` / `path=...` | `--remove-index`, 1-based | Typed `linked_test_remove_no_match`, exit 3 |

Artifact mutations cannot add and remove the same normalized path in one batch.
No-op edits preserve both item and history bytes. Empty resulting collections
are omitted from storage. Changes use the existing per-item lock, latest locked
document, synced atomic replacement, recovery journal, canonical patch order,
document hashes and record hashes. Claims and explicit force overrides follow
the existing native governance policy. Future fixture timestamps remain valid
for collection history, matching the oracle; lock acquisition and refusal
classification cap that clock at wall time so a fixture cannot reclaim a live
writer's lock.

## Evidence

`tests/search_collections_differential.rs` runs both real CLIs against disposable
trackers and compares complete stdout, stderr and exit codes. Search fixtures
contain 12 and 220 items, varied lifecycle states, priorities, timestamps,
Unicode, punctuation, phrase-like queries and no-result cases. Collection
fixtures replay identical before-images at the same path and compare every
authoritative item and append-only history byte.
Golden CLI-versus-SDK fixtures and ordinary oracle differentials share runtime-only
child environments, so host harness signals cannot change their history records.
Synthetic agent sessions cover explicit/detected identities, supplied and inferred
role/topic values, source metadata, disabled probes and resolver outcomes.

With the same configured 5-second lock wait, eight concurrent native pairs and
one concurrent oracle pair preserve their additions.
Each resulting history must contain both events with matching adjacent hashes,
and all durable artifacts must equal serial oracle replay in the observed
commit order. Parser and storage-boundary tests also exercise no-ops, malformed
selectors and durable recovery after a history publication failure.
Lock acquisition holds the cleanup gate while publishing the initial payload,
matching the oracle and preventing minimal-governance stale recovery from
stealing an initializing writer's lock. A deterministic real-filesystem
regression in `tests/support/mutation_unit.rs` protects that publication boundary.

A permanent per-item advisory lease serializes native cleanup-gate creation,
reclamation and release. Normal release explicitly unlocks before closing the
descriptor, so an inherited child descriptor cannot prolong a completed lease.
The kernel releases the lease after a crash. Only a busy advisory lease is
contention; other lease failures retain the lease path and underlying I/O error.
Missing or whitespace-only unpublished lock payloads report lock conflicts,
while nonempty malformed payloads retain stale-lock handling. Stale
reclamation checks the observed directory identity under that lease (device/inode
on Unix and a retained native file handle on Windows), then moves
that directory to a unique tombstone; deletion never targets a replacement at
the shared pathname. A controlled two-thread regression replaces the gate after
observation and verifies that the fresh gate survives. The published oracle uses
owner tokens on release but removes expired gates by pathname; mixed-runtime
concurrent gate reclamation is outside this native-writer guarantee.

Review regressions cover flags before IDs and query tokens, joined query
recovery examples, future timestamps with live locks and unlocked oracle writes,
probe attribution across non-collection mutations, description-only cursor
replay, configured oracle interpreters, and exactly one executed child test.

Feature comments contain the behavioral negative controls and final gate
receipts. Platform CI results are separate from local Linux validation.
