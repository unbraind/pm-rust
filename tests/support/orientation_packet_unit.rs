//! Packet traversal, continuation and evidence acceptance over real tracker documents.
use super::*;
use crate::list_registry::Lifecycle;
use std::fs;

/// Builds a complete metadata row with deterministic timestamps.
fn item(id: &str) -> Value {
    json!({"id":id,"title":id,"type":"Task","status":"open","priority":1,"tags":[],"created_at":"2026-08-22T10:00:00.000Z","updated_at":"2026-08-22T10:00:00.000Z"})
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
            if let Ok(result) = render_packet(
                &graph,
                "sample-root",
                &query,
                &traversal,
                &[],
                &options,
                &mut RenderMeasurements::default(),
            ) && result["missing_references"] == json!([])
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

/// Retry memoization bounds producer work and retained bytes without changing packet contents.
#[test]
fn packet_retry_cache_bounds_work_and_storage() -> Result<(), Box<dyn std::error::Error>> {
    let (_root, workspace) = workspace()?;
    let mut row = item("sample-root");
    row["dependencies"] = json!([{"id":"sample-missing","kind":"related"}]);
    let graph = Graph::new(&[row], &Lifecycle::load(workspace.pm_root())?);
    let query = Query::new(&DepsOptions::default(), None)?;
    let traversal = Traversal::new(&graph, "sample-root", &query);
    let mut cache = RenderCache::default();
    let expected = super::packet(&graph, "sample-root", &query, &traversal, &[], None, 600)?;
    for _ in 0..128 {
        assert_eq!(
            cache.produce(&graph, "sample-root", &query, &traversal, &[], None, 600)?,
            expected
        );
    }
    assert_eq!(
        cache.builds, 1,
        "a repeated producer budget must not repeat graph packing"
    );
    assert!(cache.packet_bytes > 0 && cache.packet_bytes < 2 * 1024 * 1024);
    let mut full = RenderCache {
        packet_bytes: 2 * 1024 * 1024,
        ..Default::default()
    };
    assert_eq!(
        full.produce(&graph, "sample-root", &query, &traversal, &[], None, 600)?,
        expected
    );
    assert_eq!(full.packet_bytes, 2 * 1024 * 1024);
    assert!(full.packets.is_empty());
    assert!(
        cache
            .produce(&graph, "sample-root", &query, &traversal, &[], None, 1)
            .is_err()
    );
    Ok(())
}

/// Reservations retain source/target matching and measure each root edge only once per query.
#[test]
fn packet_direct_reservations_are_reused() -> Result<(), Box<dyn std::error::Error>> {
    let (_directory, workspace) = workspace()?;
    let mut root = item("sample-root");
    root["dependencies"] = json!([
        {"id":"sample-root","kind":"related"},
        {"id":"sample-a","kind":"parent"},
        {"id":"sample-a","kind":"blocks"},
        {"id":"sample-b","kind":"related"}
    ]);
    let mut other = item("sample-b");
    other["dependencies"] = json!([{"id":"sample-root","kind":"blocked_by"}]);
    let graph = Graph::new(
        &[root, item("sample-a"), other],
        &Lifecycle::load(workspace.pm_root())?,
    );
    let query = Query::new(&DepsOptions::default(), None)?;
    COST_CALLS.with(|calls| calls.set(0));
    let traversal = Traversal::new(&graph, "sample-root", &query);
    assert_eq!(
        COST_CALLS.with(std::cell::Cell::get),
        traversal.direct.len()
    );
    for id in graph.details.keys() {
        let expected: usize = traversal
            .direct
            .iter()
            .filter(|(_, edge)| text(edge, "source") == id || text(edge, "target") == id)
            .map(|(_, edge)| cost(edge))
            .sum();
        assert_eq!(
            traversal
                .reservations
                .get(id.as_str())
                .copied()
                .unwrap_or_default(),
            expected
        );
        let reasons: BTreeSet<_> = traversal
            .direct
            .iter()
            .filter(|(next, _)| next == id)
            .map(|(next, edge)| role(edge, next))
            .collect();
        assert_eq!(
            traversal
                .reasons
                .get(id.as_str())
                .cloned()
                .unwrap_or_default(),
            reasons
        );
    }
    assert_eq!(
        traversal.reservations["sample-root"],
        traversal
            .direct
            .iter()
            .map(|(_, edge)| cost(edge))
            .sum::<usize>()
    );
    let mut previous = None;
    for budget in [10000, 9999, 9998] {
        COST_CALLS.with(|calls| calls.set(0));
        let packet = super::packet(&graph, "sample-root", &query, &traversal, &[], None, budget)?;
        let measurements = COST_CALLS.with(std::cell::Cell::get);
        assert_eq!(
            measurements,
            2 + packet["nodes"].as_array().ok_or("nodes")?.len()
                + packet["edges"].as_array().ok_or("edges")?.len(),
            "packing retries must not remeasure direct-edge reservations"
        );
        let mut comparable = packet;
        comparable["meta"]["tokenBudget"] = json!(0);
        if let Some(previous) = previous {
            assert_eq!(comparable, previous);
        }
        previous = Some(comparable);
    }
    assert_tight_node_admission(&workspace, &query)?;
    Ok(())
}

/// A cheap additional node cannot consume reservations for direct root evidence.
fn assert_tight_node_admission(
    workspace: &Workspace,
    query: &Query,
) -> Result<(), Box<dyn std::error::Error>> {
    // Reserve several root edges so a cheaper second node could otherwise displace them.
    let mut root = item("sample-root");
    root["dependencies"] = json!([
        {"id":"sample-a","kind":"parent"},
        {"id":"sample-a","kind":"blocks"},
        {"id":"sample-a","kind":"related"},
        {"id":"sample-a","kind":"implements"},
        {"id":"sample-b","kind":"related"}
    ]);
    let graph = Graph::new(
        &[root, item("sample-a"), item("sample-b")],
        &Lifecycle::load(workspace.pm_root())?,
    );
    let traversal = Traversal::new(&graph, "sample-root", query);
    let generous = super::packet(&graph, "sample-root", query, &traversal, &[], None, 10000)?;
    let first = &generous["nodes"][0];
    assert_eq!(first["id"], "sample-a");
    let tight = cost(&graph.details["sample-root"])
        + cost(&json!([]))
        + cost(first)
        + traversal.reservations["sample-a"];
    for budget in tight..tight + 4 {
        let packed = super::packet(&graph, "sample-root", query, &traversal, &[], None, budget)?;
        assert_eq!(
            packed["nodes"]
                .as_array()
                .ok_or("nodes")?
                .iter()
                .map(|node| text(node, "id"))
                .collect::<Vec<_>>(),
            vec!["sample-a"]
        );
        assert_eq!(
            *packed["edges"].as_array().ok_or("packed edges")?,
            generous["edges"]
                .as_array()
                .ok_or("edges")?
                .iter()
                .filter(
                    |edge| text(edge, "source") == "sample-a" || text(edge, "target") == "sample-a"
                )
                .cloned()
                .collect::<Vec<_>>()
        );
        assert_eq!(packed["meta"]["omittedNodes"], 1);
    }
    Ok(())
}

/// Distinct reference projections share a bounded key cache without changing delivery.
#[test]
fn packet_render_cost_keys_are_bounded() -> Result<(), Box<dyn std::error::Error>> {
    const CAP: usize = 2 * 1024 * 1024;
    let (_directory, workspace) = workspace()?;
    let mut root = item("sample-root");
    root["status"] = json!("x".repeat(8192));
    root["dependencies"] = json!(
        (0..32)
            .map(|index| json!({
                "id":format!("sample-missing-{index:02}"), "kind":"related"
            }))
            .collect::<Vec<_>>()
    );
    let graph = Graph::new(&[root], &Lifecycle::load(workspace.pm_root())?);
    for toon in [false, true] {
        let options = DepsOptions {
            toon,
            ..Default::default()
        };
        let mut query = Query::new(&options, None)?;
        query.tokens = 1_000_000;
        query.nodes = 1;
        let traversal = Traversal::new(&graph, "sample-root", &query);
        let mut cache = RenderCache::default();
        for budget in [10000, 12500] {
            query.tokens = budget;
            let expected = render_packet(
                &graph,
                "sample-root",
                &query,
                &traversal,
                &[],
                &options,
                &mut RenderMeasurements::default(),
            )?;
            let result = render_cached(
                &graph,
                "sample-root",
                &query,
                &traversal,
                &[],
                &options,
                &mut cache,
            )?;
            assert!(result.value == expected.value, "cached delivery changed");
            let retained: usize = cache.costs.keys().map(String::len).sum();
            assert!(
                retained <= CAP,
                "retained serialized shape keys exceeded two MiB: {retained}"
            );
            assert_eq!(cache.cost_bytes, retained);
        }
        assert!(cache.cost_bytes > CAP / 2);
        assert!(
            cache.costs.len() < 32,
            "the corpus must exercise a cache-cap refusal"
        );
        let retained = cache.cost_bytes;
        let entries = cache.costs.len();
        render_cached(
            &graph,
            "sample-root",
            &query,
            &traversal,
            &[],
            &options,
            &mut cache,
        )?;
        assert_eq!((cache.cost_bytes, cache.costs.len()), (retained, entries));
        query.tokens = 10000;
        render_cached(
            &graph,
            "sample-root",
            &query,
            &traversal,
            &[],
            &options,
            &mut cache,
        )?;
        assert_eq!((cache.cost_bytes, cache.costs.len()), (retained, entries));
    }
    Ok(())
}

/// Cumulative admission retains only complete producer entries within the shared byte ceiling.
#[test]
fn packet_producer_cache_cumulative_admission() -> Result<(), Box<dyn std::error::Error>> {
    const CAP: usize = 2 * 1024 * 1024;
    let (_root, workspace) = workspace()?;
    let mut root = item("sample-root");
    root["status"] = json!("x".repeat(384 * 1024));
    let graph = Graph::new(&[root], &Lifecycle::load(workspace.pm_root())?);
    let query = Query::new(&DepsOptions::default(), None)?;
    let traversal = Traversal::new(&graph, "sample-root", &query);
    let mut cache = RenderCache::default();
    let mut retained = 0;
    for budget in [1_000_000, 999_999, 999_998, 999_997] {
        let expected = super::packet(&graph, "sample-root", &query, &traversal, &[], None, budget)?;
        let bytes = crate::stringify_json(&expected, false).len();
        assert!(
            bytes < CAP && bytes > CAP / 3,
            "each packet fits but three exceed the cap"
        );
        assert_eq!(
            cache.produce(&graph, "sample-root", &query, &traversal, &[], None, budget)?,
            expected
        );
        if retained + bytes <= CAP {
            retained += bytes;
            assert!(cache.packets.contains_key(&budget));
        } else {
            assert!(
                !cache.packets.contains_key(&budget),
                "cumulative cache admission overflowed"
            );
        }
        assert_eq!(cache.packet_bytes, retained);
        assert_eq!(
            cache.packet_bytes,
            cache
                .packets
                .values()
                .map(|value| crate::stringify_json(value, false).len())
                .sum::<usize>()
        );
        assert!(cache.packet_bytes <= CAP);
    }
    assert_eq!(cache.packets.len(), 2);
    Ok(())
}

/// Shared rendering matches a fresh cache across decimal budget widths and byte alignments.
#[test]
fn packet_cached_rendering_preserves_budget_digit_widths() -> Result<(), Box<dyn std::error::Error>>
{
    let (_root, workspace) = workspace()?;
    for toon in [false, true] {
        let mut witnessed_growth = false;
        for padding in 0..8 {
            let mut root = item("sample-root");
            root["status"] = json!("x".repeat(32));
            root["title"] = json!("x".repeat(32 + padding));
            let graph = Graph::new(&[root], &Lifecycle::load(workspace.pm_root())?);
            let options = DepsOptions {
                toon,
                ..Default::default()
            };
            let mut query = Query::new(&options, None)?;
            let traversal = Traversal::new(&graph, "sample-root", &query);
            let mut cache = RenderCache::default();
            let mut previous = 0;
            for budget in [999, 1000, 9999, 10000, 1000, 999] {
                query.tokens = budget;
                let expected = render_packet(
                    &graph,
                    "sample-root",
                    &query,
                    &traversal,
                    &[],
                    &options,
                    &mut RenderMeasurements::default(),
                )?;
                let actual = render_cached(
                    &graph,
                    "sample-root",
                    &query,
                    &traversal,
                    &[],
                    &options,
                    &mut cache,
                )?;
                assert_eq!(
                    actual.value, expected.value,
                    "toon={toon}, padding={padding}, budget={budget}"
                );
                let measured = tokens(&actual.value, toon);
                assert_eq!(actual["context"]["meta"]["usedTokens"], measured);
                if budget == 1000 && previous > 0 {
                    witnessed_growth |= measured > previous;
                }
                previous = measured;
            }
        }
        assert!(
            witnessed_growth,
            "padding must expose usedTokens growth across a budget digit boundary, toon={toon}"
        );
    }
    Ok(())
}

/// Cached packet measurements equal full rendering across scalar-width and encoding boundaries.
#[test]
fn cached_render_measurements_match_exact_wire_bytes() {
    let mut cache = RenderMeasurements::default();
    for toon in [false, true] {
        for tokens in [1, 9, 10, 99, 100, 999, 1000, 10000] {
            for title in ["", "Unicode café 🚀", "different shape"] {
                let mut result = json!({"context":{"root":{"id":"sample-a","title":title},"nodes":[{"id":"sample-b","role":"related"}],"meta":{"tokenBudget":tokens,"usedTokens":1}},"projection":{"mode":"full"}});
                let measured = cache.settle(&mut result, toon);
                assert_eq!(measured, super::tokens(&result, toon));
            }
        }
    }
    // At this width boundary, both 999 and 1000 are fixed points. Preserve
    // the published stabilization's initial counter instead of choosing a minimum.
    for toon in [false, true] {
        let mut result = json!({"context":{"root":{"id":"sample-a","title":""},"meta":{"tokenBudget":1,"usedTokens":1000}}});
        let padding = 3997 - super::super::rendered_bytes(&result, toon);
        result["context"]["root"]["title"] = json!("x".repeat(padding));
        assert_eq!(cache.settle(&mut result, toon), 1000);
        assert_eq!(super::tokens(&result, toon), 1000);
    }
}

/// Reference-prefix retries retain linear packet bytes instead of a quadratic sum.
#[test]
fn measurement_cache_retention_is_linear() -> Result<(), Box<dyn std::error::Error>> {
    let mut cache = RenderMeasurements::default();
    let references: Vec<_> = (0..512)
        .map(|index| json!({"id":format!("sample-{index}"),"description":"x".repeat(128)}))
        .collect();
    let mut packet = json!({"context":{"root":{"id":"sample-root"},"meta":{"tokenBudget":120_000,"usedTokens":1}},"missing_references":references});
    let largest = crate::stringify_json(&packet, false).len();
    loop {
        cache.settle(&mut packet, false);
        assert!(
            cache
                .fixed_bytes
                .keys()
                .map(|(_, key)| key.len())
                .sum::<usize>()
                <= 8 * largest
        );
        assert!(cache.fixed_bytes.len() <= 8);
        if packet["missing_references"]
            .as_array_mut()
            .ok_or("missing references")?
            .pop()
            .is_none()
        {
            break;
        }
    }
    Ok(())
}
