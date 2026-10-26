//! Small query projections without decoding unrelated collection payloads.
use std::{borrow::Cow, fmt::Write as _, path::Path};

use crate::{ItemDocument, PmRustError};

/// Data required by one query; complete reads retain the authoritative decoder.
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Projection {
    /// Full metadata, collections and body.
    Full,
    /// List selection including hierarchy and dependency-blocking inputs.
    List,
    /// Scalar list/search filters, sorting and identity output without relationship payloads.
    Summary,
    /// Brief context ranking, agenda and collection sizes.
    Context,
    /// Context sections additionally need file links, test definitions and run totals.
    ContextSections,
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
    crate::item::decode_item(path, &project_text(content, projection))
}

/// Returns exactly the decoder input shared by execution and cost planning.
pub(crate) fn project_text(content: &str, projection: Projection) -> Cow<'_, str> {
    if projection == Projection::Full
        || content.lines().any(|line| {
            line.starts_with('"')
                || ["<<<<<<<", "=======", ">>>>>>>"]
                    .iter()
                    .any(|marker| line.starts_with(marker))
        })
    {
        return Cow::Borrowed(content);
    }
    let mut projected = String::with_capacity(content.len().min(2048));
    let mut retain = true;
    let mut lines = content.lines().peekable();
    while let Some(line) = lines.next() {
        if !line.is_empty() && !line.starts_with([' ', '\t']) {
            let key = line.split([':', '[']).next().unwrap_or_default();
            if matches!(
                projection,
                Projection::Context | Projection::ContextSections
            ) && matches!(key, "comments" | "notes" | "learnings" | "test_runs")
                && !(projection == Projection::ContextSections && key == "test_runs")
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
            retain = match key {
                "id" | "title" | "type" | "status" | "priority" | "tags" | "created_at"
                | "updated_at" | "parent" | "deadline" => true,
                "dependencies" | "blocked_by" => projection != Projection::Summary,
                "order" | "assignee" | "risk" | "release" | "comments" | "notes" | "learnings"
                | "test_runs" | "reminders" | "events" => matches!(
                    projection,
                    Projection::Context | Projection::ContextSections
                ),
                "files" | "tests" | "blocked_reason" | "unblock_note" => {
                    projection == Projection::ContextSections
                }
                _ => false,
            };
        }
        if retain {
            projected.push_str(line);
            projected.push('\n');
        }
    }
    Cow::Owned(projected)
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
