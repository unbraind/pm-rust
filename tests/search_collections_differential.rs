//! Exact CLI, storage and history parity for native search and collections.
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};
#[path = "support/mutation_differential.rs"]
mod mutation_differential;
#[path = "support/published_cli.rs"]
mod published_cli;
use mutation_differential::{copy_directory, run_minimal, write_driver};
type TestResult = Result<(), Box<dyn std::error::Error>>;
const CLOCK: &str = "2026-08-22T10:00:00.000Z";

/// Collapsed duplicates count as removals, while written entries use persisted multiset equality.
#[test]
fn duplicate_artifact_receipts_match_published_storage() -> TestResult {
    let Some(published) = published_cli::published_cli_or_skip("duplicate linked artifacts") else {
        return Ok(());
    };
    let scratch = tempfile::tempdir()?;
    let driver = write_driver(scratch.path(), &published)?;
    let node = tempfile::tempdir()?;
    let rust = tempfile::tempdir()?;
    for args in [
        vec!["init", "sample-", "--defaults"],
        vec![
            "create",
            "--id",
            "sample-item",
            "--title",
            "Links",
            "--type",
            "Task",
            "--author",
            "alpha",
        ],
    ] {
        let result = oracle(&driver, node.path(), &args)?;
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
    let workspace = pm_rust::Workspace::discover(node.path())?;
    let mut item = workspace.get("sample-item")?;
    item.metadata.extra.insert(
        "files".to_owned(),
        json!([
            {"path":"linked.rs","scope":"project","note":"restored"},
            {"path":"linked.rs","scope":"project","note":"last"}
        ]),
    );
    let file = ".agents/pm/tasks/sample-item.toon";
    fs::write(node.path().join(file), toon_format::encode_default(&item)?)?;
    copy_directory(&node.path().join(".agents"), &rust.path().join(".agents"))?;
    let args = [
        "files",
        "sample-item",
        "--add",
        "path=linked.rs,note=restored",
        "--note-conflict",
        "replace",
        "--author",
        "alpha",
        "--json",
    ];
    let expected = oracle(&driver, node.path(), &args)?;
    let actual = native(rust.path(), &args, true)?;
    assert!(
        expected.status.success(),
        "{}",
        String::from_utf8_lossy(&expected.stderr)
    );
    assert!(
        actual.status.success(),
        "{}",
        String::from_utf8_lossy(&actual.stderr)
    );
    let receipt: Value = serde_json::from_slice(&expected.stdout)?;
    assert_eq!(receipt["mutation_receipt"]["changed_count"], 0);
    assert_eq!(receipt["mutation_receipt"]["removed_count"], 1);
    assert_eq!(
        receipt["mutation_receipt"]["link_outcomes"][0]["outcome"],
        "note_replaced"
    );
    let native_receipt: Value = serde_json::from_slice(&actual.stdout)?;
    for field in ["files", "changed", "total_count", "mutation_receipt"] {
        assert_eq!(native_receipt[field], receipt[field], "{field}");
    }
    let stored = pm_rust::Workspace::discover(rust.path())?.get("sample-item")?;
    assert_eq!(
        stored.metadata.extra["files"],
        json!([{"path":"linked.rs","scope":"project","note":"restored"}])
    );
    Ok(())
}

/// Help advertises note conflict modes only for the collections that accept them.
#[test]
fn linked_note_conflict_help_and_test_refusal_match_oracle() -> TestResult {
    let Some(published) = published_cli::published_cli_or_skip("linked note conflict help") else {
        return Ok(());
    };
    let scratch = tempfile::tempdir()?;
    let driver = write_driver(scratch.path(), &published)?;
    for (command, advertised) in [("files", true), ("docs", true), ("test", false)] {
        let args = [command, "--help"];
        for result in [
            oracle(&driver, scratch.path(), &args)?,
            native(scratch.path(), &args, false)?,
        ] {
            assert!(result.status.success(), "{command}");
            assert_eq!(
                String::from_utf8(result.stdout)?.contains("--note-conflict"),
                advertised,
                "{command}"
            );
        }
    }
    assert!(
        oracle(&driver, scratch.path(), &["init", "sample-", "--defaults"])?
            .status
            .success()
    );
    let args = [
        "test",
        "sample-item",
        "--add",
        "command=cargo test",
        "--note-conflict",
        "replace",
        "--author",
        "alpha",
        "--json",
    ];
    for result in [
        oracle(&driver, scratch.path(), &args)?,
        native(scratch.path(), &args, false)?,
    ] {
        assert_eq!(result.status.code(), Some(2));
        let refusal: Value = serde_json::from_slice(&result.stderr)?;
        assert_eq!(refusal["code"], "unknown_option");
        assert_eq!(refusal["flag"], "--note-conflict");
        assert_eq!(
            refusal["recovery"]["candidate_commands"],
            json!(["item docs", "item files"])
        );
    }
    Ok(())
}

/// Executes the fixed-clock, telemetry-isolated published command.
fn oracle(driver: &Path, root: &Path, args: &[&str]) -> Result<Output, Box<dyn std::error::Error>> {
    let mut arguments = vec![driver.to_string_lossy().into_owned()];
    arguments.extend(args.iter().map(ToString::to_string));
    run_minimal(
        &PathBuf::from(std::env::var("PM_NODE_INTERPRETER").unwrap_or_else(|_| "node".to_owned())),
        &arguments,
        root,
    )
}

/// Executes the native command against the same disposable tracker.
fn native(root: &Path, args: &[&str], clock: bool) -> Result<Output, std::io::Error> {
    native_command(root, args, clock).output()
}

/// Preserves runtime and instrumentation variables while excluding application context.
fn native_command(root: &Path, args: &[&str], clock: bool) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_pm-rust"));
    command.current_dir(root).env_clear().args(args);
    for key in [
        "PATH",
        "HOME",
        "SystemRoot",
        "SystemDrive",
        "windir",
        "TEMP",
        "TMP",
        "USERPROFILE",
        "COMSPEC",
        "PATHEXT",
        "NUMBER_OF_PROCESSORS",
        "PROCESSOR_ARCHITECTURE",
        "LLVM_PROFILE_FILE",
    ] {
        if let Some(value) = std::env::var_os(key) {
            command.env(key, value);
        }
    }
    if clock {
        command.args(["--timestamp", CLOCK]);
    }
    command
}

/// Compares all observable command bytes without normalizing fields or errors.
fn output(expected: &Output, actual: &Output, args: &[&str]) {
    assert_eq!(
        actual.status.code(),
        expected.status.code(),
        "{args:?}: native stderr: {}\noracle stderr: {}",
        String::from_utf8_lossy(&actual.stderr),
        String::from_utf8_lossy(&expected.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&actual.stdout),
        String::from_utf8_lossy(&expected.stdout),
        "{args:?}: stdout"
    );
    assert_eq!(
        String::from_utf8_lossy(&actual.stderr),
        String::from_utf8_lossy(&expected.stderr),
        "{args:?}: stderr"
    );
}

/// Compares every authoritative item and append-only history byte recursively.
fn artifacts(expected: &Path, actual: &Path) -> TestResult {
    let names =
        |path: &Path| -> Result<std::collections::BTreeSet<std::ffi::OsString>, std::io::Error> {
            fs::read_dir(path)?
                .map(|entry| entry.map(|entry| entry.file_name()))
                .filter(|name| {
                    !matches!(
                        name.as_ref().ok().and_then(|name| name.to_str()),
                        Some("cache" | "runtime" | "locks")
                    )
                })
                .collect()
        };
    assert_eq!(names(expected)?, names(actual)?);
    for entry in fs::read_dir(expected)? {
        let entry = entry?;
        let dest = actual.join(entry.file_name());
        if entry.path().is_dir() {
            if !matches!(
                entry.file_name().to_str(),
                Some("cache" | "runtime" | "locks")
            ) {
                artifacts(&entry.path(), &dest)?;
            }
        } else {
            assert_eq!(
                fs::read_to_string(entry.path())?,
                fs::read_to_string(dest)?,
                "{}",
                entry.file_name().to_string_lossy()
            );
        }
    }
    Ok(())
}

/// Creates the root link and the competing link that a relative removal can select.
#[cfg(unix)]
fn seed_linked_files(driver: &Path, project: &Path, other_path: &str) -> TestResult {
    for args in [
        vec!["init", "sample-", "--defaults"],
        vec![
            "create",
            "--id",
            "sample-item",
            "--title",
            "Artifacts",
            "--type",
            "Task",
            "--author",
            "alpha",
        ],
        vec![
            "files",
            "sample-item",
            "--add",
            "source.rs",
            "--add",
            other_path,
            "--author",
            "alpha",
        ],
    ] {
        let result = oracle(driver, project, &args)?;
        assert!(
            result.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
    Ok(())
}

/// A logical SDK base names the same link as an explicit project path in the published CLI.
#[cfg(unix)]
#[test]
fn sdk_removal_preserves_logical_symlink_operand_base() -> TestResult {
    let Some(published) = published_cli::published_cli_or_skip("logical artifact base") else {
        return Ok(());
    };
    let scratch = tempfile::tempdir()?;
    let driver = write_driver(scratch.path(), &published)?;
    let oracle_project = tempfile::tempdir()?;
    let native_project = tempfile::tempdir()?;
    let outside = tempfile::tempdir()?;
    fs::write(outside.path().join("source.rs"), "external fixture")?;
    for project in [oracle_project.path(), native_project.path()] {
        fs::write(project.join("source.rs"), "project fixture")?;
        std::os::unix::fs::symlink(outside.path(), project.join("vendor"))?;
    }
    seed_linked_files(&driver, oracle_project.path(), "vendor/source.rs")?;
    copy_directory(
        &oracle_project.path().join(".agents"),
        &native_project.path().join(".agents"),
    )?;
    // The published SDK changes the process directory, which loses the logical symlink
    // name. Spell out the logical operand from the project root to compare the write.
    let expected = oracle(
        &driver,
        oracle_project.path(),
        &[
            "files",
            "sample-item",
            "--remove",
            "vendor/source.rs",
            "--full-history",
            "--author",
            "alpha",
            "--json",
        ],
    )?;
    assert!(
        expected.status.success(),
        "{}",
        String::from_utf8_lossy(&expected.stderr)
    );
    let expected: Value = serde_json::from_slice(&expected.stdout)?;
    assert_eq!(
        expected["files"],
        json!([{"path":"source.rs","scope":"project"}])
    );
    assert_eq!(expected["mutation_receipt"]["removed_count"], 1);
    let workspace = pm_rust::Workspace::discover(native_project.path())?;
    let actual = workspace.collection(
        "sample-item",
        "files",
        &pm_rust::CollectionOptions {
            remove: vec!["source.rs".to_owned()],
            full_history: true,
            author: "alpha".to_owned(),
            timestamp: Some(CLOCK.to_owned()),
            ..Default::default()
        },
        &native_project.path().join("vendor"),
    )?;
    assert_eq!(actual, expected);
    artifacts(
        workspace.pm_root(),
        &oracle_project.path().join(".agents/pm"),
    )?;
    Ok(())
}

/// Parent traversal from a directory symlink selects the link relative to its target.
#[cfg(unix)]
#[test]
fn artifact_cwd_traverses_the_symlink_target_parent() -> TestResult {
    let Some(published) = published_cli::published_cli_or_skip("symlink parent artifact base")
    else {
        return Ok(());
    };
    let scratch = tempfile::tempdir()?;
    let driver = write_driver(scratch.path(), &published)?;
    let published = tempfile::tempdir()?;
    let rust = tempfile::tempdir()?;
    for project in [published.path(), rust.path()] {
        fs::create_dir_all(project.join("vendor/deep"))?;
        std::os::unix::fs::symlink("vendor/deep", project.join("link"))?;
    }
    seed_linked_files(&driver, published.path(), "vendor/source.rs")?;
    copy_directory(
        &published.path().join(".agents"),
        &rust.path().join(".agents"),
    )?;
    // A process traversing the link enters a physical cwd, including on aliased temp roots.
    let pm_root = published.path().canonicalize()?.join(".agents/pm");
    let args = [
        "files",
        "sample-item",
        "--remove",
        "source.rs",
        "--full-history",
        "--author",
        "alpha",
        "--json",
    ];
    let expected = oracle(
        &driver,
        &published.path().join("link/.."),
        &[
            &["--pm-path", pm_root.to_str().ok_or("tracker path")?][..],
            &args,
        ]
        .concat(),
    )?;
    let actual = native(
        rust.path(),
        &[&args[..], &["--artifact-cwd", "link/.."]].concat(),
        true,
    )?;
    assert!(
        expected.status.success(),
        "{}",
        String::from_utf8_lossy(&expected.stderr)
    );
    output(&expected, &actual, &args);
    let receipt: Value = serde_json::from_slice(&actual.stdout)?;
    assert_eq!(
        receipt["files"],
        json!([{"path":"source.rs","scope":"project"}])
    );
    artifacts(&pm_root, &rust.path().join(".agents/pm"))
}

/// An external alias selects the same project-relative link in the native SDK and published CLI.
#[cfg(unix)]
#[test]
fn sdk_removal_anchors_external_directory_aliases() -> TestResult {
    let Some(published) = published_cli::published_cli_or_skip("external artifact aliases") else {
        return Ok(());
    };
    let scratch = tempfile::tempdir()?;
    let driver = write_driver(scratch.path(), &published)?;
    let seed = scratch.path().join("seed");
    fs::create_dir_all(seed.join("sub"))?;
    for path in ["source.rs", "sub/source.rs"] {
        fs::write(seed.join(path), "source fixture")?;
    }
    seed_linked_files(&driver, &seed, "sub/source.rs")?;
    for (target, remaining) in [("", "sub/source.rs"), ("sub", "source.rs")] {
        let oracle_project = tempfile::tempdir_in(scratch.path())?;
        let native_project = tempfile::tempdir_in(scratch.path())?;
        let aliases = tempfile::tempdir_in(scratch.path())?;
        let oracle_alias = aliases.path().join("oracle");
        let native_alias = aliases.path().join("native");
        for (project, alias) in [
            (oracle_project.path(), &oracle_alias),
            (native_project.path(), &native_alias),
        ] {
            copy_directory(&seed, project)?;
            std::os::unix::fs::symlink(project.join(target), alias)?;
        }
        let expected = oracle(
            &driver,
            &oracle_alias,
            &[
                "files",
                "sample-item",
                "--remove",
                "source.rs",
                "--full-history",
                "--author",
                "alpha",
                "--json",
            ],
        )?;
        assert!(
            expected.status.success(),
            "{}",
            String::from_utf8_lossy(&expected.stderr)
        );
        let expected: Value = serde_json::from_slice(&expected.stdout)?;
        assert_eq!(
            expected["files"],
            json!([{"path":remaining,"scope":"project"}])
        );
        assert_eq!(expected["mutation_receipt"]["removed_count"], 1);
        let workspace = pm_rust::Workspace::discover(native_project.path())?;
        let actual = workspace.collection(
            "sample-item",
            "files",
            &pm_rust::CollectionOptions {
                remove: vec!["source.rs".to_owned()],
                full_history: true,
                author: "alpha".to_owned(),
                timestamp: Some(CLOCK.to_owned()),
                ..Default::default()
            },
            &native_alias,
        )?;
        assert_eq!(
            actual["files"], expected["files"],
            "alias target {target:?}"
        );
        assert_eq!(actual, expected);
        artifacts(
            workspace.pm_root(),
            &oracle_project.path().join(".agents/pm"),
        )?;
    }
    Ok(())
}

/// Preserve the oracle wire comparison except for the known destructive expansion hint.
/// Both renderings must round-trip exactly before projecting the native read command.
fn scoped_collection_restore(
    mut oracle: Output,
    args: &[&str],
) -> Result<Output, Box<dyn std::error::Error>> {
    if !oracle.status.success() || !String::from_utf8_lossy(&oracle.stdout).contains("full_history")
    {
        return Ok(oracle);
    }
    let json_output = args.contains(&"--json");
    let mut receipt: Value = if json_output {
        serde_json::from_slice(&oracle.stdout)?
    } else {
        pm_rust::parse_toon(std::str::from_utf8(&oracle.stdout)?)?
    };
    let pointer = "/omission_receipt/omitted_field_groups/0/restore_with";
    if receipt.pointer(pointer).is_none() {
        return Ok(oracle);
    }
    let render = |receipt: &Value| -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        Ok(if json_output {
            format!("{}\n", pm_rust::stringify_json(receipt, true)).into_bytes()
        } else {
            pm_rust::stringify_toon(&receipt.clone().into())?.into_bytes()
        })
    };
    assert_eq!(render(&receipt)?, oracle.stdout);
    let hint = receipt.pointer_mut(pointer).ok_or("restore hint")?;
    assert_eq!(
        *hint,
        json!({"selector":"full_history","cli_flag":"--full-history","sdk_option":"fullHistory","mcp_option":"full"})
    );
    *hint = json!(pm_rust::render_pm_command([
        "--pm-path",
        ".agents/pm",
        args[0],
        args[1]
    ]));
    receipt["omission_receipt"]["omitted_field_groups"][0]["restore_guidance"] =
        json!("Run this command from the original invocation directory.");
    oracle.stdout = render(&receipt)?;
    Ok(oracle)
}

/// Builds a real tracker and verifies each collection branch with exact durable artifacts.
#[test]
#[allow(clippy::too_many_lines)]
fn collection_commands_match_oracle() -> TestResult {
    let Some(published) = published_cli::published_cli_or_skip("collection differential") else {
        return Ok(());
    };
    let scratch = tempfile::tempdir()?;
    let driver = write_driver(scratch.path(), &published)?;
    let tracker = tempfile::tempdir()?;
    for args in [
        vec!["init", "sample-", "--defaults"],
        vec![
            "create",
            "--id",
            "sample-item",
            "--title",
            "Collection fixture",
            "--type",
            "Task",
            "--author",
            "alpha",
        ],
        vec![
            "files",
            "sample-item",
            "--add",
            "path=src/z.rs,note=last",
            "--add",
            "src/a.rs",
            "--author",
            "alpha",
        ],
        vec![
            "docs",
            "sample-item",
            "--add",
            "README.md",
            "--author",
            "alpha",
        ],
        vec![
            "test",
            "sample-item",
            "--add",
            "command=cargo test,path=tests/example.rs",
            "--author",
            "alpha",
        ],
    ] {
        let result = oracle(&driver, tracker.path(), &args)?;
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
    fs::create_dir_all(tracker.path().join("src"))?;
    fs::write(tracker.path().join("src/a.rs"), "")?;
    let snapshot = tempfile::tempdir()?;
    copy_directory(
        &tracker.path().join(".agents"),
        &snapshot.path().join(".agents"),
    )?;
    let mut mismatches = 0;
    for json_flag in [vec![], vec!["--json"]] {
        for mut args in [
            vec!["files", "sample-item"],
            vec!["docs", "sample-item"],
            vec!["test", "sample-item"],
            vec!["test", "sample-item", "--list"],
            vec![
                "files",
                "sample-item",
                "--add",
                "path=src/é.rs,scope=global,note=Unicode",
                "--add",
                "./src/../other.rs",
            ],
            vec!["files", "sample-item", "--add", "src/a.rs"],
            vec!["files", "sample-item", "--add", "9:artifact.rs"],
            vec!["files", "sample-item", "--add", "path=src/a.rs,note=new"],
            vec!["files", "sample-item", "--add", "path=src/z.rs,note=last"],
            vec![
                "files",
                "sample-item",
                "--add",
                "path=src/z.rs,note=new",
                "--note-conflict",
                "REPLACE",
                "--add",
                "path=src/z.rs,note=new",
                "--full-history",
            ],
            vec![
                "files",
                "sample-item",
                "--add",
                "z.rs",
                "--append-stable",
                "--note",
                " note ",
            ],
            vec![
                "docs",
                "sample-item",
                "--add",
                "path=docs/guide.md,note=Guide",
                "--add",
                "https://example.invalid/guide",
            ],
            vec!["files", "sample-item", "--remove", "src/a.rs"],
            vec![
                "files",
                "sample-item",
                "--remove",
                "path=src/a.rs",
                "--remove",
                "src/z.rs",
            ],
            vec!["docs", "sample-item", "--remove", "README.md"],
            vec!["files", "sample-item", "--remove", "missing"],
            vec![
                "files",
                "sample-item",
                "--add",
                "path=\"a,b.rs\",note=comma,continued",
            ],
            vec![
                "files",
                "sample-item",
                "--add",
                "- path: src/b.rs\n- note: Example",
            ],
            vec!["docs", "sample-item", "--add", "[Guide](docs/guide.md)"],
            vec!["test", "sample-item", "--add", "cargo test FILTER=x"],
            vec![
                "test",
                "sample-item",
                "--add",
                "cmd=cargo fmt,path=tests/fmt.rs,timeout=3,note=Formatting",
            ],
            vec![
                "test",
                "sample-item",
                "--add",
                "command=cargo fmt",
                "--add",
                "command=cargo clippy,note=Lint",
            ],
            vec![
                "test",
                "sample-item",
                "--add",
                "command=cargo test,path=tests/example.rs",
            ],
            vec!["test", "sample-item", "--remove", "cargo test"],
            vec!["test", "sample-item", "--remove", "command=cargo test"],
            vec!["test", "sample-item", "--remove", "path=tests/example.rs"],
            vec!["test", "sample-item", "--remove-index", "1"],
            vec![
                "test",
                "sample-item",
                "--add",
                "command=cargo fmt",
                "--remove-index",
                "1",
            ],
            vec![
                "test",
                "sample-item",
                "--remove-index",
                "1",
                "--remove-index",
                "1",
            ],
            vec!["test", "sample-item", "--remove", "tests/example.rs"],
            vec!["files", "sample-item", "--add", "path=src/a.rs,note=\"\""],
            vec!["files", "sample-missing"],
            vec![
                "files",
                "sample-item",
                "--add",
                r#"path="src/a,b.rs",note="say \"hello\"""#,
            ],
            vec![
                "test",
                "sample-item",
                "--add",
                r#"command="node -e \"console.log('a,b')\"""#,
            ],
            vec![
                "test",
                "sample-item",
                "--add",
                "command=cargo test,timeout=3.9,note=Fractional",
            ],
            vec!["files", "sample-item", "--remove-index", "1"],
            vec!["docs", "sample-item", "--remove-index", "1"],
            vec!["docs", "sample-missing", "--add", "README.md"],
            vec!["files", "sample-item", "--add", "scope=project"],
            vec!["files", "sample-item", "--add", "path=foo,wrong=x"],
            vec!["test", "sample-item", "--remove", "missing"],
            vec!["test", "sample-item", "--remove-index", "99"],
            vec!["test", "sample-item", "--remove-index", "0"],
            vec!["test", "sample-item", "--remove-index", "-1"],
            vec!["files", "sample-item", "--add", "path=foo,broken"],
            vec!["files", "sample-item", "--add", "a,b"],
            vec!["files", "sample-item", "--remove", "path=x,scope=project"],
            vec!["files", "sample-item", "--remove", "wrong=x"],
            vec!["files", "sample-item", "--add", "x", "--remove", "./x"],
            vec!["test", "sample-item", "--remove", "wrong=x"],
            vec!["test", "sample-item", "--add", "path=tests/a.rs"],
            vec![
                "test",
                "sample-item",
                "--add",
                "command=cargo fmt",
                "--note-conflict",
                "append",
            ],
        ]
        .into_iter()
        .rev()
        {
            fs::remove_dir_all(tracker.path().join(".agents"))?;
            copy_directory(
                &snapshot.path().join(".agents"),
                &tracker.path().join(".agents"),
            )?;
            args.extend(["--author", "alpha"]);
            args.extend(&json_flag);
            let expected = oracle(&driver, tracker.path(), &args)?;
            let after = tempfile::tempdir()?;
            copy_directory(
                &tracker.path().join(".agents"),
                &after.path().join(".agents"),
            )?;
            fs::remove_dir_all(tracker.path().join(".agents"))?;
            copy_directory(
                &snapshot.path().join(".agents"),
                &tracker.path().join(".agents"),
            )?;
            // Native --timestamp has no published counterpart. Do not inject it into refusal retries.
            let actual = native(tracker.path(), &args, expected.status.success())?;
            let expected = scoped_collection_restore(expected, &args)?;
            let comparison = std::panic::catch_unwind(|| {
                output(&expected, &actual, &args);
                artifacts(
                    &after.path().join(".agents/pm"),
                    &tracker.path().join(".agents/pm"),
                )
            });
            match comparison {
                Ok(Ok(())) => {}
                Ok(Err(error)) => {
                    eprintln!("{args:?}: artifact mismatch: {error}");
                    mismatches += 1;
                }
                Err(_) => {
                    eprintln!("{args:?}: output or artifact comparison panicked");
                    mismatches += 1;
                }
            }
        }
    }
    assert_eq!(mismatches, 0, "collection scenarios with byte differences");
    Ok(())
}

/// Both CLIs reject unsafe note edits before modifying any linked artifacts.
#[test]
#[allow(
    clippy::too_many_lines,
    reason = "Each case includes the complete CLI arguments"
)]
fn linked_note_recovery_matches_published_semantics() -> TestResult {
    let Some(published) = published_cli::published_cli_or_skip("linked artifact note recovery")
    else {
        return Ok(());
    };
    let scratch = tempfile::tempdir()?;
    let driver = write_driver(scratch.path(), &published)?;
    let tracker = tempfile::tempdir()?;
    let rust = tempfile::tempdir()?;
    let snapshot = tempfile::tempdir()?;
    for args in [
        vec!["init", "sample-", "--defaults"],
        vec![
            "create",
            "--id",
            "sample-item",
            "--title",
            "Notes",
            "--type",
            "Task",
            "--author",
            "alpha",
        ],
    ] {
        let result = oracle(&driver, tracker.path(), &args)?;
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
    for target in [rust.path(), snapshot.path()] {
        copy_directory(&tracker.path().join(".agents"), &target.join(".agents"))?;
    }
    let cases = [
        (
            vec![
                "files",
                "sample-item",
                "--add",
                "example.rs",
                "--note-conflict",
                "broken",
            ],
            "invalid_argument_value",
            2,
            0,
        ),
        (
            vec![
                "files",
                "sample-item",
                "--remove",
                "example.rs",
                "--note-conflict",
                "append",
            ],
            "missing_required_argument",
            2,
            0,
        ),
    ];
    for (command, path, extra) in [
        ("docs", "guide.md", vec![]),
        ("files", "source.rs", vec!["--append-stable"]),
    ] {
        let first = format!("path={path},note=first");
        let second = format!("path={path},note=second");
        let third = format!("path={path},note=third");
        let mut args = vec![
            command,
            "sample-item",
            "--add",
            &first,
            "--add",
            &second,
            "--add",
            &third,
            "--remove",
            "unrelated.md",
            "--note",
            "-draft",
            "--force",
            "--full-history",
        ];
        if command == "docs" {
            args.push("--message=-draft");
        } else {
            args.extend(["--message", "Link rationale"]);
        }
        args.extend(extra);
        assert_note_refusal(
            &driver,
            tracker.path(),
            rust.path(),
            &args,
            "linked_artifact_note_conflict",
            4,
            2,
        )?;
    }
    for (args, code, exit, conflicts) in cases {
        assert_note_refusal(
            &driver,
            tracker.path(),
            rust.path(),
            &args,
            code,
            exit,
            conflicts,
        )?;
    }
    for target in [tracker.path(), rust.path()] {
        artifacts(
            &snapshot.path().join(".agents/pm"),
            &target.join(".agents/pm"),
        )?;
    }
    Ok(())
}

/// A declared output flag cannot stand in for a note, while literal hyphen text can.
#[test]
fn collection_note_value_boundaries_match_oracle() -> TestResult {
    let Some(published) = published_cli::published_cli_or_skip("collection note value boundaries")
    else {
        return Ok(());
    };
    let scratch = tempfile::tempdir()?;
    let driver = write_driver(scratch.path(), &published)?;
    let published = tempfile::tempdir()?;
    let rust = tempfile::tempdir()?;
    let snapshot = tempfile::tempdir()?;
    for args in [
        &["init", "sample-", "--defaults"][..],
        &[
            "create",
            "--id",
            "sample-item",
            "--title",
            "Notes",
            "--type",
            "Task",
            "--author",
            "alpha",
        ],
    ] {
        let result = oracle(&driver, published.path(), args)?;
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
    for target in [rust.path(), snapshot.path()] {
        copy_directory(&published.path().join(".agents"), &target.join(".agents"))?;
    }
    let args = [
        "files",
        "sample-item",
        "--add",
        "refused.rs",
        "--author=alpha",
        "--note",
        "--json",
    ];
    let expected = oracle(&driver, published.path(), &args)?;
    let actual = native(rust.path(), &args, false)?;
    assert_eq!(expected.status.code(), Some(2));
    assert_eq!(actual.status.code(), expected.status.code());
    assert!(expected.stdout.is_empty() && actual.stdout.is_empty());
    let refusal: Value = serde_json::from_slice(&expected.stderr)?;
    assert_eq!(refusal["code"], "invalid_command_usage");
    assert!(String::from_utf8_lossy(&actual.stderr).contains("--note"));
    for target in [published.path(), rust.path()] {
        artifacts(
            &snapshot.path().join(".agents/pm"),
            &target.join(".agents/pm"),
        )?;
    }

    for (file, flags, note) in [
        ("draft.rs", &["--note", "-draft"][..], "-draft"),
        (
            "short-message.rs",
            &["--note", "-mreviewed"][..],
            "-mreviewed",
        ),
        ("short-literal.rs", &["--note=-mreviewed"][..], "-mreviewed"),
        (
            "unknown.rs",
            &["--note", "--unknown-note"][..],
            "--unknown-note",
        ),
        ("attached.rs", &["--note=--json"][..], "--json"),
    ] {
        let mut args = vec!["files", "sample-item", "--add", file, "--author=alpha"];
        args.extend(flags);
        args.extend(["--full-history", "--json"]);
        let expected = oracle(&driver, published.path(), &args)?;
        let actual = native(rust.path(), &args, true)?;
        assert!(
            expected.status.success(),
            "{}",
            String::from_utf8_lossy(&expected.stderr)
        );
        output(&expected, &actual, &args);
        let receipt: Value = serde_json::from_slice(&actual.stdout)?;
        let linked = receipt["files"].as_array().ok_or("linked files")?;
        assert!(
            linked
                .iter()
                .any(|row| row["path"] == file && row["note"] == note)
        );
    }
    artifacts(
        &published.path().join(".agents/pm"),
        &rust.path().join(".agents/pm"),
    )
}

/// Explicit and implicit note repair scope follows the published CLI's invocation.
#[test]
fn linked_note_retry_scope_matches_published_cli() -> TestResult {
    let Some(published) = published_cli::published_cli_or_skip("linked note retry scope") else {
        return Ok(());
    };
    let scratch = tempfile::tempdir()?;
    let driver = write_driver(scratch.path(), &published)?;
    let cwd = scratch.path().join("default");
    let selected = scratch.path().join("selected");
    fs::create_dir(&cwd)?;
    fs::create_dir(&selected)?;
    for args in [
        vec!["init", "sample-", "--defaults"],
        vec![
            "create",
            "--id",
            "sample-item",
            "--title",
            "Notes",
            "--type",
            "Task",
            "--author",
            "alpha",
        ],
        vec![
            "files",
            "sample-item",
            "--add",
            "path=example.rs,note=first",
            "--author",
            "alpha",
        ],
    ] {
        let result = oracle(&driver, &cwd, &args)?;
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
    copy_directory(&cwd.join(".agents"), &selected.join(".agents"))?;
    for scope in [&[][..], &["--pm-path", "../selected/.agents/pm"][..]] {
        let mut args = scope.to_vec();
        args.extend([
            "files",
            "sample-item",
            "--add",
            "path=example.rs,note=second",
            "--author",
            "alpha",
            "--json",
        ]);
        let expected = oracle(&driver, &cwd, &args)?;
        let actual = native(&cwd, &args, false)?;
        assert_eq!(expected.status.code(), Some(4));
        output(&expected, &actual, &args);
        let refusal: Value = serde_json::from_slice(&expected.stderr)?;
        let retry = refusal["recovery"]["suggested_retry_args"]
            .as_array()
            .ok_or("retry")?;
        assert_eq!(
            retry[..scope.len()],
            scope.iter().map(|arg| json!(arg)).collect::<Vec<_>>()
        );
        assert_eq!(retry[scope.len()], "files");
    }
    assert_native_only_note_flags(&driver, &cwd)?;
    // The published CLI strips structured retry fields for credential-bearing
    // notes and still displays redacted commands in its diagnostic next steps.
    let args = [
        "files",
        "sample-item",
        "--add",
        "path=example.rs,note=https://fixture:synthetic-secret@example.invalid",
        "--author",
        "alpha",
        "--json",
    ];
    let expected = oracle(&driver, &cwd, &args)?;
    let actual = native(&cwd, &args, false)?;
    assert_eq!(expected.status.code(), Some(4));
    output(&expected, &actual, &args);
    let refusal: Value = serde_json::from_slice(&expected.stderr)?;
    assert!(refusal["recovery"].get("suggested_retry_args").is_none());
    assert!(refusal["recovery"].get("suggested_retry").is_none());
    assert_eq!(refusal["next_steps"][0], pm_rust::refusal::RESTORE);
    assert!(
        refusal["next_steps"][1]
            .to_string()
            .contains("[redacted]@example.invalid")
    );
    Ok(())
}

/// A native default repair with a flag-looking note runs on the published executable.
#[test]
fn default_note_retry_replays_hyphen_notes_on_published_cli() -> TestResult {
    let Some(published) = published_cli::published_cli_or_skip("default hyphen note retry") else {
        return Ok(());
    };
    let scratch = tempfile::tempdir()?;
    let driver = write_driver(scratch.path(), &published)?;
    let project = tempfile::tempdir()?;
    for args in [
        vec!["init", "sample-", "--defaults"],
        vec![
            "create",
            "--id",
            "sample-item",
            "--title",
            "Notes",
            "--type",
            "Task",
            "--author",
            "alpha",
        ],
        vec![
            "files",
            "sample-item",
            "--add",
            "path=example.rs,note=first",
            "--add",
            "unrelated.md",
            "--author",
            "alpha",
        ],
    ] {
        let result = oracle(&driver, project.path(), &args)?;
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
    let args = [
        "files",
        "sample-item",
        "--add",
        "example.rs",
        "--remove",
        "unrelated.md",
        "--note=--json",
        "--author",
        "alpha",
        "--full-history",
        "--json",
    ];
    let refused = native(project.path(), &args, false)?;
    assert_eq!(refused.status.code(), Some(4));
    let receipt: Value = serde_json::from_slice(&refused.stderr)?;
    assert_eq!(receipt["code"], "linked_artifact_note_conflict");
    let mut retry = receipt["recovery"]["suggested_retry_args"]
        .as_array()
        .ok_or("retry arguments")?
        .iter()
        .map(|arg| arg.as_str().ok_or("retry token"))
        .collect::<Result<Vec<_>, _>>()?;
    assert_eq!(
        retry,
        [
            "files",
            "sample-item",
            "--add",
            "example.rs",
            "--remove",
            "unrelated.md",
            "--note=--json",
            "--note-conflict",
            "append",
            "--full-history",
            "--author",
            "alpha"
        ]
    );
    retry.push("--json");
    let applied = oracle(&driver, project.path(), &retry)?;
    assert!(
        applied.status.success(),
        "{}",
        String::from_utf8_lossy(&applied.stderr)
    );
    let listed: Value = serde_json::from_slice(&applied.stdout)?;
    assert_eq!(listed["mutation_receipt"]["removed_count"], 1);
    assert_eq!(
        listed["files"],
        json!([{"path":"example.rs","scope":"project","note":"first; --json"}])
    );
    Ok(())
}

/// Confirms which native recovery options the published executable refuses.
fn assert_native_only_note_flags(driver: &Path, cwd: &Path) -> TestResult {
    for (command, flags) in [
        ("files", &["--artifact-cwd", "."][..]),
        ("files", &["--timestamp", CLOCK][..]),
        ("docs", &["--append-stable"][..]),
    ] {
        let args = [&[command, "sample-item"][..], flags, &["--json"]].concat();
        let native_only = oracle(driver, cwd, &args)?;
        assert_eq!(native_only.status.code(), Some(2));
        let refusal: Value = serde_json::from_slice(&native_only.stderr)?;
        assert_eq!(refusal["code"], "unknown_option");
        assert_eq!(refusal["flag"], flags[0]);
    }
    Ok(())
}

/// Compares the refusal contract and conflict rows independently of each CLI's rendered guidance.
fn assert_note_refusal(
    driver: &Path,
    published: &Path,
    rust: &Path,
    args: &[&str],
    code: &str,
    exit: i32,
    conflicts: usize,
) -> TestResult {
    let mut args = args.to_vec();
    args.extend(["--author", "alpha", "--json"]);
    let expected = oracle(driver, published, &args)?;
    let hyphen_message = args.contains(&"--message=-draft");
    // A clock override advertises native-only replay guidance.
    let actual = native(rust, &args, !hyphen_message)?;
    let conflict_rows =
        [expected, actual].map(|output| -> Result<Value, Box<dyn std::error::Error>> {
            let stderr = String::from_utf8(output.stderr)?;
            assert_eq!(output.status.code(), Some(exit), "{args:?}: {stderr}");
            assert!(output.stdout.is_empty());
            let receipt: serde_json::Value = serde_json::from_str(&stderr)?;
            assert_eq!(receipt["code"], code, "{args:?}: {stderr}");
            if hyphen_message {
                let retry = receipt["recovery"]["suggested_retry_args"]
                    .as_array()
                    .ok_or("retry args")?;
                assert!(
                    retry.windows(2).any(|pair| pair == ["--message", "-draft"]),
                    "{retry:?}"
                );
            }
            Ok(receipt["details"]["conflicts"].clone())
        });
    let [expected, actual] = conflict_rows;
    let expected = expected?;
    assert_eq!(actual?, expected, "{args:?}: conflict rows");
    if conflicts > 0 {
        let rows = expected.as_array().ok_or("conflict rows")?;
        assert_eq!(rows.len(), conflicts);
        for (row, requested) in rows.iter().zip(["second", "third"]) {
            assert_eq!(row["existing_note"], "first");
            assert_eq!(row["requested_note"], requested);
            assert_eq!(row["source"], "this_write");
        }
    }
    Ok(())
}

/// Compares the 12-item search corpus in TOON.
#[test]
fn search_matches_oracle_small_toon() -> TestResult {
    search_fixture(12, false)
}

/// Compares the 12-item search corpus in JSON.
#[test]
fn search_matches_oracle_small_json() -> TestResult {
    search_fixture(12, true)
}

/// Compares the 220-item search corpus in TOON.
#[test]
fn search_matches_oracle_large_toon() -> TestResult {
    search_fixture(220, false)
}

/// Compares the 220-item search corpus in JSON.
#[test]
fn search_matches_oracle_large_json() -> TestResult {
    search_fixture(220, true)
}

/// Compares one independent real corpus and encoding across every search scenario.
#[allow(clippy::too_many_lines)]
fn search_fixture(size: usize, json_output: bool) -> TestResult {
    let Some(published) = published_cli::published_cli_or_skip("search differential") else {
        return Ok(());
    };
    let scratch = tempfile::tempdir()?;
    let driver = write_driver(scratch.path(), &published)?;
    let tracker = tempfile::tempdir()?;
    assert!(
        oracle(&driver, tracker.path(), &["init", "sample-", "--defaults"])?
            .status
            .success()
    );
    for index in 0..size {
        let item = json!({"id":format!("sample-{index:03}"),"title":(["Rust search","Search rust!","Unicode café 🚀","phrase-ish query"][index%4]),"description":"search engine Rust C++","type":if index%3==0 {"Feature"} else {"Task"},"status":(["open","closed","canceled","in_progress"][index%4]),"priority":index%5,"tags":[if index%2==0 {"native"} else {"RUST"}],"created_at":CLOCK,"updated_at":format!("2026-08-{:02}T10:00:00.000Z",index%20+1),"comments":[{"text":"comment-only rust","author":"fixture","created_at":CLOCK}],"notes":[{"text":"note-only"}],"learnings":[{"text":"learning-only"}],"files":[{"path":"excluded-file-term","scope":"project"}],"reminders":[{"at":CLOCK,"text":"reminder-only"}],"events":[{"start_at":CLOCK,"end_at":"2026-08-22T11:00:00.000Z","title":"event-only","all_day":true,"timezone":"UTC","recurrence":{"freq":"weekly","interval":2,"count":4,"by_weekday":["mon","wed"]}}],"dependencies":[{"id":"external-dependency","kind":"related"}],"plan_mode":"plan-only","plan_steps":[{"id":"step-1","order":1,"title":"plan-step-only","status":"pending","created_at":CLOCK,"updated_at":CLOCK,"body":"step body"}],"body":"Body-only punctuation: C++ café. rust rust"});
        let folder = if index % 3 == 0 { "features" } else { "tasks" };
        fs::create_dir_all(tracker.path().join(".agents/pm").join(folder))?;
        fs::write(
            tracker
                .path()
                .join(".agents/pm")
                .join(folder)
                .join(format!("sample-{index:03}.toon")),
            format!("{}\n", toon_format::encode_default(&item)?),
        )?;
    }
    let json_flag = if json_output { vec!["--json"] } else { vec![] };
    for base in [
        vec!["search", "rust"],
        vec!["search", "Rust search"],
        vec!["search", "rust", "search"],
        vec!["search", "café"],
        vec!["search", "C++"],
        vec!["search", "phrase-ish"],
        vec!["search", "rust\u{0085}search"],
        vec!["search", "\"rust search\""],
        vec!["search", "missing"],
        vec!["search", "sample-001"],
        vec!["search", "001"],
        vec!["search", "body-only"],
        vec!["search", "note-only"],
        vec!["search", "reminder-only"],
        vec!["search", "all day"],
        vec!["search", "weekly"],
        vec!["search", "external-dependency"],
        vec!["search", "plan-only"],
        vec!["search", "plan-step-only"],
        vec!["search", "rust", "--tag", "absent"],
        vec!["search", "rust", "--limit", "3.9"],
        vec!["search", "excluded-file-term"],
        vec!["search", "rust", "--type", "feature"],
        vec!["search", "rust", "--status", "closed"],
        vec!["search", "rust", "--status", "all"],
        vec!["search", "rust", "--tag", "native"],
        vec![
            "search",
            "rust",
            "--status",
            "open,closed",
            "--type",
            "Task",
            "--tag",
            "RUST",
        ],
        vec!["search", "rust", "--limit", "0"],
        vec!["search", "rust", "--limit", "-0"],
        vec!["search", "rust", "--limit", "0x10"],
        vec![
            "search",
            "rust",
            "--limit",
            "0x100000000000000000000000000000000",
            "--output-budget",
            "unbounded",
        ],
        vec![
            "search",
            "rust",
            "--limit",
            "1e100",
            "--output-budget",
            "unbounded",
        ],
        vec!["search", "rust", "--limit", "3"],
        vec![
            "search",
            "rust",
            "--limit",
            "220",
            "--output-budget",
            "unbounded",
        ],
        vec!["search", "rust", "--output-limit", "2"],
        vec!["search", "rust", "--output-budget", "unbounded"],
        vec!["search", "rust", "--output-budget", "1200"],
        vec!["search", "rust", "--output-budget", "1"],
    ] {
        let large_case = (base.len() == 2
            && [
                "rust",
                "Rust search",
                "café",
                "C++",
                "phrase-ish",
                "rust\u{0085}search",
                "missing",
            ]
            .contains(&base[1]))
            || base.contains(&"--limit")
            || base.contains(&"--output-budget")
            || base.contains(&"--output-limit")
            || base.contains(&"--status") && base.contains(&"--tag");
        if size == 220 && !large_case {
            continue;
        }
        let mut args = base;
        args.extend(&json_flag);
        let expected = oracle(&driver, tracker.path(), &args)?;
        let actual = native(tracker.path(), &args, false)?;
        output(&expected, &actual, &args);
        if json_flag.is_empty() {
            continue;
        }
        let value: Value = serde_json::from_slice(&expected.stdout)?;
        if let Some(cursor) = value["next_cursor"]
            .as_str()
            .filter(|_| args.contains(&"--limit"))
        {
            let mut continuation = args.clone();
            continuation.extend(["--after", cursor]);
            output(
                &oracle(&driver, tracker.path(), &continuation)?,
                &native(tracker.path(), &continuation, false)?,
                &continuation,
            );
        }
    }
    Ok(())
}

/// Two native or oracle writers preserve both additions and a serial canonical history.
#[test]
#[allow(clippy::too_many_lines)]
fn concurrent_collection_writers_match_serial_oracle() -> TestResult {
    let Some(published) = published_cli::published_cli_or_skip("collection concurrency") else {
        return Ok(());
    };
    let scratch = tempfile::tempdir()?;
    let driver = write_driver(scratch.path(), &published)?;
    let tracker = tempfile::tempdir()?;
    for args in [
        vec!["init", "sample-", "--defaults"],
        vec![
            "create",
            "--id",
            "sample-item",
            "--title",
            "Concurrent fixture",
            "--type",
            "Task",
            "--author",
            "alpha",
        ],
    ] {
        assert!(oracle(&driver, tracker.path(), &args)?.status.success());
    }
    let settings_path = tracker.path().join(".agents/pm/settings.json");
    let mut settings: Value = serde_json::from_str(&fs::read_to_string(&settings_path)?)?;
    settings["locks"]["wait_ms"] = json!(5000);
    fs::write(settings_path, serde_json::to_string_pretty(&settings)?)?;
    let snapshot = tempfile::tempdir()?;
    copy_directory(
        &tracker.path().join(".agents"),
        &snapshot.path().join(".agents"),
    )?;
    for native_writer in [true; 8].into_iter().chain([false]) {
        fs::remove_dir_all(tracker.path().join(".agents"))?;
        copy_directory(
            &snapshot.path().join(".agents"),
            &tracker.path().join(".agents"),
        )?;
        let results = std::thread::scope(|scope| {
            let handles: Vec<_> = ["src/a.rs", "src/b.rs"]
                .into_iter()
                .map(|path| {
                    let driver = &driver;
                    let root = tracker.path();
                    scope.spawn(move || {
                        let args = [
                            "files",
                            "sample-item",
                            "--add",
                            path,
                            "--author",
                            "alpha",
                            "--json",
                        ];
                        if native_writer {
                            native(root, &args, true).map_err(|e| e.to_string())
                        } else {
                            oracle(driver, root, &args).map_err(|e| e.to_string())
                        }
                    })
                })
                .collect();
            handles
                .into_iter()
                .map(|handle| handle.join().map_err(|_| "writer panic".to_owned())?)
                .collect::<Result<Vec<_>, String>>()
        })?;
        for result in results {
            assert!(
                result.status.success(),
                "{}",
                String::from_utf8_lossy(&result.stderr)
            );
        }
        let history =
            fs::read_to_string(tracker.path().join(".agents/pm/history/sample-item.jsonl"))?;
        let entries: Vec<Value> = history
            .lines()
            .map(serde_json::from_str)
            .collect::<Result<_, _>>()?;
        assert_eq!(entries.len(), 3);
        assert_eq!(
            entries[1]["after_hash"],
            entries[2]["before_hash"],
            "native_writer={native_writer}; entries={}",
            serde_json::to_string_pretty(&entries)?
        );
        let first = entries[1]["patch"]
            .as_array()
            .ok_or("missing patch")?
            .iter()
            .find(|entry| entry["path"] == "/metadata/files")
            .ok_or("missing file patch")?["value"][0]["path"]
            .as_str()
            .ok_or("missing path")?;
        let second = if first == "src/a.rs" {
            "src/b.rs"
        } else {
            "src/a.rs"
        };
        let after = tempfile::tempdir()?;
        copy_directory(
            &tracker.path().join(".agents"),
            &after.path().join(".agents"),
        )?;
        fs::remove_dir_all(tracker.path().join(".agents"))?;
        copy_directory(
            &snapshot.path().join(".agents"),
            &tracker.path().join(".agents"),
        )?;
        for path in [first, second] {
            assert!(
                oracle(
                    &driver,
                    tracker.path(),
                    &["files", "sample-item", "--add", path, "--author", "alpha"]
                )?
                .status
                .success()
            );
        }
        artifacts(
            &after.path().join(".agents/pm"),
            &tracker.path().join(".agents/pm"),
        )?;
    }
    Ok(())
}

/// Usage and cursor refusals preserve typed recovery envelopes in both formats.
#[test]
fn search_refusals_match_oracle() -> TestResult {
    let Some(published) = published_cli::published_cli_or_skip("search refusals") else {
        return Ok(());
    };
    let scratch = tempfile::tempdir()?;
    let driver = write_driver(scratch.path(), &published)?;
    let tracker = tempfile::tempdir()?;
    assert!(
        oracle(&driver, tracker.path(), &["init", "sample-", "--defaults"])?
            .status
            .success()
    );
    for format in [vec![], vec!["--json"]] {
        for mut args in [
            vec!["search", ""],
            vec!["search", "tag:native"],
            vec!["search", "rust", "--status", "Unknown"],
            vec!["search", "rust", "--status", "all,open"],
            vec!["search", "rust", "--type", "Unknown"],
            vec!["search", "rust", "--type", ""],
            vec!["search", "rust", "--tag", ""],
            vec!["search", "rust", "--limit", "-1"],
            vec!["search", "rust", "--limit", "NaN"],
            vec!["search", "rust", "--output-budget", "0"],
            vec!["search", "rust", "--output-budget", "-1"],
            vec!["search", "rust", "--output-budget", " unbounded "],
            vec!["search", "rust", "--output-limit", "invalid"],
            vec!["search", "rust", "--output-limit", "0"],
            vec!["search", "rust", "--after", "!"],
            vec!["search", "rust", "--output-cursor", "!"],
        ] {
            args.extend(&format);
            output(
                &oracle(&driver, tracker.path(), &args)?,
                &native(tracker.path(), &args, false)?,
                &args,
            );
        }
    }
    Ok(())
}

/// A bounded snapshot can be replayed exactly and refuses changed projected rows.
#[test]
fn search_output_cursor_replays_and_refuses_stale_rows() -> TestResult {
    let Some(published) = published_cli::published_cli_or_skip("search output cursor") else {
        return Ok(());
    };
    let scratch = tempfile::tempdir()?;
    let driver = write_driver(scratch.path(), &published)?;
    let tracker = tempfile::tempdir()?;
    assert!(
        oracle(&driver, tracker.path(), &["init", "sample-", "--defaults"])?
            .status
            .success()
    );
    for index in 0..220 {
        let item = json!({"id":format!("sample-{index:03}"),"title":format!("Rust {}", "long Unicode café 🚀 ".repeat(15)),"description":"fixture","type":"Task","status":"open","priority":index%5,"tags":[],"created_at":CLOCK,"updated_at":CLOCK,"body":""});
        fs::write(
            tracker
                .path()
                .join(".agents/pm/tasks")
                .join(format!("sample-{index:03}.toon")),
            format!("{}\n", toon_format::encode_default(&item)?),
        )?;
    }
    for format in [vec!["--json"], vec![]] {
        let mut args = vec![
            "search",
            "rust",
            "--limit",
            "220",
            "--output-budget",
            "1600",
        ];
        args.extend(&format);
        let expected = oracle(&driver, tracker.path(), &args)?;
        output(&expected, &native(tracker.path(), &args, false)?, &args);
        if format.is_empty() {
            continue;
        }
        let value: Value = serde_json::from_slice(&expected.stdout)?;
        let cursor = value["output_budget_truncation"]["continuations"][0]["cursor"]
            .as_str()
            .ok_or("missing output continuation")?;
        let mut replay = args.clone();
        replay.extend(["--output-cursor", cursor]);
        output(
            &oracle(&driver, tracker.path(), &replay)?,
            &native(tracker.path(), &replay, false)?,
            &replay,
        );
        let path = tracker.path().join(".agents/pm/tasks/sample-000.toon");
        let before = fs::read_to_string(&path)?;
        fs::write(
            &path,
            before.replace("description: fixture", "description: Changed"),
        )?;
        output(
            &oracle(&driver, tracker.path(), &replay)?,
            &native(tracker.path(), &replay, false)?,
            &replay,
        );
        // Description is outside compact projection; a changed title invalidates the snapshot.
        fs::write(&path, before.replace("title: Rust", "title: Rust changed"))?;
        output(
            &oracle(&driver, tracker.path(), &replay)?,
            &native(tracker.path(), &replay, false)?,
            &replay,
        );
        fs::write(path, before)?;
    }
    Ok(())
}

/// Local and hosted branch provenance is identical in stored tests and canonical history.
#[test]
#[allow(clippy::too_many_lines)]
fn collection_provenance_matches_oracle() -> TestResult {
    let Some(published) = published_cli::published_cli_or_skip("test provenance") else {
        return Ok(());
    };
    let scratch = tempfile::tempdir()?;
    let driver = write_driver(scratch.path(), &published)?;
    let tracker = tempfile::tempdir()?;
    for args in [
        vec!["init", "sample-", "--defaults"],
        vec![
            "create",
            "--id",
            "sample-item",
            "--title",
            "Provenance fixture",
            "--type",
            "Task",
            "--author",
            "alpha",
        ],
    ] {
        assert!(oracle(&driver, tracker.path(), &args)?.status.success());
    }
    assert!(
        Command::new("git")
            .current_dir(tracker.path())
            .args(["init", "--initial-branch", "fixture-branch"])
            .output()?
            .status
            .success()
    );
    let snapshot = tempfile::tempdir()?;
    copy_directory(
        &tracker.path().join(".agents"),
        &snapshot.path().join(".agents"),
    )?;
    for (head, reference) in [
        ("fixture-head", ""),
        ("", "fixture-ref"),
        ("", "merge"),
        ("", ""),
    ] {
        let args = [
            "test",
            "sample-item",
            "--add",
            "command=cargo test,note=Evidence",
            "--author",
            "alpha",
            "--json",
        ];
        let mut oracle = published_cli::oracle_command(
            std::env::var("PM_NODE_INTERPRETER").unwrap_or_else(|_| "node".to_owned()),
        );
        oracle.arg(&driver).current_dir(tracker.path()).env_clear();
        for key in [
            "PATH",
            "HOME",
            "SystemRoot",
            "SystemDrive",
            "windir",
            "TEMP",
            "TMP",
            "USERPROFILE",
            "COMSPEC",
            "PATHEXT",
        ] {
            if let Some(value) = std::env::var_os(key) {
                oracle.env(key, value);
            }
        }
        oracle
            .env("FIXED_CLOCK", CLOCK)
            .env("DO_NOT_TRACK", "1")
            .env("GITHUB_HEAD_REF", head)
            .env("GITHUB_REF_NAME", reference)
            .args(args);
        let expected = oracle.output()?;
        let after = tempfile::tempdir()?;
        copy_directory(
            &tracker.path().join(".agents"),
            &after.path().join(".agents"),
        )?;
        fs::remove_dir_all(tracker.path().join(".agents"))?;
        copy_directory(
            &snapshot.path().join(".agents"),
            &tracker.path().join(".agents"),
        )?;
        let actual = native_command(tracker.path(), &args, true)
            .env("GITHUB_HEAD_REF", head)
            .env("GITHUB_REF_NAME", reference)
            .output()?;
        output(&expected, &actual, &args);
        artifacts(
            &after.path().join(".agents/pm"),
            &tracker.path().join(".agents/pm"),
        )?;
        fs::remove_dir_all(tracker.path().join(".agents"))?;
        copy_directory(
            &snapshot.path().join(".agents"),
            &tracker.path().join(".agents"),
        )?;
    }
    fs::remove_dir_all(tracker.path().join(".git"))?;
    let args = [
        "test",
        "sample-item",
        "--add",
        "command=cargo test",
        "--author",
        "alpha",
        "--json",
    ];
    let expected = oracle(&driver, tracker.path(), &args)?;
    let after = tempfile::tempdir()?;
    copy_directory(
        &tracker.path().join(".agents"),
        &after.path().join(".agents"),
    )?;
    fs::remove_dir_all(tracker.path().join(".agents"))?;
    copy_directory(
        &snapshot.path().join(".agents"),
        &tracker.path().join(".agents"),
    )?;
    let actual = native_command(tracker.path(), &args, true)
        .env(
            "PM_SOURCE_WORKSPACE_ROOT",
            tracker.path().join("unavailable-source"),
        )
        .env("PATH", scratch.path())
        .output()?;
    output(&expected, &actual, &args);
    artifacts(
        &after.path().join(".agents/pm"),
        &tracker.path().join(".agents/pm"),
    )?;
    Ok(())
}

/// Strict foreign claims, live locks and stale-lock force controls match exact refusals.
#[test]
#[allow(clippy::too_many_lines)]
fn collection_safety_refusals_match_oracle() -> TestResult {
    let Some(published) = published_cli::published_cli_or_skip("collection safety") else {
        return Ok(());
    };
    let scratch = tempfile::tempdir()?;
    let driver = write_driver(scratch.path(), &published)?;
    let tracker = tempfile::tempdir()?;
    for args in [
        vec!["init", "sample-", "--defaults"],
        vec![
            "create",
            "--id",
            "sample-item",
            "--title",
            "Safety fixture",
            "--type",
            "Task",
            "--author",
            "alpha",
        ],
    ] {
        assert!(oracle(&driver, tracker.path(), &args)?.status.success());
    }
    let settings_path = tracker.path().join(".agents/pm/settings.json");
    let mut settings: Value = serde_json::from_str(&fs::read_to_string(&settings_path)?)?;
    settings["locks"]["wait_ms"] = json!(0);
    fs::write(settings_path, serde_json::to_string_pretty(&settings)?)?;
    let snapshot = tempfile::tempdir()?;
    copy_directory(
        &tracker.path().join(".agents"),
        &snapshot.path().join(".agents"),
    )?;
    for format in [vec![], vec!["--json"]] {
        for (state, force) in [
            ("claim", false),
            ("claim", true),
            ("lock", false),
            ("stale", false),
            ("stale", true),
        ] {
            fs::remove_dir_all(tracker.path().join(".agents"))?;
            copy_directory(
                &snapshot.path().join(".agents"),
                &tracker.path().join(".agents"),
            )?;
            if state == "claim" {
                assert!(
                    oracle(
                        &driver,
                        tracker.path(),
                        &["claim", "sample-item", "--author", "other"]
                    )?
                    .status
                    .success()
                );
            } else {
                let lease_timestamp = if state == "stale" {
                    "2000-01-01T00:00:00.000Z".to_owned()
                } else {
                    pm_rust::current_timestamp()
                };
                fs::write(
                    tracker.path().join(".agents/pm/locks/sample-item.lock"),
                    serde_json::to_string(
                        &json!({"id":"sample-item","pid":1,"owner":"holder","created_at":lease_timestamp,"ttl_seconds":1800,"token":"fixture"}),
                    )?,
                )?;
            }
            let before = tempfile::tempdir()?;
            copy_directory(
                &tracker.path().join(".agents"),
                &before.path().join(".agents"),
            )?;
            let mut args = vec![
                "files",
                "sample-item",
                "--add",
                "src/a.rs",
                "--author",
                "alpha",
            ];
            if force {
                args.push("--force");
            }
            args.extend(&format);
            let expected = oracle(&driver, tracker.path(), &args)?;
            let after = tempfile::tempdir()?;
            copy_directory(
                &tracker.path().join(".agents"),
                &after.path().join(".agents"),
            )?;
            fs::remove_dir_all(tracker.path().join(".agents"))?;
            copy_directory(
                &before.path().join(".agents"),
                &tracker.path().join(".agents"),
            )?;
            let actual = native(tracker.path(), &args, true)?;
            output(&expected, &actual, &args);
            artifacts(
                &after.path().join(".agents/pm"),
                &tracker.path().join(".agents/pm"),
            )?;
        }
    }
    Ok(())
}

/// Agent attribution, session principals and resolver outcomes participate in exact history hashes.
#[test]
#[allow(clippy::too_many_lines)]
fn collection_agent_attribution_matches_oracle() -> TestResult {
    let Some(published) = published_cli::published_cli_or_skip("collection attribution") else {
        return Ok(());
    };
    let scratch = tempfile::tempdir()?;
    let driver = write_driver(scratch.path(), &published)?;
    let tracker = tempfile::tempdir()?;
    for args in [
        vec!["init", "sample-", "--defaults"],
        vec![
            "create",
            "--id",
            "sample-item",
            "--title",
            "Attribution fixture",
            "--type",
            "Task",
            "--author",
            "alpha",
        ],
    ] {
        assert!(oracle(&driver, tracker.path(), &args)?.status.success());
    }
    let settings = tracker.path().join(".agents/pm/settings.json");
    let mut value: Value = serde_json::from_slice(&fs::read(&settings)?)?;
    value["author_default"] = json!("");
    value["governance"]["preset"] = json!("strict");
    fs::write(settings, serde_json::to_vec(&value)?)?;
    let snapshot = tempfile::tempdir()?;
    copy_directory(
        &tracker.path().join(".agents"),
        &snapshot.path().join(".agents"),
    )?;
    for (asserted, claimed, foreign, version, explicit_role) in [
        (true, false, false, false, true),
        (true, false, false, true, true),
        (false, false, false, false, true),
        (false, true, false, false, true),
        (false, true, true, false, true),
        (true, false, false, false, false),
        (false, true, false, false, false),
    ] {
        fs::remove_dir_all(tracker.path().join(".agents"))?;
        copy_directory(
            &snapshot.path().join(".agents"),
            &tracker.path().join(".agents"),
        )?;
        let mut args = vec!["files", "sample-item", "--add", "src/a.rs", "--json"];
        if asserted {
            args.extend(["--author", "alpha"]);
        }
        let mut actual = native_command(tracker.path(), &args, true);
        actual
            .env("CODEX_THREAD_ID", "synthetic-collection-session")
            .env("PM_AGENT_MODEL", "synthetic-model")
            .env("PM_AGENT_EFFORT", "high")
            .env("PM_AGENT_ROLE", "implementer")
            .env("PM_AGENT_TOPIC", "collection-fixture");
        if version {
            actual.env("PM_AGENT_VERSION", "fixture-version");
        }
        if !explicit_role {
            actual
                .env_remove("PM_AGENT_ROLE")
                .env_remove("PM_AGENT_TOPIC");
        }
        let oracle_command = |args: &[&str], environment: &Command| {
            let mut command = published_cli::oracle_command(
                std::env::var("PM_NODE_INTERPRETER").unwrap_or_else(|_| "node".to_owned()),
            );
            command
                .env_clear()
                .current_dir(tracker.path())
                .arg(&driver)
                .args(args)
                .env("FIXED_CLOCK", CLOCK)
                .env("DO_NOT_TRACK", "1");
            for (key, value) in environment.get_envs() {
                if let Some(value) = value {
                    command.env(key, value);
                }
            }
            command
        };
        if claimed {
            let receipt = oracle_command(&["claim", "sample-item", "--json"], &actual).output()?;
            assert!(
                receipt.status.success(),
                "{}",
                String::from_utf8_lossy(&receipt.stderr)
            );
        }
        if foreign {
            actual.env("CODEX_THREAD_ID", "synthetic-foreign-session");
        }
        let before = tempfile::tempdir()?;
        copy_directory(
            &tracker.path().join(".agents"),
            &before.path().join(".agents"),
        )?;
        let expected = oracle_command(&args, &actual).output()?;
        let after = tempfile::tempdir()?;
        copy_directory(
            &tracker.path().join(".agents"),
            &after.path().join(".agents"),
        )?;
        fs::remove_dir_all(tracker.path().join(".agents"))?;
        copy_directory(
            &before.path().join(".agents"),
            &tracker.path().join(".agents"),
        )?;
        output(&expected, &actual.output()?, &args);
        artifacts(
            &after.path().join(".agents/pm"),
            &tracker.path().join(".agents/pm"),
        )?;
    }
    Ok(())
}

/// Synthetic session files reproduce provider probes and their exact immutable history evidence.
#[test]
#[allow(clippy::too_many_lines)]
fn collection_local_probe_history_matches_oracle() -> TestResult {
    use std::io::{Seek as _, SeekFrom, Write as _};
    let Some(published) = published_cli::published_cli_or_skip("collection local probes") else {
        return Ok(());
    };
    let scratch = tempfile::tempdir()?;
    let driver = write_driver(scratch.path(), &published)?;
    let tracker = tempfile::tempdir()?;
    for args in [
        vec!["init", "sample-", "--defaults"],
        vec![
            "create",
            "--id",
            "sample-item",
            "--title",
            "Probe fixture",
            "--type",
            "Task",
            "--author",
            "alpha",
        ],
    ] {
        assert!(oracle(&driver, tracker.path(), &args)?.status.success());
    }
    let snapshot = tempfile::tempdir()?;
    copy_directory(
        &tracker.path().join(".agents"),
        &snapshot.path().join(".agents"),
    )?;
    for (provider, large, mode) in [
        ("codex", false, "file"),
        ("codex", true, "file"),
        ("claude-code", false, "file"),
        ("claude-code", true, "file"),
        ("codex", false, "disabled"),
        ("codex", false, "missing"),
        ("claude-code", false, "alias"),
        ("codex", false, "alias"),
        ("pi", false, "environment"),
    ] {
        fs::remove_dir_all(tracker.path().join(".agents"))?;
        copy_directory(
            &snapshot.path().join(".agents"),
            &tracker.path().join(".agents"),
        )?;
        let home = tempfile::tempdir()?;
        let codex = provider == "codex";
        let path = home.path().join(if codex {
            ".codex/sessions/year/rollout-fixture-session.jsonl"
        } else {
            ".claude/projects/project/fixture-session.jsonl"
        });
        fs::create_dir_all(path.parent().ok_or("session parent")?)?;
        if mode != "missing" {
            let mut file = fs::File::create(&path)?;
            file.write_all(if codex {b"{\"type\":\"turn_context\",\"payload\":{\"model\":\"fixture-model\",\"effort\":\"low\"}}\n"}else{b"{\"message\":{\"model\":\"fixture-model\",\"version\":\"1.2.3\"}}\n"})?;
            if large {
                file.set_len(if codex { 4_195_328 } else { 1_049_600 })?;
                file.seek(SeekFrom::End(-100))?;
                file.write_all(if codex {b"\n{\"type\":\"turn_context\",\"payload\":{\"model\":\"latest-model\",\"effort\":\"high\"}}\n"}else{b"\n{\"model\":\"tail-model\",\"version\":\"9.9.9\"}\n"})?;
            }
        }
        let args = [
            "files",
            "sample-item",
            "--add",
            "src/a.rs",
            "--author",
            "alpha",
            "--json",
        ];
        let mut actual = native_command(tracker.path(), &args, true);
        actual
            .env("HOME", home.path())
            .env("USERPROFILE", home.path())
            .env("PM_AGENT_ROLE", "implementer")
            .env("PM_AGENT_TOPIC", "probe-fixture");
        match provider {
            "codex" => {
                actual.env("CODEX_THREAD_ID", "fixture-session").env(
                    "AI_AGENT",
                    if mode == "missing" {
                        "codex"
                    } else {
                        "codex/1.2.3"
                    },
                );
            }
            "pi" => {
                actual
                    .env("PI_AGENT", "1")
                    .env("PI_SESSION_ID", "fixture-session")
                    .env("PI_MODEL", "fixture-model");
            }
            _ => {
                actual
                    .env("CLAUDE_CODE", "1")
                    .env("CLAUDE_CODE_SESSION_ID", "fixture-session");
            }
        }
        if mode == "disabled" {
            actual.env("PM_AGENT_PROBES", "off");
        }
        if mode == "alias" {
            actual.env(
                if codex {
                    "CODEX_EFFORT"
                } else {
                    "CLAUDE_MODEL"
                },
                "fixture-alias",
            );
        }
        let mut expected = published_cli::oracle_command(
            std::env::var("PM_NODE_INTERPRETER").unwrap_or_else(|_| "node".to_owned()),
        );
        expected
            .env_clear()
            .current_dir(tracker.path())
            .arg(&driver)
            .args(args)
            .env("FIXED_CLOCK", CLOCK)
            .env("DO_NOT_TRACK", "1");
        for (key, value) in actual.get_envs() {
            if let Some(value) = value {
                expected.env(key, value);
            }
        }
        let expected = expected.output()?;
        let after = tempfile::tempdir()?;
        copy_directory(
            &tracker.path().join(".agents"),
            &after.path().join(".agents"),
        )?;
        fs::remove_dir_all(tracker.path().join(".agents"))?;
        copy_directory(
            &snapshot.path().join(".agents"),
            &tracker.path().join(".agents"),
        )?;
        output(&expected, &actual.output()?, &args);
        artifacts(
            &after.path().join(".agents/pm"),
            &tracker.path().join(".agents/pm"),
        )?;
    }
    Ok(())
}

/// Flag placement and multiword queries preserve the oracle's refusal bytes.
#[test]
fn reordered_refusal_arguments_match_oracle() -> TestResult {
    let Some(published) = published_cli::published_cli_or_skip("reordered refusals") else {
        return Ok(());
    };
    let scratch = tempfile::tempdir()?;
    let driver = write_driver(scratch.path(), &published)?;
    let tracker = tempfile::tempdir()?;
    assert!(
        oracle(&driver, tracker.path(), &["init", "sample-", "--defaults"])?
            .status
            .success()
    );
    for args in [
        vec!["search", "--type", "Unknown", "rust", "search", "--json"],
        vec!["search", "--type", "task", "rust", "search", "--json"],
        vec!["search", "rust", "search", "--type", "task", "--json"],
        vec!["files", "--author", "alpha", "sample-missing", "--json"],
        vec!["docs", "--author", "alpha", "sample-missing", "--json"],
        vec!["test", "--author", "alpha", "sample-missing", "--json"],
    ] {
        output(
            &oracle(&driver, tracker.path(), &args)?,
            &native(tracker.path(), &args, false)?,
            &args,
        );
    }
    Ok(())
}

/// Closed-domain recovery preserves exact output for both type flag spellings.
#[test]
fn search_type_spellings_match_oracle() -> TestResult {
    let Some(published) = published_cli::published_cli_or_skip("type spellings") else {
        return Ok(());
    };
    let scratch = tempfile::tempdir()?;
    let driver = write_driver(scratch.path(), &published)?;
    let tracker = tempfile::tempdir()?;
    assert!(
        oracle(&driver, tracker.path(), &["init", "sample-", "--defaults"])?
            .status
            .success()
    );
    for json_flag in [vec![], vec!["--json"]] {
        for mut args in [
            vec!["search", "rust", "--type", "Unknown"],
            vec!["search", "rust", "--type=Unknown"],
            vec!["search", "--type", "Unknown", "rust", "search"],
            vec!["search", "--type=Unknown", "rust", "search"],
        ] {
            args.extend(&json_flag);
            let expected = oracle(&driver, tracker.path(), &args)?;
            assert_eq!(expected.status.code(), Some(2));
            output(&expected, &native(tracker.path(), &args, false)?, &args);
        }
    }
    Ok(())
}

/// Every history-writing CLI mutation retains bounded synthetic probe evidence and exact hashes.
#[test]
#[allow(clippy::too_many_lines)]
fn mutation_probe_history_matches_oracle() -> TestResult {
    let Some(published) = published_cli::published_cli_or_skip("mutation probes") else {
        return Ok(());
    };
    let scratch = tempfile::tempdir()?;
    let driver = write_driver(scratch.path(), &published)?;
    let tracker = tempfile::tempdir()?;
    for args in [
        vec!["init", "sample-", "--defaults"],
        vec![
            "create",
            "--id",
            "sample-item",
            "--title",
            "Probe fixture",
            "--type",
            "Task",
            "--author",
            "alpha",
        ],
    ] {
        assert!(oracle(&driver, tracker.path(), &args)?.status.success());
    }
    let snapshot = tempfile::tempdir()?;
    copy_directory(
        &tracker.path().join(".agents"),
        &snapshot.path().join(".agents"),
    )?;
    for mut args in [
        vec![
            "create",
            "--id",
            "sample-other",
            "--title",
            "New fixture",
            "--type",
            "Task",
        ],
        vec!["update", "sample-item", "--title", "Changed fixture"],
        vec!["comment", "sample-item", "Probe comment"],
        vec!["claim", "sample-item"],
        vec!["release", "sample-item"],
        vec!["close", "sample-item", "Completed"],
        vec!["close", "sample-item", "Completed", "--release-assignment"],
    ] {
        fs::remove_dir_all(tracker.path().join(".agents"))?;
        copy_directory(
            &snapshot.path().join(".agents"),
            &tracker.path().join(".agents"),
        )?;
        if args[0] == "release" || args.contains(&"--release-assignment") {
            assert!(
                oracle(
                    &driver,
                    tracker.path(),
                    &["claim", "sample-item", "--author", "alpha"]
                )?
                .status
                .success()
            );
        }
        args.extend(["--author", "alpha"]);
        let home = tempfile::tempdir()?;
        let path = home
            .path()
            .join(".codex/sessions/year/rollout-fixture-session.jsonl");
        fs::create_dir_all(path.parent().ok_or("session parent")?)?;
        fs::write(
            path,
            "{\"type\":\"turn_context\",\"payload\":{\"model\":\"fixture-model\",\"effort\":\"high\"}}\n",
        )?;
        let mut actual = native_command(tracker.path(), &args, true);
        actual
            .env("HOME", home.path())
            .env("USERPROFILE", home.path())
            .env("CODEX_THREAD_ID", "fixture-session");
        let before = tempfile::tempdir()?;
        copy_directory(
            &tracker.path().join(".agents"),
            &before.path().join(".agents"),
        )?;
        let mut expected = published_cli::oracle_command(
            std::env::var("PM_NODE_INTERPRETER").unwrap_or_else(|_| "node".to_owned()),
        );
        expected
            .env_clear()
            .current_dir(tracker.path())
            .arg(&driver)
            .args(&args)
            .env("FIXED_CLOCK", CLOCK)
            .env("DO_NOT_TRACK", "1");
        for (key, value) in actual.get_envs() {
            if let Some(value) = value {
                expected.env(key, value);
            }
        }
        let expected = expected.output()?;
        assert!(
            expected.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&expected.stderr)
        );
        let after = tempfile::tempdir()?;
        copy_directory(
            &tracker.path().join(".agents"),
            &after.path().join(".agents"),
        )?;
        fs::remove_dir_all(tracker.path().join(".agents"))?;
        copy_directory(
            &before.path().join(".agents"),
            &tracker.path().join(".agents"),
        )?;
        let actual = actual.output()?;
        assert!(
            actual.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&actual.stderr)
        );
        artifacts(
            &after.path().join(".agents/pm"),
            &tracker.path().join(".agents/pm"),
        )?;
    }
    Ok(())
}

/// The oracle accepts future recipe timestamps for unlocked collection writes.
#[test]
fn future_collection_timestamps_match_unlocked_oracle() -> TestResult {
    const FUTURE: &str = "9999-12-31T23:59:59.999Z";
    let Some(published) = published_cli::published_cli_or_skip("future collection clocks") else {
        return Ok(());
    };
    let scratch = tempfile::tempdir()?;
    let driver = write_driver(scratch.path(), &published)?;
    let tracker = tempfile::tempdir()?;
    for args in [
        vec!["init", "sample-", "--defaults"],
        vec![
            "create",
            "--id",
            "sample-item",
            "--title",
            "Clock fixture",
            "--type",
            "Task",
            "--author",
            "alpha",
        ],
    ] {
        assert!(oracle(&driver, tracker.path(), &args)?.status.success());
    }
    fs::write(
        &driver,
        fs::read_to_string(&driver)?.replace("process.env.FIXED_CLOCK", &format!("\"{FUTURE}\"")),
    )?;
    for (command, add) in [
        ("files", "src/a.rs"),
        ("docs", "README.md"),
        ("test", "command=cargo test"),
    ] {
        let args = [
            command,
            "sample-item",
            "--add",
            add,
            "--author",
            "alpha",
            "--json",
        ];
        let before = tempfile::tempdir()?;
        copy_directory(
            &tracker.path().join(".agents"),
            &before.path().join(".agents"),
        )?;
        let expected = oracle(&driver, tracker.path(), &args)?;
        assert!(
            expected.status.success(),
            "{}",
            String::from_utf8_lossy(&expected.stderr)
        );
        let after = tempfile::tempdir()?;
        copy_directory(
            &tracker.path().join(".agents"),
            &after.path().join(".agents"),
        )?;
        fs::remove_dir_all(tracker.path().join(".agents"))?;
        copy_directory(
            &before.path().join(".agents"),
            &tracker.path().join(".agents"),
        )?;
        output(
            &expected,
            &native_command(tracker.path(), &args, false)
                .args(["--timestamp", FUTURE])
                .output()?,
            &args,
        );
        artifacts(
            &after.path().join(".agents/pm"),
            &tracker.path().join(".agents/pm"),
        )?;
    }
    Ok(())
}

/// Status CSV normalization matches the oracle for all-status and individual tokens.
#[test]
fn search_status_csv_matches_oracle() -> TestResult {
    let Some(published) = published_cli::published_cli_or_skip("search status CSV") else {
        return Ok(());
    };
    let scratch = tempfile::tempdir()?;
    let driver = write_driver(scratch.path(), &published)?;
    let tracker = tempfile::tempdir()?;
    assert!(
        oracle(&driver, tracker.path(), &["init", "sample-", "--defaults"])?
            .status
            .success()
    );
    for status in ["open", "closed", "canceled"] {
        let id = format!("sample-{status}");
        let created = oracle(
            &driver,
            tracker.path(),
            &[
                "create",
                "--id",
                &id,
                "--title",
                "Rust fixture",
                "--type",
                "Task",
                "--status",
                status,
                "--author",
                "alpha",
                "--close-reason",
                "Synthetic search fixture",
            ],
        )?;
        assert!(
            created.status.success(),
            "{}",
            String::from_utf8_lossy(&created.stderr)
        );
    }
    for status in [
        "all,",
        " all , ",
        ",,AlL,,",
        "",
        ", ,",
        " closed, ",
        " open, ,closed,",
        "all,closed",
        "all,all",
    ] {
        for json_output in [false, true] {
            let mut args = vec!["search", "rust", "--status", status];
            if json_output {
                args.push("--json");
            }
            let expected = oracle(&driver, tracker.path(), &args)?;
            if json_output && ["all,", " all , ", ",,AlL,,", "", ", ,"].contains(&status) {
                assert_eq!(
                    serde_json::from_slice::<Value>(&expected.stdout)?["count"],
                    3
                );
            }
            output(&expected, &native(tracker.path(), &args, false)?, &args);
        }
    }
    Ok(())
}
