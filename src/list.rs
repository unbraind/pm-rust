//! Published unbounded-list projection for the default lifecycle registry.

use std::cmp::Reverse;

use serde_json::{Map, Value, json};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use crate::{ItemFilter, PmRustError, Workspace, canonical_metadata_pairs};

#[cfg(test)]
thread_local! {
    /// Optional per-test timestamp parse counter, isolated across test threads.
    static TIMESTAMP_PARSES: std::cell::Cell<Option<usize>> = const { std::cell::Cell::new(None) };
}

/// Classifies terminal statuses in the published built-in lifecycle registry.
fn terminal(status: &str) -> bool {
    matches!(status, "closed" | "canceled")
}

/// Builds an ascending timestamp key with invalid values below valid instants.
fn timestamp_key(value: &str) -> (Option<i128>, String) {
    #[cfg(test)]
    TIMESTAMP_PARSES.with(|counter| counter.set(counter.get().map(|count| count + 1)));
    (
        OffsetDateTime::parse(value, &Rfc3339)
            .ok()
            .map(|instant| instant.unix_timestamp_nanos().div_euclid(1_000_000)),
        value.to_owned(),
    )
}

/// Projects validated documents into an exact, unbounded published envelope.
pub(crate) fn read_unbounded(
    workspace: &Workspace,
    filters: &ItemFilter,
    all: bool,
    full_projection: bool,
    now: &str,
) -> Result<Value, PmRustError> {
    for value in [&filters.status, &filters.item_type, &filters.id]
        .into_iter()
        .flatten()
    {
        if value.contains(',') {
            return Err(PmRustError::InvalidReadRequest {
                reason: "unbounded list supports single exact filters, not CSV selectors"
                    .to_owned(),
            });
        }
    }
    if all && filters.status.is_some() {
        return Err(PmRustError::InvalidReadRequest {
            reason: "all conflicts with an explicit status filter".to_owned(),
        });
    }
    let include_terminal = all || filters.status.as_deref() == Some("all");
    let full = full_projection || include_terminal;
    let mut echo = Map::new();
    if include_terminal {
        echo.insert("status".to_owned(), json!("all"));
    } else if let Some(status) = &filters.status {
        echo.insert("status".to_owned(), json!(status));
    }
    if let Some(item_type) = &filters.item_type {
        echo.insert("type".to_owned(), json!(item_type));
    }
    if let Some(id) = &filters.id {
        echo.insert("ids".to_owned(), json!(id));
    }
    echo.insert("runtime_filters".to_owned(), json!({}));
    let mut documents = workspace.read_items()?;
    documents.retain(|document| {
        let metadata = &document.metadata;
        let status_matches = if include_terminal {
            true
        } else {
            filters.status.as_ref().map_or_else(
                || !terminal(&metadata.status),
                |status| metadata.status == *status,
            )
        };
        status_matches
            && filters
                .item_type
                .as_ref()
                .is_none_or(|kind| metadata.item_type.eq_ignore_ascii_case(kind))
            && filters.id.as_ref().is_none_or(|id| metadata.id == *id)
    });
    documents.sort_by_cached_key(|document| {
        let metadata = &document.metadata;
        (
            terminal(&metadata.status),
            metadata.priority,
            Reverse(timestamp_key(&metadata.updated_at)),
            metadata.id.clone(),
        )
    });
    let items: Vec<Value> = documents.into_iter().map(|document| {
        let metadata = document.metadata;
        if full {
            Value::Object(canonical_metadata_pairs(&metadata).into_iter().collect())
        } else {
            json!({"id": metadata.id, "status": metadata.status, "type": metadata.item_type, "title": metadata.title})
        }
    }).collect();
    let projection = if full {
        json!({"mode":"full", "fields":null})
    } else {
        json!({"mode":"brief", "fields":["id","status","type","title"]})
    };
    let omissions = if full {
        vec![]
    } else {
        vec![json!({"name":"full_item_fields", "restore_with":"--full"})]
    };
    Ok(json!({
        "count": items.len(), "total": items.len(), "items": items,
        "has_more": false, "truncated": false, "next_cursor": null,
        "completeness": {"status":"complete", "unreadable_item_count":0, "unreadable_directory_count":0},
        "filters": echo, "projection": projection,
        "sorting": {"sort":"default", "order":"asc"}, "now":now,
        "omission_receipt": {"has_omissions":!omissions.is_empty(), "omitted_field_group_count":omissions.len(), "omitted_field_groups":omissions}
    }))
}

#[cfg(test)]
#[path = "../tests/support/list_unit.rs"]
mod tests;
