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
