use std::io;
use std::path::{Path, PathBuf};

use super::collect_directory_entries;
use crate::PmRustError;

#[test]
fn empty_discovery_paths_fail_before_filesystem_reads() -> Result<(), Box<dyn std::error::Error>> {
    let Err(PmRustError::Io { path, source }) = super::Workspace::discover(Path::new("")) else {
        return Err("empty discovery path did not return a typed I/O error".into());
    };
    assert!(path.as_os_str().is_empty());
    assert_eq!(source.kind(), io::ErrorKind::InvalidInput);
    Ok(())
}

#[test]
fn query_discovery_keeps_resolved_input_spelling() -> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let tracker = directory.path().join(".agents").join("pm");
    let nested = directory.path().join("src").join("nested");
    std::fs::create_dir_all(&tracker)?;
    std::fs::create_dir_all(&nested)?;
    std::fs::write(tracker.join("settings.json"), "{}")?;
    let file = nested.join("input.txt");
    std::fs::write(&file, "fixture")?;
    let resolved = std::path::absolute(&tracker)?;
    let canonical = std::fs::canonicalize(&tracker)?;
    let expected = if cfg!(windows) {
        resolved
    } else {
        canonical.clone()
    };
    let parent = nested.join("..");
    for start in [directory.path(), &nested, &file, &tracker, &parent] {
        let workspace = super::Workspace::discover(start)?;
        assert_eq!(workspace.query_pm_root(), expected);
        assert_eq!(workspace.pm_root(), canonical);
    }
    Ok(())
}

#[cfg(windows)]
#[test]
fn verbatim_windows_parent_path_reaches_the_component_fold()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let tracker = directory.path().join(".agents/pm");
    let nested = directory.path().join("nested");
    std::fs::create_dir_all(&tracker)?;
    std::fs::create_dir(&nested)?;
    std::fs::write(tracker.join("settings.json"), "{}")?;
    // PathBuf::join normalizes verbatim paths. Append to the OS string so the
    // real parent component survives absolute() and reaches discovery's fold.
    let mut spelling = std::fs::canonicalize(&nested)?.into_os_string();
    spelling.push(r"\..");
    let parent = PathBuf::from(spelling);
    let resolved = std::path::absolute(&parent)?;
    assert!(
        resolved
            .components()
            .any(|c| c == std::path::Component::ParentDir)
    );
    // Win32 refuses the verbatim parent component at canonicalize, after
    // discovery has folded the query spelling. Assert the real refusal.
    let Err(PmRustError::Io { path, source }) = super::Workspace::discover(&parent) else {
        return Err("verbatim parent path must return a typed filesystem refusal".into());
    };
    assert_eq!(path, parent);
    assert_eq!(source.raw_os_error(), Some(123));
    // The same existing path in ordinary spelling resolves successfully.
    let workspace = super::Workspace::discover(&nested.join(".."))?;
    assert_eq!(workspace.pm_root(), std::fs::canonicalize(&tracker)?);
    Ok(())
}

#[test]
fn query_spelling_preserves_windows_aliases_and_unix_physical_roots() {
    let resolved = Path::new("short-name/.agents/pm");
    let canonical = Path::new("expanded-name/.agents/pm");
    assert_eq!(super::query_root_path(resolved, canonical, true), resolved);
    assert_eq!(
        super::query_root_path(resolved, canonical, false),
        canonical
    );
}

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
