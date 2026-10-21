//! Exact bounded relationship packets over authoritative workspace metadata.
use super::{key, project, text, tokens};
use crate::{
    DepsOptions, ListOutput, PmRustError, Workspace,
    orientation::{invalid, numeric},
};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

/// Packet roles in the published precedence order.
const ROLES: [&str; 6] = [
    "prerequisite",
    "dependent",
    "ancestor",
    "descendant",
    "provenance",
    "related",
];

/// Canonicalizes built-in relationship kinds and their legacy aliases.
fn kind(raw: &str) -> Option<&'static str> {
    match raw.trim().to_lowercase().replace('-', "_").as_str() {
        "blocked_by" | "depends_on" => Some("blocked_by"),
        "blocks" => Some("blocks"),
        "parent" | "child_of" | "epic" => Some("parent"),
        "child" | "parent_child" | "task" => Some("child"),
        "related" | "related_to" => Some("related"),
        "discovered_from" => Some("discovered_from"),
        "incident_from" => Some("incident_from"),
        "implements" => Some("implements"),
        "recurs_from" => Some("recurs_from"),
        "verifies" => Some("verifies"),
        "supersedes" => Some("supersedes"),
        _ => None,
    }
}

/// Returns the inverse registered built-in kind where one exists.
fn inverse(name: &str) -> &str {
    match name {
        "parent" => "child",
        "child" => "parent",
        "blocks" => "blocked_by",
        "blocked_by" => "blocks",
        _ => name,
    }
}

/// Resolves the semantic role of an edge endpoint.
fn role(edge: &Value, node: &str) -> &'static str {
    let source = text(edge, "source") == node;
    match text(edge, "kind") {
        "blocked_by" => {
            if source {
                "dependent"
            } else {
                "prerequisite"
            }
        }
        "blocks" => {
            if source {
                "prerequisite"
            } else {
                "dependent"
            }
        }
        "parent" => {
            if source {
                "descendant"
            } else {
                "ancestor"
            }
        }
        "child" => {
            if source {
                "ancestor"
            } else {
                "descendant"
            }
        }
        "discovered_from" | "incident_from" => "provenance",
        _ => "related",
    }
}

/// Detects external references without resolving or contacting them.
fn external(id: &str, source: &str) -> bool {
    matches!(source.trim().to_lowercase().as_str(), "global" | "external")
        || id.contains("://")
        || ["github:", "gitlab:", "jira:", "linear:"]
            .iter()
            .any(|prefix| id.to_lowercase().starts_with(prefix))
}

/// Ignores absent-target markers and the historical no-active-blocker sentinel.
fn target(id: &str) -> Option<&str> {
    let id = id.trim();
    (!matches!(
        id.to_lowercase().as_str(),
        "" | "none" | "null" | "n/a" | "na" | "no-active-blocker"
    ))
    .then_some(id)
}

/// Orders edges by source, target and kind using the published collator.
fn compare(a: &Value, b: &Value) -> std::cmp::Ordering {
    ["source", "target", "kind"]
        .iter()
        .map(|key| crate::collation::compare(text(a, key), text(b, key)))
        .find(|order| !order.is_eq())
        .unwrap_or(std::cmp::Ordering::Equal)
}

/// Allocates external keys after reserving every local and dangling identity.
fn external_keys(
    canonical: &BTreeMap<String, String>,
    details: &mut BTreeMap<String, Value>,
    external_ids: BTreeSet<String>,
) -> BTreeMap<String, String> {
    let mut external_ids: Vec<_> = external_ids.into_iter().collect();
    external_ids.sort_by(|a, b| crate::collation::compare(a, b));
    let mut used: BTreeSet<_> = canonical.keys().cloned().collect();
    let mut external_keys = BTreeMap::new();
    for id in external_ids {
        let mut graph_id = id.clone();
        while used.contains(&key(&graph_id)) {
            graph_id = format!("external:{graph_id}");
        }
        used.insert(key(&graph_id));
        details.insert(
            graph_id.clone(),
            json!({"id":graph_id,"title":format!("[external] {id}"),"status":"external"}),
        );
        external_keys.insert(id, graph_id);
    }
    external_keys
}

/// Orders active missing-reference evidence before terminal holders with stable tie breakers.
fn compare_missing(a: &Value, b: &Value) -> std::cmp::Ordering {
    a["legacy_terminal"]
        .as_bool()
        .cmp(&b["legacy_terminal"].as_bool())
        .then_with(|| {
            ["holder_id", "target_id", "kind", "source"]
                .iter()
                .map(|key| crate::collation::compare(text(a, key), text(b, key)))
                .find(|order| !order.is_eq())
                .unwrap_or(std::cmp::Ordering::Equal)
        })
}

/// Indexes incident edge positions once, counting a self-loop once for its endpoint.
fn index_edges(edges: &[Value]) -> BTreeMap<String, Vec<usize>> {
    let mut adjacency: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (index, edge) in edges.iter().enumerate() {
        let source = text(edge, "source");
        let target = text(edge, "target");
        adjacency.entry(source.to_owned()).or_default().push(index);
        if target != source {
            adjacency.entry(target.to_owned()).or_default().push(index);
        }
    }
    adjacency
}

/// Immutable graph and dangling-reference evidence captured for one read.
struct Graph {
    /// Compact node details including missing and external nodes.
    details: BTreeMap<String, Value>,
    /// Sorted deduplicated directed edges.
    edges: Vec<Value>,
    /// Edge positions indexed once per endpoint, including self-loops only once.
    adjacency: BTreeMap<String, Vec<usize>>,
    /// Missing-reference storage rows in active then terminal order.
    missing: Vec<Value>,
    /// Legacy kind occurrence counts.
    aliases: Value,
}

impl Graph {
    /// Assembles the scalar and explicit relationships without reading history.
    fn new(corpus: &[Value], lifecycle: &crate::list_registry::Lifecycle) -> Self {
        let mut canonical: BTreeMap<String, String> = corpus
            .iter()
            .map(|item| (key(text(item, "id")), text(item, "id").trim().to_owned()))
            .collect();
        let known_ids: BTreeSet<_> = canonical.keys().cloned().collect();
        let mut details: BTreeMap<String,Value> = corpus.iter().map(|item| (text(item,"id").trim().to_owned(),json!({"id":text(item,"id").trim(),"title":item["title"],"status":item["status"],"type":text(item,"type").trim()}))).collect();
        let mut raw_edges = Vec::new();
        let mut external_ids = BTreeSet::new();
        let mut missing = Vec::new();
        let mut aliases: BTreeMap<String, usize> = BTreeMap::new();
        for item in corpus {
            let source = text(item, "id").trim();
            let mut references = vec![
                (text(item, "parent"), "parent", "parent", ""),
                (text(item, "blocked_by"), "blocked_by", "blocked_by", ""),
            ];
            for dep in item["dependencies"].as_array().into_iter().flatten() {
                references.push((
                    text(dep, "id"),
                    text(dep, "kind"),
                    "dependency",
                    text(dep, "source_kind"),
                ));
                if let Some(name) = kind(text(dep, "kind")) {
                    let normalized = text(dep, "kind").trim().to_lowercase().replace('-', "_");
                    if normalized != name {
                        *aliases.entry(normalized).or_default() += 1;
                    }
                }
            }
            for (id, name, storage, source_kind) in references {
                let Some(id) = target(id) else { continue };
                let is_external = storage != "parent" && external(id, source_kind);
                let normalized = key(id);
                let known = canonical.contains_key(&normalized);
                let graph_id = if is_external {
                    id.to_owned()
                } else {
                    canonical
                        .get(&normalized)
                        .map_or_else(|| id.to_owned(), Clone::clone)
                };
                if is_external {
                    external_ids.insert(id.to_owned());
                } else if !known {
                    canonical.insert(normalized, graph_id.clone());
                    details.insert(graph_id.clone(),json!({"id":graph_id,"title":format!("[missing] {graph_id}"),"status":"missing"}));
                }
                if !is_external && !known_ids.contains(&key(id)) {
                    let row = json!({"holder_id":source,"target_id":id,"kind":name,"source":storage,"holder_status":item["status"],"legacy_terminal":lifecycle.terminal(text(item,"status")),"no_active_blocker_sentinel":false});
                    if !missing.contains(&row) {
                        missing.push(row);
                    }
                }
                if let Some(name) = kind(name) {
                    raw_edges.push((
                        is_external,
                        json!({"source":source,"target":graph_id,"kind":name}),
                    ));
                }
            }
        }
        let external_keys = external_keys(&canonical, &mut details, external_ids);
        // Keep the captured namespace; plain external IDs cannot be inferred from spelling.
        let mut deduped = BTreeMap::new();
        for (is_external, mut edge) in raw_edges {
            edge["target"] = if is_external {
                json!(external_keys[text(&edge, "target")])
            } else {
                json!(canonical[&key(text(&edge, "target"))])
            };
            let mut endpoints = [
                text(&edge, "source").to_owned(),
                text(&edge, "target").to_owned(),
            ];
            if edge["kind"] == "related" {
                endpoints.sort();
            }
            let identity = (text(&edge, "kind").to_owned(), endpoints);
            deduped.insert(identity, edge);
        }
        let mut edges: Vec<_> = deduped.into_values().collect();
        edges.sort_by(compare);
        missing.sort_by(compare_missing);
        Self {
            adjacency: index_edges(&edges),
            details,
            edges,
            missing,
            aliases: json!(aliases),
        }
    }

    /// Returns stable neighboring edge rows under direction and kind constraints.
    fn neighbors(&self, id: &str, query: &Query) -> Vec<(&str, &Value)> {
        let mut rows: Vec<_> = self
            .adjacency
            .get(id)
            .into_iter()
            .flatten()
            .map(|index| &self.edges[*index])
            .filter(|edge| {
                let outgoing = text(edge, "source") == id;
                let incoming = text(edge, "target") == id;
                let undirected = edge["kind"] == "related";
                ((outgoing && (query.direction != "incoming" || undirected))
                    || (incoming && (query.direction != "outgoing" || undirected)))
                    && query.matches(text(edge, "kind"))
            })
            .map(|edge| {
                (
                    if text(edge, "source") == id {
                        text(edge, "target")
                    } else {
                        text(edge, "source")
                    },
                    edge,
                )
            })
            .collect();
        rows.sort_by(|a, b| crate::collation::compare(a.0, b.0).then_with(|| compare(a.1, b.1)));
        rows
    }
}

/// Validated packet traversal and delivery bounds.
#[derive(Clone)]
struct Query {
    /// Traversal direction relative to each node.
    direction: String,
    /// Canonical kind filter, empty for all kinds.
    kinds: BTreeSet<String>,
    /// Maximum depth from root.
    depth: usize,
    /// Returned node bound.
    nodes: usize,
    /// Returned edge bound.
    edges: usize,
    /// Requested rendered token budget.
    tokens: usize,
}

impl Query {
    /// Applies inverse semantics to relationship kind filtering.
    fn matches(&self, name: &str) -> bool {
        self.kinds.is_empty() || self.kinds.contains(name) || self.kinds.contains(inverse(name))
    }

    /// Validates command controls with the oracle's usage diagnostics.
    fn new(options: &DepsOptions, depth: Option<usize>) -> Result<Self, PmRustError> {
        let mut limits = Vec::new();
        for (raw, name, default) in [
            (&options.node_limit, "node-limit", 20),
            (&options.edge_limit, "edge-limit", 40),
            (&options.token_budget, "token-budget", 1200),
        ] {
            limits.push(
                raw.as_ref()
                    .map(|raw| {
                        numeric(
                            raw,
                            true,
                            1,
                            format!("Invalid --{name} value \"{raw}\". Use a positive integer."),
                        )
                    })
                    .transpose()?
                    .unwrap_or(default),
            );
        }
        let direction = options
            .direction
            .as_deref()
            .unwrap_or("both")
            .trim()
            .to_lowercase();
        if !["outgoing", "incoming", "both"].contains(&direction.as_str()) {
            return Err(invalid(format!(
                "Invalid --direction value \"{}\". Use \"outgoing\", \"incoming\", or \"both\".",
                options.direction.as_deref().unwrap_or_default()
            )));
        }
        let mut kinds = BTreeSet::new();
        for raw in options
            .kind
            .iter()
            .flat_map(|raw| raw.split(','))
            .map(str::trim)
            .filter(|raw| !raw.is_empty())
        {
            let Some(name) = kind(raw) else {
                return Err(invalid(format!(
                    "Invalid --kind value \"{raw}\". Registered kinds: blocked_by, blocks, child, discovered_from, implements, incident_from, parent, recurs_from, related, supersedes, verifies."
                )));
            };
            kinds.insert(name.to_owned());
        }
        Ok(Self {
            direction,
            kinds,
            depth: depth.unwrap_or(3),
            nodes: limits[0],
            edges: limits[1],
            tokens: limits[2],
        })
    }
}

/// Estimates compact row tokens using UTF-8 JSON bytes.
fn cost(value: &Value) -> usize {
    crate::stringify_json(value, false).len().div_ceil(4).max(1)
}

/// Promotes bounded root evidence from the authoritative document.
fn evidence(root: &Value) -> Vec<String> {
    let counts: Vec<_> = ["files", "tests", "docs", "comments", "notes", "learnings"]
        .iter()
        .map(|name| (*name, root[name].as_array().map_or(0, Vec::len)))
        .collect();
    if counts.iter().all(|(_, count)| *count == 0) {
        return Vec::new();
    }
    let mut evidence = vec![format!(
        "linked:{}",
        counts
            .iter()
            .map(|(name, count)| format!("{name}={count}"))
            .collect::<Vec<_>>()
            .join(",")
    )];
    for (name, label, limit) in [
        ("files", "file", 3),
        ("tests", "test", 2),
        ("docs", "doc", 2),
    ] {
        for row in root[name].as_array().into_iter().flatten().take(limit) {
            let value = if name == "tests" {
                row["command"].as_str().unwrap_or_else(|| text(row, "path"))
            } else {
                text(row, "path")
            }
            .trim();
            if !value.is_empty() {
                evidence.push(format!("{label}:{value}"));
            }
        }
    }
    evidence
}

/// Query discovery shared by every rendered-budget retry over one immutable graph.
struct Traversal<'a> {
    /// Missing-reference rows selected by the same reachability and kind controls as the packet.
    missing: Vec<&'a Value>,
    /// Stable breadth-first rows with their first incoming reason.
    discovered: Vec<(&'a str, usize, &'a str, &'static str)>,
    /// Incident root edges reused for roles and direct counts.
    direct: Vec<(&'a str, &'a Value)>,
    /// Direct edge counts in published role order.
    direct_counts: Value,
    /// Cursor contract digest, computed once per query.
    fingerprint: String,
    /// Number of visited nodes before delivery bounds.
    visited: usize,
    /// Number of inspected incident edges before delivery bounds.
    inspected: usize,
    /// Whether the requested depth omitted reachable nodes.
    truncated: bool,
}

impl<'a> Traversal<'a> {
    /// Discovers and orders the complete query once, independently of rendered budget.
    fn new(graph: &'a Graph, root: &'a str, query: &Query) -> Self {
        let mut seen = BTreeSet::from([root]);
        let mut queue = vec![(root, 0)];
        let mut discovered = Vec::new();
        let mut inspected = 0;
        let mut truncated = false;
        let mut index = 0;
        while index < queue.len() {
            let (id, distance) = queue[index];
            index += 1;
            if distance >= query.depth && truncated {
                continue;
            }
            let adjacent = graph.neighbors(id, query);
            inspected += adjacent.len();
            if distance >= query.depth {
                truncated |= adjacent.iter().any(|(id, _)| !seen.contains(id));
                continue;
            }
            for (next, edge) in adjacent {
                if seen.insert(next) {
                    discovered.push((next, distance + 1, id, role(edge, next)));
                    queue.push((next, distance + 1));
                }
            }
        }
        discovered.sort_by(|a, b| {
            a.1.cmp(&b.1)
                .then_with(|| crate::collation::compare(a.0, b.0))
        });
        let direct = graph.neighbors(root, query);
        let mut direct_counts = json!({"prerequisite":0,"dependent":0,"ancestor":0,"descendant":0,"provenance":0,"related":0});
        for (id, edge) in &direct {
            let name = role(edge, id);
            direct_counts[name] = json!(direct_counts[name].as_u64().unwrap_or_default() + 1);
        }
        let contract = json!({"rootId":root,"direction":query.direction,"kinds":query.kinds,"maxDepth":query.depth});
        let fingerprint = crate::history::sha256_digest(
            format!(
                "relationship-context\0{}",
                crate::pagination::stable(&contract)
            )
            .as_bytes(),
        );
        let missing = graph
            .missing
            .iter()
            .filter(|row| {
                seen.contains(text(row, "target_id"))
                    && seen.contains(text(row, "holder_id"))
                    && query.matches(kind(text(row, "kind")).unwrap_or_default())
            })
            .collect();
        Self {
            missing,
            discovered,
            direct,
            direct_counts,
            fingerprint,
            visited: queue.len(),
            inspected,
            truncated,
        }
    }
}

/// Builds an exact packet and its producer continuation before rendered-budget enforcement.
#[allow(clippy::too_many_lines)]
fn packet(
    graph: &Graph,
    root: &str,
    query: &Query,
    traversal: &Traversal<'_>,
    evidence: &[String],
    cursor: Option<&str>,
    budget: usize,
) -> Result<Value, PmRustError> {
    let discovered = &traversal.discovered;
    let direct = &traversal.direct;
    let mut truncated = traversal.truncated;
    let fingerprint = &traversal.fingerprint[..24];
    let start = if let Some(cursor) = cursor {
        let cursor = crate::pagination::query(cursor, fingerprint)?;
        if let Some(index) = discovered
            .iter()
            .position(|row| row.0 == text(&cursor, "after_id"))
        {
            index + 1
        } else if let Some(index) = cursor["after_index"].as_u64() {
            usize::try_from(index)
                .unwrap_or(usize::MAX)
                .min(discovered.len())
        } else {
            return Err(crate::pagination::refusal(
                "invalid_query_cursor",
                format!(
                    "Query cursor item {} is no longer present in this result set.",
                    text(&cursor, "after_id")
                ),
            ));
        }
    } else {
        0
    };
    let root_details = &graph.details[root];
    let mut used = cost(root_details) + cost(&json!(evidence));
    if used > budget {
        return Err(crate::collections::refusal(
            "Relationship context tokenBudget cannot fit root and evidence",
            1,
            json!({"code":"unknown_error"}),
        ));
    }
    let mut nodes = Vec::new();
    for (id, distance, via, initial_role) in discovered.iter().skip(start).take(query.nodes) {
        let reasons: BTreeSet<_> = direct
            .iter()
            .filter(|(next, _)| next == id)
            .map(|(next, edge)| role(edge, next))
            .collect();
        let role = if *distance == 1 {
            ROLES
                .iter()
                .copied()
                .find(|role| reasons.contains(role))
                .unwrap_or(initial_role)
        } else {
            initial_role
        };
        let reasons: Vec<_> = if *distance == 1 {
            reasons.into_iter().map(str::to_owned).collect()
        } else {
            vec![format!("{role} via {via} (depth {distance})")]
        };
        let mut node = graph.details[*id].clone();
        node["distance"] = json!(distance);
        node["role"] = json!(role);
        node["via"] = json!(via);
        node["reasons"] = json!(reasons);
        let tokens = cost(&node);
        if used + tokens > budget {
            break;
        }
        nodes.push(node);
        used += tokens;
    }
    if start < discovered.len() && nodes.is_empty() {
        return Err(crate::collections::refusal(
            "Relationship context tokenBudget cannot fit one node",
            1,
            json!({"code":"unknown_error"}),
        ));
    }
    let included: BTreeSet<_> = std::iter::once(root)
        .chain(nodes.iter().map(|node| text(node, "id")))
        .collect();
    let incident: BTreeSet<_> = included
        .iter()
        .filter_map(|id| graph.adjacency.get(*id))
        .flatten()
        .copied()
        .collect();
    let candidates: Vec<_> = incident
        .into_iter()
        .map(|index| &graph.edges[index])
        .filter(|edge| {
            included.contains(text(edge, "source"))
                && included.contains(text(edge, "target"))
                && query.matches(text(edge, "kind"))
        })
        .collect();
    let mut edges = Vec::new();
    for edge in candidates.iter().take(query.edges) {
        let tokens = cost(edge);
        if used + tokens > budget {
            break;
        }
        edges.push(*edge);
        used += tokens;
    }
    let consumed = start + nodes.len();
    let has_more = consumed < discovered.len();
    let omitted_edges = candidates.len() - edges.len();
    truncated |= has_more || omitted_edges > 0;
    let mut meta = json!({"exact":true,"truncated":truncated,"completeness":if truncated {"truncated"} else {"complete"},"nodeLimit":query.nodes,"edgeLimit":query.edges,"tokenBudget":budget,"usedTokens":used,"visitedNodes":traversal.visited,"inspectedEdges":traversal.inspected,"omittedNodes":discovered.len()-consumed,"omittedEdges":omitted_edges});
    if has_more {
        // A nonempty remaining page either delivers a node or returns the budget-floor error above.
        meta["nextCursor"] = json!(crate::pagination::after(
            fingerprint,
            &nodes[nodes.len() - 1],
            consumed - 1
        ));
    }
    let missing = &traversal.missing;
    let missing_ids: BTreeSet<_> = missing
        .iter()
        .map(|row| key(text(row, "target_id")))
        .collect();
    Ok(
        json!({"root":root_details,"summary":{"rootId":root,"rootStatus":root_details["status"],"directEdges":traversal.direct_counts,"directTotal":direct.len(),"discoveredNodes":discovered.len(),"returnedNodes":nodes.len(),"returnedEdges":edges.len(),"omittedNodes":discovered.len()-consumed,"omittedEdges":omitted_edges,"evidenceCount":evidence.len(),"hasMore":has_more},"nodes":nodes,"edges":edges,"evidence":evidence,"meta":meta,"missing_count":missing_ids.len(),"missing_references":missing}),
    )
}

/// Mirrors the oracle's IEEE scaling, flooring and clamp for its next rendered-budget attempt.
fn next_budget(current: usize, requested: usize, measured: usize) -> usize {
    let number = |value| json!(value).as_f64().unwrap_or_default();
    let scaled = number(current) * number(requested) / number(measured);
    // The positive query budget bounds this finite, nonnegative integer below the current safe integer.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let next = number(current.saturating_sub(1)).min((scaled * 0.9).floor().max(1.0)) as usize;
    next
}

/// Serves a native dependency packet and stabilizes the complete rendered token count.
pub(super) fn read(
    workspace: &Workspace,
    id: &str,
    options: &DepsOptions,
    depth: Option<usize>,
) -> Result<ListOutput, PmRustError> {
    let corpus: Vec<_> = workspace
        .stream_items(crate::read_projection::Projection::List)?
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .map(|item| serde_json::to_value(item.metadata).unwrap_or_default())
        .collect();
    let root = corpus
        .iter()
        .find(|item| key(text(item, "id")) == key(id))
        .ok_or_else(|| {
            crate::collections::refusal(
                format!("Item {id} not found"),
                3,
                json!({"code":"item_not_found","id":id}),
            )
        })?;
    let id = text(root, "id");
    let query = Query::new(options, depth)?;
    let graph = Graph::new(
        &corpus,
        &crate::list_registry::Lifecycle::load(workspace.pm_root())?,
    );
    let evidence = if options.summary {
        vec![]
    } else {
        evidence(&serde_json::to_value(workspace.get(id)?.metadata).unwrap_or_default())
    };
    let traversal = Traversal::new(&graph, id, &query);
    match render_packet(&graph, id, &query, &traversal, &evidence, options) {
        Err(error) if budget_floor(&error) => Err(budget_refusal(
            workspace, id, &graph, &query, &traversal, &evidence, options,
        )),
        Err(PmRustError::ReadCursor { code, detail }) => {
            let mut fresh = options.clone();
            fresh.cursor = None;
            let args = retry_args(workspace, id, &query, &fresh, query.tokens);
            Err(crate::collections::refusal(
                detail,
                2,
                json!({"code":code,"recovery":{"suggested_retry":command(&args),"suggested_retry_args":args}}),
            ))
        }
        result => result.and_then(|result| disclose(result, options)),
    }
}

/// Distinguishes native packet floor errors from malformed queries and storage failures.
fn budget_floor(error: &PmRustError) -> bool {
    matches!(error, PmRustError::CommandRefusal { detail, .. }
        if detail.starts_with("Relationship context tokenBudget cannot fit "))
}

/// Enforces the complete rendered ceiling without returning an earlier oversized packet.
fn render_packet(
    graph: &Graph,
    id: &str,
    query: &Query,
    traversal: &Traversal<'_>,
    evidence: &[String],
    options: &DepsOptions,
) -> Result<ListOutput, PmRustError> {
    let mut budget = query.tokens;
    // Larger limits render the same rows; skip retries that cannot remove a reference.
    let mut reference_limit = query.edges.min(traversal.missing.len());
    loop {
        let mut packet = packet(
            graph,
            id,
            query,
            traversal,
            evidence,
            options
                .cursor
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty()),
            budget,
        )?;
        let references = packet
            .as_object_mut()
            .and_then(|packet| packet.shift_remove("missing_references"))
            .unwrap_or_default();
        let missing_count = packet
            .as_object_mut()
            .and_then(|packet| packet.shift_remove("missing_count"))
            .unwrap_or_default();
        let mut result = json!({"id":id,"format":"context","node_count":packet["nodes"].as_array().map_or(1,|rows|rows.len()+1),"edge_count":packet["edges"].as_array().map_or(0,Vec::len),"edge_basis":"deduplicated_directed","missing_count":missing_count,"missing_scope":"traversal","missing_reference_count":references.as_array().map_or(0,Vec::len)});
        if !options.summary {
            result["missing_references"] = json!(
                references
                    .as_array()
                    .into_iter()
                    .flatten()
                    .take(reference_limit)
                    .collect::<Vec<_>>()
            );
            packet["meta"]["tokenBudget"] = json!(query.tokens);
            result["context"] = packet;
        }
        let mut result = project(result, options.summary, &graph.aliases);
        if options.summary {
            if tokens(&result, options.toon) > query.tokens {
                return Err(crate::collections::refusal(
                    "Relationship context tokenBudget cannot fit the rendered summary",
                    1,
                    json!({"code":"unknown_error"}),
                ));
            }
            return Ok(result.into());
        }
        loop {
            let measured = tokens(&result, options.toon);
            if result["context"]["meta"]["usedTokens"] == measured {
                break;
            }
            result["context"]["meta"]["usedTokens"] = json!(measured);
        }
        let measured = result["context"]["meta"]["usedTokens"]
            .as_u64()
            .and_then(|n| usize::try_from(n).ok())
            .unwrap_or_default();
        if measured <= query.tokens {
            return Ok(result.into());
        }
        if reference_limit > 0 {
            reference_limit -= 1;
        } else {
            budget = next_budget(budget, query.tokens, measured);
        }
    }
}

/// Classifies a retry after the preserved query has already passed argument preflight.
fn probe_delivery(result: Result<ListOutput, PmRustError>) -> super::budget_recovery::Probe {
    use super::budget_recovery::Probe;
    match result {
        Ok(result) if result.get("output_budget_exceeded").is_some() => Probe::OutputConstraints,
        Ok(_) => Probe::Fits,
        Err(_) => Probe::Floor,
    }
}

/// Applies the same default output ceiling used by a canonical retry.
fn disclose(result: ListOutput, options: &DepsOptions) -> Result<ListOutput, PmRustError> {
    crate::read_output::apply_orientation(
        result,
        &crate::ListOptions {
            toon: options.toon,
            full: options.full,
            ..Default::default()
        },
        "deps",
        &["context.nodes", "context.edges"],
    )
}

/// Renders an argument vector using the public command quoting contract.
fn command(args: &[String]) -> String {
    format!(
        "pm {}",
        args.iter()
            .map(|arg| crate::quote_command_arg(arg))
            .collect::<Vec<_>>()
            .join(" ")
    )
}

/// Preserves validated query controls in a replayable canonical command.
fn retry_args(
    workspace: &Workspace,
    id: &str,
    query: &Query,
    options: &DepsOptions,
    budget: usize,
) -> Vec<String> {
    let mut args = vec![
        "--pm-path".to_owned(),
        workspace.query_pm_root().to_string_lossy().into_owned(),
        "item".to_owned(),
        "deps".to_owned(),
        id.to_owned(),
        "--format".to_owned(),
        "context".to_owned(),
        "--output-format".to_owned(),
        if options.toon { "toon" } else { "json" }.to_owned(),
        "--token-budget".to_owned(),
        budget.to_string(),
    ];
    for (flag, value) in [
        (
            "--max-depth",
            options.max_depth.as_ref().map(|_| query.depth.to_string()),
        ),
        (
            "--node-limit",
            options.node_limit.as_ref().map(|_| query.nodes.to_string()),
        ),
        (
            "--edge-limit",
            options.edge_limit.as_ref().map(|_| query.edges.to_string()),
        ),
        ("--direction", Some(query.direction.clone())),
        (
            "--cursor",
            options
                .cursor
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_owned),
        ),
    ] {
        if let Some(value) = value {
            args.extend([flag.to_owned(), value]);
        }
    }
    for kind in &query.kinds {
        args.extend(["--kind".to_owned(), kind.clone()]);
    }
    for (flag, enabled) in [
        ("--summary", options.summary),
        ("--full", options.full),
        ("--json", !options.toon),
    ] {
        if enabled {
            args.push(flag.to_owned());
        }
    }
    args
}

/// Verifies candidate budgets against the preserved query before advertising recovery.
#[allow(clippy::too_many_arguments)]
fn budget_refusal(
    workspace: &Workspace,
    id: &str,
    graph: &Graph,
    query: &Query,
    traversal: &Traversal<'_>,
    evidence: &[String],
    options: &DepsOptions,
) -> PmRustError {
    use super::budget_recovery;
    let text_units = graph
        .details
        .values()
        .map(|node| {
            ["id", "title", "status", "type"]
                .iter()
                .map(|field| text(node, field).encode_utf16().count())
                .sum::<usize>()
        })
        .sum::<usize>();
    let work = (if options.summary {
        1
    } else {
        1 + query.edges.min(traversal.missing.len())
    })
    .saturating_mul(
        text_units
            .saturating_add(graph.edges.len())
            .saturating_add(traversal.missing.len())
            .saturating_add(
                evidence
                    .iter()
                    .map(|text| text.encode_utf16().count())
                    .sum::<usize>(),
            ),
    );
    let bounds = budget_recovery::find(query.tokens, work, &mut |budget| {
        let mut candidate = query.clone();
        candidate.tokens = budget;
        probe_delivery(
            render_packet(graph, id, &candidate, traversal, evidence, options)
                .and_then(|result| disclose(result, options)),
        )
    });
    let inspection_args = vec![
        "--pm-path".to_owned(),
        workspace.query_pm_root().to_string_lossy().into_owned(),
        "item".to_owned(),
        "deps".to_owned(),
        id.to_owned(),
        "--format".to_owned(),
        "tree".to_owned(),
        "--summary".to_owned(),
    ];
    let inspection = command(&inspection_args);
    let feasible = bounds["feasible_tokens"]
        .as_u64()
        .and_then(|value| usize::try_from(value).ok());
    let args = feasible.map_or_else(
        || inspection_args.clone(),
        |budget| retry_args(workspace, id, query, options, budget),
    );
    let retry = command(&args);
    let required = feasible.map_or_else(
        || "Inspect the compact summary and review the output constraints.".to_owned(),
        |budget| format!("Retry the preserved context query with --token-budget {budget}."),
    );
    let detail = "Relationship context tokenBudget cannot fit the required rendered packet";
    let context = json!({"code":"invalid_argument_value","required":required,
        "recovery":{"token_budget":bounds,"recovery_mode":"compact","suggested_flags":["--token-budget"],
        "suggested_retry":retry,"suggested_retry_args":args,"fallback_candidates":[{"source":"relationship_summary","command":inspection,"reason":"Inspect bounded counts separately from context rows."}],"next_best_command":inspection},
        "next_steps":[retry],"exit_code":2,"type":"urn:pm-cli:error:invalid_argument_value","title":detail,"detail":detail,
        "examples":[retry,inspection],"refusal":{"surface":"command","rejected_value":query.tokens.to_string(),"exit_code":2},"value":query.tokens.to_string()});
    crate::collections::refusal(detail, 2, context)
}

#[cfg(test)]
#[path = "../../tests/support/orientation_packet_unit.rs"]
mod tests;
