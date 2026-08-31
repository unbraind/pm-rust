//! Real-branch no-lost-update acceptance using the pm merge drivers.
//!
//! Two real git branches mutate the same tracked item — one appends a comment,
//! the other renames the title — and `git merge` reconciles both sides through
//! the published pm merge drivers. The merged workspace must contain every
//! accepted mutation from both branches: the renamed title, the appended
//! comment row, and both history records.
//!
//! The test requires the published Node CLI (it owns the merge drivers) and
//! a git binary; when either is missing it prints an explicit skip notice.

use std::fs;
use std::path::Path;
use std::process::Command;

#[path = "support/published_cli.rs"]
mod published_cli;

use published_cli::published_cli_or_skip;

/// Runs one git command and fails loudly when it exits nonzero.
fn run_git(repository: &Path, arguments: &[&str]) -> Result<String, Box<dyn std::error::Error>> {
    let output = Command::new("git")
        .current_dir(repository)
        .args(arguments)
        .env("GIT_AUTHOR_NAME", "branch-test")
        .env("GIT_AUTHOR_EMAIL", "branch-test@example.test")
        .env("GIT_COMMITTER_NAME", "branch-test")
        .env("GIT_COMMITTER_EMAIL", "branch-test@example.test")
        .output()?;
    assert!(
        output.status.success(),
        "git {:?} failed: {}",
        arguments,
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Runs the native binary inside the repository working tree.
fn run_native(repository: &Path, arguments: &[&str]) -> Result<(), Box<dyn std::error::Error>> {
    let output = Command::new(env!("CARGO_BIN_EXE_pm-rust"))
        .current_dir(repository)
        .args(["--workspace=."])
        .args(arguments)
        .output()?;
    assert!(
        output.status.success(),
        "native pm-rust {:?} failed: {} {}",
        arguments,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}

#[test]
/// Proves merging two real branches that mutate one item loses nothing.
#[allow(clippy::too_many_lines)]
fn branch_merge_preserves_mutations_from_both_sides() -> Result<(), Box<dyn std::error::Error>> {
    let Some(published) = published_cli_or_skip("The branch merge contract suite") else {
        return Ok(());
    };
    if Command::new("git").arg("--version").output().is_err() {
        println!("skip: no git binary found");
        return Ok(());
    }
    // The merge install below spawns this interpreter directly. Probing it here
    // keeps the promise the module header makes about external tools: a missing
    // one produces an explicit skip notice rather than an assertion failure on
    // an empty output several steps later.
    let node_interpreter =
        std::env::var("PM_NODE_INTERPRETER").unwrap_or_else(|_| "node".to_owned());
    if Command::new(&node_interpreter)
        .arg("--version")
        .output()
        .is_err()
    {
        println!("skip: no Node interpreter found (set PM_NODE_INTERPRETER to select one)");
        return Ok(());
    }

    let repository = tempfile::tempdir()?;
    run_git(repository.path(), &["init", "--initial-branch=main"])?;
    run_git(repository.path(), &["config", "user.name", "branch-test"])?;
    run_git(
        repository.path(),
        &["config", "user.email", "branch-test@example.test"],
    )?;

    // Seed a tracker first: pm merge install requires an initialized one.
    let root = repository.path().join(".agents/pm");
    fs::create_dir_all(&root)?;
    fs::write(
        root.join("settings.json"),
        r#"{"id_prefix":"sample-","item_format":"toon","locks":{"ttl_seconds":1800,"wait_ms":5000}}"#,
    )?;

    // Install the published clone-local merge drivers into this repository.
    let installer = published.package_root.join("dist/cli.js");
    let merge_install = Command::new(&node_interpreter)
        .current_dir(repository.path())
        .args([installer.to_string_lossy().as_ref(), "merge", "install"])
        .output()?;
    assert!(
        merge_install.status.success(),
        "pm merge install failed: {}",
        String::from_utf8_lossy(&merge_install.stderr)
    );
    run_native(
        repository.path(),
        &[
            "create",
            "--id=sample-merge",
            "--title=Merge target",
            "--type=Task",
            "--author=merge-agent",
        ],
    )?;
    run_git(repository.path(), &["add", "-A"])?;
    run_git(repository.path(), &["commit", "-m", "base state"])?;

    // Branch side appends a comment; main renames the title.
    run_git(repository.path(), &["checkout", "-b", "side"])?;
    run_native(
        repository.path(),
        &[
            "comment",
            "sample-merge",
            "note written on the side branch",
            "--author=side-agent",
        ],
    )?;
    run_git(repository.path(), &["add", "-A"])?;
    run_git(repository.path(), &["commit", "-m", "side comment"])?;
    run_git(repository.path(), &["checkout", "main"])?;
    run_native(
        repository.path(),
        &[
            "update",
            "sample-merge",
            "--title=Rename from main",
            "--author=main-agent",
        ],
    )?;
    run_git(repository.path(), &["add", "-A"])?;
    run_git(repository.path(), &["commit", "-m", "main rename"])?;
    run_git(repository.path(), &["checkout", "side"])?;
    run_git(repository.path(), &["merge", "main", "--no-edit"])?;

    // Both mutations survive in the merged item document.
    let merged = fs::read_to_string(repository.path().join(".agents/pm/tasks/sample-merge.toon"))?;
    assert!(
        merged.contains("title: Rename from main"),
        "merged item lost the main-branch title: {merged}"
    );
    assert!(
        merged.contains("note written on the side branch"),
        "merged item lost the side-branch comment: {merged}"
    );

    // Both history records survive in the merged append-only stream.
    let history = fs::read_to_string(
        repository
            .path()
            .join(".agents/pm/history/sample-merge.jsonl"),
    )?;
    assert!(history.contains(r#""op":"create""#), "history lost create");
    assert!(
        history.contains(r#""op":"update""#),
        "history lost the main-branch update: {history}"
    );
    assert!(
        history.contains(r#""op":"comment_add""#),
        "history lost the side-branch comment: {history}"
    );

    // The merged document still decodes as canonical native input.
    let read_back = Command::new(env!("CARGO_BIN_EXE_pm-rust"))
        .current_dir(repository.path())
        .args(["--workspace=.", "get", "sample-merge"])
        .output()?;
    assert!(
        read_back.status.success(),
        "merged document failed native decode: {}",
        String::from_utf8_lossy(&read_back.stderr)
    );
    Ok(())
}
