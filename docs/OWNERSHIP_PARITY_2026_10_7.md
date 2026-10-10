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

Both commands accept `--message`, `--force`, and the native `--timestamp`
intended for deterministic fixtures. Ownership timestamps must be valid UTC
RFC 3339 instants no later
than the real current UTC instant; future values fail before lock acquisition
or journal recovery.
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

## Future ownership timestamp boundary (2026-10-10)

[Review finding 4216462799](https://github.com/unbraind/pm-rust/pull/68#discussion_r4216462799)
was independently reproduced against `6bf5334f5ef8528d41f42242828ddf956bb659e0`.
Syntax validation accepted a future timestamp, and both ownership acquisition
and its diagnostic used that caller clock to calculate lock age. Force or a
permissive stale-recovery policy could therefore unlink a fresh lock, acquire
a replacement, and mutate its item while the original `ItemLock` guard remained
alive. This was a runtime failure, not just a static-analysis conclusion.

This timestamp is a **Rust extension**. The installed published **2026.10.7**
contract in `dist/sdk/lifecycle/claim.d.ts` defines `ClaimMutationOptions` and
its alias `ReleaseMutationOptions` without a timestamp field. Their shared
`GlobalOptions`, in `dist/core/shared/command-types.d.ts`, also has no timestamp.
The differential driver pins the published recipe and JavaScript clock for
testing; that mechanism does not establish a public ownership timestamp option.
No upstream core issue or whole-CLI compatibility claim follows from this bug.

After shared author/syntax validation, ownership compares the parsed instant
with `time::OffsetDateTime::now_utc()` and returns the existing
`InvalidMutationRequest` variant with `timestamp must not be in the future`.
The pinned crate is **time 0.3.55**; its source confirms `now_utc()` returns an
`OffsetDateTime` and its ordering compares instants. The comparison retains
fractional precision and adds no clock tolerance. Historical timestamps and
the automatic current timestamp remain accepted. Other mutation slices retain
their timestamp contracts. The correction precedes acquisition, stale cleanup,
and recovery, and changes no ownership policy or ordinary refusal envelope.

Six `future_ownership_*_preserves_held_lock` regressions acquire an actual
fresh `ItemLock` through production acquisition, retain that guard, and call
the public SDK's claim or release. They cover explicit force under the strict
preset, the minimal preset without force, and custom
`force_required_for_stale_lock=false` without force. Every case snapshots and
compares **item, history, and lock bytes** while the holder is still alive.
It then checks the historical-clock lock-conflict envelope, the same unchanged
bytes, token-preserving guard cleanup, and a successful historical mutation
after release. The consumer regression additionally verifies SDK and CLI
future refusals with and without force when no lock exists, and historical
and automatic-clock success. No lock or clock implementation is mocked.

Fail-on-revert proof: retain the final parser, public types and unchanged
regressions, remove only the future-instant guard in `ownership_item`, then run:

```sh
cargo +1.90.0 test --locked --lib future_ownership_
cargo +1.90.0 test --locked --test mutation_contract future_ownership_
```

Both commands must compile and fail at runtime: all six held-lock tests detect
changed durable bytes and the removed lock; the consumer test observes a
successful mutation instead of the typed refusal. Restore the production file
byte-for-byte and require both commands to pass. This checks dependency on the
correction while leaving the implementation of ownership and its normal-clock
tests available as a positive control.

The boundary prevents caller-driven fast-forwarding of a fresh lock's age.
Existing TTL expiration and stale recovery still apply; this correction adds
no lease renewal or process-liveness test for an expired holder. The original
six two-process races, exact differential clocks, byte comparisons, coverage
inventory, exclusions and four 100% thresholds remain unchanged. The local and
native-platform release gates measure their own source denominators; earlier
Windows diagnostic receipts above belong to their recorded heads.

The shared oracle runner clears its environment to make fixtures reproducible.
It now sets the published CLI's `DO_NOT_TRACK=1` process opt-out after that clear,
so synthetic commands cannot enqueue or deliver host telemetry. This changes
neither the recipe clock nor a compared item, history, success/refusal envelope,
or race assertion. Tracker and aggregate verification commands also run with
the opt-out. The first aggregate was stopped after discovering this boundary
and is not a passing receipt. Earlier invocations were not explicitly isolated
from host telemetry, so they provide no no-telemetry assurance. Existing
telemetry data is preserved; no cleanup or production-state rewrite is used to
claim isolation retroactively.

Final local correction receipts: both PM-linked `just release-check` aggregates
passed with telemetry opt-outs, including the disposable-HOME run with
global/system Git configuration disabled. Each ran **226 ordinary and 226
instrumented tests**, including the four required ownership differentials and
six process races. Both freshly generated reports cover **3,372/3,372 lines,
5,098/5,098 regions, 291/291 functions and 698/698 branches**, all **100%**.
Formatting, strict Clippy, private-item rustdoc, Cargo audit and regenerated
changelog verification passed. Source and test digests match both receipts.

LLVM reports the ten executable source files; `src/lib.rs` contains declarations
and re-exports, and `src/error.rs` declares the derived error enum, so they add
no executable source spans to that report. Tests and dependencies remain
outside the existing production-source denominator. No coverage configuration,
threshold, source inventory, or exclusion was changed. Native-platform CI is
a separate receipt at the pushed head; Windows cross-compilation alone does
not establish native execution or coverage.
