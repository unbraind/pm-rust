# Native selection and ownership lifecycle

This slice extends [pm-rust-rysu](../.agents/pm/features/pm-rust-rysu.toon)
under [pm-rust-u4al](../.agents/pm/epics/pm-rust-u4al.toon). The independent
published oracle is locked to `@unbrained/pm-cli` 2026.10.14 after all six
existing differential suites passed its 41 cases. The original task targeted
2026.10.13; the newer published version was checked before adoption. Production
selection and mutation code executes in Rust.

```sh
pm-rust claim --next --author fixture-agent --json
pm-rust claim sample-item --start --author fixture-agent --json
pm-rust release sample-item --pause --author fixture-agent --json
```

`start-task` and `pause-task` expose the corresponding composed operations.
Start persists a claim before updating the lifecycle status. Pause persists the
status update before releasing ownership. A later failure preserves the earlier
step and includes inspection guidance in its refusal. These operations deliberately
use separate item transactions, matching the published lifecycle contract.
Unchanged status updates return zero changed fields and add no history record.
Configured external status and workflow schema files override embedded settings,
including transition refusals that preserve an already completed claim.
Deprecated lifecycle aliases retain their published absence of an argv-derived role.

Claim-next scans real tracker items, filters actionable work and caller-owned
items, excludes unresolved prerequisites and active containers by default, then
applies structural ordering and the default relevance signals. Type, tag,
priority, parent descendants, sprint, release, ownership filters and the maximum
attempt count are native inputs. Competing foreign claims are retried against the
ranked snapshot; validation and live-lock failures propagate. `--if-available`
returns the published successful skip receipt when nothing remains available.

The SDK exposes `claim_with_options`, `release_with_options`, `claim_next`,
`start_task` and `pause_task`. Existing explicit ownership methods remain
available. `OwnershipOptions::release_bypass` supports an approved package
handoff that only removes ownership metadata; it does not authorize lifecycle
updates. SDK callers can supply an `AuthorIdentity` for session attribution;
otherwise the request author is used without reading ambient process identity.
The CLI resolves author flags, `PM_AUTHOR`, configured
authors and built-in harness environment signals in that order. Detected sessions
use a hashed instance principal while retaining the harness display author.
The published selection wrapper forwards that display author as asserted identity
to its atomic claims. Native claim-next preserves this distinction: selecting an
unclaimed item stores the display principal, while an explicit detected claim
stores the session principal. Claim-next can therefore exhaust a candidate held
by an explicit session claim, including the caller's own session. Differential
tests retain this observable oracle behavior.
History records retain available provenance dimensions. Derived semantic ownership
state is persisted under the shared session-state lock and used for later inferred
role and topic attribution.

The adjacent [comment ordering issue](../.agents/pm/issues/pm-rust-2ucz.toon)
and [tag ordering issue](../.agents/pm/issues/pm-rust-mttg.toon) use native ICU
English collation. Tags are trimmed, lowercased, deduplicated and sorted. Equal-time
comments sort by text, then author. The differential harness compares complete
item and history bytes, session state, process exit codes, and JSON output bytes.

This slice retains the established JSON command surface. Other output formats,
JavaScript extension execution, external blocker resolvers, custom relevance
providers, session-file probes and context usage-affinity integration require
separate native contracts. Passing these scenarios is not whole-CLI parity or
release approval.

Behavioral revert verification retained command parsing and all tests. Replacing
only `Workspace::claim_next`, only `Workspace::start_task`, only tag locale sorting,
or only equal-time comment sorting produced one runtime regression failure per
revert (exit 101). Each source file was restored byte for byte afterward.

```sh
cargo test --test claim_release_differential selection_ranks_ready_work_and_excludes_live_blockers -- --exact
cargo test --test claim_release_differential composed_lifecycle_matches_published_bytes -- --exact
cargo test --test claim_release_differential equal_timestamp_comments_and_punctuation_tags_match_published_storage -- --exact
```

A separate pre-existing mutation identifier boundary is tracked by
[pm-rust-2tb9](../.agents/pm/issues/pm-rust-2tb9.toon). A scratch tracker directory
watch confirmed transient lock creation outside the locks directory for an unknown
path-bearing identifier; the lock was removed after discovery refused the item.

Final validation used the repository's unchanged `just release-check` under an
isolated Git identity. Its stable and coverage-instrumented runs each passed
278 tests, with zero failed, ignored or filtered tests. These include 43 oracle
differential tests across six suites, 16 of them in
`tests/claim_release_differential.rs`, and four public SDK ownership contracts.
The gate also passed formatting, Clippy with warnings denied, private-item
documentation checks, dependency auditing and changelog validation. The audit
retains one pre-existing allowed unmaintained dependency advisory,
`RUSTSEC-2024-0449` (`unsafe-libyaml` through a development dependency).

| Measured production coverage | Covered / total | Percent |
| --- | ---: | ---: |
| Lines | 4,803 / 4,803 | 100 |
| Functions | 451 / 451 | 100 |
| Regions | 7,290 / 7,290 | 100 |
| Branch outcomes | 966 / 966 | 100 |

The final gate ran every test from a clean coverage build; diagnostic exclusions
and accumulated profiles were not used for acceptance. The repository has no
root npm package scripts; `just release-check` installs the locked npm oracle and
runs the same aggregate used by CI.
