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
    let contract = json!({"rootId":"sample-root","direction":"both","kinds":[],"maxDepth":3});
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
            if budget == "1" || budget == "30" {
                assert!(result.is_err());
            } else {
                assert_eq!(result?["format"], "context");
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
