//! Bounded dependency traversal storage and budget acceptance.
use super::*;
use std::fs;
/// Builds an indexed cyclic graph with missing and repeated targets.
fn fixture_index() -> Index {
    let mut index = Index::new();
    for (id, dependencies) in [
        (
            "a",
            json!([{"id":"b","kind":"related"},{"id":"missing","kind":"related"}]),
        ),
        (
            "b",
            json!([{"id":"a","kind":"related"},{"id":"missing","kind":"related"}]),
        ),
    ] {
        index.insert(
            id.to_owned(),
            json!({"id":id,"title":id,"type":"Task","status":"open","dependencies":dependencies}),
        );
    }
    index
}
/// Creates a bounded expansion policy.
fn budget() -> Budget {
    Budget {
        nodes: 200,
        edges: 400,
        tokens: 16000,
        used_nodes: 0,
        used_edges: 0,
        used_tokens: 0,
        reasons: BTreeSet::new(),
        depth: 32,
        collapse: false,
    }
}
/// Traversal preserves cycles, missing nodes and each budget's independent receipt.
#[test]
fn traversal_limits_and_counts() {
    let index = fixture_index();
    let total = counts("a", &index, usize::MAX);
    assert_eq!(total["node_count"], 3);
    assert_eq!(total["edge_count"], 4);
    assert_eq!(total["missing_count"], 1);
    assert_eq!(counts("a", &index, 0)["node_count"], 1);
    for reason in [
        "none",
        "node_limit",
        "edge_limit",
        "token_budget",
        "max_depth",
    ] {
        let mut policy = budget();
        match reason {
            "node_limit" => policy.nodes = 1,
            "edge_limit" => policy.edges = 1,
            "token_budget" => policy.tokens = 1,
            "max_depth" => policy.depth = 0,
            _ => {}
        }
        let tree = node(
            "a",
            None,
            &index,
            &BTreeSet::new(),
            &mut BTreeSet::new(),
            &mut policy,
            0,
        );
        let graph = graph(&tree);
        assert_eq!(graph["root_id"], "a");
        if reason == "none" {
            assert_eq!(graph["nodes"].as_array().map(Vec::len), Some(3));
            assert_eq!(graph["missing_ids"], json!(["missing"]));
        } else {
            assert!(policy.reasons.contains(reason));
        }
    }
    let mut policy = budget();
    policy.tokens = 80;
    let tree = node(
        "a",
        None,
        &index,
        &BTreeSet::new(),
        &mut BTreeSet::new(),
        &mut policy,
        0,
    );
    assert!(!graph(&tree).is_null());
    let mut policy = budget();
    policy.collapse = true;
    let collapsed = node(
        "a",
        None,
        &index,
        &BTreeSet::new(),
        &mut BTreeSet::from(["a".to_owned()]),
        &mut policy,
        0,
    );
    assert_eq!(collapsed["collapsed"], true);
    let mut tree = node(
        "a",
        None,
        &index,
        &BTreeSet::new(),
        &mut BTreeSet::new(),
        &mut budget(),
        0,
    );
    assert!(trim(&mut tree));
    while trim(&mut tree) {}
    assert_eq!(tree["dependencies"], json!([]));
    assert!(!trim(&mut Value::Null));
    let result = project(json!({"id":"a","format":"tree"}), true, &json!({}));
    assert_eq!(result["omission_receipt"]["has_omissions"], true);
    for toon in [true, false] {
        assert!(
            tokens(
                &project(json!({"id":"a","format":"tree"}), false, &json!({})),
                toon
            ) > 0
        );
    }
}
/// Every documented dependency value and storage boundary refuses without writes.
#[test]
fn dependency_value_refusals() -> Result<(), Box<dyn std::error::Error>> {
    let root = tempfile::tempdir()?;
    let pm = root.path().join(".agents/pm");
    fs::create_dir_all(pm.join("tasks"))?;
    fs::write(pm.join("settings.json"), "{}")?;
    let workspace = Workspace::discover(root.path())?;
    for options in [
        DepsOptions {
            format: Some("bad".to_owned()),
            ..Default::default()
        },
        DepsOptions {
            max_depth: Some("-1".to_owned()),
            ..Default::default()
        },
        DepsOptions {
            collapse: Some("bad".to_owned()),
            ..Default::default()
        },
        DepsOptions {
            full: true,
            summary: true,
            ..Default::default()
        },
    ] {
        assert!(workspace.deps("a", &options).is_err());
    }
    assert!(workspace.deps("a", &DepsOptions::default()).is_err());
    let row = json!({"id":"a","title":"A","type":"Task","status":"open","priority":1,"tags":[],"created_at":"2026-08-22T10:00:00.000Z","updated_at":"2026-08-22T10:00:00.000Z","blocked_by":"missing","dependencies":[{"id":"b","kind":"depends_on"},{"id":"b","kind":"depends_on"},{"id":"a","kind":"child_of"},{"id":"missing","kind":"task"}]});
    fs::write(pm.join("tasks/a.toon"), toon_format::encode_default(&row)?)?;
    for options in [
        DepsOptions {
            node_limit: Some("0".to_owned()),
            ..Default::default()
        },
        DepsOptions {
            edge_limit: Some("bad".to_owned()),
            ..Default::default()
        },
        DepsOptions {
            token_budget: Some("-1".to_owned()),
            ..Default::default()
        },
    ] {
        assert!(workspace.deps("a", &options).is_err());
    }
    assert_eq!(
        workspace.deps(
            "a",
            &DepsOptions {
                summary: true,
                ..Default::default()
            }
        )?["node_count"],
        3
    );
    for toon in [true, false] {
        for format in ["tree", "graph"] {
            let result = workspace.deps(
                "a",
                &DepsOptions {
                    format: Some(format.to_owned()),
                    toon,
                    token_budget: Some("1".to_owned()),
                    ..Default::default()
                },
            )?;
            assert_eq!(result["node_count"], 1);
        }
    }
    let bounded = workspace.deps(
        "a",
        &DepsOptions {
            token_budget: Some("200".to_owned()),
            ..Default::default()
        },
    )?;
    assert!(
        bounded["truncation"]["reasons"]
            .as_array()
            .is_some_and(|reasons| reasons.contains(&json!("token_budget")))
    );
    fs::write(pm.join("tasks/invalid.toon"), "invalid item")?;
    assert!(workspace.deps("a", &DepsOptions::default()).is_err());
    Ok(())
}

/// Canonical dependency kinds, aliases and scalar blockers remain distinct before traversal.
#[test]
fn dependency_alias_and_scalar_boundaries() -> Result<(), Box<dyn std::error::Error>> {
    let root = tempfile::tempdir()?;
    let pm = root.path().join(".agents/pm");
    fs::create_dir_all(pm.join("tasks"))?;
    fs::write(pm.join("settings.json"), "{}")?;
    let workspace = Workspace::discover(root.path())?;
    for blocked in ["", "no-active-blocker", "a", "missing"] {
        let row = json!({"id":"a","title":"A","type":"Task","status":"open","priority":1,"tags":[],"created_at":"2026-08-22T10:00:00.000Z","updated_at":"2026-08-22T10:00:00.000Z","blocked_by":blocked,"dependencies":[{"id":"missing","kind":"blocked_by"},{"id":"b","kind":"related_to"},{"id":"c","kind":"parent_child"},{"id":"d","kind":"epic"}]});
        fs::write(pm.join("tasks/a.toon"), toon_format::encode_default(&row)?)?;
        let (index, aliases) = index(&workspace)?;
        assert_eq!(
            index["a"]["dependencies"].as_array().map(Vec::len),
            Some(if blocked == "no-active-blocker" { 5 } else { 4 })
        );
        assert_eq!(aliases, json!({"epic":1,"parent_child":1,"related_to":1}));
    }
    assert!(
        workspace
            .deps(
                "a",
                &DepsOptions {
                    format: Some("context".to_owned()),
                    ..Default::default()
                }
            )
            .is_err()
    );
    assert!(
        workspace
            .deps(
                "a",
                &DepsOptions {
                    collapse: Some("repeated".to_owned()),
                    full: true,
                    ..Default::default()
                }
            )
            .is_ok()
    );
    Ok(())
}

/// Depth and token stops remain explicit even when a leaf has no outgoing edges.
#[test]
fn dependency_leaf_and_trim_boundaries() {
    let index = fixture_index();
    let mut limit = budget();
    limit.tokens = 0;
    let mut seen = BTreeSet::new();
    let lineage = BTreeSet::new();
    let output = node("a", None, &index, &lineage, &mut seen, &mut limit, 0);
    assert_eq!(output["truncated"], true);
    assert!(limit.reasons.contains("token_budget"));
    let mut leaf = json!({"id":"leaf","dependencies":[]});
    assert!(!trim(&mut leaf));
    let mut index = index;
    index.insert("leaf".to_owned(), leaf);
    for (depth, tokens) in [(0, usize::MAX), (32, 0)] {
        let mut limit = budget();
        limit.depth = depth;
        limit.tokens = tokens;
        let leaf = node(
            "leaf",
            None,
            &index,
            &BTreeSet::new(),
            &mut BTreeSet::new(),
            &mut limit,
            0,
        );
        assert!(leaf.get("truncated").is_none());
    }
}
