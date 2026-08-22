use std::collections::BTreeMap;
use std::fs;
use std::io;

use pm_rust::{ItemDocument, ItemFilter, ItemMetadata, ListResult};

use super::{Cli, Command, run, write_json_to};

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
