//! Published list selection, pagination and output contracts.

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
        "items": items, "count": items.len(), "total": items.len(),
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

/// Presentation and continuation controls for [`Workspace::list_page`].
///
/// String numeric controls preserve the published filter echo, including leading
/// zeros. Omitted output budgets use the built-in JSON ceiling of 6,000 tokens.
// These booleans mirror independent published CLI controls.
#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Debug, Default)]
pub struct ListOptions {
    /// Include terminal statuses and use the full default projection.
    pub all: bool,
    /// Explicitly restore complete metadata and disclose legacy alias use.
    pub full: bool,
    /// Explicitly select the identity projection, including with `all` or triage.
    pub brief: bool,
    /// Legacy producer page size; canonical output limits independently cap rows.
    pub limit: Option<String>,
    /// Zero-based number of matching rows to skip; conflicts with `after`.
    pub offset: Option<String>,
    /// Opaque producer cursor from a preceding response.
    pub after: Option<String>,
    /// Ignore the producer limit and return every remaining matching row.
    pub no_truncate: bool,
    /// Canonical row ceiling: a positive integer or `unbounded`.
    pub output_limit: Option<String>,
    /// Canonical JSON token ceiling: a positive integer or `unbounded`.
    pub output_budget: Option<String>,
    /// Snapshot-bound continuation disclosed by output budget compaction.
    pub output_cursor: Option<String>,
    /// Declared built-in context intent; currently `triage`.
    pub intent: Option<String>,
    /// Override the intent's token budget; requires `intent` and at least 256.
    pub token_budget: Option<String>,
}

/// Parses the safe integer domain shared by the native and published controls.
pub(crate) fn integer(raw: &str, minimum: usize) -> Result<usize, PmRustError> {
    let normalized = raw.trim();
    if normalized.is_empty() || !normalized.bytes().all(|b| b.is_ascii_digit()) {
        return Err(PmRustError::InvalidReadRequest {
            reason: "output control must be an integer".to_owned(),
        });
    }
    let value = normalized
        .parse::<usize>()
        .map_err(|_| PmRustError::InvalidReadRequest {
            reason: "output control exceeds the safe integer range".to_owned(),
        })?;
    if value < minimum || value as u64 > 9_007_199_254_740_991 {
        return Err(PmRustError::InvalidReadRequest {
            reason: "output control is outside the supported integer range".to_owned(),
        });
    }
    Ok(value)
}

/// Parses a canonical amount or cost control; `None` means unbounded.
pub(crate) fn bound(raw: Option<&str>) -> Result<Option<usize>, PmRustError> {
    raw.filter(|raw| *raw != "unbounded")
        .map(|raw| integer(raw, 1))
        .transpose()
}

/// Builds exactly the published CLI's semantic selection for cursor hashing.
fn cursor_fingerprint(
    workspace: &Workspace,
    filters: &ItemFilter,
    options: &ListOptions,
) -> String {
    let mut semantic = json!({"comments":true,"dependencyBlocked":false,"deps":true,"docs":true,"files":true,"learnings":true,"linkedCommand":true,"notes":true,"tests":true,"truncate":!options.no_truncate});
    let status = filters.status.as_deref();
    let variant = options.all || status == Some("all");
    let name = if variant {
        "list-all".to_owned()
    } else if let Some(status) = status {
        format!("list-{status}")
    } else {
        "list".to_owned()
    };
    semantic["projectionCommand"] = json!(name);
    if status.is_none() && !options.all {
        semantic["excludeTerminal"] = json!(true);
    }
    if options.all {
        semantic["all"] = json!(true);
    }
    if matches!(status, Some("open" | "in_progress")) {
        semantic["lifecycleBucket"] = json!(status);
    }
    if status == Some("blocked") {
        semantic["dependencyBlocked"] = json!(true);
    }
    if let Some(value) = &filters.item_type {
        semantic["type"] = json!(value);
    }
    if let Some(value) = &filters.id {
        semantic["ids"] = json!(value);
    }
    if let Some(value) = &options.intent {
        semantic["for"] = json!(value);
    }
    crate::pagination::fingerprint(
        &json!({"pmRoot":crate::pagination::query_root(&workspace.pm_root().to_string_lossy()), "status":if variant {json!("all")} else {json!(status)}, "options":semantic,"sort":"default","order":"asc","tree":false,"tree_depth":null}),
    )
}

/// Reads one producer page, then applies intent, continuation and output bounds.
pub(crate) fn read_page(
    workspace: &Workspace,
    filters: &ItemFilter,
    options: &ListOptions,
    now: &str,
) -> Result<Value, PmRustError> {
    if options.after.is_some() && options.offset.is_some() {
        return Err(PmRustError::InvalidReadRequest {
            reason: "List --after cannot be combined with --offset.".to_owned(),
        });
    }
    if options.full && options.brief {
        return Err(PmRustError::InvalidReadRequest {
            reason: "full conflicts with brief".to_owned(),
        });
    }
    let requested = options;
    let mut options = options.clone();
    let intent_budget = crate::list_intent::prepare(&mut options)?;
    let mut result = read_unbounded(
        workspace,
        filters,
        options.all,
        options.full || intent_budget.is_some(),
        now,
    )?;
    if options.brief {
        result["items"] = json!(result["items"].as_array().into_iter().flatten().map(|row| json!({"id":row["id"],"status":row["status"],"type":row["type"],"title":row["title"]})).collect::<Vec<_>>());
        result["projection"] = json!({"mode":"brief","fields":["id","status","type","title"]});
        result["omission_receipt"] = json!({"has_omissions":true,"omitted_field_group_count":1,"omitted_field_groups":[{"name":"full_item_fields","restore_with":"--full"}]});
    } else if intent_budget.is_some() && !options.full {
        crate::list_intent::project(&mut result);
    }
    let fingerprint = cursor_fingerprint(workspace, filters, &options);
    result = page_envelope(result, &options, &fingerprint)?;
    let mut echo = result["filters"].as_object().cloned().unwrap_or_default();
    echo.shift_remove("runtime_filters");
    for (key, value) in [
        ("limit", &options.limit),
        ("offset", &options.offset),
        ("after", &options.after),
    ] {
        if let Some(value) = value {
            echo.insert(key.to_owned(), json!(value));
        }
    }
    if intent_budget.is_some() && !options.full && !options.brief {
        echo.insert(
            "fields".to_owned(),
            json!(crate::list_intent::FIELDS.join(",")),
        );
    }
    if options.no_truncate {
        echo.insert("no_truncate".to_owned(), json!(true));
    }
    echo.insert("runtime_filters".to_owned(), json!({}));
    result["filters"] = Value::Object(echo);
    if let Some(budget) = intent_budget {
        result = crate::list_intent::attach(result, &options, budget);
    }
    if options.after.is_some() {
        let mut map = result.as_object().cloned().unwrap_or_default();
        for key in [
            "applied_limit",
            "completeness",
            "context_intent",
            "count",
            "filters",
            "has_more",
            "now",
            "omission_receipt",
            "projection",
            "sorting",
            "total",
            "truncated",
        ] {
            map.shift_remove(key);
        }
        map.insert(
            "continuation_contract".to_owned(),
            json!({"fingerprint":fingerprint,"metadata":"reference","restore_with":"omit --after"}),
        );
        result = Value::Object(map);
    }
    crate::read_output::apply(result, requested)
}

/// Slices the stable selection and builds the published producer envelope.
fn page_envelope(
    mut result: Value,
    options: &ListOptions,
    fingerprint: &str,
) -> Result<Value, PmRustError> {
    let rows = result["items"].as_array().cloned().unwrap_or_default();
    let start = if let Some(raw) = &options.after {
        let cursor = crate::pagination::query(raw, fingerprint)?;
        if let Some(index) = rows.iter().position(|row| row["id"] == cursor["after_id"]) {
            index + 1
        } else if let Some(index) = cursor["after_index"].as_u64() {
            usize::try_from(index).unwrap_or(usize::MAX).min(rows.len())
        } else {
            return Err(crate::pagination::refusal(
                "invalid_query_cursor",
                format!(
                    "Query cursor item {} is no longer present in this result set.",
                    cursor["after_id"].as_str().unwrap_or_default()
                ),
            ));
        }
    } else {
        options
            .offset
            .as_deref()
            .map(|raw| integer(raw, 0))
            .transpose()?
            .unwrap_or(0)
    };
    let limit = if options.no_truncate {
        None
    } else {
        options
            .limit
            .as_deref()
            .map(|raw| integer(raw, 0))
            .transpose()?
            .or_else(|| (rows.len() >= 10_000).then_some(20))
    };
    let page = rows
        .into_iter()
        .skip(start)
        .take(limit.unwrap_or(usize::MAX))
        .collect::<Vec<_>>();
    let has_more = !page.is_empty()
        && start + page.len()
            < usize::try_from(result["total"].as_u64().unwrap_or_default()).unwrap_or(usize::MAX);
    result["count"] = json!(page.len());
    // Insertion order is observable in both JSON bytes and token measurements.
    let mut ordered = serde_json::Map::new();
    ordered.insert("items".to_owned(), json!(page));
    ordered.insert("count".to_owned(), result["count"].clone());
    ordered.insert("total".to_owned(), result["total"].clone());
    if let Some(limit) = limit {
        ordered.insert("applied_limit".to_owned(), json!(limit));
    }
    result["has_more"] = json!(has_more);
    result["truncated"] = json!(has_more);
    result["next_cursor"] = if has_more {
        json!(crate::pagination::after(
            fingerprint,
            &page[page.len() - 1],
            start + page.len() - 1
        ))
    } else {
        Value::Null
    };
    for key in [
        "has_more",
        "truncated",
        "next_cursor",
        "completeness",
        "filters",
        "projection",
        "sorting",
        "now",
        "omission_receipt",
    ] {
        ordered.insert(key.to_owned(), result[key].clone());
    }
    result = Value::Object(ordered);
    Ok(result)
}
