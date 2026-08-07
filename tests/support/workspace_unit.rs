use std::io;
use std::path::{Path, PathBuf};

use super::collect_directory_entries;
use crate::PmRustError;

#[test]
fn directory_iteration_errors_retain_the_directory_path() -> Result<(), Box<dyn std::error::Error>>
{
    let path = Path::new("tracker/items");
    let entries = std::iter::once(Err(io::Error::other("iteration failed")));
    let Err(PmRustError::Io {
        path: failed,
        source,
    }) = collect_directory_entries(path, entries)
    else {
        return Err("directory iteration error was not propagated".into());
    };
    assert_eq!(failed, PathBuf::from("tracker/items"));
    assert_eq!(source.to_string(), "iteration failed");
    Ok(())
}
