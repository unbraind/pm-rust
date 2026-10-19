thread_local! {
    /// Counts real authoritative item opens, including reads through the corpus path.
    pub(super) static ITEM_OPENS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

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
    let directory = tempfile::tempdir()?;
    let missing = directory.path().join("missing");
    assert!(matches!(
        super::Workspace::discover(&missing),
        Err(PmRustError::Io { source, .. }) if source.kind() == io::ErrorKind::NotFound
    ));
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
fn directory_snapshots_skip_removed_entries_symlinks_and_non_toon_files()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let vanished = directory.path().join("vanished.toon");
    let retained = directory.path().join("retained.toon");
    let nested = directory.path().join("nested");
    let child = nested.join("child.toon");
    std::fs::write(&vanished, "removed before metadata")?;
    std::fs::write(&retained, "retained")?;
    std::fs::write(directory.path().join("ignored.txt"), "non-item")?;
    std::fs::create_dir(&nested)?;
    std::fs::write(&child, "nested item")?;
    let linked = directory.path().join("linked.toon");
    #[cfg(unix)]
    std::os::unix::fs::symlink(&retained, &linked)?;
    #[cfg(windows)]
    if let Err(error) = std::os::windows::fs::symlink_file(&retained, &linked) {
        if error.raw_os_error() == Some(1314) && std::env::var_os("CI").is_none() {
            eprintln!("skipping Windows symlink fixture off CI: ERROR_PRIVILEGE_NOT_HELD: {error}");
            return Ok(());
        }
        return Err(error.into());
    }
    let entries = super::read_directory(directory.path())?;
    assert_eq!(entries.len(), 5);
    std::fs::remove_file(&vanished)?;
    assert!(!vanished.is_symlink() && !vanished.is_dir() && !vanished.is_file());
    let mut paths = Vec::new();
    super::collect_toon_entries(entries, &mut paths)?;
    paths.sort();
    let mut listed = Vec::new();
    super::collect_lookup_paths(directory.path(), None, &mut listed)?;
    listed.sort();
    assert_eq!(listed, paths);
    assert_eq!(paths, [child, retained]);
    Ok(())
}

/// An ancestor replaced after enumeration must not hide a listed item's metadata failure.
#[test]
fn directory_snapshots_propagate_item_metadata_errors() -> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let root = directory.path().join(".agents/pm");
    let items = root.join("tasks");
    std::fs::create_dir_all(&items)?;
    std::fs::write(root.join("settings.json"), "{}")?;
    let item = items.join("sample-one.toon");
    std::fs::write(&item, "listed before ancestor replacement")?;
    let entries = super::read_directory(&items)?;
    assert_eq!(entries.len(), 1);
    std::fs::rename(&items, root.join("parked"))?;
    // A real self-referential directory link fails metadata lookup through the
    // ancestor on every platform, including when Unix tests run as root.
    #[cfg(unix)]
    std::os::unix::fs::symlink("tasks", &items)?;
    #[cfg(windows)]
    if let Err(error) = std::os::windows::fs::symlink_dir("tasks", &items) {
        if error.raw_os_error() == Some(1314) && std::env::var_os("CI").is_none() {
            eprintln!("skipping Windows symlink fixture off CI: ERROR_PRIVILEGE_NOT_HELD: {error}");
            return Ok(());
        }
        return Err(error.into());
    }
    let expected = std::fs::symlink_metadata(&item)
        .err()
        .ok_or("metadata must hit the link loop")?;
    assert_ne!(expected.kind(), io::ErrorKind::NotFound);
    let mut paths = Vec::new();
    let result = super::collect_toon_entries(entries, &mut paths);
    // Remove the cycle before checking the refusal so cleanup is deterministic.
    #[cfg(unix)]
    std::fs::remove_file(&items)?;
    #[cfg(windows)]
    std::fs::remove_dir(&items)?;
    let Err(PmRustError::Io { path, source }) = result else {
        return Err("listed item metadata failure was silently skipped".into());
    };
    assert_eq!(path, item);
    assert_eq!(source.kind(), expected.kind());
    assert_eq!(source.raw_os_error(), expected.raw_os_error());
    assert!(paths.is_empty());
    Ok(())
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

#[test]
fn workspace_equality_follows_the_canonical_tracker_not_its_spelling()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let tracker = directory.path().join(".agents").join("pm");
    let nested = directory.path().join("src").join("nested");
    std::fs::create_dir_all(&tracker)?;
    std::fs::create_dir_all(&nested)?;
    std::fs::write(tracker.join("settings.json"), "{}")?;
    // The same tracker reached through a `..` detour keeps a different lexical
    // spelling for query hashing, but it is still the same workspace.
    let direct = super::Workspace::discover(&nested)?;
    let detour = super::Workspace::discover(
        &directory
            .path()
            .join("src")
            .join("..")
            .join("src")
            .join("nested"),
    )?;
    assert_eq!(direct, detour);
    // A symlinked parent is the spelling difference that actually survives
    // discovery (macOS /tmp versus /private/tmp is the everyday case).
    #[cfg(unix)]
    {
        let link = directory.path().join("linked-root");
        std::os::unix::fs::symlink(directory.path(), &link)?;
        let through_link = super::Workspace::discover(&link.join("src").join("nested"))?;
        assert_eq!(direct, through_link);
    }
    let other_directory = tempfile::tempdir()?;
    let other_tracker = other_directory.path().join(".agents").join("pm");
    std::fs::create_dir_all(&other_tracker)?;
    std::fs::write(other_tracker.join("settings.json"), "{}")?;
    assert_ne!(direct, super::Workspace::discover(other_directory.path())?);
    Ok(())
}

/// A cold lookup in a large tracker opens one file, independent of unrelated damage.
#[test]
fn single_item_get_opens_only_the_listed_file() -> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let tracker = directory.path().join(".agents/pm");
    std::fs::create_dir_all(tracker.join("custom-types/nested"))?;
    std::fs::write(tracker.join("settings.json"), "{}")?;
    for index in 0..3000 {
        let id = format!("sample-{index:04}");
        std::fs::write(
            tracker.join(format!("custom-types/nested/{id}.toon")),
            format!(
                "id: {id}\ntitle: Example\ntype: Custom\nstatus: open\npriority: 1\ncreated_at: date\nupdated_at: date\nbody: retained\n"
            ),
        )?;
    }
    // This refuses under the old whole-corpus lookup before reaching the target.
    std::fs::write(
        tracker.join("custom-types/nested/sample-0000.toon"),
        "<<<<<<< damaged\n",
    )?;
    let workspace = super::Workspace::discover(directory.path())?;
    ITEM_OPENS.set(0);
    assert_eq!(workspace.get("sample-2999")?.body, "retained");
    assert_eq!(ITEM_OPENS.get(), 1);
    ITEM_OPENS.set(0);
    assert!(matches!(
        workspace.get("missing"),
        Err(PmRustError::ItemNotFound { .. })
    ));
    assert_eq!(ITEM_OPENS.get(), 0);
    Ok(())
}

/// Duplicate IDs come from directory entries before any content can hide the collision.
#[test]
fn single_item_get_checks_listing_duplicates_and_strict_target()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let tracker = directory.path().join(".agents/pm");
    std::fs::create_dir_all(tracker.join("tasks"))?;
    std::fs::create_dir_all(tracker.join("custom"))?;
    std::fs::write(tracker.join("settings.json"), "{}")?;
    let path = tracker.join("tasks/sample-one.toon");
    let duplicate = tracker.join("custom/sample-one.toon");
    std::fs::write(&path, "malformed target")?;
    std::fs::write(&duplicate, "different malformed target")?;
    let workspace = super::Workspace::discover(directory.path())?;
    ITEM_OPENS.set(0);
    assert!(matches!(
        workspace.get("sample-one"),
        Err(PmRustError::DuplicateItemId { .. })
    ));
    assert_eq!(ITEM_OPENS.get(), 0);
    std::fs::remove_file(&duplicate)?;
    for content in [
        "<<<<<<< ours\n",
        "bad: incomplete",
        "id: different\ntitle: Example\ntype: Task\nstatus: open\npriority: 1\ncreated_at: date\nupdated_at: date\n",
    ] {
        std::fs::write(&path, content)?;
        assert!(matches!(
            workspace.get("sample-one"),
            Err(PmRustError::InvalidItemDocument { .. })
        ));
    }
    Ok(())
}

/// Directory-entry metadata failures retain the failing entry, rather than guessing its type.
#[test]
fn lookup_listing_type_failure_preserves_entry_path() {
    let path = Path::new("tasks/sample-one.toon");
    assert!(
        matches!(super::listing_type(path, Err(io::Error::other("fixture"))), Err(PmRustError::Io { path: failed, .. }) if failed == path)
    );
}

/// Streaming readers still reject duplicate decoded identities, independently of projection.
#[test]
fn streamed_items_refuse_duplicate_ids() -> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let tracker = directory.path().join(".agents/pm");
    std::fs::create_dir_all(tracker.join("tasks"))?;
    std::fs::write(tracker.join("settings.json"), "{}")?;
    let content = "id: same\ntitle: Example\ntype: Task\nstatus: open\npriority: 1\ncreated_at: date\nupdated_at: date\n";
    std::fs::create_dir_all(tracker.join("custom"))?;
    for folder in ["tasks", "custom"] {
        std::fs::write(tracker.join(format!("{folder}/same.toon")), content)?;
    }
    for index in 0..70 {
        let id = format!("tail-{index:03}");
        std::fs::write(
            tracker.join(format!("tasks/{id}.toon")),
            content.replace("id: same", &format!("id: {id}")),
        )?;
    }
    // Preserve source-order refusal precedence even when a later worker fails I/O.
    std::fs::write(tracker.join("tasks/zzz-invalid.toon"), [0xff])?;
    let workspace = super::Workspace::discover(directory.path())?;
    assert!(matches!(
        workspace.read_items(),
        Err(PmRustError::DuplicateItemId { .. })
    ));
    let items = workspace
        .stream_items(crate::read_projection::Projection::List)?
        .collect::<Result<Vec<_>, _>>();
    assert!(matches!(items, Err(PmRustError::DuplicateItemId { .. })));
    assert!(matches!(
        workspace.search("example", &crate::SearchOptions::default()),
        Err(PmRustError::DuplicateItemId { .. })
    ));
    Ok(())
}

/// Summary streaming preserves sorting, totals, cross-item filters and raw read failures.
#[test]
fn sdk_streamed_summary_selection_and_read_failures() -> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let root = directory.path().join(".agents/pm");
    std::fs::create_dir_all(root.join("tasks"))?;
    std::fs::write(root.join("settings.json"), "{}")?;
    for id in ["sample-one", "sample-two"] {
        let status = if id == "sample-two" { "closed" } else { "open" };
        std::fs::write(
            root.join(format!("tasks/{id}.toon")),
            format!(
                "id: {id}\ntitle: Needle\ntype: Task\nstatus: {status}\npriority: 1\ntags[1]: alpha\ncreated_at: date\nupdated_at: date\n"
            ),
        )?;
    }
    let workspace = super::Workspace::discover(directory.path())?;
    let summary = workspace.list(crate::ItemFilter::default())?;
    assert_eq!(summary.total, 2);
    let complete = workspace.list_page(
        &crate::ItemFilter::default(),
        &crate::ListOptions {
            all: true,
            ..Default::default()
        },
        "2026-08-22T10:00:00.000Z",
    )?;
    assert_eq!(complete["count"], 2);
    let selected = workspace.list(crate::ItemFilter {
        status: Some("closed".to_owned()),
        ..Default::default()
    })?;
    assert_eq!(selected.items.len(), 1);
    assert_eq!(selected.items[0].id, "sample-two");
    assert_eq!(selected.total, 2);
    assert_eq!(
        summary
            .items
            .iter()
            .map(|row| row.id.as_str())
            .collect::<Vec<_>>(),
        ["sample-one", "sample-two"]
    );
    let filters = crate::ItemFilter {
        id: Some("sample-one".to_owned()),
        ..Default::default()
    };
    let page = workspace.list_page(
        &filters,
        &crate::ListOptions {
            tag: Some("alpha".to_owned()),
            ..Default::default()
        },
        "2026-08-22T10:00:00.000Z",
    )?;
    assert_eq!(page["count"], 1);
    std::fs::write(root.join("tasks/sample-one.toon"), [0xff])?;
    assert!(matches!(
        workspace.get("sample-one"),
        Err(PmRustError::Io { .. })
    ));
    assert!(matches!(
        workspace.read_items(),
        Err(PmRustError::Io { .. })
    ));
    assert!(matches!(
        workspace.list(crate::ItemFilter::default()),
        Err(PmRustError::Io { .. })
    ));
    assert!(matches!(
        workspace.search("needle", &crate::SearchOptions::default()),
        Err(PmRustError::Io { .. })
    ));
    Ok(())
}

/// A lost tracker is a real initializer refusal for every direct streaming boundary.
#[test]
fn streamed_initializers_refuse_a_deleted_tracker() -> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let root = directory.path().join(".agents/pm");
    std::fs::create_dir_all(&root)?;
    std::fs::write(root.join("settings.json"), "{}")?;
    let workspace = super::Workspace::discover(directory.path())?;
    std::fs::remove_dir_all(root)?;
    assert!(workspace.read_items().is_err());
    assert!(workspace.item_sources().is_err());
    assert!(
        workspace
            .stream_items(crate::read_projection::Projection::Summary)
            .is_err()
    );
    assert!(workspace.list(crate::ItemFilter::default()).is_err());
    Ok(())
}

/// A nested enumeration failure reaches the SDK selectors after readable settings.
#[test]
fn streamed_queries_refuse_unlistable_nested_directory() -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;
    #[cfg(windows)]
    use std::os::windows::fs::OpenOptionsExt;
    #[cfg(unix)]
    if rustix::process::geteuid().is_root() {
        return Ok(());
    }
    let directory = tempfile::tempdir()?;
    let root = directory.path().join(".agents/pm");
    let locked = root.join("tasks/nested/locked");
    std::fs::create_dir_all(&locked)?;
    let locked = std::fs::canonicalize(locked)?;
    std::fs::write(root.join("settings.json"), "{}")?;
    let workspace = super::Workspace::discover(directory.path())?;
    #[cfg(unix)]
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000))?;
    #[cfg(windows)]
    let held = std::fs::OpenOptions::new()
        .access_mode(1)
        .share_mode(0)
        .custom_flags(0x0200_0000)
        .open(&locked)?;
    let results = [
        workspace.get("missing").map(|_| ()),
        workspace.read_items().map(|_| ()),
        workspace.list(crate::ItemFilter::default()).map(|_| ()),
        workspace
            .list_page(
                &crate::ItemFilter::default(),
                &crate::ListOptions::default(),
                "2026-08-22T10:00:00.000Z",
            )
            .map(|_| ()),
        workspace
            .context(
                &crate::ContextOptions {
                    no_extension_health: true,
                    ..Default::default()
                },
                "2026-08-22T10:00:00.000Z",
            )
            .map(|_| ()),
        workspace
            .search("needle", &crate::SearchOptions::default())
            .map(|_| ()),
    ];
    #[cfg(unix)]
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o700))?;
    #[cfg(windows)]
    drop(held);
    for result in results {
        assert!(matches!(result, Err(PmRustError::Io { path, .. }) if path == locked));
    }
    Ok(())
}
