//! Contract tests for the release pipeline in `.github/workflows/`.
//!
//! The four defects these tests lock out were all invisible to `cargo test`
//! on a working copy, because they lived entirely in workflow YAML or in the
//! relationship between a workflow and the repository's other pinned files:
//!
//! * **pm-rust-b9yi** — the release workflow pushed the version bump straight
//!   to protected `main` (`git push origin HEAD:main`), so GitHub rejected
//!   the push with `GH006` and `set -e` killed the job before the tag push:
//!   a partial release. The tag can only ever land after the protected merge.
//! * **pm-rust-wbvn** — the release job stamped the zero-padded git tag
//!   (`v2026.08.08`) verbatim into `Cargo.toml`, which Cargo rejects with
//!   `invalid leading zero in minor version number`, and it ran
//!   `just release-check` while installing only `just`.
//! * **pm-rust-cu3d** — workflow checkouts were depth-1 clones, so the
//!   identity audit walked almost no history (passing vacuously) and the
//!   changelog gate could not see the tag refs it derives its window from.
//! * **pm-rust-r9r2** — the release commit's GitHub Actions bot identity was
//!   missing from the approved allowlist, so the first release would have
//!   written a commit that this repository's own audit rejects on every
//!   subsequent run.
//! * **pm-rust-1ps2** — the changelog gate verified against whatever
//!   `CHANGELOG_DATE` happened to reach it, so a contributor who regenerated
//!   with a private override got a green gate locally and a red gate in CI
//!   with byte-identical inputs. The check now always verifies against the
//!   canonical pinned constant, and the release flow rewrites that constant
//!   before generating, so the committed state is self-consistent everywhere.
//!
//! The workflows themselves are parsed as YAML — not grepped as text — so the
//! assertions survive comment edits and reformatting but fail when any of the
//! structural guarantees above is reverted. Where a guarantee spans files
//! (the justfile's nightly, the in-tree toolchain pin, the identity
//! allowlist), both sides of the relationship are read from disk so the test
//! fails on a divergence in either direction.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::process::Command;

use serde_yaml_ng::Value;

/// A boxed dynamic error used by these tests.
type BoxError = Box<dyn std::error::Error>;

/// One workflow step with the fields the contract tests inspect.
struct Step {
    /// The step's `name`, falling back to its `uses` action reference.
    label: String,
    /// The action the step `uses`, when it is a composite-action step.
    uses: Option<String>,
    /// The inline `run` script, when the step runs shell commands.
    run: Option<String>,
    /// The step's `with` mapping, when present.
    with: Option<Value>,
}

/// Reads a repository file, anchored at the crate manifest directory.
fn read_repo_file(relative: &str) -> Result<String, BoxError> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(relative);
    std::fs::read_to_string(&path)
        .map_err(|e| -> BoxError { format!("{}: {e}", path.display()).into() })
}

/// Parses a workflow file into a YAML value.
fn parse_workflow(relative: &str) -> Result<Value, BoxError> {
    serde_yaml_ng::from_str(&read_repo_file(relative)?)
        .map_err(|e| -> BoxError { format!("{relative}: {e}").into() })
}

/// Looks up a key in a YAML mapping value.
fn map_get<'a>(value: &'a Value, key: &str) -> Option<&'a Value> {
    value.as_mapping()?.get(Value::String(key.to_owned()))
}

/// Collects every job of a workflow with its ordered steps.
///
/// Step order is preserved because a YAML sequence is a list; the release
/// transaction tests depend on that ordering being observable.
fn workflow_jobs(workflow: &Value) -> Result<Vec<(String, Vec<Step>)>, BoxError> {
    let jobs = map_get(workflow, "jobs")
        .ok_or("workflow declares no jobs")?
        .as_mapping()
        .ok_or("jobs is not a mapping")?;
    let mut collected = Vec::new();
    for (key, job) in jobs {
        let job_name = key.as_str().ok_or("job key is not a string")?.to_owned();
        let steps_yaml = map_get(job, "steps")
            .ok_or_else(|| format!("job {job_name} declares no steps"))?
            .as_sequence()
            .ok_or_else(|| format!("job {job_name} steps is not a sequence"))?;
        let mut steps = Vec::new();
        for step in steps_yaml {
            let uses = map_get(step, "uses")
                .and_then(Value::as_str)
                .map(str::to_owned);
            let run = map_get(step, "run")
                .and_then(Value::as_str)
                .map(str::to_owned);
            let with = map_get(step, "with").cloned();
            let label = map_get(step, "name")
                .and_then(Value::as_str)
                .map(str::to_owned)
                .or_else(|| uses.clone())
                .unwrap_or_else(|| "<unnamed step>".to_owned());
            steps.push(Step {
                label,
                uses,
                run,
                with,
            });
        }
        collected.push((job_name, steps));
    }
    Ok(collected)
}

/// Returns the ordered steps of a single named job.
fn job_steps(workflow: &Value, job_name: &str) -> Result<Vec<Step>, BoxError> {
    workflow_jobs(workflow)?
        .into_iter()
        .find(|(name, _)| name == job_name)
        .map(|(_, steps)| steps)
        .ok_or_else(|| format!("job {job_name} not found").into())
}

/// Returns the `run` script of the step with the given name.
fn step_run<'a>(steps: &'a [Step], step_name: &str) -> Result<&'a str, BoxError> {
    steps
        .iter()
        .find(|step| step.label == step_name)
        .and_then(|step| step.run.as_deref())
        .ok_or_else(|| format!("step '{step_name}' has no run script").into())
}

/// Folds shell backslash continuations so each element is one logical command.
///
/// The release workflow already writes `git push` across continuation lines,
/// so scanning physical lines would let `git push \` followed by
/// `origin HEAD:main` pass every refspec check: the first physical line
/// carries no refspec and the second carries no `git push`. Folding first is
/// what keeps [`line_pushes_to_main`] from holding vacuously.
fn logical_lines(script: &str) -> Vec<String> {
    let mut folded: Vec<String> = Vec::new();
    let mut pending = String::new();
    for line in script.lines() {
        let trimmed = line.trim_end();
        if let Some(head) = trimmed.strip_suffix('\\') {
            pending.push_str(head);
            pending.push(' ');
            continue;
        }
        pending.push_str(trimmed);
        folded.push(std::mem::take(&mut pending));
    }
    if !pending.is_empty() {
        folded.push(pending);
    }
    folded
}

/// Whether one logical script line is a `git push` that writes to `main`
/// directly.
///
/// Matches both refspec forms (`HEAD:main`, `HEAD:refs/heads/main`) and the
/// bare branch-name form (`git push origin main`). Pushing `HEAD` to a
/// release branch or pushing a tag refspec is deliberately not a match.
/// Callers must pass output of [`logical_lines`], never a raw physical line.
fn line_pushes_to_main(line: &str) -> bool {
    if !line.contains("git push") {
        return false;
    }
    let tokens: Vec<&str> = line.split_whitespace().collect();
    tokens
        .iter()
        .any(|token| matches!(*token, "HEAD:main" | "HEAD:refs/heads/main"))
        || tokens.iter().any(|token| token.trim_matches('"') == "main")
}

/// Extracts the awk program that strips leading zeroes from a version.
///
/// Both the stamp step and the verify step embed the same `awk -F- '...'`
/// program; `None` means the step lost its conversion entirely.
fn leading_zero_awk_program(script: &str) -> Option<&str> {
    let marker = "awk -F- '";
    let start = script.find(marker)? + marker.len();
    let rest = &script[start..];
    let end = rest.find('\'')?;
    Some(&rest[..end])
}

/// Whether Cargo accepts a crate whose manifest carries the given version.
///
/// Runs the real `cargo verify-project` against a throwaway manifest, so the
/// tests assert Cargo's own version rules rather than a restatement of them.
fn cargo_accepts_version(version: &str) -> Result<bool, BoxError> {
    let dir = tempfile::tempdir()?;
    std::fs::create_dir_all(dir.path().join("src"))?;
    std::fs::write(dir.path().join("src/main.rs"), "fn main() {}\n")?;
    let manifest = format!(
        "[package]\nname = \"release-contract-probe\"\nversion = \"{version}\"\nedition = \"2021\"\n"
    );
    std::fs::write(dir.path().join("Cargo.toml"), manifest)?;
    let output = Command::new("cargo")
        .arg("verify-project")
        .arg("--manifest-path")
        .arg(dir.path().join("Cargo.toml"))
        .output()
        .map_err(|e| -> BoxError { format!("failed to run cargo: {e}").into() })?;
    Ok(output.status.success())
}

/// pm-rust-b9yi: no step in the release workflow may push directly to the
/// protected `main` branch.
///
/// `main` is protected across the fleet (required checks, enforce-admins,
/// required conversation resolution), so any such push is rejected with
/// `GH006` and `set -e` kills the job mid-transaction — the exact failure
/// that killed every daily release on 2026-08-10.
#[test]
fn release_workflow_never_pushes_to_protected_main() -> Result<(), BoxError> {
    let workflow = parse_workflow(".github/workflows/release.yml")?;
    for (job_name, steps) in workflow_jobs(&workflow)? {
        for step in &steps {
            let Some(run) = step.run.as_deref() else {
                continue;
            };
            let offending = offending_push_lines(run);
            assert!(
                offending.is_empty(),
                "job '{job_name}' step '{}' pushes directly to protected main, which branch protection rejects with GH006 (pm-rust-b9yi):\n  {}",
                step.label,
                offending.join("\n  ")
            );
        }
    }
    Ok(())
}

/// Every logical line of `script` that pushes directly to protected `main`.
///
/// The single scan path shared by the workflow contract and its own
/// regression test, so the two cannot disagree: if this stops folding
/// continuations, both fail.
fn offending_push_lines(script: &str) -> Vec<String> {
    logical_lines(script)
        .into_iter()
        .filter(|line| line_pushes_to_main(line))
        .collect()
}

/// pm-rust-b9yi: the push scanner must survive the continuation form the
/// release workflow already uses.
///
/// Without folding, `git push \\` and `origin HEAD:main` are two physical
/// lines and neither matches on its own — the first carries no refspec, the
/// second carries no `git push` — so the contract above would report a clean
/// workflow while it pushed straight to protected `main`. This test fails if
/// the fold is removed, which is the only thing that makes the guard real.
#[test]
fn split_push_to_main_is_still_detected() {
    let split = "git push \\\n  origin HEAD:main\n";
    assert!(
        !split.lines().any(line_pushes_to_main),
        "precondition: neither physical line matches on its own, which is why folding is required"
    );
    assert_eq!(
        offending_push_lines(split).len(),
        1,
        "the shared scan must expose a push to main written across continuation lines"
    );

    let branch = "git push \\\n  --force-with-lease=\"refs/heads/release:abc\" \\\n  origin \"HEAD:refs/heads/release\"\n";
    assert!(
        offending_push_lines(branch).is_empty(),
        "a folded push to a release branch must not be mistaken for a push to main"
    );

    let tag = "git push \\\n  origin \"refs/tags/v1.2.3\"\n";
    assert!(
        offending_push_lines(tag).is_empty(),
        "a folded tag push must not be mistaken for a push to main"
    );
}

/// Every history-dependent checkout defect in one job's steps.
///
/// Checks **every** `actions/checkout` step, not only the first: a job that
/// re-checks out the source after another step, or checks out a second
/// repository, would otherwise keep the contract green while the effective
/// working tree is shallow. An empty result means every checkout in the job
/// fetches full history and tags.
///
/// This is the single scan path shared by the workflow contract and its own
/// regression test, so the two cannot disagree.
fn shallow_checkout_problems(steps: &[Step]) -> Vec<String> {
    let checkouts: Vec<&Step> = steps
        .iter()
        .filter(|step| {
            step.uses
                .as_deref()
                .is_some_and(|uses| uses.starts_with("actions/checkout"))
        })
        .collect();
    if checkouts.is_empty() {
        return vec!["has no checkout step".to_owned()];
    }
    let mut problems = Vec::new();
    for checkout in checkouts {
        let with = checkout.with.as_ref();
        let fetch_depth = with
            .and_then(|with| map_get(with, "fetch-depth"))
            .and_then(Value::as_u64);
        let fetch_tags = with
            .and_then(|with| map_get(with, "fetch-tags"))
            .and_then(Value::as_bool);
        if fetch_depth != Some(0) {
            problems.push(format!(
                "checkout '{}' uses fetch-depth {fetch_depth:?}; a shallow clone makes the identity audit vacuous and breaks the changelog gate",
                checkout.label
            ));
        }
        if fetch_tags != Some(true) {
            problems.push(format!(
                "checkout '{}' sets no fetch-tags; pm-changelog --all-release-tags derives its release window from tag refs",
                checkout.label
            ));
        }
    }
    problems
}

/// pm-rust-cu3d: a second, shallower checkout in the same job must be caught.
///
/// The scan originally inspected only the first `actions/checkout` step, so a
/// job that re-checked out the source shallowly after its first full checkout
/// kept the contract green with the effective tree shallow. Reverting the scan
/// to the first step only makes this test fail.
#[test]
fn a_later_shallow_checkout_in_the_same_job_is_caught() {
    let deep = |label: &str| Step {
        label: label.to_owned(),
        uses: Some("actions/checkout@v7".to_owned()),
        run: None,
        with: serde_yaml_ng::from_str("fetch-depth: 0\nfetch-tags: true\n").ok(),
    };
    let shallow = Step {
        label: "re-checkout".to_owned(),
        uses: Some("actions/checkout@v7".to_owned()),
        run: None,
        with: serde_yaml_ng::from_str("fetch-depth: 1\nfetch-tags: true\n").ok(),
    };

    assert!(
        shallow_checkout_problems(&[deep("checkout")]).is_empty(),
        "a single full checkout must report no problem"
    );
    let problems = shallow_checkout_problems(&[deep("checkout"), shallow]);
    assert_eq!(
        problems.len(),
        1,
        "the later shallow checkout must be reported, got {problems:?}"
    );
    assert!(
        problems[0].contains("re-checkout"),
        "the report must name the offending step, got {problems:?}"
    );
    assert!(
        shallow_checkout_problems(&[]).len() == 1,
        "a job with no checkout at all is itself a problem"
    );
}

/// pm-rust-b9yi: the release transaction is ordered so the tag can only land
/// after `main` has already advanced through the protected PR merge.
///
/// The invariant, in the order the steps must appear: the merge step cannot
/// run after the verify step, the verify step cannot run after the tag push,
/// and the tag push cannot run after the GitHub release is created. Reversing
/// any pair reintroduces a state where the workflow tags or releases a
/// commit that protected `main` never accepted.
#[test]
fn release_tag_cannot_precede_the_protected_merge() -> Result<(), BoxError> {
    let workflow = parse_workflow(".github/workflows/release.yml")?;

    // The merge route needs pull-request write access and the tag push needs
    // contents write; the GH006 version of the workflow had neither.
    let permissions = map_get(&workflow, "permissions")
        .and_then(Value::as_mapping)
        .ok_or("release workflow declares no permissions mapping")?;
    assert_eq!(
        permissions
            .get(Value::String("contents".into()))
            .and_then(Value::as_str),
        Some("write"),
        "the tag push requires contents: write"
    );
    assert_eq!(
        permissions
            .get(Value::String("pull-requests".into()))
            .and_then(Value::as_str),
        Some("write"),
        "the protected-PR merge requires pull-requests: write"
    );

    let steps = job_steps(&workflow, "release")?;
    let index_of = |name: &str| -> Result<usize, BoxError> {
        steps
            .iter()
            .position(|step| step.label == name)
            .ok_or_else(|| -> BoxError { format!("release job lost its '{name}' step").into() })
    };
    let merge = index_of("Merge release metadata through protected PR")?;
    let verify = index_of("Verify merged release")?;
    let tag = index_of("Push release tag")?;
    let github_release = index_of("Create GitHub release")?;
    assert!(
        merge < verify && verify < tag && tag < github_release,
        "release transaction order must be merge({merge}) -> verify({verify}) -> tag({tag}) -> release({github_release}); the tag push may only run after the protected merge is verified (pm-rust-b9yi)"
    );

    // The tag step must be tag-only: it references a tag refspec and pushes
    // nothing else.
    let tag_run = step_run(&steps, "Push release tag")?;
    assert!(
        tag_run.contains("refs/tags/"),
        "Push release tag no longer pushes a tag refspec"
    );
    for line in tag_run.lines() {
        assert!(
            !line.contains("git push") || line.contains("refs/tags/"),
            "Push release tag must only push tag refspecs, never a branch:\n  {line}"
        );
    }
    Ok(())
}

/// pm-rust-wbvn (version half): the stamp step and the merged-release
/// verification must apply the same leading-zero-stripping conversion.
///
/// If the two copies drift, verification compares the padded tag form against
/// a stripped manifest (or the reverse) and rejects every release.
#[test]
fn stamp_and_verify_share_one_leading_zero_conversion() -> Result<(), BoxError> {
    let workflow = parse_workflow(".github/workflows/release.yml")?;
    let steps = job_steps(&workflow, "release")?;
    let stamp = leading_zero_awk_program(step_run(
        &steps,
        "Stamp crate version in Cargo.toml",
    )?)
    .ok_or("the stamp step lost its leading-zero strip; Cargo rejects padded versions (pm-rust-wbvn)")?;
    let verify = leading_zero_awk_program(step_run(&steps, "Verify merged release")?)
        .ok_or("the verify step lost its leading-zero strip; it would compare mismatched forms and reject every release (pm-rust-wbvn)")?;
    assert_eq!(
        stamp, verify,
        "the stamp and verify steps must convert the tag identically"
    );
    Ok(())
}

/// pm-rust-wbvn (version half, against real Cargo): the strip is necessary,
/// not cosmetic — Cargo itself must still reject the padded form the git tag
/// carries and accept the stripped form the workflow stamps.
#[test]
fn cargo_rejects_padded_versions_and_accepts_stripped_ones() -> Result<(), BoxError> {
    assert!(
        !cargo_accepts_version("2026.08.08")?,
        "Cargo accepts the padded form 2026.08.08; the strip and this contract can both be deleted"
    );
    assert!(
        cargo_accepts_version("2026.8.8")?,
        "Cargo rejects the stripped form 2026.8.8 the workflow stamps"
    );
    assert!(
        cargo_accepts_version("2026.8.8-2")?,
        "Cargo rejects the stripped suffixed form 2026.8.8-2 the workflow stamps on re-releases"
    );
    Ok(())
}

/// pm-rust-wbvn (version half, behavioural): runs the awk program extracted
/// from the live workflow YAML over representative tags and checks every
/// output against real Cargo.
///
/// Unix-only: it executes `awk`, which is not part of the Windows runner
/// image; the workflow itself only ever runs on `ubuntu-latest`.
#[cfg(unix)]
#[test]
fn workflow_awk_strips_padding_the_way_cargo_requires() -> Result<(), BoxError> {
    let workflow = parse_workflow(".github/workflows/release.yml")?;
    let steps = job_steps(&workflow, "release")?;
    let awk_program =
        leading_zero_awk_program(step_run(&steps, "Stamp crate version in Cargo.toml")?)
            .ok_or("the stamp step lost its leading-zero strip (pm-rust-wbvn)")?;

    let cases = [
        ("2026.08.08", "2026.8.8"),
        ("2026.12.05", "2026.12.5"),
        ("2026.8.8", "2026.8.8"),
        ("2026.8.8-2", "2026.8.8-2"),
    ];
    for (tag_version, expected) in cases {
        let dir = tempfile::tempdir()?;
        let input_path = dir.path().join("version.txt");
        std::fs::write(&input_path, tag_version)?;
        let output = Command::new("awk")
            .arg("-F-")
            .arg(awk_program)
            .arg(&input_path)
            .output()
            .map_err(|e| -> BoxError { format!("failed to run awk: {e}").into() })?;
        assert!(
            output.status.success(),
            "the workflow's awk program exited with {} on input {tag_version}",
            output.status
        );
        let converted = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        assert_eq!(
            converted, expected,
            "the workflow's conversion of {tag_version} produced {converted}"
        );
    }

    // On the padded inputs the direction matters: the converted form must be
    // accepted by Cargo while the padded form still is not.
    for padded in ["2026.08.08", "2026.12.05"] {
        let dir = tempfile::tempdir()?;
        let input_path = dir.path().join("version.txt");
        std::fs::write(&input_path, padded)?;
        let output = Command::new("awk")
            .arg("-F-")
            .arg(awk_program)
            .arg(&input_path)
            .output()
            .map_err(|e| -> BoxError { format!("failed to run awk: {e}").into() })?;
        let converted = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        assert!(
            cargo_accepts_version(&converted)?,
            "Cargo must accept the workflow's conversion of {padded} ({converted})"
        );
        assert!(
            !cargo_accepts_version(padded)?,
            "Cargo must still reject the padded tag form {padded}"
        );
    }
    Ok(())
}

/// pm-rust-wbvn (toolchain half): the release job must install every gate
/// tool the CI release-check job installs.
///
/// The release job may install a superset, never a subset: it runs the same
/// `just release-check` aggregate, so a missing tool there fails inside the
/// gate rather than before it — the exact failure this item records.
#[test]
fn release_job_installs_every_gate_tool_ci_installs() -> Result<(), BoxError> {
    let ci = parse_workflow(".github/workflows/ci.yml")?;
    let release = parse_workflow(".github/workflows/release.yml")?;
    let ci_steps = job_steps(&ci, "release-check")?;
    let release_steps = job_steps(&release, "release")?;

    let tools_of = |steps: &[Step]| -> Result<BTreeSet<String>, BoxError> {
        let mut tools = BTreeSet::new();
        for step in steps {
            let Some(uses) = step.uses.as_deref() else {
                continue;
            };
            if !uses.starts_with("taiki-e/install-action") {
                continue;
            }
            let tool = step
                .with
                .as_ref()
                .and_then(|with| map_get(with, "tool"))
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    format!("install-action step '{}' declares no tool list", step.label)
                })?;
            for name in tool.split(',') {
                tools.insert(name.trim().to_owned());
            }
        }
        Ok(tools)
    };
    let ci_tools = tools_of(&ci_steps)?;
    let release_tools = tools_of(&release_steps)?;
    assert!(
        !ci_tools.is_empty(),
        "the CI release-check job installs no taiki-e tools; this parity check would be vacuous"
    );
    for tool in &ci_tools {
        assert!(
            release_tools.contains(tool),
            "the release job runs `just release-check` but does not install `{tool}` (pm-rust-wbvn)"
        );
    }

    Ok(())
}

/// pm-rust-wbvn (toolchain half, pins): the toolchains both gate jobs install
/// must match the ones the repository itself pins.
///
/// This enforces the lockstep note recorded in `rust-toolchain.toml` ("nothing
/// enforces that today"): the stable channel both workflows install equals
/// the in-tree pin, and the nightly used for branch coverage equals the one
/// the justfile names. A divergence in either direction fails here instead of
/// inside a release job.
#[test]
fn workflow_toolchains_match_the_repository_pins() -> Result<(), BoxError> {
    let ci = parse_workflow(".github/workflows/ci.yml")?;
    let release = parse_workflow(".github/workflows/release.yml")?;
    let ci_steps = job_steps(&ci, "release-check")?;
    let release_steps = job_steps(&release, "release")?;

    // The nightly that measures branch coverage is named by the justfile;
    // both workflows must install exactly it, with llvm-tools-preview.
    let justfile = read_repo_file("justfile")?;
    let mut nightlies = Vec::new();
    for line in justfile.lines() {
        // A justfile comment naming a superseded nightly must not count as a
        // second pin, or this assertion fails on a recipe that pins exactly one.
        if line.trim_start().starts_with('#') {
            continue;
        }
        if let Some(at) = line.find("+nightly-") {
            let rest = &line[at + 1..];
            let token = rest.split_whitespace().next().unwrap_or(rest);
            nightlies.push(token.to_owned());
        }
    }
    let unique_nightlies: BTreeSet<&str> = nightlies.iter().map(String::as_str).collect();
    assert_eq!(
        unique_nightlies.len(),
        1,
        "the justfile must pin exactly one nightly for coverage, found {unique_nightlies:?}"
    );
    let nightly = unique_nightlies
        .iter()
        .next()
        .copied()
        .ok_or("the justfile pins no nightly")?;

    let toolchains_of = |steps: &[Step]| -> Result<Vec<(String, Vec<String>)>, BoxError> {
        let mut installed = Vec::new();
        for step in steps {
            let Some(uses) = step.uses.as_deref() else {
                continue;
            };
            if !uses.starts_with("dtolnay/rust-toolchain") {
                continue;
            }
            let with = step
                .with
                .as_ref()
                .ok_or_else(|| format!("toolchain step '{}' declares no with", step.label))?;
            let toolchain = map_get(with, "toolchain")
                .and_then(Value::as_str)
                .ok_or_else(|| format!("toolchain step '{}' pins no toolchain", step.label))?;
            let components = map_get(with, "components")
                .and_then(Value::as_str)
                .map(|list| {
                    list.split(',')
                        .map(|component| component.trim().to_owned())
                        .collect()
                })
                .unwrap_or_default();
            installed.push((toolchain.to_owned(), components));
        }
        Ok(installed)
    };
    for (workflow_name, steps) in [("ci.yml", &ci_steps), ("release.yml", &release_steps)] {
        let installed = toolchains_of(steps)?;
        assert!(
            installed.iter().any(|(toolchain, components)| {
                toolchain == nightly && components.iter().any(|c| c == "llvm-tools-preview")
            }),
            "{workflow_name} must install {nightly} with llvm-tools-preview: `just release-check` measures branch coverage with it"
        );
    }

    // Stable lockstep with the in-tree toolchain pin.
    let toolchain_file = read_repo_file("rust-toolchain.toml")?;
    let channel = toolchain_file
        .lines()
        .find_map(|line| line.strip_prefix("channel = "))
        .ok_or("rust-toolchain.toml declares no channel")?
        .trim()
        .trim_matches('"');
    for (workflow_name, steps) in [("ci.yml", &ci_steps), ("release.yml", &release_steps)] {
        let installed = toolchains_of(steps)?;
        assert!(
            installed.iter().any(|(toolchain, components)| {
                toolchain == channel
                    && components.iter().any(|c| c == "clippy")
                    && components.iter().any(|c| c == "rustfmt")
            }),
            "{workflow_name} must install stable {channel} with clippy and rustfmt, matching rust-toolchain.toml"
        );
    }
    Ok(())
}

/// pm-rust-cu3d: every workflow job that runs the identity audit or the
/// changelog gate must check out the complete history with tag refs.
///
/// A depth-1 clone makes the audit pass without inspecting history and makes
/// `pm-changelog --all-release-tags` fail with `E_MISSING_TAG_HISTORY`.
#[test]
fn auditing_and_changelog_jobs_checkout_full_history() -> Result<(), BoxError> {
    let workflow_names = [".github/workflows/ci.yml", ".github/workflows/release.yml"];
    for workflow_name in workflow_names {
        let workflow = parse_workflow(workflow_name)?;
        for (job_name, steps) in workflow_jobs(&workflow)? {
            let needs_history = steps.iter().any(|step| {
                step.run.as_deref().is_some_and(|run| {
                    run.contains("cargo test")
                        || run.contains("release-check")
                        || run.contains("pm-changelog")
                })
            });
            if !needs_history {
                continue;
            }
            let problems = shallow_checkout_problems(&steps);
            assert!(
                problems.is_empty(),
                "{workflow_name} job {job_name} runs history-dependent gates but {} (pm-rust-cu3d)",
                problems.join("; ")
            );
        }
    }
    Ok(())
}

/// pm-rust-r9r2: every commit identity the release workflow configures must
/// be allowlisted, and the allowlist must carry the justification comment.
#[test]
fn release_commit_identity_is_allowlisted_and_justified() -> Result<(), BoxError> {
    let allowlist_raw = read_repo_file(".github/approved-git-identities.txt")?;
    let allowlist: BTreeSet<&str> = allowlist_raw
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .collect();

    let workflow = parse_workflow(".github/workflows/release.yml")?;
    let steps = job_steps(&workflow, "release")?;
    let mut configured = Vec::new();
    for step in &steps {
        let Some(run) = step.run.as_deref() else {
            continue;
        };
        for line in logical_lines(run) {
            // `git config user.email`, its --global/--local/--system forms, and
            // `git -c user.email=` all set the identity the release commit is
            // authored with. Matching only the bare form would let a scoped
            // form introduce an unallowlisted identity with this test green.
            let trimmed = line.trim();
            let email = ["git config user.email ", "git -c user.email="]
                .iter()
                .find_map(|prefix| trimmed.strip_prefix(prefix))
                .or_else(|| {
                    let rest = trimmed.strip_prefix("git config ")?;
                    ["--global ", "--local ", "--system "]
                        .iter()
                        .find_map(|scope| rest.strip_prefix(scope))
                        .and_then(|rest| rest.strip_prefix("user.email "))
                });
            if let Some(email) = email {
                let email = email
                    .split_whitespace()
                    .next()
                    .unwrap_or("")
                    .trim_matches(|c| c == '"' || c == '\'');
                configured.push((step.label.clone(), email.to_owned()));
            }
        }
    }
    assert!(
        !configured.is_empty(),
        "the release job configures no commit identity; the release commit would fall back to an uncontrolled default (pm-rust-r9r2)"
    );
    for (step_label, email) in &configured {
        assert!(
            allowlist.contains(email.as_str()),
            "release step '{step_label}' commits as {email}, which is not in approved-git-identities.txt; the identity audit would reject the release commit on every subsequent run (pm-rust-r9r2)"
        );
    }
    assert!(
        allowlist_raw
            .lines()
            .any(|line| line.trim_start().starts_with('#') && line.contains("release.yml")),
        "the allowlist must explain in a comment why the release workflow's bot identity is listed (pm-rust-r9r2)"
    );
    Ok(())
}

/// Extracts the body of one justfile recipe as its ordered, trimmed lines.
///
/// A recipe starts at a line whose first word is `name` followed by `:` (with
/// optional arguments before the colon) and spans every following line that is
/// indented deeper than the recipe header. Comment-only lines between recipes
/// are skipped so they cannot terminate or extend a body. `Err` means the
/// recipe is missing entirely, which every caller treats as a contract
/// failure rather than an empty body.
fn justfile_recipe_body(justfile: &str, name: &str) -> Result<Vec<String>, BoxError> {
    let mut lines = justfile.lines().peekable();
    while let Some(line) = lines.next() {
        let trimmed = line.trim_start();
        if trimmed.starts_with('#') || trimmed.is_empty() {
            continue;
        }
        let header = trimmed.split(':').next().unwrap_or(trimmed);
        let recipe_name = header.split_whitespace().next().unwrap_or(header);
        if recipe_name != name || !trimmed.contains(':') {
            continue;
        }
        let mut body = Vec::new();
        for line in lines.by_ref() {
            let trimmed = line.trim_start();
            if trimmed.starts_with('#') && line.starts_with("    #") {
                body.push(trimmed.to_owned());
                continue;
            }
            if trimmed.is_empty() {
                continue;
            }
            if !line.starts_with(char::is_whitespace) {
                break;
            }
            body.push(trimmed.to_owned());
        }
        return Ok(body);
    }
    Err(format!("justfile declares no recipe '{name}'").into())
}

/// pm-rust-1ps2: the changelog gate must verify against the canonical pinned
/// date, never against whatever `CHANGELOG_DATE` a caller happened to set.
///
/// The pre-fix justfile forwarded the variable into the nested check process,
/// so `just CHANGELOG_DATE=X release-check` verified a different file than CI
/// verifies with byte-identical inputs — the local-green/CI-red divergence
/// this item records. Neither `changelog-check` nor `release-check` may carry
/// that forwarding anymore.
#[test]
fn changelog_check_verifies_against_the_canonical_date_only() -> Result<(), BoxError> {
    let justfile = read_repo_file("justfile")?;

    let check = justfile_recipe_body(&justfile, "changelog-check")?;
    assert!(
        !check.iter().any(|line| line.contains("CHANGELOG_DATE=")),
        "changelog-check must not forward a CHANGELOG_DATE override into the verification; that forwarding is the divergence channel (pm-rust-1ps2):\n  {}",
        check.join("\n  ")
    );
    assert!(
        check
            .iter()
            .any(|line| *line == "just changelog-full --check"),
        "changelog-check must verify through a fresh plain `just` process, got:\n  {}",
        check.join("\n  ")
    );

    let release = justfile_recipe_body(&justfile, "release-check")?;
    assert!(
        !release.iter().any(|line| line.contains("CHANGELOG_DATE=")),
        "release-check must not forward a CHANGELOG_DATE override either (pm-rust-1ps2)"
    );
    assert!(
        release.iter().any(|line| *line == "just changelog-check"),
        "release-check must end in the canonical changelog-check recipe"
    );

    // The helper itself must stay honest: a missing recipe is an error, not a
    // silently passing empty scan.
    assert!(
        justfile_recipe_body(&justfile, "no-such-recipe").is_err(),
        "a missing recipe must be reported as an error"
    );
    Ok(())
}

/// pm-rust-1ps2: the legitimate override channel survives — generation still
/// honors an explicit date, only verification ignores it.
///
/// If someone "fixes" the divergence by hardcoding the date into the
/// generation recipes too, a release can no longer stamp its own date and
/// this test fails.
#[test]
fn generation_recipes_still_honor_an_explicit_date_override() -> Result<(), BoxError> {
    let justfile = read_repo_file("justfile")?;
    for recipe in ["changelog", "changelog-full", "release-notes"] {
        let body = justfile_recipe_body(&justfile, recipe)?;
        assert!(
            body.iter().any(|line| line.contains("{{CHANGELOG_DATE}}")),
            "recipe '{recipe}' no longer passes {{CHANGELOG_DATE}} to pm-changelog; an explicit release date could not reach generation anymore"
        );
    }
    Ok(())
}

/// pm-rust-1ps2: the justfile must not read the wall clock anywhere.
///
/// A runtime-clock default would churn the heading date once per day and fail
/// the gate on every day after the first — strictly worse clock dependence
/// than the frozen constant. The date stays a visible pin in the justfile.
#[test]
fn the_justfile_never_reads_the_wall_clock() -> Result<(), BoxError> {
    let justfile = read_repo_file("justfile")?;
    let offenders: Vec<&str> = justfile
        .lines()
        .filter(|line| !line.trim_start().starts_with('#'))
        .filter(|line| line.contains("date +") || line.contains("%Y") || line.contains("$DATE"))
        .collect();
    assert!(
        offenders.is_empty(),
        "the justfile must derive no date from the clock, found:\n  {}",
        offenders.join("\n  ")
    );
    Ok(())
}

/// pm-rust-1ps2: the changelog toolchain must pin the pm CLI/SDK build, not a
/// floating range.
///
/// pm-changelog declares its SDK as `>=2026.8.3`, so an unpinned install
/// resolves to latest. Under SDK >=2026.8.20 the same tracker read truncates
/// at the default output budget and regeneration silently drops closed items
/// the committed CHANGELOG.md carries — CI and a warm-cache working copy
/// disagreed with byte-identical inputs. Both packages must be pinned exactly
/// in the justfile and in every release-workflow npx invocation.
#[test]
fn changelog_toolchain_pins_the_sdk_exactly() -> Result<(), BoxError> {
    let justfile = read_repo_file("justfile")?;
    assert!(
        justfile.contains("PM_CHANGELOG_PKG := \"pm-changelog@2026.8.6\""),
        "the justfile must pin pm-changelog exactly"
    );
    assert!(
        justfile.contains("PM_CLI_PKG := \"@unbrained/pm-cli@2026.8.6\""),
        "the justfile must pin @unbrained/pm-cli exactly; the floating range resolves to latest and truncates tracker reads (pm-rust-1ps2)"
    );
    assert!(
        justfile.contains("--package={{PM_CHANGELOG_PKG}} --package={{PM_CLI_PKG}}"),
        "the shared pm-changelog recipe must install both pinned packages into one npx prefix"
    );

    let workflow = parse_workflow(".github/workflows/release.yml")?;
    for (job_name, steps) in workflow_jobs(&workflow)? {
        for step in &steps {
            let Some(run) = step.run.as_deref() else {
                continue;
            };
            for line in logical_lines(run) {
                if !line.contains("npx") || !line.contains("pm-changelog") {
                    continue;
                }
                assert!(
                    line.contains("--package=pm-changelog@2026.8.6")
                        && line.contains("--package=@unbrained/pm-cli@2026.8.6"),
                    "{job_name} step '{}' invokes pm-changelog without pinning both packages exactly; the SDK would float to latest and truncate regeneration (pm-rust-1ps2):\n  {line}",
                    step.label
                );
            }
        }
    }
    Ok(())
}

/// pm-rust-1ps2: the release workflow must keep the canonical date and the
/// generated changelog in one atomic, self-consistent commit.
///
/// The generate step rewrites the justfile constant before generating and
/// proves the rewrite landed; both check invocations run plain because the
/// constant already names the release date; the commit step includes the
/// justfile so main never carries a heading date the constant disagrees with.
#[test]
fn release_workflow_keeps_the_pin_and_the_generation_atomic() -> Result<(), BoxError> {
    let workflow = parse_workflow(".github/workflows/release.yml")?;
    let steps = job_steps(&workflow, "release")?;

    let generate = step_run(&steps, "Generate changelog and release notes")?;
    assert!(
        generate.contains("sed -i \"s/^CHANGELOG_DATE := "),
        "the generate step must rewrite the canonical CHANGELOG_DATE constant before generating (pm-rust-1ps2)"
    );
    assert!(
        generate.contains("grep -qx \"CHANGELOG_DATE := "),
        "the generate step must verify the constant rewrite landed instead of trusting sed's exit code"
    );

    let checks = step_run(&steps, "Run release checks")?;
    assert_eq!(
        checks.trim(),
        "just release-check",
        "the release job must run the gate without a CHANGELOG_DATE override; the constant was pinned one step earlier (pm-rust-1ps2)"
    );

    let commit = step_run(&steps, "Commit release files")?;
    assert!(
        commit
            .lines()
            .any(|line| line.trim() == "git add Cargo.toml Cargo.lock CHANGELOG.md justfile"),
        "the release commit must include the justfile so the pinned date and the heading date land atomically (pm-rust-1ps2)"
    );

    let verify = step_run(&steps, "Verify merged release")?;
    assert!(
        verify.contains("just release-check") && !verify.contains("CHANGELOG_DATE="),
        "the merged-release verification must run the plain gate with no override (pm-rust-1ps2)"
    );
    Ok(())
}
