//! Native Rust identity-audit gate for Git commit author and committer
//! identities.
//!
//! This integration test is the release gate for commit-identity hygiene. It
//! scans **all** Git objects—both reachable from refs and unreachable (dangling
//! or orphaned)—extracts the author and committer email addresses from every
//! commit object, and fails closed when any identity is not listed in
//! [`.github/approved-git-identities.txt`](../.github/approved-git-identities.txt).
//!
//! Unlike a TypeScript-only script copied from a sibling package, this audit is
//! implemented natively in Rust so it runs as part of `cargo test` and the
//! `release:check` aggregate without a Node.js runtime.
//!
//! # How the audit works
//!
//! 1. **Parse the allowlist** from `.github/approved-git-identities.txt`,
//!    skipping comment lines (starting with `#`) and blank lines.
//! 2. **Enumerate every Git object** via `git cat-file --batch-all-objects
//!    --batch-check`, which yields all objects regardless of reachability.
//! 3. **Filter to commit objects** and read each commit body with
//!    `git cat-file commit <oid>`.
//! 4. **Parse the `author` and `committer` lines** to extract the email address
//!    enclosed in `<...>`.
//! 5. **Fail closed** if any email is not in the allowlist.
//!
//! # Coverage of unreachable objects
//!
//! `git log --all` only visits commits reachable from refs. Objects that were
//! orphaned by `git commit --amend`, a branch deletion, or a `gc` that has not
//! yet run are invisible to that traversal. `git cat-file --batch-all-objects`
//! enumerates every loose and packed object, so the audit catches identities
//! that exist only in unreachable objects.

use std::collections::HashSet;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;

/// A boxed dynamic error used by test helpers.
type BoxError = Box<dyn std::error::Error>;

/// Parses the approved-git-identities allowlist file.
///
/// Comment lines (starting with `#`) and blank lines are ignored. Each
/// remaining line is trimmed and collected.
fn parse_allowlist(path: &Path) -> Result<HashSet<String>, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("failed to read approved-git-identities.txt: {e}"))?;
    let mut identities = HashSet::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        identities.insert(trimmed.to_owned());
    }
    Ok(identities)
}

/// Extracts the email address from a Git `author` or `committer` header line.
///
/// Git commit headers use the format `author Name <email> timestamp tz`.
/// The email is the text between the first `<` and the last `>`.
fn extract_email(header_line: &str) -> Option<&str> {
    let start = header_line.find('<')?;
    let end = header_line.rfind('>')?;
    if end > start {
        Some(&header_line[start + 1..end])
    } else {
        None
    }
}

/// One identity found in a Git commit object.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct Identity {
    /// Email address extracted from the author or committer line.
    email: String,
    /// Object ID of the commit that carried this identity.
    oid: String,
    /// Whether this came from the `author` or `committer` line.
    role: &'static str,
}

/// Scans all Git objects (reachable and unreachable) for commit identities.
///
/// Returns every `(email, oid, role)` triple found in any commit object in the
/// repository at `repo_root`.
fn audit_all_objects(repo_root: &Path) -> Result<Vec<Identity>, String> {
    // Step 1: enumerate all objects with their type.
    let batch_check = Command::new("git")
        .args(["-C"])
        .arg(repo_root)
        .args([
            "cat-file",
            "--batch-all-objects",
            "--batch-check",
            "--unordered",
        ])
        .output()
        .map_err(|e| format!("failed to run git cat-file --batch-check: {e}"))?;
    if !batch_check.status.success() {
        return Err(format!(
            "git cat-file --batch-check failed: {}",
            String::from_utf8_lossy(&batch_check.stderr)
        ));
    }

    let check_output = String::from_utf8_lossy(&batch_check.stdout);
    let mut commit_oids = Vec::new();
    for line in check_output.lines() {
        // Format: "<oid> <type> <size>"
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 2 && parts[1] == "commit" {
            commit_oids.push(parts[0].to_owned());
        }
    }

    // Step 2: read each commit body and extract author/committer emails.
    let mut identities = Vec::new();
    for oid in &commit_oids {
        let body = Command::new("git")
            .args(["-C"])
            .arg(repo_root)
            .args(["cat-file", "commit", oid])
            .output()
            .map_err(|e| format!("failed to run git cat-file commit {oid}: {e}"))?;
        if !body.status.success() {
            // An object may be pruned between the two calls; skip it.
            continue;
        }
        let body_text = String::from_utf8_lossy(&body.stdout);
        for line in body_text.lines() {
            if let Some(rest) = line.strip_prefix("author ") {
                if let Some(email) = extract_email(rest) {
                    identities.push(Identity {
                        email: email.to_owned(),
                        oid: oid.clone(),
                        role: "author",
                    });
                }
            } else if let Some(rest) = line.strip_prefix("committer ")
                && let Some(email) = extract_email(rest)
            {
                identities.push(Identity {
                    email: email.to_owned(),
                    oid: oid.clone(),
                    role: "committer",
                });
            }
            // The blank line after headers ends the header block.
            if line.is_empty() {
                break;
            }
        }
    }
    Ok(identities)
}

/// Runs the full identity audit against `repo_root` using the allowlist at
/// `allowlist_path`. Returns `Ok(())` when all identities are approved, or
/// `Err(message)` listing every unapproved identity.
fn run_audit(repo_root: &Path, allowlist_path: &Path) -> Result<(), String> {
    let allowlist = parse_allowlist(allowlist_path)?;
    let identities = audit_all_objects(repo_root)?;
    let mut unapproved: Vec<&Identity> = identities
        .iter()
        .filter(|identity| !allowlist.contains(&identity.email))
        .collect();
    if unapproved.is_empty() {
        return Ok(());
    }
    unapproved.sort_by(|a, b| a.email.cmp(&b.email).then(a.oid.cmp(&b.oid)));
    let mut message = String::from("unapproved commit identities found:\n");
    for identity in &unapproved {
        writeln!(
            message,
            "  {} <{}> in commit {}",
            identity.role, identity.email, identity.oid
        )
        .map_err(|e| format!("write failed: {e}"))?;
    }
    write!(
        message,
        "\napproved identities (from {}): {allowlist:?}",
        allowlist_path.display()
    )
    .map_err(|e| format!("write failed: {e}"))?;
    Err(message)
}

/// Locates the repository root from the Cargo manifest directory.
fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Runs a Git command and returns an error if it fails.
fn run_git(repo: &Path, args: &[&str], env: &[(&str, &str)]) -> Result<(), BoxError> {
    let mut command = Command::new("git");
    command.args(["-C"]).arg(repo);
    for (key, value) in env {
        command.env(key, value);
    }
    command.args(args);
    let status = command
        .status()
        .map_err(|e| -> BoxError { format!("git {} failed: {e}", args.join(" ")).into() })?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("git {} exited with {status}", args.join(" ")).into())
    }
}

/// Captures a Git command's standard output as UTF-8 text.
fn git_output(repo: &Path, args: &[&str]) -> Result<String, BoxError> {
    let output = Command::new("git")
        .args(["-C"])
        .arg(repo)
        .args(args)
        .output()
        .map_err(|e| -> BoxError { format!("git {} failed: {e}", args.join(" ")).into() })?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        Err(format!("git {} exited with {}", args.join(" "), output.status).into())
    }
}

/// Writes a file and propagates the error.
fn write_file(path: &Path, content: &str) -> Result<(), BoxError> {
    std::fs::write(path, content)?;
    Ok(())
}

/// Builds the dangling-identity scenario shared by the unreachable-object
/// tests: a reachable tip authored by an approved identity, and the commit it
/// replaced — reachable from no ref — authored by `bad@unreachable.example`,
/// which is not in the allowlist.
///
/// Returns the allowlist path. The amend carries `--reset-author`, which is
/// load-bearing rather than tidiness: a plain `--amend` keeps the original
/// author, leaving the bad identity on the reachable tip, which would make
/// the dangling-object assertions pass for the wrong reason — the pm-rust-r9r2
/// defect. `good@example.com` is in the scenario's allowlist so the audit can
/// fail *only* because of the dangling commit.
fn build_dangling_identity_scenario(repo: &Path) -> Result<PathBuf, BoxError> {
    run_git(repo, &["init", "--quiet"], &[])?;
    let allowlist = repo.join("allowlist.txt");
    write_file(&allowlist, "approved@example.com\ngood@example.com\n")?;

    write_file(&repo.join("file.txt"), "v1")?;
    run_git(repo, &["add", "file.txt"], &[])?;
    run_git(
        repo,
        &["commit", "--quiet", "-m", "original"],
        &[
            ("GIT_AUTHOR_NAME", "Bad Author"),
            ("GIT_AUTHOR_EMAIL", "bad@unreachable.example"),
            ("GIT_COMMITTER_NAME", "Bad Committer"),
            ("GIT_COMMITTER_EMAIL", "bad@unreachable.example"),
        ],
    )?;

    // Amend with an approved identity, making the original commit unreachable.
    write_file(&repo.join("file.txt"), "v2")?;
    run_git(repo, &["add", "file.txt"], &[])?;
    run_git(
        repo,
        // `--reset-author` is what confines the bad identity to the dangling
        // commit; see the function documentation above.
        &[
            "commit",
            "--quiet",
            "--amend",
            "--reset-author",
            "-m",
            "amended",
        ],
        &[
            ("GIT_AUTHOR_NAME", "Good Author"),
            ("GIT_AUTHOR_EMAIL", "good@example.com"),
            ("GIT_COMMITTER_NAME", "Good Committer"),
            ("GIT_COMMITTER_EMAIL", "good@example.com"),
        ],
    )?;
    Ok(allowlist)
}

/// Verifies that every commit identity in this repository (reachable and
/// unreachable) is listed in `.github/approved-git-identities.txt`.
///
/// This test IS the release gate. When it runs in CI it audits the real
/// repository history.
#[test]
fn all_commit_identities_are_approved() -> Result<(), BoxError> {
    let root = repo_root();
    let allowlist = root.join(".github/approved-git-identities.txt");
    assert!(
        allowlist.is_file(),
        "approved-git-identities.txt not found at {}",
        allowlist.display()
    );
    run_audit(&root, &allowlist).map_err(|e| -> BoxError { e.into() })
}

/// Proves the audit rejects an unapproved identity by creating a temporary
/// Git repo with a commit authored by an unapproved address.
///
/// This test must fail (i.e., the audit must detect the unapproved identity)
/// to prove the gate is not a rubber stamp.
#[test]
fn audit_rejects_unapproved_identity() -> Result<(), BoxError> {
    let temp = tempfile::tempdir()?;
    let repo = temp.path();

    run_git(repo, &["init", "--quiet"], &[])?;
    let allowlist = repo.join("allowlist.txt");
    write_file(&allowlist, "approved@example.com\n")?;
    write_file(&repo.join("file.txt"), "content")?;
    run_git(repo, &["add", "file.txt"], &[])?;
    run_git(
        repo,
        &["commit", "--quiet", "-m", "test commit"],
        &[
            ("GIT_AUTHOR_NAME", "Unapproved Author"),
            ("GIT_AUTHOR_EMAIL", "unapproved@leaked.example"),
            ("GIT_COMMITTER_NAME", "Unapproved Committer"),
            ("GIT_COMMITTER_EMAIL", "unapproved@leaked.example"),
        ],
    )?;

    let result = run_audit(repo, &allowlist);
    assert!(
        result.is_err(),
        "audit must reject unapproved identity but returned Ok"
    );
    if let Err(error) = &result {
        assert!(
            error.contains("unapproved@leaked.example"),
            "error must name the unapproved identity: {error}"
        );
    }
    Ok(())
}

/// Proves the audit passes when all identities are in the allowlist.
#[test]
fn audit_passes_with_approved_identity() -> Result<(), BoxError> {
    let temp = tempfile::tempdir()?;
    let repo = temp.path();

    run_git(repo, &["init", "--quiet"], &[])?;
    let allowlist = repo.join("allowlist.txt");
    write_file(&allowlist, "approved@example.com\n")?;
    write_file(&repo.join("file.txt"), "content")?;
    run_git(repo, &["add", "file.txt"], &[])?;
    run_git(
        repo,
        &["commit", "--quiet", "-m", "test commit"],
        &[
            ("GIT_AUTHOR_NAME", "Approved Author"),
            ("GIT_AUTHOR_EMAIL", "approved@example.com"),
            ("GIT_COMMITTER_NAME", "Approved Committer"),
            ("GIT_COMMITTER_EMAIL", "approved@example.com"),
        ],
    )?;

    run_audit(repo, &allowlist).map_err(|e| -> BoxError { e.into() })
}

/// Proves the audit catches identities in unreachable (dangling) objects.
///
/// This test creates a commit, then amends it, leaving the original commit as
/// an unreachable object. The audit must still detect the identity in the
/// unreachable commit.
#[test]
fn audit_catches_unreachable_objects() -> Result<(), BoxError> {
    let temp = tempfile::tempdir()?;
    let repo = temp.path();

    let allowlist = build_dangling_identity_scenario(repo)?;

    let result = run_audit(repo, &allowlist);
    assert!(
        result.is_err(),
        "audit must catch the unreachable object with the bad identity"
    );
    if let Err(error) = &result {
        assert!(
            error.contains("bad@unreachable.example"),
            "error must name the unreachable identity: {error}"
        );
    }
    Ok(())
}

/// Proves the dangling-object scenario cannot pass vacuously (pm-rust-r9r2).
///
/// Two guards, each aimed at one historical way the dangling-object test
/// proved nothing:
///
/// * the bad identity must be invisible to every ref walk — if it shows up in
///   `git log --all`, the amend lost `--reset-author` and the audit catches
///   the identity from the reachable tip whether or not the all-objects walk
///   works at all;
/// * the audit must still reject the repository — with the ref walk clean,
///   that rejection can only come from the unreachable commit, so a mutation
///   that disables the all-objects walk (for example enumerating commits with
///   `git rev-list --all` instead of `cat-file --batch-all-objects`) makes
///   this test fail rather than pass silently.
#[test]
fn dangling_identity_test_cannot_pass_vacuously() -> Result<(), BoxError> {
    let temp = tempfile::tempdir()?;
    let repo = temp.path();

    let allowlist = build_dangling_identity_scenario(repo)?;

    let reachable_authors = git_output(repo, &["log", "--all", "--format=%ae"])?;
    // Guard the guard: the ref walk must have seen something, otherwise the
    // check below would hold vacuously on an empty repository.
    assert!(
        !reachable_authors.trim().is_empty(),
        "scenario repository has no reachable commits"
    );
    for email in reachable_authors
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        assert_ne!(
            email, "bad@unreachable.example",
            "the bad identity is reachable from refs, so the dangling-object test proves nothing: the amend must reset the author (pm-rust-r9r2)"
        );
    }

    let result = run_audit(repo, &allowlist);
    assert!(
        result.is_err(),
        "with the ref walk clean, the audit can only fail via the unreachable commit; if it passes, the all-objects walk is disabled (pm-rust-r9r2)"
    );
    Ok(())
}

/// Proves `extract_email` parses the standard Git header format.
#[test]
fn extract_email_parses_standard_headers() {
    assert_eq!(
        extract_email("author Name <user@example.com> 1234567890 +0000"),
        Some("user@example.com")
    );
    assert_eq!(
        extract_email("committer Name <a@b.com> 1234567890 +0000"),
        Some("a@b.com")
    );
    assert_eq!(extract_email("no angle brackets here"), None);
    assert_eq!(extract_email("reversed > <"), None);
}
