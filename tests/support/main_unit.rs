use serde_json::Map;
use std::fs;
use std::io;

use pm_rust::{ItemDocument, ItemFilter, ItemMetadata, ListResult};

use super::{Cli, Command, run, write_json_to};

/// Bootstrap scope is explicit input, independent of incidental test-runner arguments.
#[test]
fn round6_bootstrap_scope_is_independent_of_harness_arguments() {
    for (raw, expected) in [
        (vec![], vec![]),
        (vec!["--native"], vec![]),
        (vec!["--native", "--", "--workspace=literal"], vec![]),
        (vec!["--native", "close", "--workspace=literal"], vec![]),
        (vec!["--native", "--workspace"], vec![]),
        (
            vec!["--native", "--workspace", "tracker with spaces", "claim"],
            vec!["--pm-path", "tracker with spaces"],
        ),
        (
            vec!["--native", "--workspace=tracker", "claim"],
            vec!["--pm-path", "tracker"],
        ),
    ] {
        let raw = raw.into_iter().map(str::to_owned).collect::<Vec<_>>();
        assert_eq!(super::invocation_scope(&raw), expected, "{raw:?}");
    }
}

/// Orientation dispatch preserves budget exits and opt-in strict history verification.
#[test]
fn orientation_dispatch_boundaries() -> Result<(), Box<dyn std::error::Error>> {
    use clap::Parser as _;
    let root = tempfile::tempdir()?;
    let pm = root.path().join(".agents/pm");
    fs::create_dir_all(pm.join("tasks"))?;
    fs::create_dir(pm.join("history"))?;
    fs::write(pm.join("settings.json"), "{}")?;
    let item = serde_json::json!({"id":"sample-a","title":"Orientation","type":"Task","status":"open","priority":1,"created_at":"2026-08-22T10:00:00.000Z","updated_at":"2026-08-22T10:00:00.000Z"});
    fs::write(
        pm.join("tasks/sample-a.toon"),
        toon_format::encode_default(&item)?,
    )?;
    for (flags, ok) in [
        (
            vec!["context", "--limit", "1", "--json", "--no-extension-health"],
            true,
        ),
        (
            vec!["context", "--token-budget", "1", "--no-extension-health"],
            false,
        ),
        (vec!["history", "sample-a", "--compact"], true),
        (vec!["history", "sample-a", "--full", "--json"], true),
        (vec!["history", "sample-a", "--full", "--compact"], false),
        (vec!["history", "sample-a", "--strict-exit"], false),
        (vec!["history", "sample-a", "--verify"], true),
        (
            vec!["history", "sample-a", "--verify", "--strict-exit"],
            false,
        ),
        (vec!["deps", "sample-a", "--json"], true),
    ] {
        let mut arguments = vec![
            "pm-rust",
            "--workspace",
            root.path().to_str().ok_or("UTF8")?,
        ];
        arguments.extend(flags);
        assert_eq!(run(Cli::try_parse_from(arguments)?).is_ok(), ok);
    }
    Ok(())
}

/// Orientation refusals retain recovery namespaces across invalid values and missing identities.
#[test]
fn orientation_error_recovery_contract() {
    let strings = |args: &[&str]| args.iter().map(|arg| (*arg).to_owned()).collect::<Vec<_>>();
    for (args, detail, context) in [
        (
            vec!["context", "--limit", "bad"],
            "--limit must be a non-negative integer",
            serde_json::json!({"code":"invalid_argument_value"}),
        ),
        (
            vec!["deps", "missing"],
            "Item missing not found",
            serde_json::json!({"code":"item_not_found","id":"missing"}),
        ),
        (
            vec!["history", "sample-a", "--strict-exit"],
            "--strict-exit requires --verify (it gates on the verification result).",
            serde_json::json!({}),
        ),
    ] {
        let result = super::orientation_error_json(detail, &context, &strings(&args), 2);
        assert_eq!(result["exit_code"], 2);
        assert!(result["recovery"].is_object());
    }
    assert_eq!(
        super::published_read_arguments([
            "context",
            "--timestamp",
            "ignored",
            "--workspace=ignored",
            "--limit",
            "1"
        ]),
        strings(&["context", "--workspace=ignored", "--limit", "1"])
    );
}

/// One owned argument stream preserves command namespaces, hidden controls and close aliases.
#[test]
fn owned_published_argument_streams() {
    let cases: &[(&[&str], &[&str])] = &[
        (&[], &[]),
        (
            &[
                "context",
                "--workspace",
                "fixture",
                "--timestamp",
                "clock",
                "--workspace=fixture",
                "--timestamp=clock",
                "--workspace-extra=kept",
                "--timestamp-extra=kept",
                "--json",
            ],
            &[
                "context",
                "--workspace",
                "fixture",
                "--workspace=fixture",
                "--workspace-extra=kept",
                "--timestamp-extra=kept",
                "--json",
            ],
        ),
        (
            &[
                "close",
                "sample-a",
                "--expected",
                "expected",
                "--actual",
                "actual",
                "--json",
            ],
            &[
                "close",
                "sample-a",
                "--expected-result",
                "expected",
                "--actual-result",
                "actual",
                "--json",
            ],
        ),
        (&["item", "deps", "sample-a"], &["deps", "sample-a"]),
        (&["files", "sample-a"], &["item", "files", "sample-a"]),
        (&["docs", "sample-a"], &["item", "docs", "sample-a"]),
        (&["test", "sample-a"], &["item", "test", "sample-a"]),
    ];
    for (input, expected) in cases {
        let arguments: Vec<String> = input.iter().map(|arg| (*arg).to_owned()).collect();
        assert_eq!(super::published_read_arguments(arguments), *expected);
    }
}

/// New command dispatchers persist real collections and propagate read/mutation failures.
#[test]
fn search_and_collection_dispatch_boundaries() -> Result<(), Box<dyn std::error::Error>> {
    use clap::Parser as _;
    let directory = tempfile::tempdir()?;
    let root = directory.path().join(".agents/pm");
    fs::create_dir_all(root.join("tasks"))?;
    fs::write(root.join("settings.json"), "{}")?;
    fs::create_dir(root.join("history"))?;
    let item = serde_json::json!({"id":"sample-a","title":"Rust search","type":"Task","status":"open","priority":1,"created_at":"2026-08-22T10:00:00.000Z","updated_at":"2026-08-22T10:00:00.000Z"});
    fs::write(
        root.join("tasks/sample-a.toon"),
        toon_format::encode_default(&item)?,
    )?;
    for flags in [
        vec!["search", "rust", "--json"],
        vec!["search", "rust"],
        vec!["files", "sample-a", "--json"],
        vec!["docs", "sample-a"],
        vec!["test", "sample-a", "--list"],
        vec![
            "files",
            "sample-a",
            "--add",
            "src/lib.rs",
            "--author",
            "fixture",
        ],
        vec![
            "docs",
            "sample-a",
            "--add",
            "README.md",
            "--author",
            "fixture",
        ],
        vec![
            "test",
            "sample-a",
            "--add",
            "command=cargo test",
            "--author",
            "fixture",
        ],
    ] {
        let mut args = vec![
            "pm-rust",
            "--workspace",
            directory.path().to_str().ok_or("invalid path")?,
        ];
        args.extend(flags);
        run(Cli::try_parse_from(args)?)?;
    }
    for flags in [
        vec!["search", "rust", "--output-budget", "1"],
        vec!["search", "rust", "--after", "!"],
        vec!["files", "missing"],
        vec!["docs", "missing"],
        vec!["test", "missing"],
    ] {
        let mut args = vec![
            "pm-rust",
            "--workspace",
            directory.path().to_str().ok_or("invalid path")?,
        ];
        args.extend(flags);
        assert!(run(Cli::try_parse_from(args)?).is_err());
    }
    let workspace = pm_rust::Workspace::discover(directory.path())?;
    assert_eq!(
        workspace.get("sample-a")?.metadata.extra["files"][0]["path"],
        "src/lib.rs"
    );
    let page = workspace.search("rust", &pm_rust::SearchOptions::default())?;
    assert!(super::write_search_to(&mut WriteFailure, &page, true).is_err());
    let unavailable = fs::canonicalize(directory.path().join("absent-cwd"));
    assert!(
        super::run_collection_with_cwd(
            &workspace,
            "files",
            super::CollectionArgs {
                id: "sample-a".to_owned(),
                ..super::CollectionArgs::default()
            },
            unavailable
        )
        .is_err()
    );
    fs::write(root.join("settings.json"), "invalid")?;
    assert!(
        super::run_collection(
            &workspace,
            "files",
            super::CollectionArgs {
                id: "sample-a".to_owned(),
                ..super::CollectionArgs::default()
            }
        )
        .is_err()
    );
    fs::remove_file(root.join("settings.json"))?;
    assert!(super::run_collection(&workspace, "files", super::CollectionArgs::default()).is_err());
    Ok(())
}

/// Search usage refusals distinguish values, required inputs and output-control namespaces.
#[test]
fn search_error_value_and_output_control_guidance() {
    let strings = |args: &[&str]| args.iter().map(|arg| (*arg).to_owned()).collect::<Vec<_>>();
    for (flag, detail, surface) in [
        (
            "--limit",
            "--limit must be a non-negative number",
            "--limit",
        ),
        (
            "--tag",
            "--tag requires at least one non-empty value",
            "--tag",
        ),
        (
            "--output-budget",
            "--output-budget must be a positive integer or unbounded.",
            "search",
        ),
        (
            "--output-limit",
            "--output-limit must be a positive integer or unbounded.",
            "search",
        ),
    ] {
        let args = strings(&["search", " q ", flag, "invalid"]);
        let payload = super::command_error_json(detail, &serde_json::json!({}), &args, 2);
        assert_eq!(payload["code"], "invalid_argument_value");
        assert_eq!(payload["refusal"]["surface"], surface);
        assert_eq!(payload["recovery"]["normalized_args"][1], " q ");
        assert!(payload["next_steps"][0].is_string());
    }
}

/// Structured errors retain closed-domain guidance, command namespaces and omitted values.
#[test]
fn collection_and_search_error_guidance_edges() -> Result<(), Box<dyn std::error::Error>> {
    let strings = |args: &[&str]| args.iter().map(|arg| (*arg).to_owned()).collect::<Vec<_>>();
    for args in [
        strings(&["search", "q", "--type", "Unknown"]),
        strings(&["search", "q", "--json"]),
        strings(&["search", "q", "--type"]),
    ] {
        let context = serde_json::json!({"code":"invalid_argument_value","recovery":{"allowed_values":["Task"]},"examples":["pm search <query> --help"],"next_steps":["Retry"]});
        let payload = super::command_error_json("--type must be Task", &context, &args, 2);
        assert_eq!(
            payload["recovery"]["allowed_values"],
            serde_json::json!(["Task"])
        );
        let mut bytes = Vec::new();
        super::write_cursor_refusal_to(&mut bytes, &payload)?;
        assert!(String::from_utf8(bytes)?.contains("allowed_values: Task"));
    }
    for (detail, context, args) in [
        (
            "Search query must not be empty",
            serde_json::json!({}),
            strings(&["search", ""]),
        ),
        (
            "Invalid --tag value",
            serde_json::json!({}),
            strings(&["search", "q", "--tag", ""]),
        ),
        (
            "Other",
            serde_json::json!({"examples":["pm search <query> --help"]}),
            strings(&["search", "q"]),
        ),
        (
            "Other",
            serde_json::json!({"examples": "custom"}),
            strings(&["search", "q"]),
        ),
        (
            "Other",
            serde_json::json!({"examples": [null, "custom"]}),
            strings(&["search", "q"]),
        ),
        (
            "Item missing not found",
            serde_json::json!({}),
            strings(&["item", "files", "missing"]),
        ),
        (
            "Item missing not found",
            serde_json::json!({"next_steps":["Hint"]}),
            strings(&["item", "docs", "missing"]),
        ),
        (
            "--add requires path",
            serde_json::json!({}),
            strings(&["item", "files", "sample-a", "--add", "scope=x"]),
        ),
        (
            "Invalid scope",
            serde_json::json!({}),
            strings(&["item", "test", "sample-a"]),
        ),
        (
            "Other",
            serde_json::json!({"code":"custom"}),
            strings(&["item", "files", "sample-a"]),
        ),
        (
            "Commander does not recognize option --remove-index for this command path.",
            serde_json::json!({"code":"unknown_option","flag":"--remove-index","value":"--remove-index","recovery":{"candidate_commands":["item test"],"candidate_commands_total":1,"option_scope":"declared_elsewhere"}}),
            strings(&["item", "files", "sample-a", "--remove-index", "1"]),
        ),
    ] {
        let payload = super::command_error_json(detail, &context, &args, 2);
        assert_eq!(payload["exit_code"], 2);
        super::write_cursor_refusal_to(&mut Vec::new(), &payload)?;
    }
    for recovery in [
        serde_json::json!({"allowed_values":"custom","candidate_commands":{}}),
        serde_json::json!({"allowed_values":[null,"Task"]}),
    ] {
        super::write_cursor_refusal_to(&mut Vec::new(), &serde_json::json!({"recovery":recovery}))?;
    }
    Ok(())
}

/// A final rejected flag has no following value to include in its refusal.
#[test]
fn flag_only_refusals_do_not_invent_a_rejected_value() {
    let exclusive = ["claim", "--next", "--start"].map(str::to_owned);
    assert_eq!(
        super::command_refusal_detail("Use --next or --start, not both.", &exclusive, 2),
        serde_json::json!({"surface":"--next", "exit_code":2})
    );
    let args = ["claim", "--next"].map(str::to_owned);
    assert_eq!(
        super::command_refusal_detail("Invalid --next.", &args, 2),
        serde_json::json!({"surface":"--next", "exit_code":2})
    );
    for control in ["--author", "--json"] {
        let args = ["claim", "--start", control].map(str::to_owned);
        assert_eq!(
            super::command_refusal_detail("Invalid --start.", &args, 2),
            serde_json::json!({"surface":"--start", "exit_code":2})
        );
    }
}

/// Every ownership dispatcher propagates malformed settings and missing-item faults.
#[test]
fn ownership_dispatch_preserves_settings_and_alias_errors() -> Result<(), Box<dyn std::error::Error>>
{
    let directory = tempfile::tempdir()?;
    let root = directory.path().join(".agents/pm");
    fs::create_dir_all(&root)?;
    fs::write(root.join("settings.json"), "invalid")?;
    let args = || super::OwnershipArgs {
        id: Some("sample-missing".to_owned()),
        author: Some("fixture-agent".to_owned()),
        timestamp: None,
        message: None,
        force: false,
        json: true,
    };
    for command in [
        Command::Claim {
            args: args(),
            if_available: false,
            next: false,
            start: false,
            selection: super::SelectionArgs::default(),
        },
        Command::Release {
            args: args(),
            pause: false,
        },
        Command::StartTask(args()),
        Command::PauseTask(args()),
    ] {
        assert!(
            run(Cli {
                workspace: directory.path().to_path_buf(),
                command
            })
            .is_err()
        );
    }
    fs::write(root.join("settings.json"), "{}")?;
    for command in [Command::StartTask(args()), Command::PauseTask(args())] {
        assert!(
            run(Cli {
                workspace: directory.path().to_path_buf(),
                command
            })
            .is_err()
        );
    }
    let workspace = pm_rust::Workspace::discover(directory.path())?;
    fs::remove_file(root.join("settings.json"))?;
    assert!(super::ownership_request(&workspace, args(), false).is_err());
    Ok(())
}

/// Recovery metadata deduplicates repeated flags while retaining argument order.
#[test]
fn repeated_flags_are_listed_once_in_refusal_recovery() {
    let args = ["list", "--json", "--json", "value"].map(str::to_owned);
    let payload = super::cursor_error_json("invalid_query_cursor", "invalid", &args);
    assert_eq!(
        payload["recovery"]["provided_fields"],
        serde_json::json!(["--json"])
    );
}

/// Deliberately failing response proves encoding errors reach the caller.
struct SerializationFailure;

impl serde::Serialize for SerializationFailure {
    fn serialize<S: serde::Serializer>(&self, _: S) -> Result<S::Ok, S::Error> {
        Err(serde::ser::Error::custom("response cannot be encoded"))
    }
}

#[test]
fn response_serialization_errors_are_propagated() {
    assert!(write_json_to(&mut Vec::new(), &SerializationFailure).is_err());
}

#[test]
fn published_recovery_strips_both_native_control_spellings() {
    for args in [
        vec![
            "--workspace",
            "fixture",
            "list",
            "--timestamp",
            "clock",
            "--json",
            "--after",
            "bad",
        ],
        vec![
            "--workspace=fixture",
            "list",
            "--timestamp=clock",
            "--json",
            "--after",
            "bad",
        ],
    ] {
        assert_eq!(
            super::published_read_arguments(args.into_iter().map(str::to_owned)),
            ["list", "--json", "--after", "bad"]
        );
    }
    assert_eq!(
        super::published_read_arguments(
            ["--workspace-extra=fixture", "--timestamp-extra=clock"]
                .into_iter()
                .map(str::to_owned)
        ),
        ["--workspace-extra=fixture", "--timestamp-extra=clock"]
    );
}

struct NewlineFailure {
    document_complete: bool,
}

impl io::Write for NewlineFailure {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        if self.document_complete && buffer == b"\n" {
            return Err(io::Error::new(io::ErrorKind::BrokenPipe, "closed"));
        }
        self.document_complete = buffer.ends_with(b"}");
        Ok(buffer.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

struct WriteFailure;

impl io::Write for WriteFailure {
    fn write(&mut self, _buffer: &[u8]) -> io::Result<usize> {
        Err(io::Error::new(io::ErrorKind::BrokenPipe, "closed"))
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

struct FlushFailure;

impl io::Write for FlushFailure {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        Ok(buffer.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Err(io::Error::new(io::ErrorKind::BrokenPipe, "closed"))
    }
}

#[test]
fn wire_bytes_match_the_shared_renderer_and_carry_the_newline()
-> Result<(), Box<dyn std::error::Error>> {
    let value = serde_json::json!({
        "z": [{}, [], null, true, "line\n\"quoted\"", 30.0, 30.5, 1e20, -0.0, 7],
        "a": 2
    });
    let mut bytes = Vec::new();
    write_json_to(&mut bytes, &value)?;
    let expected = pm_rust::stringify_json(&value, true);
    assert_eq!(String::from_utf8(bytes)?, format!("{expected}\n"));
    assert!(expected.starts_with("{\n  \"z\": [\n    {},\n"));
    Ok(())
}

#[test]
fn refusal_payload_write_errors_are_propagated() {
    // Cursor refusals print their payload to stderr and ignore write errors:
    // a closed diagnostic pipe must not change the exit path.
    let payload = serde_json::json!({"code": "read_output_cursor_stale"});
    assert!(write_json_to(&mut WriteFailure, &payload).is_err());
    assert!(
        write_json_to(
            &mut NewlineFailure {
                document_complete: false,
            },
            &payload,
        )
        .is_err()
    );
    assert!(write_json_to(&mut FlushFailure, &payload).is_err());
}

#[test]
fn trailing_newline_and_flush_errors_are_propagated() {
    let list = ListResult {
        items: Vec::new(),
        count: 0,
        total: 0,
        filters: ItemFilter::default(),
    };
    let item = ItemDocument {
        metadata: ItemMetadata {
            id: "demo".to_owned(),
            title: "Demo".to_owned(),
            description: String::new(),
            item_type: "Task".to_owned(),
            status: "open".to_owned(),
            priority: 1,
            tags: Vec::new(),
            created_at: "2026-08-06T00:00:00Z".to_owned(),
            updated_at: "2026-08-06T00:00:00Z".to_owned(),
            parent: None,
            extra: Map::new(),
        },
        body: String::new(),
    };
    assert!(write_json_to(&mut WriteFailure, &list).is_err());
    assert!(
        write_json_to(
            &mut NewlineFailure {
                document_complete: false,
            },
            &list,
        )
        .is_err()
    );
    assert!(write_json_to(&mut FlushFailure, &list).is_err());
    assert!(write_json_to(&mut WriteFailure, &item).is_err());
    assert!(
        write_json_to(
            &mut NewlineFailure {
                document_complete: false,
            },
            &item,
        )
        .is_err()
    );
    assert!(write_json_to(&mut FlushFailure, &item).is_err());
}

#[test]
fn run_dispatches_create_success_and_error_paths() -> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let root = directory.path().join(".agents/pm");
    fs::create_dir_all(&root)?;
    fs::write(root.join("settings.json"), "{}")?;
    let command = || Cli {
        workspace: directory.path().to_path_buf(),
        command: Command::Create {
            id: "unit-create".to_owned(),
            title: "Unit create".to_owned(),
            description: "description".to_owned(),
            item_type: "Task".to_owned(),
            status: "open".to_owned(),
            priority: 1,
            tags: vec!["unit".to_owned()],
            body: "body".to_owned(),
            author: "unit-agent".to_owned(),
            timestamp: Some("2026-08-07T10:06:30.183Z".to_owned()),
            message: Some("message".to_owned()),
            force_stale_lock: false,
        },
    };
    run(command())?;
    assert!(run(command()).is_err());
    Ok(())
}

#[test]
#[allow(clippy::too_many_lines)]
fn run_dispatches_every_mutation_and_its_error_halves() -> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let root = directory.path().join(".agents/pm");
    fs::create_dir_all(&root)?;
    fs::write(root.join("settings.json"), "{}")?;
    let cli = |command: Command| Cli {
        workspace: directory.path().to_path_buf(),
        command,
    };

    // Create succeeds once, then the duplicate refusal flows through the
    // payload builder's error propagation.
    run(cli(Command::Create {
        id: "unit-dispatch".to_owned(),
        title: "Dispatch".to_owned(),
        description: String::new(),
        item_type: "Task".to_owned(),
        status: "open".to_owned(),
        priority: 1,
        tags: Vec::new(),
        body: String::new(),
        author: "unit-agent".to_owned(),
        timestamp: Some("2026-08-07T10:06:30.183Z".to_owned()),
        message: None,
        force_stale_lock: false,
    }))?;
    assert!(
        run(cli(Command::Create {
            id: "unit-dispatch".to_owned(),
            title: "Duplicate".to_owned(),
            description: String::new(),
            item_type: "Task".to_owned(),
            status: "open".to_owned(),
            priority: 1,
            tags: Vec::new(),
            body: String::new(),
            author: "unit-agent".to_owned(),
            timestamp: Some("2026-08-07T10:06:30.183Z".to_owned()),
            message: None,
            force_stale_lock: false,
        }))
        .is_err()
    );

    // Update covers both a successful whole-field run and refusals.
    assert!(
        run(cli(Command::Update {
            id: "unit-dispatch".to_owned(),
            title: None,
            description: None,
            status: None,
            priority: None,
            tags_csv: None,
            body: None,
            author: Some("unit-agent".to_owned()),
            timestamp: Some("2026-08-07T10:06:30.183Z".to_owned()),
            message: None,
            force_stale_lock: false,
        }))
        .is_err()
    );
    run(cli(Command::Update {
        id: "unit-dispatch".to_owned(),
        title: Some("Renamed in dispatch".to_owned()),
        description: None,
        status: None,
        priority: None,
        tags_csv: Some("b,a".to_owned()),
        body: None,
        author: Some("unit-agent".to_owned()),
        timestamp: Some("2026-08-07T10:06:30.183Z".to_owned()),
        message: None,
        force_stale_lock: false,
    }))?;

    // Comment and close cover success plus their typed refusals.
    assert!(
        run(cli(Command::Comment {
            id: "unit-dispatch".to_owned(),
            text: "   ".to_owned(),
            author: Some("unit-agent".to_owned()),
            timestamp: Some("2026-08-07T10:06:30.183Z".to_owned()),
            message: None,
            force_stale_lock: false,
        }))
        .is_err()
    );
    run(cli(Command::Comment {
        id: "unit-dispatch".to_owned(),
        text: "dispatch note".to_owned(),
        author: Some("unit-agent".to_owned()),
        timestamp: Some("2026-08-07T10:06:30.183Z".to_owned()),
        message: None,
        force_stale_lock: false,
    }))?;
    let ownership_args = |author: &str| super::OwnershipArgs {
        id: Some(
            if author.trim().is_empty() {
                "unit-missing"
            } else {
                "unit-dispatch"
            }
            .to_owned(),
        ),
        author: Some(author.to_owned()),
        timestamp: Some("2026-08-07T10:06:30.183Z".to_owned()),
        message: None,
        force: false,
        json: true,
    };
    assert!(
        run(cli(Command::Claim {
            args: ownership_args(" "),
            if_available: false,
            next: false,
            start: false,
            selection: super::SelectionArgs::default(),
        }))
        .is_err()
    );
    run(cli(Command::Claim {
        args: ownership_args("unit-agent"),
        if_available: false,
        next: false,
        start: false,
        selection: super::SelectionArgs::default(),
    }))?;
    assert!(
        run(cli(Command::Release {
            args: ownership_args(" "),
            pause: false
        }))
        .is_err()
    );
    run(cli(Command::Release {
        args: ownership_args("unit-agent"),
        pause: false,
    }))?;
    run(cli(Command::Close {
        id: "unit-dispatch".to_owned(),
        text: None,
        fields: super::CloseArgs::default(),
        reason: Some("dispatch done".to_owned()),
        author: Some("unit-agent".to_owned()),
        timestamp: Some("2026-08-07T10:06:30.183Z".to_owned()),
        force_stale_lock: false,
    }))?;
    assert!(
        run(cli(Command::Close {
            id: "unit-dispatch".to_owned(),
            text: None,
            fields: super::CloseArgs::default(),
            reason: Some("again".to_owned()),
            author: Some("unit-agent".to_owned()),
            timestamp: Some("2026-08-07T10:06:30.183Z".to_owned()),
            force_stale_lock: false,
        }))
        .is_err()
    );

    // Get over an unknown item fails through the read executor.
    assert!(
        run(cli(Command::Get {
            id: "sample-missing".to_owned(),
        }))
        .is_err()
    );
    Ok(())
}

/// Native-only controls are absent from recovery; repeated public flags deduplicate.
#[test]
fn cursor_recovery_arguments_preserve_public_flags() {
    let args = super::published_read_arguments(
        [
            "--workspace",
            "fixture",
            "list",
            "--json",
            "--json",
            "--timestamp",
            "fixed",
            "--after",
            "cursor",
        ]
        .into_iter()
        .map(str::to_owned),
    );
    assert_eq!(args, ["list", "--json", "--json", "--after", "cursor"]);
    let producer = super::cursor_error_json("invalid_query_cursor", "reason", &args);
    assert_eq!(
        producer["recovery"]["provided_fields"],
        serde_json::json!(["--json", "--after"])
    );
    assert!(producer["next_steps"].is_array());
    for cursor in ["--cursor", "--cursor=invalid"] {
        let args = vec![
            "deps".to_owned(),
            "sample-root".to_owned(),
            cursor.to_owned(),
        ];
        let payload = super::cursor_error_json("invalid_query_cursor", "reason", &args);
        assert_eq!(
            payload["next_steps"][0],
            "Repeat the original query without --cursor to obtain a fresh cursor."
        );
    }
    let output = super::cursor_error_json("read_output_cursor_stale", "reason", &args);
    assert!(output.get("next_steps").is_none());
}

/// A non-UTF-8 argument is published lossily inside the normal cursor refusal.
#[cfg(any(unix, windows))]
#[test]
fn non_utf8_arguments_publish_the_cursor_refusal_envelope() {
    use std::ffi::OsString;
    #[cfg(unix)]
    use std::os::unix::ffi::OsStringExt;
    #[cfg(windows)]
    use std::os::windows::ffi::OsStringExt;

    #[cfg(unix)]
    let malformed = OsString::from_vec(vec![0xff]);
    #[cfg(windows)]
    let malformed = OsString::from_wide(&[0xD800]);
    let mut workspace = OsString::from("--workspace=");
    workspace.push(&malformed);
    let mut after = OsString::from("--after=");
    after.push(&malformed);
    let args = super::published_read_arguments([
        OsString::from("--workspace"),
        malformed,
        workspace,
        OsString::from("list"),
        OsString::from("--json"),
        after,
    ]);
    assert_eq!(args[..2], ["list".to_owned(), "--json".to_owned()]);
    assert!(args[2].starts_with("--after="));
    assert!(args[2].contains('\u{FFFD}'));
    let payload =
        super::cursor_error_json("invalid_query_cursor", "Query cursor is malformed.", &args);
    assert_eq!(payload["exit_code"], 2);
    assert_eq!(payload["code"], "invalid_query_cursor");
    assert_eq!(payload["refusal"]["exit_code"], 2);
    assert_eq!(
        payload["recovery"]["attempted_command"],
        format!("pm list --json {}", pm_rust::quote_command_arg(&args[2]))
    );
    assert!(payload["next_steps"].is_array());
}

/// Closure TOON preserves arrays, nested receipts and write failures.
#[test]
fn closure_toon_nested_arrays_and_io_errors() -> Result<(), Box<dyn std::error::Error>> {
    let value = serde_json::json!({"id":"sample-close","previous_assignee":null,"warnings":["first,second"],"recovery":{"missing":["--resolution"]}});
    let mut bytes = Vec::new();
    super::write_close_toon(&mut bytes, &value, "")?;
    assert_eq!(
        String::from_utf8(bytes)?,
        "id: \"sample-close\"\nwarnings[1]: \"first,second\"\nrecovery:\n  missing[1]: \"--resolution\"\n"
    );
    super::write_close_toon(&mut Vec::new(), &serde_json::json!(null), "")?;
    for value in [
        serde_json::json!({"array":["row"]}),
        serde_json::json!({"nested":{"key":"value"}}),
        serde_json::json!({"key":"value"}),
    ] {
        assert!(super::write_close_toon(&mut WriteFailure, &value, "").is_err());
    }
    Ok(())
}

/// Closure recovery normalizes aliases and quotes complete arguments before projection.
#[test]
fn closure_recovery_and_refusal_boundaries() {
    let detail = "Missing closure evidence. ".repeat(8);
    let long = super::complete_refusal(
        &super::ownership_error_json(
            &detail,
            &serde_json::json!({"code":"close_reason_required"}),
            &["close".to_owned(), "sample-close".to_owned()],
        ),
        &[],
    );
    assert_eq!(
        long["title"].as_str().unwrap_or_default().chars().count(),
        120
    );
    assert!(long["title"].as_str().unwrap_or_default().ends_with("..."));
    assert_eq!(long["detail"], detail);
    let args = super::published_read_arguments([
        "close",
        "sample-close",
        "--expected",
        "value",
        "--actual",
        " ",
        "--validate-close",
        "strict",
        "--json",
    ]);
    assert_eq!(args[2], "--expected-result");
    assert_eq!(args[4], "--actual-result");
    let payload = super::ownership_error_json(
        "Cannot close: missing actual_result (--actual-result).",
        &serde_json::json!({"code":"close_validation_missing_fields","recovery":{"missing":["--actual-result"],"suggested_retry":"pm update sample-close --actual-result value"}}),
        &args,
    );
    assert_eq!(
        payload["recovery"]["missing"],
        serde_json::json!(["--actual-result"])
    );
    assert_eq!(
        super::close_error_refusal(
            "Cannot close: missing actual_result (--actual-result).",
            &payload,
            2
        ),
        serde_json::json!({"surface":"--actual-result","rejected_value":" ","exit_code":2})
    );
    for (payload, detail, expected) in [
        (
            serde_json::json!({}),
            "Failure",
            serde_json::json!({"surface":"close","exit_code":2}),
        ),
        (
            serde_json::json!({"recovery":{"missing":["--resolution"]}}),
            "Failure",
            serde_json::json!({"surface":"--resolution","exit_code":2}),
        ),
        (
            serde_json::json!({"recovery":{"normalized_args":["close","--resolution"]}}),
            "Missing (--resolution).",
            serde_json::json!({"surface":"--resolution","exit_code":2}),
        ),
        (
            serde_json::json!({"flag":"--completed-at","recovery":{"normalized_args":["close","--completed-at","bad"]}}),
            "Invalid date",
            serde_json::json!({"surface":"--completed-at","rejected_value":"bad","exit_code":2}),
        ),
    ] {
        assert_eq!(super::close_error_refusal(detail, &payload, 2), expected);
    }
    for next in ["--json", "--author"] {
        let payload =
            serde_json::json!({"recovery":{"normalized_args":["close","--resolution",next]}});
        assert_eq!(
            super::close_error_refusal("Missing --resolution.", &payload, 2),
            serde_json::json!({"surface":"--resolution","exit_code":2})
        );
    }
}

struct HeaderOnly {
    remaining: usize,
}
impl io::Write for HeaderOnly {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.remaining == 0 {
            return Err(io::ErrorKind::BrokenPipe.into());
        }
        let size = bytes.len().min(self.remaining);
        self.remaining -= size;
        Ok(size)
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
/// Closure output propagates serialization, recursive write and final flush failures.
#[test]
fn closure_receipt_writer_failure_boundaries() -> Result<(), Box<dyn std::error::Error>> {
    use io::Write as _;
    let value = serde_json::json!({"id":"sample-close"});
    for json in [true, false] {
        let mut bytes = Vec::new();
        super::write_close_receipt_to(&mut bytes, &value, json)?;
        assert!(!bytes.is_empty());
        assert!(super::write_close_receipt_to(&mut WriteFailure, &value, json).is_err());
        assert!(super::write_close_receipt_to(&mut FlushFailure, &value, json).is_err());
    }
    let mut writer = HeaderOnly { remaining: 8 };
    assert!(
        super::write_close_toon(
            &mut writer,
            &serde_json::json!({"nested":{"id":"value"}}),
            ""
        )
        .is_err()
    );
    writer.flush()?;
    Ok(())
}

/// List encodings and cursor diagnostics propagate real writer and flush failures.
#[test]
fn list_and_cursor_writers_propagate_io_failures() -> Result<(), Box<dyn std::error::Error>> {
    let value: pm_rust::ListOutput = serde_json::json!({"items":[],"count":0}).into();
    for json in [true, false] {
        super::write_list_to(&mut Vec::new(), &value, json)?;
        assert!(super::write_list_to(&mut WriteFailure, &value, json).is_err());
        assert!(super::write_list_to(&mut FlushFailure, &value, json).is_err());
    }
    for payload in [
        serde_json::json!({}),
        serde_json::json!({"recovery":{"provided_fields":[]}}),
        super::cursor_error_json(
            "invalid_query_cursor",
            "Query cursor is malformed.",
            &["list".to_owned(), "--after".to_owned(), "%%%".to_owned()],
        ),
    ] {
        let mut bytes = Vec::new();
        super::write_cursor_refusal_to(&mut bytes, &payload)?;
        assert!(String::from_utf8(bytes)?.starts_with("Error:"));
        assert!(super::write_cursor_refusal_to(&mut WriteFailure, &payload).is_err());
        assert!(super::write_cursor_refusal_to(&mut FlushFailure, &payload).is_err());
    }
    // The pair-preserving cut keeps the astral pair intact, so bounded JSON
    // carries a compacted prefix with no lone-surrogate escape and the TOON
    // codec renders it (GH-1478).
    let directory = tempfile::tempdir()?;
    let root = directory.path().join(".agents/pm");
    fs::create_dir_all(root.join("tasks"))?;
    fs::write(root.join("settings.json"), "{}")?;
    for index in 0..20 {
        let item = serde_json::json!({"id":format!("demo-{index:04}"),"title":format!("{}😀{}", "x".repeat(239), "y".repeat(300)),"type":"Task","status":"open","priority":1,"created_at":"2026-10-01T00:00:00.000Z","updated_at":"2026-10-01T00:00:00.000Z"});
        fs::write(
            root.join(format!("tasks/demo-{index:04}.toon")),
            toon_format::encode_default(&item)?,
        )?;
    }
    let workspace = pm_rust::Workspace::discover(directory.path())?;
    let cut = workspace.list_page(
        &pm_rust::ItemFilter::default(),
        &pm_rust::ListOptions {
            output_budget: Some("1500".to_owned()),
            ..pm_rust::ListOptions::default()
        },
        "clock",
    )?;
    assert!(!pm_rust::stringify_json(&cut, false).contains("\\ud83d"));
    assert!(pm_rust::stringify_json(&cut, false).contains("x…"));
    assert!(super::write_list_to(&mut Vec::new(), &cut, false).is_ok());
    // A retained malformed override still refuses TOON rendering: the cut
    // machinery keeps the codec rejection for values that were already
    // malformed before compaction (GH-1478 preserves those for the encoder).
    let malformed = pm_rust::ListOutput::with_json_overrides(
        (*cut).clone(),
        std::collections::BTreeMap::from([("/items/0/title".to_owned(), "\"\\ud83d\"".to_owned())]),
    );
    assert!(super::write_list_to(&mut Vec::new(), &malformed, false).is_err());
    Ok(())
}

/// Recovery placeholders use the parsed query after flags and join every positional token.
#[test]
fn search_examples_use_parsed_query() {
    for args in [
        vec!["search", "--type", "Unknown", "rust", "search"],
        vec!["search", "rust", "search", "--type", "Unknown"],
    ] {
        let args = args.into_iter().map(str::to_owned).collect::<Vec<_>>();
        let payload = super::search_error_json(
            "Invalid type",
            &serde_json::json!({"code":"custom","examples":["pm search <query> --type Task --json"]}),
            &args,
            2,
        );
        assert_eq!(
            payload["examples"][0],
            "pm search \"rust search\" --type Task --json"
        );
    }
}
/// Parser context outranks argv order; incomplete contexts never blame an allowed option.
#[test]
fn orientation_argument_context_and_fallback() {
    use clap::error::{ContextKind, ContextValue, ErrorKind};
    let args: Vec<String> = ["history", "sample-a", "--json", "--limit=1", "-v"]
        .into_iter()
        .map(str::to_owned)
        .collect();
    let allowed = ["--json", "--limit"];
    let mut error = clap::Error::raw(ErrorKind::UnknownArgument, "synthetic parser context");
    assert_eq!(
        super::rejected_orientation_argument(&error, &args, &allowed),
        "-v"
    );
    error.insert(
        ContextKind::InvalidArg,
        ContextValue::String("extra".to_owned()),
    );
    assert_eq!(
        super::rejected_orientation_argument(&error, &args, &allowed),
        "extra"
    );
    error.insert(
        ContextKind::InvalidArg,
        ContextValue::String("--bogus=value".to_owned()),
    );
    assert_eq!(
        super::rejected_orientation_argument(&error, &args, &allowed),
        "--bogus"
    );
    error.insert(ContextKind::InvalidArg, ContextValue::Strings(vec![]));
    assert_eq!(
        super::rejected_orientation_argument(&error, &args[..4], &allowed),
        "option"
    );
    let fallback: Vec<String> = ["context", "--json", "--bogus=value"]
        .into_iter()
        .map(str::to_owned)
        .collect();
    assert_eq!(
        super::rejected_orientation_argument(&error, &fallback, &allowed),
        "--bogus"
    );
}

/// Assurance dispatch discovers declarations and returns typed missing-operand/read refusals.
#[test]
fn assurance_dispatch_is_read_only() -> Result<(), Box<dyn std::error::Error>> {
    use clap::Parser as _;
    let directory = tempfile::tempdir()?;
    let root = directory.path().join(".agents/pm");
    std::fs::create_dir_all(&root)?;
    std::fs::write(root.join("settings.json"), "{}")?;
    std::fs::write(root.join("assurance.json"), r#"{"gates":[{"id":"known"}]}"#)?;
    for (flags, success) in [
        (vec!["assurance", "list", "gate", "--json"], true),
        (vec!["assurance", "list", "gate"], true),
        (vec!["assurance", "show", "gate", "known", "--json"], true),
        (vec!["assurance", "show", "measurement"], false),
        (
            vec!["assurance", "show", "measurement", "missing", "--json"],
            false,
        ),
    ] {
        let mut args = vec![
            "pm-rust",
            "--workspace",
            directory.path().to_str().ok_or("UTF8")?,
        ];
        args.extend(flags);
        assert_eq!(run(Cli::try_parse_from(args)?).is_ok(), success);
    }
    for recovery in [
        serde_json::Value::Null,
        serde_json::json!({"suggested_retry":"pm assurance list gate"}),
    ] {
        let context = serde_json::json!({"reason":"assurance_evaluation_refused","value":"missing","changed":false,"recovery":recovery});
        let payload = super::command_error_json(
            "missing declaration",
            &context,
            &["assurance".to_owned()],
            2,
        );
        assert_eq!(payload["exit_code"], 2);
        assert_eq!(payload["changed"], false);
    }
    Ok(())
}

/// Retrying a contended mutation must keep an explicitly selected native workspace.
#[test]
fn lock_recovery_keeps_explicit_workspace_scope() {
    for raw in [
        vec!["--workspace", "tracker directory"],
        vec!["--workspace=tracker directory"],
        vec!["--pm-path", "tracker directory"],
        vec!["--pm-path=tracker directory"],
        vec!["--path", "tracker directory"],
        vec!["--path=tracker directory"],
    ] {
        let mut payload = serde_json::json!({"code":"lock_conflict","recovery":{"normalized_args":["comment","sample-one","note"],"suggested_retry_args":["comment","sample-one","note"]}});
        super::preserve_refusal_scope(
            &mut payload,
            &raw.iter().map(|arg| (*arg).to_owned()).collect::<Vec<_>>(),
        );
        assert_eq!(
            payload["recovery"]["suggested_retry_args"],
            serde_json::json!([
                "--pm-path",
                "tracker directory",
                "comment",
                "sample-one",
                "note"
            ])
        );
    }
    for raw in [
        vec!["comment"],
        vec!["--workspace"],
        vec!["--pm-path"],
        vec!["--path"],
    ] {
        let mut payload = serde_json::json!({});
        super::preserve_refusal_scope(
            &mut payload,
            &raw.iter().map(|arg| (*arg).to_owned()).collect::<Vec<_>>(),
        );
        assert_eq!(payload, serde_json::json!({}));
    }
}

/// Recovery envelopes preserve literal empty and whitespace values from the invocation.
#[test]
fn close_recovery_preserves_literal_argument_values() {
    for value in [" ", "", " value "] {
        let args = ["close", "sample-close", "--actual-result", value]
            .map(str::to_owned)
            .to_vec();
        let payload = super::command_error_json(
            "Cannot close: missing actual_result (--actual-result).",
            &serde_json::json!({"code":"close_validation_missing_fields","recovery":{"missing":["--actual-result"]}}),
            &args,
            2,
        );
        assert_eq!(
            payload["recovery"]["normalized_args"],
            serde_json::json!(args)
        );
        assert_eq!(payload["refusal"]["rejected_value"], value);
    }
}
/// Canonical packet retry namespaces normalize after scope options, preserving literal values.
#[test]
fn canonical_packet_retry_arguments_are_accepted() -> Result<(), Box<dyn std::error::Error>> {
    for prefix in [vec![], vec!["--pm-path", "."], vec!["--workspace=."]] {
        let mut args = vec!["pm-rust"];
        args.extend(prefix);
        args.extend(["item", "deps", "sample-root", "--output-format", "json"]);
        let normalized =
            super::native_arguments(args.into_iter().map(std::ffi::OsString::from).collect());
        let cli = <super::Cli as clap::Parser>::try_parse_from(normalized)?;
        assert!(
            matches!(cli.command, super::Command::Deps { output_format: Some(format), .. } if format == "json")
        );
    }
    for args in [
        vec![],
        vec!["pm-rust"],
        vec!["pm-rust", "--workspace"],
        vec!["pm-rust", "--workspace", "item", "deps", "root"],
        vec!["pm-rust", "item", "get", "root"],
        vec!["pm-rust", "deps", "item"],
    ] {
        let args = args
            .into_iter()
            .map(std::ffi::OsString::from)
            .collect::<Vec<_>>();
        assert_eq!(super::native_arguments(args.clone()), args);
    }
    Ok(())
}

/// Canonical comment retry namespaces parse after scope aliases without changing literal text.
#[test]
fn canonical_comment_retry_arguments_are_accepted() -> Result<(), Box<dyn std::error::Error>> {
    for prefix in [vec![], vec!["--pm-path", "."], vec!["--workspace=."]] {
        let mut args = vec!["pm-rust"];
        args.extend(prefix);
        args.extend([
            "item",
            "comments",
            "sample-one",
            " note ",
            "--author",
            "test-agent",
        ]);
        let normalized =
            super::native_arguments(args.into_iter().map(std::ffi::OsString::from).collect());
        let cli = <super::Cli as clap::Parser>::try_parse_from(normalized)?;
        assert!(matches!(cli.command, super::Command::Comment { text, .. } if text == " note "));
    }
    Ok(())
}

/// Budget receipts stay compact and distinguish unavailable advice from a proven minimum.
#[test]
fn packet_recovery_receipts_preserve_verified_evidence() -> Result<(), Box<dyn std::error::Error>> {
    for minimum in [serde_json::json!(123), serde_json::Value::Null] {
        let payload = serde_json::json!({"code":"invalid_argument_value","title":"Budget floor","detail":"Budget floor","required":"Retry","recovery":{"token_budget":{"requested_tokens":1,"feasible_tokens":minimum,"minimum_feasible_tokens":minimum},"suggested_retry":"pm retry","next_best_command":"pm inspect","fallback_candidates":[{"source":"relationship_summary","command":"pm inspect","reason":"Bounded counts"}]}});
        assert_eq!(
            super::command_error_json("Budget floor", &payload, &["deps".to_owned()], 2),
            payload
        );
        let mut output = Vec::new();
        super::write_cursor_refusal_to(&mut output, &payload)?;
        let text = String::from_utf8(output)?;
        assert!(text.contains("minimum_token_budget:"));
        assert!(text.contains("suggested_retry: pm retry"));
        assert!(!text.contains("attempted_command:"));
    }
    let context = serde_json::json!({"code":"invalid_query_cursor","recovery":{"suggested_retry":"pm retry","suggested_retry_args":["deps","a"]}});
    let payload = super::command_error_json(
        "Query cursor is malformed.",
        &context,
        &["deps".to_owned(), "--cursor".to_owned(), "bad".to_owned()],
        2,
    );
    assert_eq!(payload["next_steps"][1], "pm retry");
    let mut output = Vec::new();
    super::write_cursor_refusal_to(&mut output, &payload)?;
    assert!(String::from_utf8(output)?.contains("suggested_retry: pm retry"));
    Ok(())
}

/// Explicit encoding controls take precedence for structured and human refusals.
#[test]
fn explicit_error_encoding_has_the_published_precedence() {
    for (args, expected) in [
        (vec![], false),
        (vec!["--json"], true),
        (vec!["--output-format", "json"], true),
        (vec!["--json", "--output-format", "toon"], false),
        (vec!["--output-format=json"], true),
        (vec!["--output-format=toon"], false),
    ] {
        assert_eq!(
            super::json_requested(&args.into_iter().map(str::to_owned).collect::<Vec<_>>()),
            expected
        );
    }
}
/// Search refusals retain empty and whitespace argument facts exactly as supplied.
#[test]
fn search_recovery_keeps_literal_argument_values() {
    for value in ["", " ", " padded "] {
        let args = vec!["search".to_owned(), value.to_owned(), "--json".to_owned()];
        let result = super::search_error_json(
            "Search query must not be empty",
            &serde_json::json!({"code":"command_failed"}),
            &args,
            2,
        );
        assert_eq!(
            result["recovery"]["normalized_args"],
            serde_json::json!(args)
        );
        let args = vec![
            "search".to_owned(),
            "rust".to_owned(),
            "--type".to_owned(),
            value.to_owned(),
        ];
        let result =
            super::command_refusal_detail("--type requires at least one non-empty value", &args, 2);
        assert_eq!(result["rejected_value"], value);
    }
}

/// Unknown runtime failures preserve their human fallback and explicit JSON encoding.
#[test]
fn command_refusal_encoding_and_recovery_boundaries() -> Result<(), Box<dyn std::error::Error>> {
    let context = serde_json::json!({"code":"unknown_error"});
    for command in ["context", "assurance"] {
        for json in [false, true] {
            let mut args = vec![command.to_owned()];
            if json {
                args.push("--json".to_owned());
            }
            let payload = super::command_error_json("runtime failed", &context, &args, 1);
            let mut output = Vec::new();
            super::write_command_refusal_to(
                &mut output,
                "runtime failed",
                &context,
                &args,
                &payload,
            )?;
            if json {
                assert_eq!(
                    serde_json::from_slice::<serde_json::Value>(&output)?["code"],
                    "unknown_error"
                );
            } else {
                assert_eq!(output, b"runtime failed\n");
            }
            assert!(
                super::write_command_refusal_to(
                    &mut WriteFailure,
                    "runtime failed",
                    &context,
                    &args,
                    &payload
                )
                .is_err()
            );
        }
    }
    let args = vec!["context".to_owned()];
    assert_eq!(
        super::command_error_json("runtime failed", &context, &args, 1)["code"],
        "unknown_error"
    );
    for command in ["close", "claim", "context", "search", "item"] {
        let args = vec![
            command.to_owned(),
            "sample-one".to_owned(),
            "--limit".to_owned(),
            "bad".to_owned(),
        ];
        let payload = super::command_error_json(
            "Invalid --limit.",
            &serde_json::json!({"code":"invalid_argument_value"}),
            &args,
            2,
        );
        assert_eq!(payload["exit_code"], 2);
        assert!(payload["refusal"].is_object());
    }
    let bare_cursor = super::command_error_json(
        "cursor invalid",
        &serde_json::json!({"code":"invalid_query_cursor"}),
        &[
            "deps".to_owned(),
            "sample-one".to_owned(),
            "--cursor".to_owned(),
            "bad".to_owned(),
        ],
        2,
    );
    assert_eq!(bare_cursor["code"], "invalid_query_cursor");
    assert!(bare_cursor["recovery"].is_object());
    let context = serde_json::json!({"code":"invalid_query_cursor","recovery":{"suggested_retry":"pm item deps sample-one","scope":"sample-one"}});
    let args = vec![
        "item".to_owned(),
        "deps".to_owned(),
        "sample-one".to_owned(),
    ];
    let payload = super::command_error_json("cursor invalid", &context, &args, 2);
    assert_eq!(payload["recovery"]["scope"], "sample-one");
    assert!(
        payload["next_steps"]
            .as_array()
            .ok_or("steps")?
            .iter()
            .any(|step| step == "pm item deps sample-one")
    );
    super::write_command_refusal_to(&mut Vec::new(), "cursor invalid", &context, &args, &payload)?;
    let list: pm_rust::ListOutput = serde_json::json!({"items":[],"count":0}).into();
    assert!(
        super::write_list_to(
            &mut NewlineFailure {
                document_complete: false
            },
            &list,
            true
        )
        .is_err()
    );
    Ok(())
}

/// Every discovery alias is removed before classifying published refusal arguments.
#[test]
fn published_refusal_arguments_strip_all_workspace_aliases() {
    for flag in ["--workspace", "--pm-path", "--path"] {
        for args in [
            vec![
                flag.to_owned(),
                "tracker directory".to_owned(),
                "context".to_owned(),
                "--json".to_owned(),
            ],
            vec![
                format!("{flag}=tracker directory"),
                "context".to_owned(),
                "--json".to_owned(),
            ],
        ] {
            assert_eq!(super::published_read_arguments(args), ["context", "--json"]);
        }
    }
}

/// Session frames retain canonical payloads and propagate real writer/codec failures.
#[test]
fn session_frame_writer_preserves_errors_and_framing() -> Result<(), Box<dyn std::error::Error>> {
    let mut page = pm_rust::SessionPage {
        output: serde_json::json!({"items":[{"title":"example"}],"count":1}).into(),
        receipt: pm_rust::SessionReceipt {
            protocol: "pm-rust/read-session/v1",
            snapshot_sha256: "synthetic".to_owned(),
            source_items: 1,
            source_bytes: 100,
            admitted_memory_bytes: 1000,
            memory_ceiling_bytes: 2000,
            pages: 1,
            complete: true,
        },
    };
    for json in [false, true] {
        let mut bytes = Vec::new();
        super::write_session_page_to(&mut bytes, &page, json)?;
        if json {
            let parsed: serde_json::Value = serde_json::from_slice(&bytes)?;
            assert_eq!(parsed["receipt"]["complete"], true);
        } else {
            assert!(String::from_utf8(bytes)?.ends_with("---\n"));
        }
        assert!(super::write_session_page_to(&mut WriteFailure, &page, json).is_err());
        assert!(super::write_session_page_to(&mut FlushFailure, &page, json).is_err());
    }
    page.output = pm_rust::ListOutput::with_json_overrides(
        (*page.output).clone(),
        std::collections::BTreeMap::from([("/items/0/title".to_owned(), "\"\\ud83d\"".to_owned())]),
    );
    assert!(super::write_session_page_to(&mut Vec::new(), &page, false).is_err());
    Ok(())
}

/// Every workspace operation still refuses missing trackers before reading or writing data.
#[test]
fn native_adapter_workspace_discovery_refusals() -> Result<(), Box<dyn std::error::Error>> {
    use clap::Parser as _;
    let directory = tempfile::tempdir()?;
    let cases: &[&[&str]] = &[
        &["read-session"],
        &["changes"],
        &["assurance", "list", "schema"],
        &["context"],
        &["history", "demo-1"],
        &["deps", "demo-1"],
        &["search", "keyword"],
        &["files", "demo-1"],
        &["docs", "demo-1"],
        &["test", "demo-1"],
        &["list"],
        &["get", "demo-1"],
        &[
            "create", "--id", "demo-1", "--title", "Example", "--type", "Task", "--author",
            "caller",
        ],
        &["update", "demo-1", "--author", "caller"],
        &["comment", "demo-1", "text", "--author", "caller"],
        &["claim", "demo-1"],
        &["release", "demo-1"],
        &["start-task", "demo-1"],
        &["pause-task", "demo-1"],
        &["close", "demo-1", "done", "--author", "caller"],
    ];
    for args in cases {
        let mut arguments = vec![
            "pm-rust".to_owned(),
            "--workspace".to_owned(),
            directory.path().to_string_lossy().into_owned(),
        ];
        arguments.extend(args.iter().map(ToString::to_string));
        let error = run(Cli::try_parse_from(arguments)?)
            .err()
            .ok_or("missing tracker must refuse")?;
        assert!(
            error
                .downcast_ref::<pm_rust::PmRustError>()
                .is_some_and(|error| matches!(error, pm_rust::PmRustError::TrackerNotFound { .. })),
            "{args:?}: {error}"
        );
    }
    assert!(!directory.path().join(".agents").exists());
    Ok(())
}

/// Adapter stdin and output failures are recoverable errors, including broken pipes.
#[test]
fn native_adapter_stdio_refusals() -> Result<(), Box<dyn std::error::Error>> {
    /// A caller-owned stream that fails before yielding any input bytes.
    struct ReadFailure;
    impl io::Read for ReadFailure {
        fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
            Err(io::Error::other("synthetic read failure"))
        }
    }
    let directory = tempfile::tempdir()?;
    assert!(super::run_adapter_check(&mut ReadFailure, &mut Vec::new(), true).is_err());
    for json in [false, true] {
        assert!(super::run_adapter_check(&mut &b"{}"[..], &mut WriteFailure, json).is_err());
        let valid = br#"{"manifest":{"abi":"pm-native-adapter/1","capabilities":["get"],"max_frame_bytes":4096},"request":{"abi":"pm-native-adapter/1","request_id":"call-1","operation":{"action":"get","id":"demo-1"}}}"#;
        assert!(super::run_adapter_check(&mut &valid[..], &mut FlushFailure, json).is_err());
        for (input, ok) in [(&b"{}"[..], false), (&valid[..], true)] {
            let mut output = Vec::new();
            let result = super::run_with_io(
                Cli {
                    workspace: directory.path().to_path_buf(),
                    command: Command::AdapterCheck { json },
                },
                &mut &input[..],
                &mut output,
            );
            assert_eq!(result.is_ok(), ok);
            if json {
                let value: serde_json::Value = serde_json::from_slice(&output)?;
                assert_eq!(value["ok"], ok);
            } else {
                assert!(String::from_utf8(output)?.contains(if ok {
                    "ok: true"
                } else {
                    "ok: false"
                }));
            }
        }
        let output = serde_json::json!({"ok":true}).into();
        // Fail only the trailing newline, after successful frame emission.
        let mut only_frame = vec![0u8; pm_rust::stringify_json(&output, false).len()];
        if json {
            assert!(
                super::write_adapter_receipt_to(&mut only_frame.as_mut_slice(), &output, json)
                    .is_err()
            );
        }
    }
    let cut = pm_rust::ListOutput::with_json_overrides(
        serde_json::json!({"title":"cut"}),
        std::collections::BTreeMap::from([("/title".to_owned(), "\"\\ud83d\"".to_owned())]),
    );
    assert!(super::write_adapter_receipt_to(&mut Vec::new(), &cut, false).is_err());
    assert!(!directory.path().join(".agents").exists());
    Ok(())
}

/// Query-cost dispatch propagates broken output streams without returning success.
#[test]
fn explain_writer_refusals() -> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let root = directory.path().join(".agents/pm");
    fs::create_dir_all(&root)?;
    fs::write(root.join("settings.json"), "{}")?;
    for json in [false, true] {
        let cli = || Cli {
            workspace: directory.path().to_path_buf(),
            command: Command::Explain {
                read: "list".to_owned(),
                full: false,
                max_entries: 10,
                max_bytes: 1024,
                json,
            },
        };
        assert!(super::run_with_io(cli(), &mut &b""[..], &mut WriteFailure).is_err());
        assert!(super::run_with_io(cli(), &mut &b""[..], &mut FlushFailure).is_err());
    }
    Ok(())
}

/// Incremental CLI pages and refusals retain caller-owned output and cancellation boundaries.
#[test]
fn changes_writer_refusals() -> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let root = directory.path().join(".agents/pm");
    fs::create_dir_all(root.join("history"))?;
    fs::write(root.join("settings.json"), "{}")?;
    for json in [false, true] {
        let cli = || Cli {
            workspace: directory.path().to_path_buf(),
            command: Command::Changes {
                cursor: None,
                limit: 20,
                output_bytes: 65536,
                cancel_file: Some(directory.path().join("absent")),
                json,
            },
        };
        assert!(super::run_with_io(cli(), &mut &b""[..], &mut WriteFailure).is_err());
        assert!(super::run_with_io(cli(), &mut &b""[..], &mut FlushFailure).is_err());
        let mut output = Vec::new();
        super::run_with_io(cli(), &mut &b""[..], &mut output)?;
        let value: serde_json::Value = if json {
            serde_json::from_slice(&output)?
        } else {
            toon_format::decode_default(std::str::from_utf8(&output)?)?
        };
        let page: pm_rust::changes::ChangePage = serde_json::from_value(value)?;
        assert!(page.events.is_empty());
        assert!(page.skipped.is_empty());
        assert!(!page.has_more);
        assert!(!page.next_cursor.is_empty());
        assert!(output.ends_with(b"\n"));

        fs::write(directory.path().join("absent"), b"cancel")?;
        let mut refused_output = Vec::new();
        let error = super::run_with_io(cli(), &mut &b""[..], &mut refused_output)
            .err()
            .ok_or("an existing cancellation marker must refuse before emission")?;
        assert!(matches!(
            error.downcast_ref::<pm_rust::changes::ChangeRefusal>(),
            Some(pm_rust::changes::ChangeRefusal::Cancelled)
        ));
        assert!(refused_output.is_empty());
        fs::remove_file(directory.path().join("absent"))?;
    }
    Ok(())
}

/// Admission stdin, source selection and delivery failures keep typed recovery guidance.
#[test]
fn admission_stdio_refusals() -> Result<(), Box<dyn std::error::Error>> {
    /// An unreadable caller-owned stdin stream.
    struct ReadFailure;
    impl io::Read for ReadFailure {
        fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
            Err(io::Error::other("read failure"))
        }
    }
    let directory = tempfile::tempdir()?;
    let root = directory.path().join(".agents/pm");
    let cli = |json| Cli {
        workspace: directory.path().to_path_buf(),
        command: Command::PreviewItemMutations { json },
    };
    let error = super::run_with_io(cli(true), &mut ReadFailure, &mut Vec::new())
        .err()
        .ok_or("stdin refusal")?;
    assert_eq!(
        error
            .downcast_ref::<pm_rust::admission::AdmissionRefusal>()
            .ok_or("typed stdin refusal")?
            .code,
        "admission_input"
    );
    let input = serde_json::to_vec(
        &serde_json::json!({"author":"fixture-agent","mutations":[{"op":"create","id":"demo-new","options":{"title":"Fixture","type":"Task","status":"open","priority":2,"createMode":"progressive","unset":["parent","assignee"],"allowDuplicate":true}}]}),
    )?;
    let error = super::run_with_io(cli(true), &mut input.as_slice(), &mut Vec::new())
        .err()
        .ok_or("source refusal")?;
    assert_eq!(
        error
            .downcast_ref::<pm_rust::admission::AdmissionRefusal>()
            .ok_or("typed source refusal")?
            .code,
        "admission_source"
    );
    fs::create_dir_all(&root)?;
    fs::write(root.join("settings.json"), "{}")?;
    fs::write(root.join("settings.json"), "{")?;
    let error = super::run_with_io(cli(true), &mut input.as_slice(), &mut Vec::new())
        .err()
        .ok_or("preview source refusal")?;
    assert_eq!(
        error
            .downcast_ref::<pm_rust::admission::AdmissionRefusal>()
            .ok_or("typed preview source refusal")?
            .code,
        "admission_source"
    );
    fs::write(root.join("settings.json"), "{}")?;
    for json in [false, true] {
        assert!(super::run_with_io(cli(json), &mut input.as_slice(), &mut WriteFailure).is_err());
        assert!(super::run_with_io(cli(json), &mut input.as_slice(), &mut FlushFailure).is_err());
    }
    Ok(())
}

/// Discovery never hides a pipe/write/flush failure as a successful inventory.
#[test]
fn capabilities_writer_refusals() -> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    for json in [false, true] {
        let refusal = pm_rust::discovery::DiscoveryRequest::from_json(b"null")
            .err()
            .ok_or("invalid request must refuse")?;
        let mut bytes = Vec::new();
        let error = super::write_discovery_to(&mut bytes, Err(refusal), json)
            .err()
            .ok_or("delivery must preserve producer refusal")?;
        assert_eq!(
            error
                .downcast_ref::<pm_rust::discovery::DiscoveryRefusal>()
                .ok_or("typed refusal")?
                .code,
            pm_rust::discovery::DiscoveryRefusalCode::InvalidRequest
        );
        assert!(bytes.is_empty());
        let cli = || Cli {
            workspace: directory.path().join("missing"),
            command: Command::Capabilities {
                protocol: pm_rust::discovery::DISCOVERY_PROTOCOL.into(),
                operation: None,
                output_bytes: pm_rust::discovery::DISCOVERY_MAX_BYTES,
                adapter: None,
                json,
            },
        };
        assert!(super::run_with_io(cli(), &mut &b""[..], &mut WriteFailure).is_err());
        assert!(super::run_with_io(cli(), &mut &b""[..], &mut FlushFailure).is_err());
    }
    let mut response = pm_rust::discovery::discover_capabilities(
        &pm_rust::discovery::DiscoveryRequest::default(),
    )?;
    response.output_bytes = 1;
    for json in [false, true] {
        let mut bytes = Vec::new();
        let error = super::write_discovery_to(&mut bytes, Ok(response.clone()), json)
            .err()
            .ok_or("delivery must refuse an invalid response")?;
        assert!(
            error
                .downcast_ref::<pm_rust::discovery::DiscoveryRefusal>()
                .is_some()
        );
        assert!(bytes.is_empty());
    }
    Ok(())
}

/// Scope comes only from bootstrap arguments, never positional or option-valued close text.
#[test]
fn positional_close_text_cannot_select_recovery_workspace() {
    for raw in [
        vec!["close", "sample-one", "--json", "--", "--pm-path=other"],
        vec!["close", "sample-one", "--reason", "--pm-path=other"],
        vec!["close", "sample-one", "--reason", "--pm-path", "other"],
    ] {
        assert!(
            super::invocation_scope(&raw.iter().map(|arg| (*arg).to_owned()).collect::<Vec<_>>())
                .is_empty()
        );
    }
}

/// String-only producers retain their contract shape while inheriting the selected scope.
#[test]
fn string_only_recovery_retains_scope_without_inventing_arguments() {
    let mut payload = serde_json::json!({"code":"command_failed","recovery":{"suggested_retry":"pm history sample-one --strict-exit --json --verify"}});
    super::preserve_refusal_scope(
        &mut payload,
        &[
            "--pm-path".to_owned(),
            "other".to_owned(),
            "history".to_owned(),
        ],
    );
    assert_eq!(
        payload["recovery"]["suggested_retry"],
        "pm --pm-path other history sample-one --strict-exit --json --verify"
    );
    assert!(payload["recovery"].get("suggested_retry_args").is_none());
}

/// Long ownership provenance, structured details and pre-scoped retries retain human recovery.
#[test]
fn round5_human_and_scope_refusal_corpus() -> Result<(), Box<dyn std::error::Error>> {
    let holder = "holder".repeat(30);
    let payload = serde_json::json!({"code":"start_blocked_by_open_dependency","title":"Blocked","detail":"Blocked","required":"Close the gate","claimed_by":holder,"claimed_at":"2026-08-22T10:00:00.000Z","claim_source":"unknown","details":{"open_blockers":[{"id":"gate"}]},"examples":["pm get gate"],"recovery":{"normalized_args":["claim","own","--start"],"suggested_retry":"pm get gate"},"next_steps":["pm get gate"]});
    let mut out = vec![];
    super::write_cursor_refusal_to(&mut out, &payload)?;
    let out = String::from_utf8(out)?;
    assert!(out.contains(&format!(
        "Held by: {}...",
        holder.chars().take(120).collect::<String>()
    )));
    assert!(out.contains("Details:\n  {"));
    assert!(out.contains("Examples:"));
    let raw = [
        "--pm-path".to_owned(),
        "tracker".to_owned(),
        "history".to_owned(),
        "own".to_owned(),
    ];
    for retry in [
        "pm --pm-path tracker get own",
        "pm --path tracker get own",
        "external get own",
    ] {
        let mut payload =
            serde_json::json!({"code":"command_failed","recovery":{"suggested_retry":retry}});
        super::preserve_refusal_scope(&mut payload, &raw);
        assert_eq!(payload["recovery"]["suggested_retry"], retry);
    }
    let mut absent = serde_json::json!({"code":"command_failed"});
    super::preserve_refusal_scope(&mut absent, &raw);
    assert!(absent["recovery"].is_null());
    Ok(())
}

/// Partial recovery producers retain usable scope and human guidance when optional arrays are absent.
#[test]
fn round5_partial_recovery_producer_shapes() -> Result<(), Box<dyn std::error::Error>> {
    let raw = ["--pm-path", "tracker", "close", "sample-one"].map(str::to_owned);
    let mut payload = serde_json::json!({"code":"close_validation_missing_fields","title":"Missing evidence","recovery":{"normalized_args":["--pm-path","tracker","close","sample-one"],"provided_fields":["--pm-path"],"next_best_command":"external update"}});
    super::preserve_refusal_scope(&mut payload, &raw);
    assert_eq!(
        payload["recovery"]["provided_fields"],
        serde_json::json!(["--pm-path"])
    );
    assert_eq!(payload["recovery"]["next_best_command"], "external update");
    let mut bytes = vec![];
    super::write_cursor_refusal_to(&mut bytes, &payload)?;
    assert!(String::from_utf8(bytes)?.contains("Missing evidence"));
    payload["recovery"]
        .as_object_mut()
        .ok_or("recovery")?
        .shift_remove("next_best_command");
    let mut bytes = vec![];
    super::write_cursor_refusal_to(&mut bytes, &payload)?;
    assert!(!String::from_utf8(bytes)?.contains("next_best_command:"));
    payload["recovery"]["next_best_command"] = serde_json::json!("pm update sample-one");
    payload["recovery"]["missing"] = serde_json::json!(["reason"]);
    payload["claimed_by"] = serde_json::json!("short-holder");
    super::preserve_refusal_scope(&mut payload, &raw);
    let mut bytes = vec![];
    super::write_cursor_refusal_to(&mut bytes, &payload)?;
    let rendered = String::from_utf8(bytes)?;
    assert!(rendered.contains("missing: reason"));
    assert!(rendered.contains("Held by: short-holder (unknown)"));
    assert!(rendered.contains("next_best_command: pm --pm-path tracker update sample-one"));
    let mut receipt =
        serde_json::json!({"follow_up":{"suggested_command_args":["update","sample-one"]}});
    super::project_close_receipt_scope(&mut receipt, &raw);
    assert_eq!(
        receipt["follow_up"]["suggested_command_args"],
        serde_json::json!(["--pm-path", "tracker", "update", "sample-one"])
    );
    let mut retry = serde_json::json!({"recovery":{"suggested_retry_args":["close","sample-one"]}});
    super::apply_close_error_recovery(
        &mut retry,
        &serde_json::json!({}),
        &["close".to_owned(), "sample-one".to_owned()],
    );
    assert_eq!(retry["recovery"]["suggested_retry"], "pm close sample-one");
    retry["recovery"]["suggested_retry_args"] =
        serde_json::json!(["--pm-path", "tracker", "close", "sample-one"]);
    super::apply_close_error_recovery(
        &mut retry,
        &serde_json::json!({}),
        &["close".to_owned(), "sample-one".to_owned()],
    );
    assert_eq!(retry["recovery"]["suggested_retry"], "pm close sample-one");
    super::project_close_receipt_scope(&mut receipt, &raw);
    assert_eq!(
        receipt["follow_up"]["suggested_command_args"],
        serde_json::json!(["--pm-path", "tracker", "update", "sample-one"])
    );
    let mut parts = vec![];
    super::append_recovery_domains(
        &mut parts,
        &serde_json::json!({"allowed_values":(0..10).map(|i|format!("choice-{i}")).collect::<Vec<_>>()}),
    );
    assert!(
        parts
            .iter()
            .any(|part| part.contains("allowed_values_omitted: 2"))
    );
    assert!(!parts[0].contains("choice-8"));
    assert_eq!(
        super::invocation_scope(&["--".to_owned(), "--pm-path=ignored".to_owned()]),
        Vec::<String>::new()
    );
    Ok(())
}

/// Historical non-object comment entries remain intact when the CLI indexes a new comment receipt.
#[test]
fn round5_historical_comment_receipt_preserves_scalar_rows()
-> Result<(), Box<dyn std::error::Error>> {
    let temp = tempfile::tempdir()?;
    let root = temp.path().join(".agents/pm");
    fs::create_dir_all(root.join("tasks"))?;
    fs::create_dir(root.join("history"))?;
    fs::write(root.join("settings.json"), "{}")?;
    let item = serde_json::json!({"id":"sample-a","title":"Comments","type":"Task","status":"open","priority":2,"created_at":"2026-08-22T10:00:00.000Z","updated_at":"2026-08-22T10:00:00.000Z","comments":[null,"legacy"]});
    fs::write(
        root.join("tasks/sample-a.toon"),
        toon_format::encode_default(&item)?,
    )?;
    let workspace = pm_rust::Workspace::discover(temp.path())?;
    let identity =
        pm_rust::AuthorIdentity::resolve(Some("alpha"), None, &std::collections::BTreeMap::new());
    let output = super::comment_payload(
        &workspace,
        &identity,
        &pm_rust::CommentItem {
            id: "sample-a".to_owned(),
            text: "New comment".to_owned(),
            author: "alpha".to_owned(),
            timestamp: Some("2026-08-22T10:00:00.000Z".to_owned()),
            message: None,
            provenance_role: None,
            force_stale_lock: false,
        },
    )?;
    let comments = output["item"]["comments"]
        .as_array()
        .ok_or("comment rows")?;
    assert!(comments.contains(&serde_json::Value::Null));
    assert!(comments.contains(&serde_json::json!("legacy")));
    assert!(
        comments
            .iter()
            .any(|row| row["text"] == "New comment" && row["index"].is_number())
    );
    Ok(())
}

/// A real intervening claim contributes ownership fields to the two-step close receipt only once.
#[test]
fn round5_close_release_receipt_counts_an_intervening_claim()
-> Result<(), Box<dyn std::error::Error>> {
    use clap::Parser as _;
    let temp = tempfile::tempdir()?;
    let root = temp.path().join(".agents/pm");
    fs::create_dir_all(root.join("tasks"))?;
    fs::create_dir(root.join("history"))?;
    fs::write(root.join("settings.json"), "{}")?;
    run(Cli::try_parse_from([
        "pm-rust",
        "--workspace",
        temp.path().to_str().ok_or("workspace")?,
        "create",
        "--id",
        "sample-one",
        "--title",
        "Closure",
        "--type",
        "Task",
        "--author",
        "alpha",
        "--timestamp",
        "2026-08-22T10:00:00.000Z",
    ])?)?;
    let workspace = pm_rust::Workspace::discover(temp.path())?;
    let closed = workspace.close(pm_rust::CloseItem {
        id: "sample-one".to_owned(),
        reason: "Done".to_owned(),
        author: "alpha".to_owned(),
        timestamp: Some("2026-08-22T10:00:01.000Z".to_owned()),
        provenance_role: None,
        fields: pm_rust::CloseFields {
            validate_close: Some("off".to_owned()),
            ..Default::default()
        },
        force_stale_lock: false,
    })?;
    let request = pm_rust::OwnershipItem {
        id: "sample-one".to_owned(),
        author: "alpha".to_owned(),
        timestamp: Some("2026-08-22T10:00:02.000Z".to_owned()),
        message: None,
        force: true,
        if_available: false,
        provenance_role: None,
    };
    let release = workspace.release(&request)?;
    assert_eq!(release.changed_field_count, 0);
    assert_eq!(
        super::close_release_changed_field_count(
            &closed.close_changed_fields,
            release.changed_field_count
        ),
        4
    );
    workspace.claim(&request)?;
    let release = workspace.release(&request)?;
    assert_eq!(release.changed_field_count, 2);
    assert_eq!(
        super::close_release_changed_field_count(
            &closed.close_changed_fields,
            release.changed_field_count
        ),
        6
    );
    let mut duplicate_fields = closed.close_changed_fields;
    duplicate_fields.extend(["assignee".to_owned(), "claim_principal".to_owned()]);
    assert_eq!(
        super::close_release_changed_field_count(&duplicate_fields, release.changed_field_count),
        6
    );
    Ok(())
}

/// Receipt conversion propagates serializer failure before attempting row indexing.
#[test]
fn round5_comment_receipt_serialization_failure_is_preserved() {
    assert!(super::indexed_comment_receipt(serde_json::to_value(SerializationFailure)).is_err());
}

/// Delimited close retries and human fallback diagnostics preserve their distinct contracts.
#[test]
fn round5_delimited_recovery_and_human_fallback() -> Result<(), Box<dyn std::error::Error>> {
    let mut delimited = serde_json::json!({"recovery":{"missing":["--reason"]}});
    super::apply_close_error_recovery(
        &mut delimited,
        &serde_json::json!({"code":"close_validation_missing_fields"}),
        &["close".to_owned(), "--".to_owned(), "sample-one".to_owned()],
    );
    assert!(
        delimited["recovery"]["suggested_retry"]
            .as_str()
            .ok_or("retry")?
            .contains("--reason \"<value>\" -- sample-one")
    );
    for (context, args, expected) in [
        (
            serde_json::json!({"code":"unknown_error"}),
            vec!["claim".to_owned()],
            "detail",
        ),
        (
            serde_json::json!({"code":"partial_failure","why":"No rollback was performed."}),
            vec!["claim".to_owned()],
            "No rollback was performed.",
        ),
        (
            serde_json::json!({"code":"close_validation_missing_fields"}),
            vec!["close".to_owned()],
            "What is required:",
        ),
    ] {
        let mut bytes = vec![];
        let source = serde_json::json!({"code":context["code"],"title":"Title","detail":"detail"});
        super::write_command_refusal_to(&mut bytes, "detail", &context, &args, &source)?;
        assert!(String::from_utf8(bytes)?.contains(expected));
    }
    Ok(())
}

/// A public port/path URL cannot absorb the next URL's credential span.
#[test]
fn round7_public_url_port_boundaries() {
    for separator in [" ", ",", ";"] {
        let source = format!(
            "https://public.invalid:443/p{separator}https://u:secret-password@private.invalid"
        );
        let expected =
            format!("https://public.invalid:443/p{separator}https://[redacted]@private.invalid");
        let mut payload = serde_json::json!({"detail":source});
        super::refusal::redact(&mut payload, &[]);
        assert_eq!(payload["detail"], expected);
    }
    for source in [
        "https://public.invalid:443/p",
        "https://www.npmjs.com/package/@scope/x",
    ] {
        let mut payload = serde_json::json!({"detail":source});
        super::refusal::redact(&mut payload, &[]);
        assert_eq!(payload["detail"], source);
    }
    let mut nested =
        serde_json::json!({"detail":"https://u:long-lived-secret://tail@private.invalid"});
    super::refusal::redact(&mut nested, &[]);
    assert_eq!(nested["detail"], "https://[redacted]@private.invalid");
}

/// Executable next steps inherit tracker scope even when no recovery bundle exists.
#[test]
fn round10_executable_steps_keep_tracker_scope() {
    let raw = [
        "--pm-path",
        "other tracker",
        "claim",
        "sample-own",
        "--start",
    ]
    .map(str::to_owned);
    for recovery in [
        serde_json::Value::Null,
        serde_json::json!({"normalized_args":["claim","sample-own"]}),
    ] {
        let mut payload = serde_json::json!({"next_steps":["pm update sample-own --unset blocked_by","Read the blocker first.","pm --pm-path chosen get sample-own","external get sample-own"],"recovery":recovery});
        super::preserve_refusal_scope(&mut payload, &raw);
        assert_eq!(
            payload["next_steps"],
            serde_json::json!([
                "pm --pm-path \"other tracker\" update sample-own --unset blocked_by",
                "Read the blocker first.",
                "pm --pm-path chosen get sample-own",
                "external get sample-own"
            ])
        );
    }
    let mut payload = serde_json::json!({"next_steps":["pm update sample-own --unset blocked_by"]});
    let original = payload.clone();
    super::preserve_refusal_scope(&mut payload, &["claim".to_owned()]);
    assert_eq!(payload, original);
    let mut payload = serde_json::json!({"recovery":{"normalized_args":["claim","sample-own"]}});
    super::preserve_refusal_scope(&mut payload, &raw);
    assert!(payload.get("next_steps").is_none());
    let mut payload = serde_json::json!({"next_steps":"Read the blocker","recovery":null});
    super::preserve_refusal_scope(&mut payload, &raw);
    assert_eq!(payload["next_steps"], "Read the blocker");
}

/// Positional close reasons remain lossless after the delimiter in same-command retries.
#[test]
fn round10_close_retry_preserves_delimited_reason() -> Result<(), Box<dyn std::error::Error>> {
    for reason in [
        "--timestamp=done",
        "--workspace=literal",
        "--expected",
        "ordinary reason",
    ] {
        let raw = [
            "close",
            "sample-one",
            "--validate-close",
            "strict",
            "--json",
            "--",
            reason,
        ];
        let args = super::published_read_arguments(raw);
        assert_eq!(args.last().map(String::as_str), Some(reason));
        let context = serde_json::json!({"code":"close_validation_missing_fields","recovery":{"missing":["--actual-result","--expected-result"]}});
        let payload = super::ownership_error_json("Missing evidence", &context, &args);
        if reason == "--expected" {
            // The pinned CLI withdraws a retry when positional data spells a missing evidence flag.
            assert!(payload["recovery"]["suggested_retry_args"].is_null());
            assert_eq!(
                payload["recovery"]["normalized_args"],
                serde_json::json!(args)
            );
            continue;
        }
        let retry = payload["recovery"]["suggested_retry_args"]
            .as_array()
            .ok_or("retry")?;
        let delimiter = retry
            .iter()
            .position(|arg| arg == "--")
            .ok_or("delimiter")?;
        assert_eq!(&retry[delimiter + 1..], &[serde_json::json!(reason)]);
        assert!(
            retry[..delimiter]
                .iter()
                .any(|arg| arg == "--actual-result")
        );
    }
    Ok(())
}

/// Pre-scoped advice and delimiter ambiguity preserve every supported spelling.
#[test]
fn round10_recovery_scope_and_alias_controls() {
    let raw = ["--pm-path", "selected", "claim"].map(str::to_owned);
    for flag in ["--pm-path", "--path", "--workspace"] {
        for separator in [" ", "="] {
            let command = format!("pm {flag}{separator}chosen get sample-own");
            let mut value = serde_json::json!({"next_steps":[command,7],"recovery":{"suggested_retry":command}});
            super::preserve_refusal_scope(&mut value, &raw);
            assert_eq!(value["next_steps"][0], command);
            assert_eq!(value["next_steps"][1], 7);
            assert_eq!(value["recovery"]["suggested_retry"], command);
        }
    }
    let missing = ["--expected-result", "--actual-result", "--resolution"].map(str::to_owned);
    for flag in [
        "--expected",
        "--expected_result",
        "--expected-result",
        "--actual",
        "--actual_result",
        "--actual-result",
        "--resolution",
    ] {
        assert!(!super::close_retry_unambiguous(
            &["close", "id", "--", flag].map(str::to_owned),
            &missing
        ));
        assert!(super::close_retry_unambiguous(
            &["close", "id", flag].map(str::to_owned),
            &missing
        ));
    }
    assert!(super::close_retry_unambiguous(
        &["close", "id", "--", "ordinary reason"].map(str::to_owned),
        &missing
    ));
}

/// A delimited scope-shaped reason cannot override bootstrap scope or output controls.
#[test]
fn round11_bootstrap_scope_ignores_command_data() -> Result<(), Box<dyn std::error::Error>> {
    for flag in ["--pm-path", "--path", "--workspace"] {
        for reason in [
            "--pm-path=literal",
            "--workspace=literal",
            "--json",
            "--output-format=json",
            "--author=literal",
        ] {
            let raw = [
                flag,
                "selected",
                "close",
                "sample-one",
                "--validate-close",
                "strict",
                "--json",
                "--",
                reason,
            ]
            .map(str::to_owned);
            let args = super::published_read_arguments(&raw);
            let context = serde_json::json!({"code":"close_validation_missing_fields","recovery":{"missing":["--actual-result","--expected-result"]}});
            let mut payload = super::ownership_error_json("Missing evidence", &context, &args);
            super::preserve_refusal_scope(&mut payload, &raw);
            for key in ["normalized_args", "suggested_retry_args"] {
                let values = payload["recovery"][key]
                    .as_array()
                    .ok_or("recovery argv missing")?;
                assert_eq!(
                    &values[..2],
                    &[
                        serde_json::json!("--pm-path"),
                        serde_json::json!("selected")
                    ]
                );
                assert_eq!(values.last(), Some(&serde_json::json!(reason)));
            }
            for key in ["attempted_command", "suggested_retry"] {
                assert!(
                    payload["recovery"][key]
                        .as_str()
                        .unwrap_or_default()
                        .starts_with("pm --pm-path selected close ")
                );
            }
        }
    }
    Ok(())
}

/// Declared values and delimited positionals cannot supply format, author or workspace controls.
#[test]
fn round11_option_values_never_become_controls() {
    for raw in [
        vec!["close", "sample-one", "--", "--json"],
        vec!["close", "sample-one", "--completed-at", "--json"],
        vec!["deps", "sample-one", "--", "--output-format=json"],
    ] {
        assert!(!super::json_requested(
            &raw.into_iter().map(str::to_owned).collect::<Vec<_>>()
        ));
    }
    let raw = [
        "close",
        "sample-one",
        "--message",
        "literal --pm-path=wrong --json --author=wrong",
        "--",
        "--pm-path=reason",
    ]
    .map(str::to_owned);
    assert!(super::invocation_scope(&raw).is_empty());
    assert_eq!(super::published_read_arguments(&raw), raw);
    for value in [
        "--workspace=literal",
        "--pm-path=literal",
        "--timestamp=literal",
        "--expected",
        "--actual",
        "--json",
        "--output-format=json",
        "--author=literal",
    ] {
        let raw = ["close", "sample-one", "--completed-at", value].map(str::to_owned);
        assert_eq!(super::published_read_arguments(&raw), raw);
        assert!(!super::json_requested(&raw));
        assert!(super::invocation_scope(&raw).is_empty());
    }

    assert_eq!(
        super::invocation_scope(&["--workspace", "-", "close"].map(str::to_owned)),
        ["--pm-path", "-"]
    );
    let bare = ["pm-rust", "item"].map(std::ffi::OsString::from).to_vec();
    assert_eq!(super::native_arguments(bare.clone()), bare);
    assert!(!super::json_requested(&["search", "-"].map(str::to_owned)));
    for control in ["--author", "--json", "--output-format"] {
        let args = ["deps", "sample-one", "--bogus", control, "value"].map(str::to_owned);
        let refusal = super::command_refusal_detail("--bogus requires a value", &args, 2);
        assert!(refusal.get("rejected_value").is_none());
    }
    let args = ["context", "--limit", "--json"].map(str::to_owned);
    assert_eq!(
        super::command_refusal_detail("--limit must be a number", &args, 2)["rejected_value"],
        "--json"
    );

    for command in [
        "pm --pm-path - close sample-one",
        "pm --workspace \"chosen tracker\" close sample-one",
        "pm --path=chosen close sample-one",
    ] {
        assert!(
            super::scope_recovery_command(
                command,
                &["--pm-path".to_owned(), "selected".to_owned()]
            )
            .is_none()
        );
    }

    for path in ["--json", "--author=literal", "--"] {
        let raw = [format!("--workspace={path}"), "close".to_owned()];
        let scope = super::invocation_scope(&raw);
        assert_eq!(scope, [format!("--pm-path={path}")]);
        assert_eq!(super::Invocation::parse(&scope).scope(), Some(path));
        assert!(!super::json_requested(&scope));
    }
}

/// A hyphen-permitting value consumes the delimiter token before output controls.
#[test]
fn round12_hyphen_value_delimiter() {
    let args: Vec<String> = ["context", "--limit", "--", "--json"]
        .map(str::to_owned)
        .to_vec();
    let parsed = crate::invocation::Invocation::parse(&args);
    assert_eq!(parsed.options[0].value, Some("--"));
    assert!(super::json_requested(&args));
    let positional: Vec<String> = ["close", "sample-one", "--", "--json"]
        .map(str::to_owned)
        .to_vec();
    assert!(!super::json_requested(&positional));
}
