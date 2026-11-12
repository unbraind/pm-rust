# Round-six CI corrections

This round is limited to the two CI failures on `9123e7e`. There are no new
review findings or production behavior changes. The published oracle remains
locked to `2026.11.11`.

## Windows item paths

The assertion in `round5_invalid_utf8_blocker_policy_matches_published_bytes`
compares the SDK oracle context on the left with the native context on the right.
The job log shows the SDK retaining the caller's Windows short-name spelling;
native already reports the extended canonical spelling. Native discovery calls
`fs::canonicalize`, whereas the published SDK's item-store reader reports the
located path derived from its supplied project root.

The native diagnostic keeps its stable canonical path. The comparison uses one
shared test helper, `canonical_item_path_context`, which calls the filesystem's
`fs::canonicalize` on each `context.item_path`. This resolves actual aliases,
including short names and extended paths, without stripping prefixes or assuming
that similar strings identify the same file. Only that field is replaced in a
cloned context. Every other context field, exit status, complete CLI stdout and
stderr bytes, and durable no-write snapshot retain their existing assertions.

`round6_item_path_comparison_preserves_other_refusal_fields` uses real files in a
non-git directory. A lexical alias and an already-canonical path must agree;
another physical file must differ. Changes to code, item ID, byte length, empty
state and detail must remain visible. Missing/non-string paths and nonexistent
files refuse normalization. The original invalid bytes remain unchanged.

## Aggregate branch coverage

Ubuntu job `131002709747` reports:

| Gated dimension | Covered / total | Percent |
| --- | ---: | ---: |
| Lines | 18740 / 18740 | 100% |
| Functions | 1921 / 1921 | 100% |
| Regions | 30534 / 30534 | 100% |
| Branches | 4253 / 4254 | 99.97649271274095% |

The diagnostic identifies `src/main.rs`. The missing branch comes from recovery
bootstrap scanning in `invocation_scope`, rather than an operating-system-only
implementation. CI supplies `RUST_TEST_THREADS=2` through its environment and
passes no test-runner arguments. Local round-five commands passed
`--test-threads=2`, which incidentally exercised the unrecognised-bootstrap-flag
fallthrough when unit tests captured their process arguments.

Without that incidental argument, the unit function instance misses the
fallthrough. The native instance reaches that path in the complete suite but
never sees a leading argument delimiter. Complementary coverage in different
function instances does not make the reported function complete.

`round6_bootstrap_scope_is_independent_of_harness_arguments` supplies all cases
explicitly: empty input, an ignored native control, delimiter and positional
boundaries, a missing workspace operand, and both workspace spellings following
the ignored control. The fresh-profile unit instance changes from **5/6 branch
outcomes and 30/31 regions** to **6/6 and 31/31**, without any harness argv.

No production source, coverage threshold, workflow, exclusion, dependency, oracle
pin or release-gate command changes. The release aggregate still requires 100%
lines, functions, regions and branches. CI certifies the complete aggregate and
Windows execution; this Linux host supplies the focused behavioral and clean
profile evidence.

## Validation and negative controls

The affected targets pass **126 tests**, with zero failures or ignored tests:

| Target | Passed |
| --- | ---: |
| `claim_release_differential` | 42 |
| Binary unit target | 59 |
| `cli_dispatch` | 25 |

The command forms are:

```sh
PM_RUST_REQUIRE_PUBLISHED_CLI=1 CARGO_BUILD_JOBS=2 RUST_TEST_THREADS=2 timeout 1200 cargo +1.90.0 test --locked -j 2 --test claim_release_differential
CARGO_LLVM_COV_TARGET_DIR=target/round6-cov CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2 RUST_TEST_THREADS=2 PM_RUST_REQUIRE_PUBLISHED_CLI=1 timeout 1200 cargo +nightly-2026-08-06 llvm-cov --branch --json --output-path target/round6-clean-fixed.json --locked -j 2 --bin pm-rust --test cli_dispatch
```

The coverage run uses a separate target directory and fresh profiles. It supplies
the thread bound through the environment, matching CI, with no test-runner argv.
The targeted report establishes all six branch outcomes and all 31 regions of
the affected unit function. It is not a replacement for the complete aggregate
report, which remains for CI at all four unchanged 100% thresholds.

| Negative control in a disposable copy | Negative result | After restoration |
| --- | --- | --- |
| Replace the path comparison helper with its prior identity-comparison behavior | New alias regression fails at an assertion | Both focused regressions pass |
| Prematurely return at an ignored native bootstrap control | New scope regression fails at an assertion | Both focused regressions pass |
| Revert only the added scope test to the prior test file, using fresh profiles and no argv | 58 binary tests pass, but the affected coverage gate fails: 5/6 outcomes, 30/31 regions | 59 binary tests pass; the gate passes at 6/6 and 31/31 |

The third control is the literal reversal of the coverage-only fix; the second
additionally proves the new test detects incorrect behavior. The first restores
the previous comparison behavior without filesystem alias resolution. No
compilation failure is counted as proof. All scratch changes were restored;
the reviewed checkout never contained a reverted implementation.

Every invocation has a 1200-second cap and at most two build jobs or test threads.
Full-suite aggregate runs remain for CI because the previously measured normal
and instrumented passes exceed the permitted local window. Linux local evidence
does not certify the Windows runner; its original regression remains enabled for
fresh CI.

Rust typecheck, clippy with warnings denied, fmt, and private-item documentation
also pass:

```sh
timeout 1200 cargo +1.90.0 check --locked -j 2 --all-targets --all-features
timeout 1200 cargo +1.90.0 clippy --locked -j 2 --all-targets --all-features -- -D warnings
timeout 1200 cargo +1.90.0 fmt --all -- --check
RUSTDOCFLAGS='--document-private-items -D missing-docs' timeout 1200 cargo +1.90.0 doc --locked -j 2 --all-features --no-deps
```

After the final tracker write and claim release, `just changelog-full`,
`just changelog-check`, the exact CI health command
`node tests/oracle/launch.mts health --check-only --require-merge-drivers --strict-exit`,
and the seven-test identity target pass. History remains a byte-prefix extension
of the prior head and main wherever present; added-content privacy checks pass.
The parity item remains in progress with its claim released.
