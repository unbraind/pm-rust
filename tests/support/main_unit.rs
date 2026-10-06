use std::collections::BTreeMap;
use std::fs;
use std::io;

use pm_rust::{ItemDocument, ItemFilter, ItemMetadata, ListResult};

use super::{Cli, Command, run, write_json_to};

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
            extra: BTreeMap::new(),
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
    run(cli(Command::Close {
        id: "unit-dispatch".to_owned(),
        reason: "dispatch done".to_owned(),
        author: "unit-agent".to_owned(),
        timestamp: Some("2026-08-07T10:06:30.183Z".to_owned()),
        force_stale_lock: false,
    }))?;
    assert!(
        run(cli(Command::Close {
            id: "unit-dispatch".to_owned(),
            reason: "again".to_owned(),
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
