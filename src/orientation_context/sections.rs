//! Optional context sections assembled from the captured metadata projection.
use super::{blockers, focus, optional, status_rank, text};
use crate::{PmRustError, Workspace, list_registry::Lifecycle, orientation::records};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

/// Canonical section enumeration, also used by the omission receipt.
pub(super) const NAMES: [&str; 10] = [
    "hierarchy",
    "activity",
    "progress",
    "blockers",
    "recently_created",
    "unparented",
    "files",
    "workload",
    "staleness",
    "tests",
];

/// Resolves enabled sections while retaining the oracle's depth-specific order.
pub(super) fn selected(depth: &str, settings: &Value) -> Vec<&'static str> {
    let names: &[&str] = match depth {
        "brief" => &[],
        "standard" => &[
            "hierarchy",
            "activity",
            "progress",
            "recently_created",
            "unparented",
            "workload",
        ],
        "deep" => &[
            "hierarchy",
            "activity",
            "progress",
            "recently_created",
            "unparented",
            "workload",
            "blockers",
            "files",
            "staleness",
            "tests",
        ],
        _ => &[
            "hierarchy",
            "activity",
            "progress",
            "recently_created",
            "unparented",
            "blockers",
            "files",
            "workload",
            "staleness",
            "tests",
        ],
    };
    names
        .iter()
        .copied()
        .filter(|name| {
            depth == "full"
                || settings["context"]["sections"][name]
                    .as_bool()
                    .unwrap_or(true)
        })
        .collect()
}

/// Compares structural criticality for hierarchy children and unparented rows.
fn critical(a: &Value, b: &Value, lifecycle: &Lifecycle) -> std::cmp::Ordering {
    status_rank(a, lifecycle)
        .cmp(&status_rank(b, lifecycle))
        .then_with(|| optional(&a["priority"], &b["priority"], false))
        .then_with(|| optional(&a["order"], &b["order"], false))
        .then_with(|| optional(&a["deadline"], &b["deadline"], true))
        .then_with(|| super::date_compare(text(b, "updated_at"), text(a, "updated_at")))
        .then_with(|| crate::collation::compare(text(a, "id"), text(b, "id")))
}

/// Captured parent index; traversal deliberately includes cyclic child edges like the oracle.
pub(super) struct Hierarchy<'a>(BTreeMap<&'a str, Vec<&'a Value>>);

impl<'a> Hierarchy<'a> {
    /// Indexes normalized parent strings once per context response.
    pub(super) fn new(corpus: &'a [Value], normalize: bool) -> Self {
        let mut children: BTreeMap<&str, Vec<&Value>> = BTreeMap::new();
        for item in corpus {
            let parent = if normalize {
                text(item, "parent").trim()
            } else {
                text(item, "parent")
            };
            if !parent.is_empty() {
                children.entry(parent).or_default().push(item);
            }
        }
        Self(children)
    }

    /// Returns depth-first descendants with the published visited-parent cycle guard.
    fn descendants(&self, id: &str) -> Vec<&'a Value> {
        let mut result = Vec::new();
        let mut pending = vec![id];
        let mut seen = BTreeSet::new();
        while let Some(id) = pending.pop() {
            if seen.insert(id) {
                for child in self.0.get(id).into_iter().flatten() {
                    result.push(*child);
                    pending.push(text(child, "id"));
                }
            }
        }
        result
    }

    /// Adds descendant completion fields only when at least one descendant exists.
    pub(super) fn enrich(&self, row: &mut Value, lifecycle: &Lifecycle) {
        let descendants = self.descendants(text(row, "id"));
        if !descendants.is_empty() {
            let closed = descendants
                .iter()
                .filter(|item| lifecycle.matches(text(item, "status"), "closed"))
                .count();
            row["children_total"] = json!(descendants.len());
            row["children_closed"] = json!(closed);
            row["completion_pct"] = completion(closed, descendants.len());
            for key in ["blocked", "blocker_ids"] {
                if let Some(value) = row.as_object_mut().and_then(|row| row.shift_remove(key)) {
                    row[key] = value;
                }
            }
        }
    }
}

/// Computes JavaScript's nonnegative rounded percentage.
fn completion(closed: usize, total: usize) -> Value {
    // Keep IEEE division before rounding: the oracle renders 29/200 as 14%, not 15%.
    // Closed descendants are a subset of a nonempty collection, so the cast is in 0..=100.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let percent = (super::number(closed) / super::number(total) * 100.0).round() as u64;
    json!(percent)
}

/// Reads compact activity from all retained streams, including deleted-item history.
fn activity(workspace: &Workspace, limit: usize) -> Result<Value, PmRustError> {
    let path = workspace.pm_root().join("history");
    let entries = match std::fs::read_dir(&path) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(json!([])),
        Err(source) => return Err(PmRustError::Io { path, source }),
    };
    let mut rows = Vec::new();
    for entry in entries {
        let entry = activity_entry(entry, &path)?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if let Some(id) = name.strip_suffix(".jsonl") {
            for entry in records(workspace, id)? {
                let mut row = json!({"id":id,"op":entry["op"].as_str().unwrap_or("unknown"),"ts":text(&entry,"ts"),"author":entry["author"].as_str().unwrap_or("unknown")});
                if entry["message"]
                    .as_str()
                    .is_some_and(|message| !message.is_empty())
                {
                    row["msg"] = entry["message"].clone();
                }
                rows.push(row);
            }
        }
    }
    rows.sort_by(|a, b| {
        crate::collation::compare(text(b, "ts"), text(a, "ts"))
            .then_with(|| crate::collation::compare(text(a, "id"), text(b, "id")))
            .then_with(|| crate::collation::compare(text(a, "op"), text(b, "op")))
    });
    rows.truncate(limit);
    Ok(json!(rows))
}

/// Preserves a directory iterator's original failure and history directory context.
fn activity_entry(
    entry: std::io::Result<std::fs::DirEntry>,
    path: &std::path::Path,
) -> Result<std::fs::DirEntry, PmRustError> {
    entry.map_err(|source| PmRustError::Io {
        path: path.to_path_buf(),
        source,
    })
}

/// Builds hierarchy counts and child summaries from already selected high-level parents.
fn hierarchy(
    parents: &[&Value],
    tree: &Hierarchy<'_>,
    lifecycle: &Lifecycle,
    limit: usize,
    blocked: Option<&BTreeSet<String>>,
) -> Value {
    let mut parents = parents.to_vec();
    if blocked.is_none() {
        parents.sort_by(|a, b| critical(a, b, lifecycle));
    }
    let mut rows = Vec::new();
    for parent in parents {
        let descendants = tree.descendants(text(parent, "id"));
        let closed = descendants
            .iter()
            .filter(|item| lifecycle.matches(text(item, "status"), "closed"))
            .count();
        let is_blocked = |item: &&Value| {
            blocked.map_or_else(
                || lifecycle.role(text(item, "status"), "blocked"),
                |ids| ids.contains(text(item, "id")),
            )
        };
        let blocked_count = descendants
            .iter()
            .filter(|item| {
                !lifecycle.matches(text(item, "status"), "closed")
                    && (blocked.is_some()
                        || !lifecycle.matches(text(item, "status"), "in_progress"))
                    && is_blocked(item)
            })
            .count();
        let progress = descendants
            .iter()
            .filter(|item| {
                (blocked.is_none() || !is_blocked(item))
                    && lifecycle.matches(text(item, "status"), "in_progress")
            })
            .count();
        let open = descendants
            .iter()
            .filter(|item| {
                !is_blocked(item)
                    && lifecycle.role(text(item, "status"), "active")
                    && !lifecycle.matches(text(item, "status"), "in_progress")
            })
            .count();
        if blocked.is_some() {
            if !descendants.is_empty() {
                rows.push(json!({"id":parent["id"],"title":parent["title"],"type":parent["type"],"total":descendants.len(),"closed":closed,"open":open,"in_progress":progress,"blocked":blocked_count,"completion_pct":completion(closed,descendants.len())}));
            }
        } else {
            let mut children = tree.0.get(text(parent, "id")).cloned().unwrap_or_default();
            children.sort_by(|a, b| critical(a, b, lifecycle));
            let children: Vec<_> = children.into_iter().take(limit).map(|child| {
                let grandchildren = tree.descendants(text(child,"id"));
                json!({"id":child["id"],"title":child["title"],"type":child["type"],"status":child["status"],"children_total":grandchildren.len(),"children_closed":grandchildren.iter().filter(|item|lifecycle.matches(text(item,"status"),"closed")).count()})
            }).collect();
            rows.push(json!({"id":parent["id"],"title":parent["title"],"type":parent["type"],"status":parent["status"],"children_total":descendants.len(),"children_closed":closed,"children_open":open,"children_in_progress":progress,"children_blocked":blocked_count,"children":children}));
        }
    }
    if blocked.is_some() {
        rows.sort_by(|a, b| {
            a["completion_pct"]
                .as_f64()
                .unwrap_or_default()
                .total_cmp(&b["completion_pct"].as_f64().unwrap_or_default())
        });
    }
    rows.truncate(limit);
    json!(rows)
}

/// Builds selected optional sections without reading full item bodies or collection prose.
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
pub(super) fn build(
    workspace: &Workspace,
    names: &[&str],
    corpus: &[Value],
    active: &[&Value],
    blocked_items: &[&Value],
    blocked: &BTreeSet<String>,
    lifecycle: &Lifecycle,
    now: &str,
    limit: usize,
    settings: &Value,
) -> Result<Value, PmRustError> {
    let mut result = json!({});
    let tree = Hierarchy::new(corpus, true);
    let progress_tree = Hierarchy::new(corpus, false);
    let nonterminal: Vec<_> = corpus
        .iter()
        .filter(|item| !lifecycle.terminal(text(item, "status")))
        .collect();
    let parents: Vec<_> = active
        .iter()
        .copied()
        .filter(|item| matches!(text(item, "type"), "Epic" | "Feature"))
        .collect();
    for name in names {
        result[name] = match *name {
            "hierarchy" => hierarchy(&parents,&tree,lifecycle,limit,None),
            "progress" => hierarchy(&parents,&progress_tree,lifecycle,limit,Some(blocked)),
            "activity" => activity(workspace, settings["context"]["activity_limit"].as_u64().and_then(|n|usize::try_from(n).ok()).unwrap_or(10))?,
            "recently_created" | "unparented" => {
                let mut items = nonterminal.clone();
                if *name == "recently_created" {
                    items.sort_by(|a,b| super::date_compare(text(b,"created_at"),text(a,"created_at")).then_with(||crate::collation::compare(text(a,"id"),text(b,"id"))));
                } else {
                    items.retain(|item| text(item,"parent").trim().is_empty() && !matches!(text(item,"type"),"Epic"|"Feature"));
                    items.sort_by(|a,b| critical(a,b,lifecycle));
                }
                json!(items.into_iter().take(limit).map(|item| {
                    // Auxiliary focus lists intentionally do not carry dependency blocking flags.
                    let mut row = focus(item,corpus,&BTreeSet::new(),lifecycle);
                    tree.enrich(&mut row,lifecycle);
                    if *name == "recently_created" { row["created_at"] = item["created_at"].clone(); }
                    row
                }).collect::<Vec<_>>())
            },
            "blockers" => json!(blocked_items.iter().take(limit).map(|item| {
                let mut ids = blockers(item,corpus,lifecycle);
                if ids.is_empty() { ids = super::blocker_ids(item,corpus,lifecycle,true); }
                let id = ids.first();
                let target = id.and_then(|id|corpus.iter().find(|item|text(item,"id").eq_ignore_ascii_case(id)));
                json!({"id":item["id"],"title":item["title"],"blocked_by":id,"blocked_by_title":target.map(|item|&item["title"]),"blocked_by_status":target.map(|item|&item["status"]),"blocked_reason":item["blocked_reason"],"unblock_note":item["unblock_note"]})
            }).collect::<Vec<_>>()),
            "files" | "workload" => {
                let mut groups: Vec<(Value, BTreeSet<String>, usize)> = Vec::new();
                for item in active {
                    let keys: Vec<_> = if *name == "files" { item["files"].as_array().into_iter().flatten().map(|file|file["path"].clone()).collect() } else { vec![item["assignee"].clone()] };
                    for key in keys {
                        let index = groups.iter().position(|(existing,_,_)| *existing == key).unwrap_or_else(|| { groups.push((key,BTreeSet::new(),0)); groups.len()-1 });
                        groups[index].1.insert(text(item,"id").to_owned());
                        groups[index].2 += usize::from(lifecycle.matches(text(item,"status"),"in_progress"));
                    }
                }
                groups.sort_by(|a,b|b.1.len().cmp(&a.1.len()));
                json!(groups.into_iter().take(limit).map(|(key,items,progress)|if *name == "files" { json!({"path":key,"references":items.len(),"items":items}) } else { json!({"assignee":key,"active":items.len(),"in_progress":progress,"items":items}) }).collect::<Vec<_>>())
            },
            "staleness" => {
                let now = crate::list::timestamp_key(now).0.unwrap_or_default();
                let days = settings["context"]["stale_threshold_days"].as_i64().unwrap_or(7);
                let mut rows: Vec<_> = nonterminal.iter().filter_map(|item| {
                    let age = now - crate::list::timestamp_key(text(item,"updated_at")).0?;
                    (age > i128::from(days) * 86_400_000).then(||json!({"id":item["id"],"title":item["title"],"status":item["status"],"updated_at":item["updated_at"],"stale_days":age/86_400_000}))
                }).collect();
                rows.sort_by_key(|item|std::cmp::Reverse(item["stale_days"].as_i64().unwrap_or_default()));
                rows.truncate(limit);
                json!(rows)
            },
            _ => {
                let mut with_tests = 0;
                let mut with_runs = 0;
                let mut totals = [0u64;3];
                let mut failing = BTreeSet::new();
                for item in active {
                    with_tests += usize::from(super::count(item,"tests") > 0);
                    with_runs += usize::from(super::count(item,"test_runs") > 0);
                    for run in item["test_runs"].as_array().into_iter().flatten() {
                        for (index,key) in ["passed","failed","skipped"].iter().enumerate() { totals[index] += run[key].as_u64().unwrap_or_default(); }
                        if run["failed"].as_u64().unwrap_or_default() > 0 { failing.insert(text(item,"id")); }
                    }
                }
                json!({"items_with_tests":with_tests,"items_with_recent_runs":with_runs,"recent_runs":{"passed":totals[0],"failed":totals[1],"skipped":totals[2]},"items_failing":failing})
            }
        };
    }
    Ok(result)
}

#[cfg(test)]
#[path = "../../tests/support/orientation_sections_unit.rs"]
mod tests;
