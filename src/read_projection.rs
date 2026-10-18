//! Small query projections without decoding unrelated collection payloads.
use std::{fmt::Write as _, path::Path};

use crate::{ItemDocument, PmRustError};

/// Data required by one query; complete reads retain the authoritative decoder.
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Projection {
    /// Full metadata, collections and body.
    Full,
    /// List selection, hierarchy, sort keys and the identity projection.
    List,
    /// Brief context ranking, agenda and collection sizes.
    Context,
}

/// Selects complete root fields, retaining indentation and the strict decoder.
///
/// Quoted root keys fall back to the complete decoder rather than guessing their
/// identity. Merge markers anywhere in the source always use the refusal path.
pub(crate) fn decode(
    path: &Path,
    content: &str,
    projection: Projection,
) -> Result<ItemDocument, PmRustError> {
    if projection == Projection::Full
        || content.lines().any(|line| {
            line.starts_with('"')
                || ["<<<<<<<", "=======", ">>>>>>>"]
                    .iter()
                    .any(|marker| line.starts_with(marker))
        })
    {
        return crate::item::decode_item(path, content);
    }
    let mut projected = String::with_capacity(content.len().min(2048));
    let mut retain = true;
    let mut lines = content.lines().peekable();
    while let Some(line) = lines.next() {
        if !line.is_empty() && !line.starts_with([' ', '\t']) {
            let key = line.split([':', '[']).next().unwrap_or_default();
            if projection == Projection::Context
                && matches!(key, "comments" | "notes" | "learnings" | "test_runs")
                && let Some(count) = collection_count(line)
            {
                // Tabular canonical rows occupy exactly one indented line each.
                // Check the count instead of trusting an unvalidated array header.
                let mut rows = lines.clone();
                let actual = rows
                    .by_ref()
                    .take_while(|row| row.starts_with("  "))
                    .count();
                if actual == count {
                    projected.push_str(key);
                    let _ = write!(projected, "[{count}]: ");
                    for index in 0..count {
                        if index > 0 {
                            projected.push(',');
                        }
                        projected.push_str("null");
                    }
                    projected.push('\n');
                    for _ in 0..count {
                        lines.next();
                    }
                    retain = false;
                    continue;
                }
            }
            retain = if projection == Projection::List {
                matches!(
                    key,
                    "id" | "title"
                        | "type"
                        | "status"
                        | "priority"
                        | "tags"
                        | "created_at"
                        | "updated_at"
                        | "parent"
                        | "dependencies"
                        | "blocked_by"
                        | "deadline"
                )
            } else {
                !matches!(
                    key,
                    "body"
                        | "description"
                        | "files"
                        | "docs"
                        | "tests"
                        | "plan_steps"
                        | "plan_resume_context"
                )
            };
        }
        if retain {
            projected.push_str(line);
            projected.push('\n');
        }
    }
    crate::item::decode_item(path, &projected)
}

/// Extracts a canonical collection's declared size without materializing its rows.
fn collection_count(header: &str) -> Option<usize> {
    if header.ends_with(": []") {
        return Some(0);
    }
    let (_, tail) = header.split_once('[')?;
    let (count, suffix) = tail.split_once(']')?;
    if !suffix.starts_with('{') || !suffix.ends_with("}:") {
        return None;
    }
    count.parse().ok()
}

#[cfg(test)]
#[path = "../tests/support/read_projection_unit.rs"]
mod tests;
