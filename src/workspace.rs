//! Deterministic workspace discovery and read-only item queries.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::item::decode_item;
use crate::{ItemDocument, ItemSummary, PmRustError};

const NON_ITEM_DIRECTORIES: [&str; 6] = [
    "extensions",
    "history",
    "locks",
    "runtime",
    "schema",
    "search",
];

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
#[derive(Clone, Debug, PartialEq)]
pub struct Workspace {
    pm_root: PathBuf,
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
        let mut current = fs::canonicalize(supplied).map_err(|source| PmRustError::Io {
            path: supplied.to_path_buf(),
            source,
        })?;
        if current.is_file() {
            current.pop();
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
                return Ok(Self { pm_root: current });
            }
            let candidate = current.join(".agents/pm");
            if candidate.join("settings.json").is_file() {
                return Ok(Self { pm_root: candidate });
            }
            if !current.pop() {
                return Err(PmRustError::TrackerNotFound {
                    start: discovery_start,
                });
            }
        }
    }

    /// Returns the canonical tracker root.
    #[must_use]
    pub fn pm_root(&self) -> &Path {
        &self.pm_root
    }

    /// Reads and validates every stored TOON item, sorted by identifier.
    ///
    /// # Errors
    ///
    /// Returns a typed filesystem, document-validation, or duplicate-id error.
    pub fn read_items(&self) -> Result<Vec<ItemDocument>, PmRustError> {
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

        let mut by_id: BTreeMap<String, (PathBuf, ItemDocument)> = BTreeMap::new();
        for path in paths {
            let content = fs::read_to_string(&path).map_err(|source| PmRustError::Io {
                path: path.clone(),
                source,
            })?;
            let document = decode_item(&path, &content)?;
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

    /// Lists stable item projections using exact filters.
    ///
    /// # Errors
    ///
    /// Returns any error produced while reading and validating stored items.
    pub fn list(&self, filters: ItemFilter) -> Result<ListResult, PmRustError> {
        let documents = self.read_items()?;
        let total = documents.len();
        let items = documents
            .iter()
            .filter(|document| filters.matches(document))
            .map(ItemSummary::from)
            .collect::<Vec<_>>();
        Ok(ListResult {
            count: items.len(),
            items,
            total,
            filters,
        })
    }

    /// Reads one item by exact stable identifier.
    ///
    /// # Errors
    ///
    /// Returns an item-read error or [`PmRustError::ItemNotFound`].
    pub fn get(&self, id: &str) -> Result<ItemDocument, PmRustError> {
        self.read_items()?
            .into_iter()
            .find(|document| document.metadata.id == id)
            .ok_or_else(|| PmRustError::ItemNotFound { id: id.to_owned() })
    }
}

fn read_directory(path: &Path) -> Result<Vec<fs::DirEntry>, PmRustError> {
    let entries = fs::read_dir(path).map_err(|source| PmRustError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    collect_directory_entries(path, entries)
}

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

fn collect_toon_paths(path: &Path, paths: &mut Vec<PathBuf>) -> Result<(), PmRustError> {
    for entry in read_directory(path)? {
        let entry_path = entry.path();
        if entry_path.is_symlink() {
            continue;
        }
        if entry_path.is_dir() {
            collect_toon_paths(&entry_path, paths)?;
        } else if entry_path.is_file()
            && entry_path.extension().is_some_and(|value| value == "toon")
        {
            paths.push(entry_path);
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "../tests/support/workspace_unit.rs"]
mod tests;
