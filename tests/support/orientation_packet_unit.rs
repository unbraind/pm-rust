//! Packet traversal, continuation and evidence acceptance over real tracker documents.
use super::*;
use crate::list_registry::Lifecycle;
use std::fs;

/// Builds a complete metadata row with deterministic timestamps.
fn item(id: &str) -> Value {
    json!({"id":id,"title":id,"type":"Task","status":"open","priority":1,"tags":[],"created_at":"2026-08-22T10:00:00.000Z","updated_at":"2026-08-22T10:00:00.000Z"})
}

/// Builds the kernel's minimal node-detail form independently of tracker storage.
fn kernel_graph(ids: &[&str], mut edges: Vec<Value>) -> Graph {
    edges.sort_by(compare);
    Graph {
        details: ids
            .iter()
            .map(|id| ((*id).to_owned(), json!({"id":id})))
            .collect(),
        adjacency: index_edges(&edges),
        edges,
        missing: Vec::new(),
        aliases: json!({}),
    }
}

/// Required endpoints lead producer pages, and cursors reject the previous packing policy.
#[test]
fn packet_required_root_evidence_orders_pages() -> Result<(), Box<dyn std::error::Error>> {
    let mut graph = kernel_graph(
        &["root", "a-child", "z-blocker"],
        vec![
            json!({"source":"a-child","target":"root","kind":"parent"}),
            json!({"source":"root","target":"z-blocker","kind":"blocked_by"}),
        ],
    );
    graph.details.get_mut("a-child").ok_or("child details")?["title"] =
        json!("padding ".repeat(35));
    let mut query = Query::new(&DepsOptions::default(), None)?;
    let bounded = packet(&graph, "root", &query, &[], None, 130)?;
    assert_eq!(
        bounded["edges"],
        json!([{"source":"root","target":"z-blocker","kind":"blocked_by"}])
    );
    assert!(
        bounded["meta"]["usedTokens"]
            .as_u64()
            .is_some_and(|used| used <= 130)
    );
    query.nodes = 1;
    let first = packet(&graph, "root", &query, &[], None, 1200)?;
    assert_eq!(first["nodes"][0]["id"], "z-blocker");
    let cursor = first["meta"]["nextCursor"].as_str().ok_or("cursor")?;
    let second = packet(&graph, "root", &query, &[], Some(cursor), 1200)?;
    assert_eq!(second["nodes"][0]["id"], "a-child");
    assert!(second["meta"].get("nextCursor").is_none());
    let legacy = json!({"rootId":"root","direction":"both","kinds":[],"maxDepth":3});
    let fingerprint = crate::history::sha256_digest(
        format!(
            "relationship-context\0{}",
            crate::pagination::stable(&legacy)
        )
        .as_bytes(),
    );
    let cursor = crate::pagination::after(&fingerprint[..24], &json!({"id":"a-child"}), 0);
    assert!(matches!(
        packet(&graph, "root", &query, &[], Some(&cursor), 1200),
        Err(PmRustError::ReadCursor {
            code: "invalid_query_cursor",
            ..
        })
    ));
    Ok(())
}

/// An unaffordable edge must not hide a later feasible edge or its omission count.
#[test]
fn packet_skips_oversized_edges() -> Result<(), Box<dyn std::error::Error>> {
    let oversized = "a".repeat(300);
    let graph = kernel_graph(
        &["root", &oversized, "z"],
        vec![
            json!({"source":"root","target":oversized,"kind":"related"}),
            json!({"source":"root","target":"z","kind":"related"}),
        ],
    );
    let result = packet(
        &graph,
        "root",
        &Query::new(&DepsOptions::default(), None)?,
        &[],
        None,
        175,
    )?;
    assert_eq!(result["nodes"].as_array().map(Vec::len), Some(2));
    assert_eq!(
        result["edges"],
        json!([{"source":"root","target":"z","kind":"related"}])
    );
    assert_eq!(result["meta"]["omittedEdges"], 1);
    assert!(
        result["meta"]["usedTokens"]
            .as_u64()
            .is_some_and(|used| used <= 175)
    );
    Ok(())
}

/// Reserve the parent and blocker relationships before optional distant sibling rows.
#[test]
fn packet_reserves_root_edges_before_distant_nodes() -> Result<(), Box<dyn std::error::Error>> {
    let siblings: Vec<_> = (0..10).map(|i| format!("sibling-{i}")).collect();
    let mut ids = vec!["root", "parent", "blocker"];
    ids.extend(siblings.iter().map(String::as_str));
    let mut edges = vec![
        json!({"source":"root","target":"parent","kind":"parent"}),
        json!({"source":"root","target":"blocker","kind":"blocked_by"}),
    ];
    edges.extend(
        siblings
            .iter()
            .map(|id| json!({"source":id,"target":"parent","kind":"parent"})),
    );
    let mut graph = kernel_graph(&ids, edges);
    for (id, row) in &mut graph.details {
        row["title"] = json!(id.repeat(3));
    }
    let result = packet(
        &graph,
        "root",
        &Query::new(&DepsOptions::default(), None)?,
        &[],
        None,
        240,
    )?;
    let edges = result["edges"].as_array().ok_or("edges")?;
    assert!(edges.contains(&json!({"source":"root","target":"blocker","kind":"blocked_by"})));
    assert!(edges.contains(&json!({"source":"root","target":"parent","kind":"parent"})));
    assert_eq!(result["meta"]["truncated"], true);
    assert!(
        result["meta"]["usedTokens"]
            .as_u64()
            .is_some_and(|used| used <= 240)
    );
    Ok(())
}

/// Creates a real tracker for packet acceptance without SDK mocks.
fn workspace() -> Result<(tempfile::TempDir, Workspace), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let pm = directory.path().join(".agents/pm");
    fs::create_dir_all(pm.join("tasks"))?;
    fs::write(pm.join("settings.json"), "{}")?;
    Ok((directory, Workspace::discover(&pm)?))
}

/// Every built-in semantic family resolves consistently in both edge directions.
#[test]
fn packet_roles_kinds_and_target_markers() {
    for name in [
        "blocked_by",
        "blocks",
        "parent",
        "child",
        "related",
        "discovered_from",
        "incident_from",
        "implements",
        "recurs_from",
        "verifies",
        "supersedes",
        "depends_on",
        "child-of",
        "epic",
        "parent_child",
        "task",
        "related_to",
    ] {
        let canonical = kind(name).unwrap_or_default();
        assert!(!canonical.is_empty());
        let edge = json!({"source":"a","target":"b","kind":canonical});
        for id in ["a", "b"] {
            assert!(ROLES.contains(&role(&edge, id)));
        }
        assert!(!inverse(canonical).is_empty());
    }
    assert!(kind("unknown").is_none());
    for marker in ["", "none", "NULL", "n/a", "na", "no-active-blocker"] {
        assert!(target(marker).is_none());
    }
    assert_eq!(target(" a "), Some("a"));
    for (id, source) in [
        ("https://example.invalid/item", ""),
        ("github:fixture/issue", ""),
        ("a", "global"),
        ("a", "external"),
    ] {
        assert!(external(id, source));
    }
    assert!(!external("a", "project"));
    assert_eq!(compare(&json!({}), &json!({})), std::cmp::Ordering::Equal);
}

/// Invalid packet controls refuse before traversal and aliases produce canonical kind filters.
#[test]
fn packet_option_domains() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(next_budget(470, 470, 846), 234);
    assert_eq!(next_budget(10, 1000, 1), 9);
    assert_eq!(next_budget(20, 1, 10000), 1);
    for options in [
        DepsOptions {
            direction: Some("sideways".to_owned()),
            ..Default::default()
        },
        DepsOptions {
            kind: vec!["unknown".to_owned()],
            ..Default::default()
        },
        DepsOptions {
            node_limit: Some("0".to_owned()),
            ..Default::default()
        },
        DepsOptions {
            edge_limit: Some("bad".to_owned()),
            ..Default::default()
        },
        DepsOptions {
            token_budget: Some("0".to_owned()),
            ..Default::default()
        },
    ] {
        assert!(Query::new(&options, None).is_err());
    }
    let query = Query::new(
        &DepsOptions {
            kind: vec!["parent,depends_on,".to_owned()],
            ..Default::default()
        },
        Some(0),
    )?;
    assert!(query.matches("child"));
    assert!(query.matches("blocked_by"));
    assert!(!query.matches("related"));
    Ok(())
}

/// Builds a cyclic graph with inverse, duplicate, missing and external references.
fn fixture_graph(lifecycle: &Lifecycle) -> Graph {
    let mut root = item("sample-root");
    root["parent"] = json!("sample-parent");
    root["blocked_by"] = json!("sample-blocker");
    root["dependencies"] = json!([
        {"id":"sample-parent","kind":"depends_on"},
        {"id":"sample-blocker","kind":"blocked_by"},
        {"id":"sample-missing","kind":"related_to"},
        {"id":"sample-missing","kind":"related_to"},
        {"id":"github:fixture/issue","kind":"related"},
        {"id":"sample-unknown","kind":"unknown"},
        {"id":"none","kind":"related"}
    ]);
    let mut parent = item("sample-parent");
    parent["parent"] = json!("sample-root");
    let mut child = item("sample-child");
    child["parent"] = json!("sample-root");
    child["blocked_by"] = json!("sample-blocker");
    let mut blocker = item("sample-blocker");
    blocker["status"] = json!("closed");
    blocker["dependencies"] = json!([{"id":"sample-missing","kind":"related"}]);
    let mut grandchild = item("sample-grandchild");
    grandchild["parent"] = json!("sample-child");
    Graph::new(&[root, parent, child, blocker, grandchild], lifecycle)
}

/// External targets reserve separate keys from local, missing and previously allocated IDs.
#[test]
fn packet_external_namespace_keys() -> Result<(), Box<dyn std::error::Error>> {
    let (_directory, workspace) = workspace()?;
    let lifecycle = crate::list_registry::Lifecycle::load(workspace.pm_root())?;
    let mut root = item("sample-root");
    root["dependencies"] = json!([
        {"id":"plain-global","kind":"related","source_kind":"global"},
        {"id":"plain-external","kind":"related","source_kind":"external"},
        {"id":"sample-local","kind":"related","source_kind":"external"},
        {"id":"sample-missing","kind":"related","source_kind":"external"},
        {"id":"sample-missing","kind":"related"},
        {"id":"https://example.invalid/item","kind":"related"}
    ]);
    let graph = Graph::new(
        &[root, item("sample-local"), item("external:sample-local")],
        &lifecycle,
    );
    for id in [
        "plain-global",
        "plain-external",
        "external:external:sample-local",
        "external:sample-missing",
        "https://example.invalid/item",
    ] {
        assert_eq!(graph.details[id]["status"], "external");
    }
    assert_eq!(graph.details["sample-local"]["status"], "open");
    assert_eq!(graph.details["sample-missing"]["status"], "missing");
    let packet = packet(
        &graph,
        "sample-root",
        &Query::new(&DepsOptions::default(), None)?,
        &[],
        None,
        10000,
    )?;
    assert_eq!(packet["nodes"].as_array().map(Vec::len), Some(6));
    Ok(())
}

/// Cyclic, duplicate, inverse and dangling edges retain stable selection and truthful bounds.
#[test]
fn packet_traversal_and_continuation() -> Result<(), Box<dyn std::error::Error>> {
    let (_directory, workspace) = workspace()?;
    let lifecycle = crate::list_registry::Lifecycle::load(workspace.pm_root())?;
    let graph = fixture_graph(&lifecycle);
    assert_eq!(graph.aliases["depends_on"], 1);
    assert_eq!(graph.aliases["related_to"], 2);
    assert_eq!(graph.details["sample-missing"]["status"], "missing");
    let mut query = Query::new(&DepsOptions::default(), None)?;
    let complete = packet(&graph, "sample-root", &query, &[], None, 10000)?;
    assert_eq!(complete["meta"]["completeness"], "complete");
    assert_eq!(complete["summary"]["directEdges"]["ancestor"], 1);
    assert!(
        complete["nodes"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|node| node["distance"] == 2)
    );
    query.nodes = 1;
    let first = packet(&graph, "sample-root", &query, &[], None, 10000)?;
    let cursor = first["meta"]["nextCursor"].as_str().ok_or("cursor")?;
    let second = packet(&graph, "sample-root", &query, &[], Some(cursor), 10000)?;
    assert_ne!(first["nodes"][0]["id"], second["nodes"][0]["id"]);
    let contract = json!({"rootId":"sample-root","direction":"both","kinds":[],"maxDepth":3,"ordering":"required-root-evidence-first-v1"});
    let fingerprint = crate::history::sha256_digest(
        format!(
            "relationship-context\0{}",
            crate::pagination::stable(&contract)
        )
        .as_bytes(),
    );
    let mut decoded = crate::pagination::query(cursor, &fingerprint[..24])?;
    decoded["after_id"] = json!("deleted");
    let fallback = crate::pagination::encode(&decoded);
    assert!(packet(&graph, "sample-root", &query, &[], Some(&fallback), 10000).is_ok());
    decoded
        .as_object_mut()
        .map(|value| value.shift_remove("after_index"));
    assert!(
        packet(
            &graph,
            "sample-root",
            &query,
            &[],
            Some(&crate::pagination::encode(&decoded)),
            10000
        )
        .is_err()
    );
    assert!(packet(&graph, "sample-root", &query, &[], Some("bad"), 10000).is_err());
    assert!(packet(&graph, "sample-root", &query, &[], None, 1).is_err());
    assert!(
        packet(
            &graph,
            "sample-root",
            &query,
            &[],
            None,
            cost(&graph.details["sample-root"]) + 1
        )
        .is_err()
    );
    query.nodes = 20;
    query.edges = 1;
    let edges = packet(&graph, "sample-root", &query, &[], None, 10000)?;
    assert!(edges["meta"]["omittedEdges"].as_u64().unwrap_or_default() > 0);
    query.nodes = 1;
    let first = packet(&graph, "sample-root", &query, &[], None, 10000)?;
    let exact_node_budget =
        cost(&graph.details["sample-root"]) + cost(&json!([])) + cost(&first["nodes"][0]);
    let no_edge_room = packet(&graph, "sample-root", &query, &[], None, exact_node_budget)?;
    assert_eq!(no_edge_room["summary"]["returnedNodes"], 1);
    assert_eq!(no_edge_room["summary"]["returnedEdges"], 0);
    for direction in ["incoming", "outgoing", "both"] {
        query.direction = direction.to_owned();
        for depth in [0, 1, 2] {
            query.depth = depth;
            let bounded = packet(&graph, "sample-root", &query, &[], None, 10000)?;
            assert_eq!(bounded["meta"]["exact"], true);
        }
    }
    Ok(())
}

/// Root evidence and rendered budgets remain bounded through the public native API.
#[test]
fn packet_evidence_and_rendered_budget() -> Result<(), Box<dyn std::error::Error>> {
    let (_directory, workspace) = workspace()?;
    let mut root = item("sample-root");
    assert!(evidence(&root).is_empty());
    root["files"] = json!([{"path":"src/lib.rs"},{"path":" "}]);
    root["tests"] = json!([{"command":"cargo test"},{"path":"tests/fixture.rs"}]);
    root["docs"] = json!([{"path":"README.md"}]);
    root["parent"] = json!("sample-parent");
    for row in [root, item("sample-parent")] {
        fs::write(
            workspace
                .pm_root()
                .join(format!("tasks/{}.toon", text(&row, "id"))),
            toon_format::encode_default(&row)?,
        )?;
    }
    for toon in [true, false] {
        for budget in ["1", "30", "300", "800", "1200"] {
            let result = workspace.deps(
                "sample-root",
                &DepsOptions {
                    format: Some("context".to_owned()),
                    toon,
                    token_budget: Some(budget.to_owned()),
                    ..Default::default()
                },
            );
            match result {
                Ok(result) => {
                    assert_eq!(result["format"], "context");
                    assert!(
                        result["context"]["meta"]["usedTokens"]
                            .as_u64()
                            .unwrap_or_default()
                            <= budget.parse::<u64>()?
                    );
                }
                Err(PmRustError::CommandRefusal {
                    exit_code, context, ..
                }) => {
                    assert_eq!(exit_code, 2);
                    assert_eq!(context["code"], "invalid_argument_value");
                    assert!(
                        context["recovery"]["token_budget"]["feasible_tokens"]
                            .as_u64()
                            .is_some_and(
                                |tokens| tokens > budget.parse::<u64>().unwrap_or_default()
                            )
                    );
                }
                Err(error) => return Err(error.into()),
            }
        }
    }
    let summary = workspace.deps(
        "sample-root",
        &DepsOptions {
            format: Some("context".to_owned()),
            summary: true,
            ..Default::default()
        },
    )?;
    assert!(summary.get("context").is_none());
    for cursor in ["", " ", "bad"] {
        let result = workspace.deps(
            "sample-root",
            &DepsOptions {
                format: Some("context".to_owned()),
                cursor: Some(cursor.to_owned()),
                ..Default::default()
            },
        );
        assert_eq!(result.is_err(), cursor == "bad");
    }
    assert!(
        workspace
            .deps(
                "missing",
                &DepsOptions {
                    format: Some("context".to_owned()),
                    ..Default::default()
                }
            )
            .is_err()
    );
    Ok(())
}

/// Rendered-budget retries remove actual dangling-reference rows before reducing packet content.
#[test]
fn packet_rendered_budget_removes_missing_references() -> Result<(), Box<dyn std::error::Error>> {
    let (_directory, workspace) = workspace()?;
    let mut root = item("sample-root");
    root["blocked_by"] = json!("sample-missing");
    fs::write(
        workspace.pm_root().join("tasks/sample-root.toon"),
        toon_format::encode_default(&root)?,
    )?;
    let graph = Graph::new(&[root], &Lifecycle::load(workspace.pm_root())?);
    for toon in [true, false] {
        let mut options = DepsOptions {
            format: Some("context".to_owned()),
            toon,
            ..Default::default()
        };
        let mut query = Query::new(&options, None)?;
        let traversal = Traversal::new(&graph, "sample-root", &query);
        let mut compacted = None;
        for budget in 1..=1200 {
            query.tokens = budget;
            options.token_budget = Some(budget.to_string());
            if let Ok(result) =
                render_packet(&graph, "sample-root", &query, &traversal, &[], &options)
                && result["missing_references"] == json!([])
            {
                assert!(
                    result["context"]["meta"]["usedTokens"]
                        .as_u64()
                        .unwrap_or_default()
                        <= u64::try_from(budget)?
                );
                compacted = Some(result);
                break;
            }
        }
        let result = compacted.ok_or("no feasible reference-compacted packet")?;
        assert_eq!(result["missing_reference_count"], 1);
        assert_eq!(result["missing_count"], 1);
        assert_eq!(result["context"]["root"]["id"], "sample-root");
    }
    Ok(())
}

/// Summary reads avoid malformed evidence payloads while full packets preserve storage failures.
#[test]
fn packet_storage_and_evidence_projection_boundaries() -> Result<(), Box<dyn std::error::Error>> {
    let (directory, workspace) = workspace()?;
    let pm = workspace.pm_root();
    let path = pm.join("tasks/sample-root.toon");
    let content = toon_format::encode_default(&item("sample-root"))?;
    fs::write(
        &path,
        format!("{content}\nfiles[2]{{path}}:\n  src/lib.rs\n"),
    )?;
    let options = DepsOptions {
        format: Some("context".to_owned()),
        ..Default::default()
    };
    assert!(
        workspace
            .deps(
                "sample-root",
                &DepsOptions {
                    summary: true,
                    ..options.clone()
                }
            )
            .is_ok()
    );
    assert!(workspace.deps("sample-root", &options).is_err());
    fs::write(&path, &content)?;
    assert!(
        workspace
            .deps(
                "sample-root",
                &DepsOptions {
                    direction: Some("sideways".to_owned()),
                    ..options.clone()
                }
            )
            .is_err()
    );
    fs::write(pm.join("settings.json"), "invalid JSON")?;
    assert!(workspace.deps("sample-root", &options).is_err());
    fs::write(pm.join("settings.json"), "{}")?;
    fs::write(&path, "invalid metadata")?;
    assert!(workspace.deps("sample-root", &options).is_err());
    fs::rename(pm, directory.path().join("moved-tracker"))?;
    assert!(workspace.deps("sample-root", &options).is_err());
    Ok(())
}

/// Exercises packet construction with a fresh traversal for independent unit queries.
fn packet(
    graph: &Graph,
    root: &str,
    query: &Query,
    evidence: &[String],
    cursor: Option<&str>,
    budget: usize,
) -> Result<Value, PmRustError> {
    super::packet(
        graph,
        root,
        query,
        &Traversal::new(graph, root, query),
        evidence,
        cursor,
        budget,
    )
}

/// Unrelated dense components must not turn isolated-node lookup into a full edge scan.
#[test]
fn packet_adjacency_bounds_unrelated_edge_work() -> Result<(), Box<dyn std::error::Error>> {
    let (_directory, workspace) = workspace()?;
    let lifecycle = Lifecycle::load(workspace.pm_root())?;
    let mut rows = vec![item("sample-root")];
    for index in 0..20000 {
        let mut row = item(&format!("sample-{index}"));
        row["parent"] = json!("sample-hub");
        rows.push(row);
    }
    rows.push(item("sample-hub"));
    let graph = Graph::new(&rows, &lifecycle);
    let query = Query::new(&DepsOptions::default(), None)?;
    let started = std::time::Instant::now();
    for _ in 0..2000 {
        assert!(graph.neighbors("sample-root", &query).is_empty());
    }
    assert!(started.elapsed() < std::time::Duration::from_secs(1));
    Ok(())
}

/// Self-loops count once and directed, undirected and inverse kind filters retain ordering.
#[test]
fn packet_adjacency_preserves_incident_edge_semantics() -> Result<(), Box<dyn std::error::Error>> {
    let (_directory, workspace) = workspace()?;
    let lifecycle = Lifecycle::load(workspace.pm_root())?;
    let mut root = item("sample-root");
    root["dependencies"] = json!([
        {"id":"sample-root","kind":"related"},
        {"id":"sample-out","kind":"parent"},
        {"id":"sample-related","kind":"related"}
    ]);
    let mut incoming = item("sample-in");
    incoming["parent"] = json!("sample-root");
    let graph = Graph::new(
        &[
            root,
            incoming,
            item("sample-out"),
            item("sample-related"),
            item("isolated"),
        ],
        &lifecycle,
    );
    let complete = packet(
        &graph,
        "sample-root",
        &Query::new(&DepsOptions::default(), None)?,
        &[],
        None,
        10000,
    )?;
    assert_eq!(complete["summary"]["returnedEdges"], 4);
    assert_eq!(complete["summary"]["returnedNodes"], 3);
    for (direction, expected) in [
        (
            "both",
            vec!["sample-in", "sample-out", "sample-related", "sample-root"],
        ),
        (
            "incoming",
            vec!["sample-in", "sample-related", "sample-root"],
        ),
        (
            "outgoing",
            vec!["sample-out", "sample-related", "sample-root"],
        ),
    ] {
        let mut query = Query::new(
            &DepsOptions {
                direction: Some(direction.to_owned()),
                ..Default::default()
            },
            None,
        )?;
        let ids: Vec<_> = graph
            .neighbors("sample-root", &query)
            .iter()
            .map(|row| row.0)
            .collect();
        assert_eq!(ids, expected);
        query.kinds.insert("child".to_owned());
        assert!(
            graph
                .neighbors("sample-root", &query)
                .iter()
                .all(|(_, edge)| edge["kind"] == "parent")
        );
        assert!(graph.neighbors("isolated", &query).is_empty());
    }
    Ok(())
}
/// Recovery preserves a real producer cursor and explicit controls, with cost-limited fallback.
#[test]
fn packet_budget_recovery_replays_preserved_queries() -> Result<(), Box<dyn std::error::Error>> {
    let (_directory, workspace) = workspace()?;
    for (id, parent) in [
        ("sample-root", "sample-parent"),
        ("sample-parent", "sample-grand"),
        ("sample-grand", ""),
    ] {
        let mut row = item(id);
        if !parent.is_empty() {
            row["parent"] = json!(parent);
        }
        fs::write(
            workspace.pm_root().join(format!("tasks/{id}.toon")),
            toon_format::encode_default(&row)?,
        )?;
    }
    let mut options = DepsOptions {
        format: Some("context".to_owned()),
        max_depth: Some("2".to_owned()),
        node_limit: Some("1".to_owned()),
        edge_limit: Some("2".to_owned()),
        direction: Some("both".to_owned()),
        kind: vec!["parent,child".to_owned()],
        full: true,
        ..Default::default()
    };
    let first = workspace.deps("sample-root", &options)?;
    let cursor = first["context"]["meta"]["nextCursor"]
        .as_str()
        .ok_or("producer cursor")?
        .to_owned();
    options.cursor = Some(cursor.clone());
    options.token_budget = Some("1".to_owned());
    let Err(PmRustError::CommandRefusal {
        context, exit_code, ..
    }) = workspace.deps("sample-root", &options)
    else {
        return Err("bounded continuation must refuse".into());
    };
    assert_eq!(exit_code, 2);
    let args = context["recovery"]["suggested_retry_args"]
        .as_array()
        .ok_or("retry args")?;
    assert!(
        args.windows(2)
            .any(|pair| pair == [json!("--cursor"), json!(cursor)])
    );
    for flag in [
        "--max-depth",
        "--node-limit",
        "--edge-limit",
        "--direction",
        "--kind",
        "--full",
        "--json",
    ] {
        assert!(args.contains(&json!(flag)));
    }
    let feasible = context["recovery"]["token_budget"]["feasible_tokens"]
        .as_u64()
        .ok_or("feasible budget")?;
    options.token_budget = Some(feasible.to_string());
    let retry = workspace.deps("sample-root", &options)?;
    assert!(
        retry["context"]["meta"]["usedTokens"]
            .as_u64()
            .ok_or("used tokens")?
            <= feasible
    );
    options.cursor = None;
    options.summary = true;
    options.full = false;
    options.token_budget = Some("100".to_owned());
    assert!(workspace.deps("sample-root", &options).is_err());
    let mut root = item("sample-root");
    root["title"] = json!("x".repeat(1_000_001));
    fs::write(
        workspace.pm_root().join("tasks/sample-root.toon"),
        toon_format::encode_default(&root)?,
    )?;
    options.summary = false;
    options.token_budget = Some("1".to_owned());
    let Err(PmRustError::CommandRefusal { context, .. }) = workspace.deps("sample-root", &options)
    else {
        return Err("costly search must refuse".into());
    };
    assert_eq!(
        context["recovery"]["token_budget"]["reason"],
        "query_cost_limit"
    );
    assert!(
        context["recovery"]["suggested_retry"]
            .as_str()
            .is_some_and(|command| command.ends_with("--format tree --summary"))
    );
    Ok(())
}
/// Retry classification preserves native floors, separate output constraints and valid delivery.
#[test]
fn packet_probe_classification_respects_refusal_boundaries() {
    use crate::orientation_deps::budget_recovery::Probe;
    let floor = crate::collections::refusal(
        "Relationship context tokenBudget cannot fit one node",
        1,
        json!({"code":"unknown_error"}),
    );
    assert!(budget_floor(&floor));
    assert!(probe_delivery(Err(floor)) == Probe::Floor);
    assert!(!budget_floor(&crate::collections::refusal(
        "Unrelated argument failure",
        2,
        json!({})
    )));
    assert!(!budget_floor(&PmRustError::ReadCursor {
        code: "invalid_query_cursor",
        detail: "bad cursor".to_owned()
    }));
    assert!(probe_delivery(Ok(json!({"context":{"nodes":[]}}).into())) == Probe::Fits);
    assert!(probe_delivery(Ok(json!({"output_budget_exceeded":{"reason":"requested_budget_infeasible","omitted_result":true}}).into())) == Probe::OutputConstraints);
}
