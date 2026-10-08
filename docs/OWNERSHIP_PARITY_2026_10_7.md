# Native explicit ownership parity with CLI 2026.10.7

Implemented feature: [pm-rust-zukw](../.agents/pm/features/pm-rust-zukw.toon),
under [pm-rust-u4al](../.agents/pm/epics/pm-rust-u4al.toon).

The native CLI and SDK implement explicit `claim <id>` and `release <id>` for
canonical items and asserted authors. The native command surface emits JSON
receipts; alternate published rendering formats are outside this slice.
Production executes entirely in Rust;
the locked published CLI is an independent test oracle only.

```sh
pm-rust claim sample-item --author fixture-agent --json
pm-rust claim sample-item --author fixture-agent --if-available --json
pm-rust release sample-item --author fixture-agent --json
```

Both commands accept `--message`, `--force`, and the fixture-only `--timestamp`.
An asserted author is also its claim principal. Claim writes `assignee` and
`claim_principal` under the item lock, then journals the canonical item and
history event. A repeated same-owner claim adds no history. Release removes
both fields and records a maintenance event even if already unclaimed.
An item remaining `in_progress` after release carries the published warning
and pause suggestion.

Claim rejects a foreign holder unless `--force` or `--if-available` is supplied;
the latter is a successful no-op with a skip receipt. Terminal claims require
`--force`. Release obeys the governance preset: `minimal` allows non-owner
release, `default` warns internally, and `strict` refuses. A `custom` preset
uses its explicit policy with the published default fallback. Force bypasses
ownership checks and permits stale-lock recovery; it never removes a live lock.
Timed lock refusals report measured elapsed wait. Ownership lock expiry uses
the payload creation clock and configured TTL, with
preset precedence for whether stale recovery requires force. Other mutation
slices retain their existing filesystem-age lock contract.

`tests/claim_release_differential.rs` compares complete JSON success/refusal
envelopes and exit codes, and exact item plus history bytes after every step,
against `tests/oracle`'s lockfile installation of CLI **2026.10.7**. It covers
ownership policies, same-owner claims, skipped foreign claims, forced takeover,
non-owner release, empty release, terminal claims, in-progress release warnings,
and active/stale lock refusal and recovery. Refusals preserve durable bytes.
Every completed fixture is checked by `pm history <id> --verify --strict-exit`.

The real concurrency test runs six independent races. A barrier releases two
threads which each spawn a separate native process against the same item.
Each race requires exactly one winner, a loser with the published conflict
JSON and exit **4**, and exactly two stored history lines (create plus claim).
The winning state is compared byte-for-byte with the oracle claiming as that
winner, and verified with the published history verifier.

The behavioral revert retained the CLI parser, SDK method signatures and tests,
replacing both `Workspace::claim` and `Workspace::release` bodies with a typed
unavailable-command refusal. The unchanged differential command compiled and
then failed all four tests at runtime (exit 101). In the race, both processes
failed and the one-winner assertion observed zero winners. Restoring the
implementation restored the passing comparisons. The existing create/update/comment/close
differential still passed with ownership behavior reverted, providing a positive
control. This proves the regressions
depend on ownership behavior rather than merely command availability.

Verification commands:

```sh
just oracle-install
PM_NODE_CLI=tests/oracle/node_modules/@unbrained/pm-cli PM_RUST_REQUIRE_PUBLISHED_CLI=1 cargo +1.90.0 test --locked --test claim_release_differential
just changelog-full
just release-check
```

Both clean release gates passed: the ordinary environment and
`env HOME=$(mktemp -d) GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1 just release-check`.
Each ran 219 tests in both its ordinary and instrumented passes, with no
failures or skips. Coverage was 100% for all 3,367 lines, 290 functions,
5,090 regions, and 696 branches.

The release gate requires all source line, function, region, and branch coverage
percentages to equal 100. It also enforces formatting, clippy, rustdoc, tests,
identity audit, dependency audit, and the complete regenerated changelog.
The same gate is required with a disposable HOME and global/system Git config
disabled; existing Rust toolchain/cache locations remain explicitly configured.

This slice excludes `claim --next`, composed `claim --start` / `release --pause`,
automatically detected author/session principals, extension ownership bypass,
semantic attribution state, hooks, custom lifecycle registries, and workflow
policies. Those are tracked by [pm-rust-rysu](../.agents/pm/features/pm-rust-rysu.toon); passing this slice does not claim
whole-CLI parity or authorize a package release.
