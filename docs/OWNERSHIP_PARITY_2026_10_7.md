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

## Windows diagnostic-read regression (2026-10-10)

At candidate `ea29da3aa8265b597595f0ed9a3c95be08da0dbb`, the Windows aggregate
in [run 37709763894](https://github.com/unbraind/pm-rust/actions/runs/37709763894)
passed the tests but covered only 3,362/3,363 lines and 5,085/5,087 regions.
Functions (290/290) and branches (696/696) were complete. The missing regions
were the diagnostic read error in `ownership_lock_refusal` and its propagation
through `ownership_item`; acquisition's non-contention error was already covered.

The Unix permission fixture could not exercise that chain on Windows. The
unit fixture and the SDK consumer regression now hold a real Windows file
handle with `share_mode(0)`. With lock waiting disabled, acquisition reaches
the conflict diagnostic, whose read fails with sharing violation (OS error 32).
Both layers assert that the original I/O error and lock path survive, item and
history bytes stay unchanged, and claiming succeeds after releasing the handle
and removing the fixture lock. This correction changes tests and documentation
only; production ownership code and the **2026.10.7** oracle remain unchanged.

The diagnostic regression also strengthens the Unix fixture with durable-byte
and retry assertions. Its fail-on-revert proof temporarily swallows the native
diagnostic read error: the unchanged consumer test must compile and fail at
runtime because a conflict refusal replaces the original I/O error. Production
source is restored before verification and commit. Windows execution and exact
coverage are established by the candidate's native Windows aggregate, rather
than inferred from the cross-compilation check.

The independent [toon-format repair, PR #72](https://github.com/unbraind/pm-rust/pull/72)
upgrades the codec and oracle to 2026.10.9. These candidates are deliberately
verified separately. Package context links to the companion tracker for session
`pm-cli-website-session-2026-10-10` through its
[published tracker directory](https://github.com/unbraind/pm-cli-companion/tree/main/.agents/pm);
the supplied session has no verified item link on companion main.

At append candidate `1a6d111685209f65023b7e08efd0c5e11af1592a`, local
correction verification passed through the feature's linked PM tests: the
diagnostic consumer regression, all four required claim/release differentials,
Windows GNU all-target/all-feature check and strict Clippy, and both ordinary
and isolated-HOME `just release-check` runs. Each aggregate ran 219 ordinary
and 219 instrumented tests and covered all 3,367 lines, 5,090 regions, 290
functions and 696 branches at 100%. Formatting, private-item rustdoc, dependency
audit and regenerated changelog verification also passed. The error-swallowing
revert compiled and failed the diagnostic regression at runtime (exit 101);
restored source passed. Native Windows coverage remains a separate CI receipt
for this append-only correction, not a claim made from these Linux results.

That candidate's [Windows run 38022248758](https://github.com/unbraind/pm-rust/actions/runs/38022248758)
passed the exclusive-handle unit fixture but failed the SDK fixture's exact-path
assertion. `Workspace::discover` canonicalizes the storage root, while the
temporary directory can retain a different Windows prefix spelling. The SDK
fixture now derives its lock path from `Workspace::pm_root()` instead of
comparing those spellings. This preserves the exact sharing-violation assertion
and changes no production behavior. The corrected fixture passes the linked
consumer regression, Windows cross-check and strict target Clippy. The latest
append's native CI verifies all full aggregate gates independently of the local
receipts above.
