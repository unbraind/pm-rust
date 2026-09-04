# pm-rust release and changelog targets.
#
# Since pm-rust has no package.json, these justfile recipes expose the
# pm-changelog tooling that every other fleet package provides via npm
# scripts. They are runnable in CI and documented in the README.
#
# The `--date` parameter is fixed so the generated changelog heading does not
# depend on the clock. This prevents CI from failing on a random day in a
# random timezone (see pm-rl-yxhe).
#
# The fixed date is also the SINGLE source of truth for verification:
# `changelog-check` deliberately runs in a fresh `just` process without
# forwarding any command-line CHANGELOG_DATE override, so it always verifies
# against this constant. A contributor who regenerates the changelog with a
# private override and commits it gets a red gate locally too, instead of a
# local-green/CI-red divergence (pm-rust-1ps2). The one legitimate override is
# a release run, which rewrites this constant to the release date BEFORE
# generating, so the committed state stays self-consistent and plain
# `just release-check` passes everywhere without any override.

# The fixed heading date for the current release version.
# Update this when cutting a new release; the Unreleased section never
# carries a date.
CHANGELOG_DATE := "2026-08-07"

# The pm-changelog npm package version used by the fleet, pinned together
# with the exact pm CLI/SDK build it resolves. pm-changelog declares its SDK
# as a floating range (>=2026.8.3), so an unpinned install resolves to latest
# and its tracker reads silently truncate under the newer output-budget
# contract — dropping closed items from regeneration while the committed
# CHANGELOG.md keeps them (pm-rust-1ps2). Both packages move together, and a
# bump here must regenerate CHANGELOG.md in the same change.
PM_CHANGELOG_PKG := "pm-changelog@2026.8.22"
PM_CLI_PKG := "@unbrained/pm-cli@2026.9.4"

# The item URL base for changelog links.
ITEM_URL_BASE := "https://github.com/unbraind/pm-rust/blob/main/.agents/pm"

# Reads the crate version from Cargo.toml (parsed, never regex-replaced).
crate-version := `sed -n 's/^version *= *"\([^"]*\)"/\1/p' Cargo.toml`

# Runs pm-changelog with the common flags shared by all recipes.
_pm-changelog *flags:
    npx --yes --package={{PM_CHANGELOG_PKG}} --package={{PM_CLI_PKG}} -- pm-changelog \
        --pm-root .agents/pm \
        --pm-arg=--output-budget --pm-arg=unbounded \
        --pm-arg=--output-limit --pm-arg=unbounded \
        --item-url-base {{ITEM_URL_BASE}} \
        --respect-item-release \
        --conventional \
        --include-links \
        --item-ref-style toon \
        --include-metadata \
        {{flags}}

# Regenerate the full changelog from all release tags (prepend mode).
changelog:
    just _pm-changelog \
        --mode prepend \
        --output CHANGELOG.md \
        --release-version "{{crate-version}}" \
        --date {{CHANGELOG_DATE}} \
        --since-previous-tag \
        --until-release-tag

# Rebuild the complete changelog from all release tag windows.
changelog-full *extra:
    just _pm-changelog \
        --mode replace \
        --output CHANGELOG.md \
        --all-release-tags \
        --release-version "{{crate-version}}" \
        --date {{CHANGELOG_DATE}} \
        {{extra}}

# Verify the committed changelog matches the generated one (CI gate).
# Runs in a fresh `just` process WITHOUT forwarding CHANGELOG_DATE: whatever a
# caller overrides on the command line, this check verifies against the
# canonical pinned constant above. That is what makes the gate deterministic
# across machines and dates — the committed file and the verified generation
# can no longer disagree about the date (pm-rust-1ps2). A release run keeps
# them consistent by rewriting the constant before it generates.
changelog-check:
    just changelog-full --check

# Print release notes for the current release window to stdout.
release-notes:
    just _pm-changelog \
        --stdout \
        --all-release-tags \
        --release-version "{{crate-version}}" \
        --date {{CHANGELOG_DATE}} \
        --github-step-summary

# The aggregate release gate: build, clippy (deny warnings), fmt check,
# full test suite with coverage, docstring/doc-coverage gate, identity
# audit, and changelog check.
# The differential suites skip when no published Node CLI is discoverable, so a
# release-check that does not demand one passes while proving nothing about
# conformance — the same vacuous-pass this repository just fixed in CI. Requiring
# it here means every path that gates a release enforces the comparison, not only
# the CI test job.
release-check:
    @test -n "${PM_NODE_CLI:-}" || command -v pm >/dev/null 2>&1 || \
      { echo "release-check needs the published pm CLI: set PM_NODE_CLI or install {{PM_CLI_PKG}}" >&2; exit 1; }
    cargo +1.90.0 fmt --all -- --check
    cargo +1.90.0 clippy --locked --all-targets --all-features -- -D warnings
    RUSTDOCFLAGS='--document-private-items -D missing-docs' cargo +1.90.0 doc --locked --all-features --no-deps
    PM_RUST_REQUIRE_PUBLISHED_CLI=1 cargo +1.90.0 test --locked --all-targets --all-features
    cargo +nightly-2026-08-06 llvm-cov --locked --branch --all-targets --all-features --json --output-path coverage-branch.json
    jq -e '.data[0].totals.lines.percent == 100 and .data[0].totals.functions.percent == 100 and .data[0].totals.regions.percent == 100 and .data[0].totals.branches.percent == 100' coverage-branch.json
    cargo audit
    just changelog-check