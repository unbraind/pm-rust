//! Bounded dependency tree and graph reads.
mod packet;
use crate::orientation::{complete_receipt, invalid, numeric};
use crate::{DepsOptions, ListOutput, PmRustError, Workspace};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

/// Normalized directed dependency index.
type Index = BTreeMap<String, Value>;

/// Traversal counters shared by every recursive expansion.
struct Budget {
    /// Delivered node ceiling.
    nodes: usize,
    /// Delivered edge ceiling.
    edges: usize,
    /// Estimated output ceiling.
    tokens: usize,
    /// Accumulated nodes.
    used_nodes: usize,
    /// Accumulated edges.
    used_edges: usize,
    /// Accumulated rough token estimate.
    used_tokens: usize,
    /// Truncation reasons in canonical order.
    reasons: BTreeSet<&'static str>,
    /// Effective traversal depth.
    depth: usize,
    /// Collapse repeated expansions.
    collapse: bool,
}

/// Returns a string member, using the empty sentinel for absent values.
fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value[key].as_str().unwrap_or_default()
}

/// Canonical identity lookup spelling.
fn key(id: &str) -> String {
    #[cfg(test)]
    KEY_CALLS.with(|calls| calls.set(calls.get() + 1));
    id.trim().to_lowercase()
}

#[cfg(test)]
thread_local! {
    /// Per-test work counter for deterministic dependency normalization budgets.
    static KEY_CALLS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Converts authoritative documents into normalized, sorted dependency rows.
fn index(workspace: &Workspace) -> Result<(Index, Value), PmRustError> {
    let corpus = workspace.read_items()?;
    let known: BTreeSet<String> = corpus.iter().map(|item| key(&item.metadata.id)).collect();
    let mut index = Index::new();
    let mut aliases: BTreeMap<String, u64> = BTreeMap::new();
    for item in &corpus {
        let metadata = &item.metadata;
        let mut dependencies: Vec<Value> = metadata.extra.get("dependencies").and_then(Value::as_array).into_iter().flatten().map(|dep| json!({"id":text(dep,"id").trim(),"kind":text(dep,"kind").trim().to_lowercase()})).collect();
        dependencies.sort_by(|a, b| {
            crate::collation::compare(text(a, "kind"), text(b, "kind"))
                .then_with(|| crate::collation::compare(text(a, "id"), text(b, "id")))
        });
        for dep in &dependencies {
            let normalized = text(dep, "kind").replace('-', "_");
            let kind = normalized.as_str();
            let canonical = match kind {
                "depends_on" => "blocked_by",
                "child_of" | "epic" => "parent",
                "parent_child" | "task" => "child",
                "related_to" => "related",
                _ => kind,
            };
            if canonical != kind {
                *aliases.entry(kind.to_owned()).or_default() += 1;
            }
        }
        let mut seen = BTreeSet::new();
        dependencies.retain(|dep| seen.insert((key(text(dep, "id")), key(text(dep, "kind")))));
        let blocked = metadata
            .extra
            .get("blocked_by")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim();
        // The oracle appends dangling scalar references in target/kind order,
        // including references held by terminal items but never existing parents.
        let mut scalar_references = [
            (
                metadata.parent.as_deref().unwrap_or_default().trim(),
                "parent",
            ),
            (blocked, "blocked_by"),
        ];
        scalar_references.sort_by(|a, b| {
            crate::collation::compare(a.0, b.0).then_with(|| crate::collation::compare(a.1, b.1))
        });
        for (target, kind) in scalar_references {
            let target_key = key(target);
            if !(target.is_empty()
                || known.contains(&target_key)
                || dependencies
                    .iter()
                    .any(|dep| key(text(dep, "id")) == target_key && text(dep, "kind") == kind))
            {
                dependencies.push(json!({"id":target,"kind":kind}));
            }
        }
        index.insert(key(&metadata.id),json!({"id":metadata.id,"title":metadata.title,"type":metadata.item_type,"status":metadata.status,"dependencies":dependencies}));
    }
    Ok((index, json!(aliases)))
}

/// Counts all unique reachable identities and directed edges with bounded expansion.
fn counts(id: &str, index: &Index, depth: usize) -> Value {
    let mut nodes = BTreeSet::new();
    let mut edges = BTreeSet::new();
    let mut missing = BTreeSet::new();
    let mut expanded = BTreeSet::new();
    let mut pending = std::collections::VecDeque::from([(key(id), 0)]);
    while let Some((id, level)) = pending.pop_front() {
        nodes.insert(id.clone());
        let Some(item) = index.get(&id) else {
            missing.insert(id);
            continue;
        };
        if level >= depth || !expanded.insert(id.clone()) {
            continue;
        }
        for dep in item["dependencies"].as_array().into_iter().flatten() {
            let target = key(text(dep, "id"));
            edges.insert((id.clone(), target.clone(), text(dep, "kind").to_owned()));
            pending.push_back((target, level + 1));
        }
    }
    json!({"node_count":nodes.len(),"edge_count":edges.len(),"edge_basis":"deduplicated_directed","missing_count":missing.len()})
}

/// Expands one canonical tree node with cycle and resource guards.
fn node(
    id: &str,
    via: Option<&str>,
    index: &Index,
    lineage: &BTreeSet<String>,
    expanded: &mut BTreeSet<String>,
    budget: &mut Budget,
    depth: usize,
) -> Value {
    let lookup = key(id);
    let item = index.get(&lookup);
    let mut node = json!({"id":item.map_or(id,|item|text(item,"id"))});
    if let Some(item) = item {
        for name in ["title", "type", "status"] {
            node[name] = item[name].clone();
        }
    }
    if let Some(via) = via {
        node["via"] = json!(via);
    }
    node["missing"] = json!(item.is_none());
    node["cycle"] = json!(lineage.contains(&lookup));
    budget.used_nodes += 1;
    budget.used_tokens += crate::stringify_json(&node, false).len().div_ceil(4);
    node["dependencies"] = json!([]);
    let Some(item) = item else {
        return node;
    };
    let dependencies = item["dependencies"].as_array().cloned().unwrap_or_default();
    if budget.used_tokens >= budget.tokens && !dependencies.is_empty() {
        node["truncated"] = json!(true);
        budget.reasons.insert("token_budget");
        return node;
    }
    if lineage.contains(&lookup) {
        return node;
    }
    if depth >= budget.depth {
        if !dependencies.is_empty() {
            node["truncated"] = json!(true);
            budget.reasons.insert("max_depth");
        }
        return node;
    }
    if budget.collapse && !expanded.insert(lookup.clone()) {
        node["collapsed"] = json!(true);
        return node;
    }
    let mut lineage = lineage.clone();
    lineage.insert(lookup);
    let mut children = Vec::new();
    for dep in &dependencies {
        let cost = crate::stringify_json(
            &json!({"from":node["id"],"to":dep["id"],"kind":dep["kind"]}),
            false,
        )
        .encode_utf16()
        .count()
        .div_ceil(3)
        .max(1);
        let reason = if budget.used_nodes >= budget.nodes {
            Some("node_limit")
        } else if budget.used_edges >= budget.edges {
            Some("edge_limit")
        } else if budget.used_tokens + cost >= budget.tokens {
            Some("token_budget")
        } else {
            None
        };
        if let Some(reason) = reason {
            node["truncated"] = json!(true);
            budget.reasons.insert(reason);
            break;
        }
        budget.used_edges += 1;
        budget.used_tokens += cost;
        children.push(self::node(
            text(dep, "id"),
            Some(text(dep, "kind")),
            index,
            &lineage,
            expanded,
            budget,
            depth + 1,
        ));
    }
    node["dependencies"] = json!(children);
    node
}

/// Converts the rendered tree to its deduplicated graph projection.
fn graph(tree: &Value) -> Value {
    let mut nodes = BTreeMap::new();
    let mut edges = BTreeMap::new();
    let mut pending = vec![tree];
    while let Some(node) = pending.pop() {
        let id = text(node, "id");
        let mut projected = json!({"id":id});
        for name in ["title", "type", "status", "missing"] {
            if let Some(value) = node.get(name) {
                projected[name] = value.clone();
            }
        }
        nodes.insert(id.to_owned(), projected);
        for child in node["dependencies"].as_array().into_iter().flatten() {
            edges.insert(
                (
                    id.to_owned(),
                    text(child, "id").to_owned(),
                    text(child, "via").to_owned(),
                ),
                json!({"from":id,"to":child["id"],"kind":child["via"]}),
            );
            pending.push(child);
        }
    }
    let mut nodes: Vec<_> = nodes.into_values().collect();
    nodes.sort_by(|a, b| crate::collation::compare(text(a, "id"), text(b, "id")));
    let mut edges: Vec<_> = edges.into_values().collect();
    edges.sort_by(|a, b| {
        ["from", "to", "kind"]
            .iter()
            .map(|name| crate::collation::compare(text(a, name), text(b, name)))
            .find(|order| !order.is_eq())
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let missing: Vec<_> = nodes
        .iter()
        .filter(|node| node["missing"] == true)
        .map(|node| node["id"].clone())
        .collect();
    json!({"root_id":tree["id"],"nodes":nodes,"edges":edges,"missing_ids":missing})
}

/// Adds projection declarations and their corresponding omission receipt.
fn project(mut result: Value, summary: bool, aliases: &Value) -> Value {
    let group = if result["format"] == "tree" {
        "dependency_tree"
    } else if result["format"] == "graph" {
        "dependency_graph"
    } else {
        "relationship_context"
    };
    result["legacy_alias_counts"] = aliases.clone();
    result["projection"] = json!({"mode":if summary{"summary"}else{"full"},"declared_field_groups":[{"name":group,"restore_with":"--full"}],"included_field_groups":if summary{vec![]}else{vec![group]}});
    result["omission_receipt"] = if summary {
        json!({"has_omissions":true,"omitted_field_group_count":1,"omitted_field_groups":[{"name":group,"restore_with":"--full"}]})
    } else {
        complete_receipt()
    };
    result
}

/// Measures the complete output including the internal row-contract declaration.
fn tokens(result: &Value, toon: bool) -> usize {
    let mut measured = result.clone();
    if result["format"] == "context" {
        // The oracle settles packet cost before the caller attaches legacy alias counts.
        measured["legacy_alias_counts"] = json!({});
    }
    let omission = measured
        .as_object_mut()
        .and_then(|map| map.shift_remove("omission_receipt"))
        .unwrap_or_default();
    let keys = if result["format"] == "graph" {
        vec!["graph.nodes", "graph.edges"]
    } else if result["format"] == "context" {
        vec!["context.nodes", "context.edges"]
    } else {
        vec![]
    };
    let mut contract = json!({"command":"deps","row_kind":if keys.is_empty(){"none"}else{"collection"},"row_keys":keys,"fields":"unsupported"});
    if !keys.is_empty() {
        contract["jq_selector"] = json!(
            ".row_contract.row_keys[] as $key | getpath($key | split(\".\")) | if type == \"array\" then .[] else if type == \"object\" then to_entries[] else empty end end"
        );
    }
    contract["toon_encoding"] = json!("tabular_when_uniform");
    measured["row_contract"] = contract;
    measured["omission_receipt"] = omission;
    let output = ListOutput::from(measured);
    let bytes = if toon {
        crate::toon_output::render_output(&output).len()
    } else {
        crate::stringify_json(&output, true).len() + 1
    };
    bytes.div_ceil(4)
}

/// Removes the deepest last dependency while retaining the parent's truncation marker.
fn trim(tree: &mut Value) -> bool {
    let Some(children) = tree["dependencies"].as_array_mut() else {
        return false;
    };
    if children.is_empty() {
        return false;
    }
    for child in children.iter_mut().rev() {
        if trim(child) {
            return true;
        }
    }
    children.pop();
    tree["truncated"] = json!(true);
    true
}

impl Workspace {
    /// Lists bounded outgoing dependency trees or graphs without modifying the tracker.
    ///
    /// # Errors
    /// Returns selection, storage, numeric-control or unsupported-format errors.
    #[allow(clippy::too_many_lines)]
    pub fn deps(&self, id: &str, options: &DepsOptions) -> Result<ListOutput, PmRustError> {
        let format = options
            .format
            .as_deref()
            .unwrap_or("tree")
            .trim()
            .to_lowercase();
        if !["tree", "graph", "context"].contains(&format.as_str()) {
            return Err(invalid(format!(
                "Invalid --format value \"{}\". Use \"tree\", \"graph\", or \"context\".",
                options.format.as_deref().unwrap_or_default()
            )));
        }
        let depth = options
            .max_depth
            .as_ref()
            .map(|raw| {
                numeric(
                    raw,
                    true,
                    0,
                    format!("Invalid --max-depth value \"{raw}\". Use a non-negative integer."),
                )
            })
            .transpose()?;
        let collapse = options
            .collapse
            .as_deref()
            .unwrap_or("none")
            .trim()
            .to_lowercase();
        if !["none", "repeated"].contains(&collapse.as_str()) {
            return Err(invalid(format!(
                "Invalid --collapse value \"{}\". Use \"none\" or \"repeated\".",
                options.collapse.as_deref().unwrap_or_default()
            )));
        }
        if options.summary && options.full {
            return Err(invalid("--summary cannot be combined with --full"));
        }
        if format == "context" {
            return packet::read(self, id, options, depth);
        }
        let (index, aliases) = index(self)?;
        if !index.contains_key(&key(id)) {
            return Err(crate::collections::refusal(
                format!("Item {id} not found"),
                3,
                json!({"code":"item_not_found","id":id}),
            ));
        }
        if options.summary {
            let counts = counts(id, &index, depth.unwrap_or(usize::MAX));
            let mut result = json!({"id":id,"format":format});
            for (name, value) in counts.as_object().into_iter().flatten() {
                result[name] = value.clone();
            }
            return Ok(project(result, true, &aliases).into());
        }
        let mut limits = Vec::new();
        for (raw, flag, default) in [
            (&options.node_limit, "node-limit", 200),
            (&options.edge_limit, "edge-limit", 400),
            (&options.token_budget, "token-budget", 16000),
        ] {
            limits.push(
                raw.as_ref()
                    .map(|raw| {
                        numeric(
                            raw,
                            true,
                            1,
                            format!("Invalid --{flag} value \"{raw}\". Use a positive integer."),
                        )
                    })
                    .transpose()?
                    .unwrap_or(default),
            );
        }
        let mut budget = Budget {
            nodes: limits[0],
            edges: limits[1],
            tokens: limits[2],
            used_nodes: 0,
            used_edges: 0,
            used_tokens: 0,
            reasons: BTreeSet::new(),
            depth: depth.unwrap_or(32),
            collapse: format == "graph" || collapse == "repeated",
        };
        let mut tree = node(
            id,
            None,
            &index,
            &BTreeSet::new(),
            &mut BTreeSet::new(),
            &mut budget,
            0,
        );
        let totals = counts(id, &index, usize::MAX);
        loop {
            let graph = graph(&tree);
            let mut result = json!({"id":id,"format":format,"node_count":graph["nodes"].as_array().map_or(0,Vec::len),"edge_count":graph["edges"].as_array().map_or(0,Vec::len),"edge_basis":"deduplicated_directed","missing_count":graph["missing_ids"].as_array().map_or(0,Vec::len)});
            if !budget.reasons.is_empty() {
                result["truncation"] = json!({"reasons":budget.reasons,"limits":{"max_depth":budget.depth,"node_limit":budget.nodes,"edge_limit":budget.edges,"token_budget":budget.tokens},"estimated_tokens":0,"total_node_count":totals["node_count"],"total_edge_count":totals["edge_count"],"omitted_node_count":totals["node_count"].as_u64().unwrap_or(0).saturating_sub(result["node_count"].as_u64().unwrap_or(0)),"omitted_edge_count":totals["edge_count"].as_u64().unwrap_or(0).saturating_sub(result["edge_count"].as_u64().unwrap_or(0))});
            }
            result[if format == "tree" { "tree" } else { "graph" }] = if format == "tree" {
                tree.clone()
            } else {
                graph
            };
            let mut result = project(result, false, &aliases);
            let mut cost = tokens(&result, options.toon);
            if result.get("truncation").is_some() {
                loop {
                    result["truncation"]["estimated_tokens"] = json!(cost);
                    let next = tokens(&result, options.toon);
                    if next == cost {
                        break;
                    }
                    cost = next;
                }
            }
            if cost <= budget.tokens {
                return Ok(result.into());
            }
            if !trim(&mut tree) {
                return Ok(result.into());
            }
            budget.reasons.insert("token_budget");
        }
    }
}

#[cfg(test)]
#[path = "../tests/support/orientation_deps_unit.rs"]
mod tests;
