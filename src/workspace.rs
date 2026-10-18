//! Deterministic workspace discovery and read-only item queries.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::item::decode_item;
use crate::mutation::{close_item, comment_item, create_item, update_item};
use crate::{
    CloseItem, CommentItem, CreateItem, CreateResult, ItemDocument, ItemSummary, MutationResult,
    OwnershipItem, OwnershipResult, PmRustError, UpdateItem,
};

pub(crate) const NON_ITEM_DIRECTORIES: [&str; 6] = [
    "extensions",
    "history",
    "locks",
    "runtime",
    "schema",
    "search",
];

/// One owned file source or its authoritative filesystem refusal.
type ItemSource = Result<(PathBuf, String), PmRustError>;

/// Exact filters supported by the first read-only list contract.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct ItemFilter {
    /// Exact lifecycle state to retain.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// Exact runtime item type to retain, compared case-insensitively.
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub item_type: Option<String>,
    /// Exact stable item identifier to retain.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
}

impl ItemFilter {
    /// Reports whether a document satisfies every configured exact filter.
    fn matches(&self, document: &ItemDocument) -> bool {
        self.status
            .as_ref()
            .is_none_or(|status| document.metadata.status == *status)
            && self
                .item_type
                .as_ref()
                .is_none_or(|item_type| document.metadata.item_type.eq_ignore_ascii_case(item_type))
            && self
                .id
                .as_ref()
                .is_none_or(|id| document.metadata.id == *id)
    }
}

/// Stable collection envelope returned by [`Workspace::list`].
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ListResult {
    /// Filtered item projections sorted by stable identifier.
    pub items: Vec<ItemSummary>,
    /// Number of returned items.
    pub count: usize,
    /// Number of valid stored items before filters.
    pub total: usize,
    /// Exact applied filters.
    pub filters: ItemFilter,
}

/// A discovered canonical pm tracker with read-only operations.
#[derive(Clone, Debug)]
pub struct Workspace {
    pm_root: PathBuf,
    query_pm_root: PathBuf,
}

/// Two workspaces are equal when they name the same canonical tracker.
///
/// `query_pm_root` keeps the spelling a caller discovered through (needed for
/// Windows query hashing), so it differs for one tracker reached through a
/// symlinked parent or `/tmp` versus `/private/tmp`; it must not split one
/// tracker into two unequal values.
impl PartialEq for Workspace {
    fn eq(&self, other: &Self) -> bool {
        self.pm_root == other.pm_root
    }
}

impl Workspace {
    /// Discovers a canonical tracker at the supplied path or an ancestor.
    ///
    /// A tracker is identified by `.agents/pm/settings.json`. Callers may also
    /// pass the tracker root itself.
    ///
    /// # Errors
    ///
    /// Returns [`PmRustError::Io`] when the supplied path cannot be resolved,
    /// or [`PmRustError::TrackerNotFound`] when no tracker marker exists.
    pub fn discover(start: &Path) -> Result<Self, PmRustError> {
        let supplied = start;
        let resolved = std::path::absolute(supplied).map_err(|source| PmRustError::Io {
            path: supplied.to_path_buf(),
            source,
        })?;
        let mut query_current =
            resolved
                .components()
                .fold(PathBuf::new(), |mut root, component| {
                    if component == std::path::Component::ParentDir {
                        root.pop();
                    } else {
                        root.push(component.as_os_str());
                    }
                    root
                });
        let mut current = fs::canonicalize(supplied).map_err(|source| PmRustError::Io {
            path: supplied.to_path_buf(),
            source,
        })?;
        if current.is_file() {
            current.pop();
            query_current.pop();
        }
        let discovery_start = current.clone();

        loop {
            if current.join("settings.json").is_file()
                && current.file_name().is_some_and(|name| name == "pm")
                && current
                    .parent()
                    .and_then(Path::file_name)
                    .is_some_and(|name| name == ".agents")
            {
                return Ok(Self {
                    pm_root: current,
                    query_pm_root: query_current,
                });
            }
            let candidate = current.join(".agents/pm");
            if !candidate.is_symlink() && candidate.join("settings.json").is_file() {
                return Ok(Self {
                    pm_root: candidate,
                    query_pm_root: query_current.join(".agents").join("pm"),
                });
            }
            if !current.pop() {
                return Err(PmRustError::TrackerNotFound {
                    start: discovery_start,
                });
            }
            query_current.pop();
        }
    }

    /// Returns the canonical tracker root.
    #[must_use]
    pub fn pm_root(&self) -> &Path {
        &self.pm_root
    }

    /// Returns the platform's working-directory spelling for published query hashes.
    pub(crate) fn query_pm_root(&self) -> &Path {
        query_root_path(&self.query_pm_root, &self.pm_root, cfg!(windows))
    }

    /// Reads and validates every stored TOON item, sorted by identifier.
    ///
    /// # Errors
    ///
    /// Returns a typed filesystem, document-validation, or duplicate-id error.
    #[cfg(not(test))]
    pub fn read_items(&self) -> Result<Vec<ItemDocument>, PmRustError> {
        self.read_items_impl()
    }

    /// Implements the corpus reader identically for ordinary and observed test calls.
    pub(crate) fn read_items_impl(&self) -> Result<Vec<ItemDocument>, PmRustError> {
        let paths = self.item_paths()?;

        let mut by_id: BTreeMap<String, (PathBuf, ItemDocument)> = BTreeMap::new();
        for path in paths {
            let document = read_item(&path)?;
            if let Some((first, _)) = by_id.get(&document.metadata.id) {
                return Err(PmRustError::DuplicateItemId {
                    id: document.metadata.id,
                    first: first.clone(),
                    second: path,
                });
            }
            by_id.insert(document.metadata.id.clone(), (path, document));
        }
        Ok(by_id.into_values().map(|(_, document)| document).collect())
    }

    /// Lists actual regular item paths without opening or decoding item contents.
    pub(crate) fn item_paths(&self) -> Result<Vec<PathBuf>, PmRustError> {
        let mut paths = Vec::new();
        for entry in read_directory(&self.pm_root)? {
            let entry_path = entry.path();
            if entry_path.is_symlink()
                || !entry_path.is_dir()
                || NON_ITEM_DIRECTORIES.contains(&entry.file_name().to_string_lossy().as_ref())
            {
                continue;
            }
            collect_toon_paths(&entry_path, &mut paths)?;
        }
        paths.sort();

        Ok(paths)
    }

    /// Lists regular filenames, optionally selecting one ID, without per-item metadata calls.
    fn lookup_paths(&self, id: Option<&str>) -> Result<Vec<PathBuf>, PmRustError> {
        let mut found = Vec::new();
        for (path, kind) in listing_entries(&self.pm_root)? {
            if kind.is_dir()
                && !NON_ITEM_DIRECTORIES.contains(
                    &path
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .as_ref(),
                )
            {
                collect_lookup_paths(&path, id, &mut found)?;
            }
        }
        found.sort();
        Ok(found)
    }

    /// Streams one decoded projection at a time in deterministic path order.
    pub(crate) fn stream_items(
        &self,
        projection: crate::read_projection::Projection,
    ) -> Result<impl Iterator<Item = Result<ItemDocument, PmRustError>>, PmRustError> {
        let mut seen = BTreeMap::new();
        Ok(self.item_sources()?.map(move |source| {
            let (path, content) = source?;
            decode_unique(&path, &content, projection, &mut seen)
        }))
    }

    /// Opens sources one at a time, allowing metadata filters before payload decoding.
    pub(crate) fn item_sources(&self) -> Result<impl Iterator<Item = ItemSource>, PmRustError> {
        #[cfg(test)]
        crate::ownership::tests::READ_ITEMS_SCANS.with(|count| count.set(count.get() + 1));
        let mut paths = self.lookup_paths(None)?;
        paths.sort_by(|a, b| a.file_stem().cmp(&b.file_stem()).then_with(|| a.cmp(b)));
        #[cfg(any(windows, target_os = "macos"))]
        {
            // Bound both raw-content memory and thread creation. Decoding remains
            // ordered on the caller, preserving first-error and cursor semantics.
            let batches: Vec<_> = paths.chunks(256).map(<[PathBuf]>::to_vec).collect();
            Ok(batches
                .into_iter()
                .flat_map(|batch| read_parallel_batch(&batch)))
        }
        #[cfg(not(any(windows, target_os = "macos")))]
        {
            Ok(paths.into_iter().map(|path| {
                let content = read_item_text(&path)?;
                Ok((path, content))
            }))
        }
    }

    /// Lists stable item projections using exact filters.
    ///
    /// # Errors
    ///
    /// Returns any error produced while reading and validating stored items.
    pub fn list(&self, filters: ItemFilter) -> Result<ListResult, PmRustError> {
        let mut total = 0;
        let mut items = Vec::new();
        for document in self.stream_items(crate::read_projection::Projection::Summary)? {
            let document = document?;
            total += 1;
            if filters.matches(&document) {
                items.push(ItemSummary::from(&document));
            }
        }
        items.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(ListResult {
            count: items.len(),
            items,
            total,
            filters,
        })
    }

    /// Returns the published PM CLI 2026.10.2 unbounded-list JSON envelope.
    ///
    /// Uses the tracker lifecycle registry, with built-in roles as defaults.
    /// Default reads exclude terminal items and project id/status/type/title;
    /// `all` or status `all` includes them and projects complete metadata.
    /// Ordering is terminal-last, priority ascending, update time descending,
    /// then canonical lowercase ASCII ID. The caller supplies the read clock.
    /// Pagination, runtime registries, hooks and output budgeting are outside
    /// this slice. Existing [`Self::list`] retains its native contract.
    ///
    /// # Errors
    ///
    /// Fails closed on stored-item read errors, unsupported CSV filters, or
    /// combining `all` with an explicit status filter.
    pub fn list_unbounded(
        &self,
        filters: &ItemFilter,
        all: bool,
        now: &str,
    ) -> Result<serde_json::Value, PmRustError> {
        crate::list::read_unbounded(self, filters, all, false, now)
    }

    /// Returns complete item metadata with the same unbounded list selection.
    ///
    /// Restores fields omitted by [`Self::list_unbounded`] without including
    /// terminal items unless `all` or an explicit status requests them. The
    /// caller supplies the read clock; ordering and filtering are unchanged.
    ///
    /// # Errors
    ///
    /// Fails closed on stored-item read errors, unsupported CSV filters, or
    /// combining `all` with an explicit status filter.
    pub fn list_unbounded_full(
        &self,
        filters: &ItemFilter,
        all: bool,
        now: &str,
    ) -> Result<serde_json::Value, PmRustError> {
        crate::list::read_unbounded(self, filters, all, true, now)
    }

    /// Returns the published list envelope with paging and bounded output receipts.
    ///
    /// Cursors bind the resolved tracker root and selection. Producer cursors
    /// follow item identity; output cursors additionally bind the row snapshot.
    /// The caller supplies the read clock, which does not bind either cursor.
    /// Serialize the returned [`crate::ListOutput`] itself to preserve escaped
    /// UTF-16 cuts. Its immutable JSON value view uses U+FFFD for split units.
    ///
    /// # Errors
    ///
    /// Fails on unreadable items, unsupported filters, invalid output controls,
    /// mismatched query cursors, or stale output continuations.
    pub fn list_page(
        &self,
        filters: &ItemFilter,
        options: &crate::ListOptions,
        now: &str,
    ) -> Result<crate::ListOutput, PmRustError> {
        crate::list::read_page(self, filters, options, now)
    }

    /// Reads one item by exact stable identifier.
    ///
    /// # Errors
    ///
    /// Returns an item-read error or [`PmRustError::ItemNotFound`].
    pub fn get(&self, id: &str) -> Result<ItemDocument, PmRustError> {
        let mut found = None;
        for path in self.lookup_paths(Some(id))? {
            if let Some(first) = found {
                return Err(PmRustError::DuplicateItemId {
                    id: id.to_owned(),
                    first,
                    second: path,
                });
            }
            found = Some(path);
        }
        let path = found.ok_or_else(|| PmRustError::ItemNotFound { id: id.to_owned() })?;
        let document = read_item(&path)?;
        if document.metadata.id != id {
            return Err(PmRustError::InvalidItemDocument {
                path,
                reason: "stored id does not match the item filename".to_owned(),
            });
        }
        Ok(document)
    }

    /// Creates one canonical item under a crash-recoverable per-item transaction.
    ///
    /// The first mutation slice deliberately requires an explicit identifier and
    /// supports canonical built-in item folders only. It writes both the TOON
    /// document and its create-history stream without invoking another runtime.
    ///
    /// # Errors
    ///
    /// Returns a typed validation, lock, conflict, recovery, or filesystem
    /// error. Existing item and history bytes are never overwritten.
    pub fn create(&self, request: CreateItem) -> Result<CreateResult, PmRustError> {
        create_item(&self.pm_root, request)
    }

    /// Applies one validated field update to an existing item in place.
    ///
    /// The mutation writes canonical TOON bytes and appends the matching
    /// `update` history record without invoking another runtime.
    ///
    /// # Errors
    ///
    /// Returns a typed validation, lock, conflict, recovery, or filesystem
    /// error. The item must already exist and at least one of `title`,
    /// `description`, `status`, `priority`, `tags` or `body` must be supplied.
    /// Supplying a field is what is required, not changing its value: a request
    /// that sets a field to what it already holds is accepted and writes a
    /// history record, matching the published CLI.
    pub fn update(&self, request: UpdateItem) -> Result<MutationResult, PmRustError> {
        update_item(&self.pm_root, request)
    }

    /// Appends one comment row to an existing item's comment list.
    ///
    /// # Errors
    ///
    /// Returns a typed validation, lock, conflict, recovery, or filesystem
    /// error. The item and the comment text must both exist.
    pub fn comment(&self, request: &CommentItem) -> Result<MutationResult, PmRustError> {
        comment_item(&self.pm_root, request)
    }

    /// Atomically claims an explicit item for an asserted author.
    ///
    /// # Errors
    /// Returns validation, lock, ownership, recovery, or filesystem failures.
    pub fn claim(&self, request: &OwnershipItem) -> Result<OwnershipResult, PmRustError> {
        self.claim_with_options(request, &crate::OwnershipOptions::default())
    }

    /// Releases ownership according to the tracker's ownership policy.
    ///
    /// # Errors
    /// Returns validation, lock, ownership, recovery, or filesystem failures.
    pub fn release(&self, request: &OwnershipItem) -> Result<OwnershipResult, PmRustError> {
        self.release_with_options(request, &crate::OwnershipOptions::default())
    }

    /// Closes one open item with an immutable closing summary.
    ///
    /// # Errors
    ///
    /// Returns a typed validation, lock, conflict, recovery, or filesystem
    /// error. Repeating terminal closure requires an explicit force override.
    pub fn close(&self, request: CloseItem) -> Result<MutationResult, PmRustError> {
        // Recovery hints spell the tracker path as Node does (no verbatim prefix or long-name expansion on Windows).
        close_item(&self.pm_root, self.query_pm_root(), request)
    }
}

/// Reads at most 256 files with four workers and preserves listing order.
///
/// Overlaps file-open latency without retaining every raw document.
/// Keep prefetch separate from decoding; direct get never uses it.
#[cfg(any(windows, target_os = "macos"))]
fn read_parallel_batch(batch: &[PathBuf]) -> Vec<ItemSource> {
    let mut groups: [Vec<ItemSource>; 4] = std::array::from_fn(|_| Vec::new());
    std::thread::scope(|scope| {
        for (paths, rows) in batch.chunks(64).zip(groups.iter_mut()) {
            scope.spawn(move || {
                for path in paths {
                    rows.push(read_item_text(path).map(|content| (path.clone(), content)));
                }
            });
        }
    });
    groups.into_iter().flatten().collect()
}

/// Decodes one projection and rejects repeated authoritative identities while streaming.
pub(crate) fn decode_unique(
    path: &Path,
    content: &str,
    projection: crate::read_projection::Projection,
    seen: &mut BTreeMap<String, PathBuf>,
) -> Result<ItemDocument, PmRustError> {
    let document = crate::read_projection::decode(path, content, projection)?;
    if let Some(first) = seen.insert(document.metadata.id.clone(), path.to_path_buf()) {
        return Err(PmRustError::DuplicateItemId {
            id: document.metadata.id,
            first,
            second: path.to_path_buf(),
        });
    }
    Ok(document)
}

/// Opens one authoritative item file, also observing real opens in unit tests.
fn read_item_text(path: &Path) -> Result<String, PmRustError> {
    #[cfg(test)]
    tests::ITEM_OPENS.with(|count| count.set(count.get() + 1));
    fs::read_to_string(path).map_err(|source| PmRustError::Io {
        path: path.to_path_buf(),
        source,
    })
}

/// Strictly decodes the complete authoritative file selected by its listing entry.
fn read_item(path: &Path) -> Result<ItemDocument, PmRustError> {
    decode_item(path, &read_item_text(path)?)
}

/// Matches Node's working-directory spelling without expanding Windows aliases.
fn query_root_path<'a>(resolved: &'a Path, canonical: &'a Path, windows: bool) -> &'a Path {
    if windows { resolved } else { canonical }
}

/// Reads every entry in a directory while retaining the path in typed errors.
pub(crate) fn read_directory(path: &Path) -> Result<Vec<fs::DirEntry>, PmRustError> {
    let entries = fs::read_dir(path).map_err(|source| PmRustError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    collect_directory_entries(path, entries)
}

/// Collects a directory iterator and maps entry failures to the directory path.
fn collect_directory_entries(
    path: &Path,
    entries: impl Iterator<Item = io::Result<fs::DirEntry>>,
) -> Result<Vec<fs::DirEntry>, PmRustError> {
    entries
        .collect::<Result<Vec<_>, _>>()
        .map_err(|source| PmRustError::Io {
            path: path.to_path_buf(),
            source,
        })
}

/// Maps directory-entry type failures without replacing them with guessed folders.
fn listing_type(
    path: &Path,
    result: io::Result<fs::FileType>,
) -> Result<fs::FileType, PmRustError> {
    result.map_err(|source| PmRustError::Io {
        path: path.to_path_buf(),
        source,
    })
}

/// Captures paths and cached entry types through one fallible listing boundary.
fn listing_entries(path: &Path) -> Result<Vec<(PathBuf, fs::FileType)>, PmRustError> {
    read_directory(path)?
        .into_iter()
        .map(|entry| {
            let path = entry.path();
            listing_type(&path, entry.file_type()).map(|kind| (path, kind))
        })
        .collect()
}

/// Recurses actual type directories, ignoring symbolic links and unrelated filenames.
fn collect_lookup_paths(
    path: &Path,
    id: Option<&str>,
    paths: &mut Vec<PathBuf>,
) -> Result<(), PmRustError> {
    for (entry_path, kind) in listing_entries(path)? {
        if kind.is_dir() {
            collect_lookup_paths(&entry_path, id, paths)?;
        } else if kind.is_file()
            && entry_path
                .extension()
                .is_some_and(|extension| extension == "toon")
            && id.is_none_or(|id| entry_path.file_stem().is_some_and(|name| name == id))
        {
            paths.push(entry_path);
        }
    }
    Ok(())
}

/// Recursively collects regular TOON files without following symbolic links.
fn collect_toon_paths(path: &Path, paths: &mut Vec<PathBuf>) -> Result<(), PmRustError> {
    collect_toon_entries(read_directory(path)?, paths)
}

/// Processes a directory snapshot, ignoring entries removed before metadata reads.
fn collect_toon_entries(
    entries: Vec<fs::DirEntry>,
    paths: &mut Vec<PathBuf>,
) -> Result<(), PmRustError> {
    for entry in entries {
        let entry_path = entry.path();
        let Ok(metadata) = entry_path.symlink_metadata() else {
            continue;
        };
        if metadata.is_dir() {
            collect_toon_paths(&entry_path, paths)?;
        } else if metadata.is_file() && entry_path.extension().is_some_and(|value| value == "toon")
        {
            paths.push(entry_path);
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "../tests/support/workspace_unit.rs"]
mod tests;
