# pm-rust release and changelog targets.
#
# Since pm-rust has no package.json, these justfile recipes expose the
# pm-changelog tooling that every other fleet package provides via npm
# scripts. They are runnable in CI and documented in the README.
#
# The `--date` parameter is fixed so the generated changelog heading does not
# depend on the clock. This prevents CI from failing on a random day in a
# random timezone (see pm-rl-yxhe).

# The fixed heading date for the current release version.
# Update this when cutting a new release; the Unreleased section never
# carries a date.
CHANGELOG_DATE := "2026-08-07"

# The pm-changelog npm package version used by the fleet.
PM_CHANGELOG_PKG := "pm-changelog@2026.8.6"

# The item URL base for changelog links.
ITEM_URL_BASE := "https://github.com/unbraind/pm-rust/blob/main/.agents/pm"

# Reads the crate version from Cargo.toml (parsed, never regex-replaced).
crate-version := `sed -n 's/^version *= *"\([^"]*\)"/\1/p' Cargo.toml`

# Runs pm-changelog with the common flags shared by all recipes.
_pm-changelog *flags:
    npx --yes --package={{PM_CHANGELOG_PKG}} -- pm-changelog \
        --pm-root .agents/pm \
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
# CHANGELOG_DATE is forwarded explicitly: a nested `just` starts a new process
# and does NOT inherit a command-line override, so without this a release job
# running `just CHANGELOG_DATE=<release date> release-check` would verify against
# the hardcoded default and reject the changelog it had just generated.
changelog-check:
    just CHANGELOG_DATE={{CHANGELOG_DATE}} changelog-full --check

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
release-check:
    cargo +1.90.0 fmt --all -- --check
    cargo +1.90.0 clippy --locked --all-targets --all-features -- -D warnings
    RUSTDOCFLAGS='--document-private-items -D missing-docs' cargo +1.90.0 doc --locked --all-features --no-deps
    cargo +1.90.0 test --locked --all-targets --all-features
    cargo +nightly-2026-08-06 llvm-cov --locked --branch --all-targets --all-features --json --output-path coverage-branch.json
    jq -e '.data[0].totals.lines.percent == 100 and .data[0].totals.functions.percent == 100 and .data[0].totals.regions.percent == 100 and .data[0].totals.branches.percent == 100' coverage-branch.json
    cargo audit
    just CHANGELOG_DATE={{CHANGELOG_DATE}} changelog-check