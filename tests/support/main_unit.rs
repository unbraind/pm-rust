use serde_json::Map;
use std::fs;
use std::io;

use pm_rust::{ItemDocument, ItemFilter, ItemMetadata, ListResult};

use super::{Cli, Command, run, write_json_to};

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
        strings(&["context", "--limit", "1"])
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
            author: "unit-agent".to_owned(),
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
        author: "unit-agent".to_owned(),
        timestamp: Some("2026-08-07T10:06:30.183Z".to_owned()),
        message: None,
        force_stale_lock: false,
    }))?;

    // Comment and close cover success plus their typed refusals.
    assert!(
        run(cli(Command::Comment {
            id: "unit-dispatch".to_owned(),
            text: "   ".to_owned(),
            author: "unit-agent".to_owned(),
            timestamp: Some("2026-08-07T10:06:30.183Z".to_owned()),
            message: None,
            force_stale_lock: false,
        }))
        .is_err()
    );
    run(cli(Command::Comment {
        id: "unit-dispatch".to_owned(),
        text: "dispatch note".to_owned(),
        author: "unit-agent".to_owned(),
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
        author: "unit-agent".to_owned(),
        timestamp: Some("2026-08-07T10:06:30.183Z".to_owned()),
        force_stale_lock: false,
    }))?;
    assert!(
        run(cli(Command::Close {
            id: "unit-dispatch".to_owned(),
            text: None,
            fields: super::CloseArgs::default(),
            reason: Some("again".to_owned()),
            author: "unit-agent".to_owned(),
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
    let long = super::ownership_error_json(
        &detail,
        &serde_json::json!({"code":"close_validation_missing_fields"}),
        &["close".to_owned(), "sample-close".to_owned()],
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
    ] {
        let mut payload =
            serde_json::json!({"recovery":{"normalized_args":["comment","sample-one","note"]}});
        super::preserve_lock_scope(
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
    for raw in [vec!["comment"], vec!["--workspace"]] {
        let mut payload = serde_json::json!({});
        super::preserve_lock_scope(
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
        let normalized = super::native_arguments(args.into_iter().map(std::ffi::OsString::from));
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
        super::orientation_error_json("runtime failed", &context, &args, 1)["code"],
        "unknown_error"
    );
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
