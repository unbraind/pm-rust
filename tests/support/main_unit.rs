use serde_json::Map;
use std::fs;
use std::io;

use pm_rust::{ItemDocument, ItemFilter, ItemMetadata, ListResult};

use super::{Cli, Command, run, write_json_to};

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
        format!("pm {}", args.join(" "))
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
        serde_json::json!({"surface":"--actual-result","rejected_value":"--validate-close","exit_code":2})
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
