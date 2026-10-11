# TOON 0.6.1 upgrade and candidate assessment

Owner: [pm-rust-guou](https://github.com/unbraind/pm-rust/blob/main/.agents/pm/issues/pm-rust-guou.toon).
Companion: session `pm-cli-website-session-2026-10-10` in the
[companion tracker](https://github.com/unbraind/pm-cli-companion/tree/main/.agents/pm).
The session item was not present on remote main during inspection.

## Codec cause and supported API

[Dependency PR #70](https://github.com/unbraind/pm-rust/pull/70), head
`67ffa1bc9288f733b45b5d41c459ddfd284e1e9b`, fails all six Rust gates in
[run 37883277216](https://github.com/unbraind/pm-rust/actions/runs/37883277216).
The crate compiles; the failure is the old assertion that a quoted `0.` row
must retain quotes. The installed 0.6.1 source implements TOON v4.1's
normative number grammar: decoding `0.` now returns a string. Non-strict mode
uses that same grammar. Removed coercion options cannot restore the old probe.

Use the public `toon_format::needs_quoting(value, ',')` API for quote safety.
The encoder grammar also protects leading zeros such as `05`, even though the
decoder reads them as strings. A decoder probe would strip those quotes and
diverge from the published encoder. The published 2026.10.9 artifact confirms
`05` is quoted and `0.` is unquoted. The unit assertion is explicitly corrected
to that measured behavior; no published output is rewritten or normalized.
Strict item decoding, the read-side empty-array dialect adapter and exact
history hashing remain enforced. The 0.6.1 encoder already emits `field: []`,
so the obsolete `[0]:` write translation is removed. A direct empty-tag
regression and the existing complete-byte fixtures enforce canonical storage.

The supported quoting API is the entire scalar rule. The old additional ASCII
whitelist duplicated an obsolete decoder-era grammar; v4.1 encoder output
could not reach its closures. The first aggregates correctly rejected
unreachable legacy encoder paths. Removing that whitelist aligns the rule with
the inspected published encoder, rather than maintaining a second grammar or
changing coverage thresholds.

The npm registry latest tag is 2026.10.9 and the crate latest is 0.6.1 at
inspection. The committed oracle manifest and lock, compatibility constant,
changelog SDK pin and guarded release tooling use 2026.10.9. The changelog
generator remains pinned to 2026.9.25. No release workflow is invoked.

The new numeric-string differential creates numeric-looking tags, updates
scalar metadata, and appends numeric-looking tabular comments. It compares
complete item and history bytes after each operation using both real CLIs.
Original mutation and list golden fixtures are unchanged. The existing strict
read tests continue to reject malformed TOON.

Native-only fail-on-revert proof: restore the old decoder-based quote-safety
implementation while retaining 0.6.1, the 2026.10.9 oracle and the new tests.
`cargo test --locked --test conformance_differential numeric_string_quoting_matches_published_item_and_history_bytes`
compiles, then fails at runtime with exit 101 on the numeric-looking metadata
item bytes. Restoring the supported quoting API passes the same test. The
PM-linked complete differential passes both tests against the locked oracle.

Final Linux `pm test pm-rust-guou --run --match 'just release-check' --progress`
passes: formatting, strict all-target/all-feature Clippy, private-item rustdoc,
208 ordinary tests and 208 instrumented tests with the oracle required,
dependency audit and pinned changelog verification. Source coverage is exactly
3050/3050 lines, 4612/4612 regions, 274/274 functions and 610/610 branches.
No coverage exclusions or thresholds changed. Windows GNU all-target/all-feature
cross-compilation passes through its linked PM command. Native macOS/Windows
execution and substantive exact-head review remain separate CI/review evidence.
Changelog regeneration leaves its bytes unchanged because these items remain
active for orchestrator verification.

Two wider synthetic tag defects remain independent follow-up work in
[pm-rust-mttg](https://github.com/unbraind/pm-rust/blob/main/.agents/pm/issues/pm-rust-mttg.toon):
punctuation-bearing tag order (`+1,-0,.5`) and indexed history patches for
array replacement. The codec fixture creates its tag array directly and tests
punctuation-bearing strings as scalar fields; it does not conceal those gaps
by discarding history fields.

A second independent fixed-clock finding is tracked in
[pm-rust-2ucz](https://github.com/unbraind/pm-rust/blob/main/.agents/pm/issues/pm-rust-2ucz.toon):
two equal-timestamp comments are appended in native order but reordered by the
published CLI. The codec regression uses one comment containing both quoting
categories so it isolates scalar behavior without suppressing any output bytes.

## Existing candidate assessment

[Native ownership PR #68](https://github.com/unbraind/pm-rust/pull/68), head
`ea29da3aa8265b597595f0ed9a3c95be08da0dbb`, passes all ordinary platform
tests and the Linux/macOS aggregates. Its Windows aggregate in
[run 37709763894](https://github.com/unbraind/pm-rust/actions/runs/37709763894)
fails coverage: 3362/3363 lines and 5085/5087 regions, with all 290 functions
and 696 branches covered. Missing regions are `mutation.rs` 1828:89-90 and
1889:14-15: the lock-refusal read error propagation and its propagation through
the ownership operation. Both lie on the same diagnostic failure chain. A
Windows read-sharing refusal can cover both; an invalid payload fails earlier
in acquisition and does not exercise the second region. This is a separate
Windows filesystem-fixture gap.

[List parity PR #67](https://github.com/unbraind/pm-rust/pull/67), head
`f74e30432be57c3af3fc9c2c5c2e6fef835798d4`, passes all six Rust gates and
CodeQL in [run 37709208698](https://github.com/unbraind/pm-rust/actions/runs/37709208698).
There are no review threads or approving reviews. CodeRabbit's success comes
with an explicit automatic-review skip; Sourcery is quota-limited/skipped,
Greptile's exact-head comment says its trial ended, and cubic is neutral.
Those records do not establish code review approval. GitHub reports the
candidate behind main, and main requires strict base currency plus conversation
resolution. It remains open pending base currency and substantive exact-head
review; no merge is performed.
