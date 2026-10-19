//! Optional-section lifecycle, traversal, history and projection acceptance.
use super::*;
use std::fmt::Write as _;
use std::fs;
const NOW: &str = "2026-08-22T10:00:00.000Z";

/// Creates a real minimal tracker and default lifecycle registry.
fn workspace() -> Result<(tempfile::TempDir, Workspace, Lifecycle), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let pm = directory.path().join(".agents/pm");
    fs::create_dir_all(pm.join("tasks"))?;
    fs::write(pm.join("settings.json"), "{}")?;
    let workspace = Workspace::discover(&pm)?;
    let lifecycle = Lifecycle::load(&pm)?;
    Ok((directory, workspace, lifecycle))
}

/// Builds complete metadata without replacing storage or SDK behavior.
fn item(id: &str, status: &str) -> Value {
    json!({"id":id,"title":id,"type":"Task","status":status,"priority":1,"tags":[],"created_at":NOW,"updated_at":NOW})
}

/// Enabled sections, cyclic descendants and whitespace parents preserve published boundaries.
#[test]
fn section_selection_and_descendant_cycles() -> Result<(), Box<dyn std::error::Error>> {
    let (_directory, _workspace, lifecycle) = workspace()?;
    assert!(selected("brief", &json!({})).is_empty());
    assert_eq!(selected("standard", &json!({})).len(), 6);
    assert_eq!(selected("deep", &json!({})).len(), 10);
    assert_eq!(
        selected("full", &json!({"context":{"sections":{"tests":false}}})).len(),
        10
    );
    assert_eq!(
        selected("deep", &json!({"context":{"sections":{"tests":false}}})).len(),
        9
    );
    let mut parent = item("sample-parent", "open");
    parent["parent"] = json!("sample-child");
    let mut child = item("sample-child", "closed");
    child["parent"] = json!("sample-parent");
    let mut spaced = item("sample-spaced", "open");
    spaced["parent"] = json!(" sample-parent ");
    let corpus = vec![parent, child, spaced];
    let tree = Hierarchy::new(&corpus, true);
    assert_eq!(tree.descendants("sample-parent").len(), 3);
    assert_eq!(
        Hierarchy::new(&corpus, false)
            .descendants("sample-parent")
            .len(),
        2
    );
    let mut row = json!({"id":"sample-parent","blocked":true,"blocker_ids":["sample-external"]});
    tree.enrich(&mut row, &lifecycle);
    assert_eq!(row["children_closed"], 1);
    assert_eq!(row["completion_pct"], 33);
    assert_eq!(row["blocked"], true);
    let mut leaf = json!({"id":"absent"});
    tree.enrich(&mut leaf, &lifecycle);
    assert!(leaf.get("children_total").is_none());
    assert_eq!(completion(1, 8), json!(13));
    assert_eq!(completion(29, 200), json!(14));
    Ok(())
}

/// Optional summaries retain resolved blocker evidence, mixed lifecycle counts and stable ties.
#[test]
fn section_summaries_with_resolved_and_missing_blockers() -> Result<(), Box<dyn std::error::Error>>
{
    let (_directory, workspace, lifecycle) = workspace()?;
    let mut epic = item("sample-epic", "in_progress");
    epic["type"] = json!("Epic");
    epic["files"] = json!([{"path":"src/lib.rs"},{"path":"src/lib.rs"}]);
    epic["tests"] = json!([{"command":"cargo test"}]);
    let mut feature = item("sample-feature", "open");
    feature["type"] = json!("Feature");
    feature["priority"] = json!(2);
    feature["assignee"] = json!("fixture");
    let mut child = item("sample-child", "open");
    child["parent"] = json!("sample-epic");
    child["files"] = json!([{"path":"src/lib.rs"},{"path":"src/main.rs"}]);
    child["test_runs"] = json!([{"passed":1,"failed":0},{"failed":1,"skipped":2}]);
    let mut done = item("sample-done", "closed");
    done["parent"] = json!("sample-epic");
    let mut blocked = item("sample-blocked", "blocked");
    blocked["parent"] = json!("sample-epic");
    blocked["blocked_by"] = json!("sample-done");
    blocked["updated_at"] = json!("2026-08-01T10:00:00.000Z");
    let mut missing = item("sample-missing", "blocked");
    missing["blocked_by"] = json!("sample-unavailable");
    let mut progress = item("sample-progress", "in_progress");
    progress["parent"] = json!("sample-epic");
    let mut idle = item("sample-idle", "draft");
    idle["updated_at"] = json!("not-a-date");
    let corpus = vec![epic, feature, child, done, blocked, missing, progress, idle];
    let active = vec![&corpus[1], &corpus[0], &corpus[2], &corpus[6]];
    let blocked = BTreeSet::from(["sample-blocked".to_owned(), "sample-missing".to_owned()]);
    let result = build(
        &workspace,
        &NAMES,
        &corpus,
        &active,
        &[&corpus[4], &corpus[5]],
        &blocked,
        &lifecycle,
        NOW,
        10,
        &json!({}),
    )?;
    assert_eq!(result["blockers"][0]["blocked_by_status"], "closed");
    assert!(result["blockers"][1]["blocked_by_status"].is_null());
    assert_eq!(result["hierarchy"][0]["children_in_progress"], 1);
    assert_eq!(result["progress"][0]["open"], 1);
    assert_eq!(result["files"][0]["references"], 2);
    assert_eq!(result["tests"]["items_failing"], json!(["sample-child"]));
    assert_eq!(result["staleness"][0]["stale_days"], 21);
    let mut custom = json!({"schema":{"statuses":[{"id":"working","roles":["blocked"]},{"id":"open","roles":["active"]}],"workflow":{"in_progress_status":"working"}}});
    fs::write(
        workspace.pm_root().join("settings.json"),
        custom.to_string(),
    )?;
    fs::create_dir_all(workspace.pm_root().join("schema"))?;
    fs::write(
        workspace.pm_root().join("schema/workflows.json"),
        json!({"workflow":{"in_progress_status":"working"}}).to_string(),
    )?;
    let custom_lifecycle = Lifecycle::load(workspace.pm_root())?;
    let mut custom_corpus = corpus.clone();
    custom_corpus[6]["status"] = json!("working");
    let result = build(
        &workspace,
        &["hierarchy", "progress"],
        &custom_corpus,
        &[&custom_corpus[0]],
        &[],
        &BTreeSet::from(["sample-progress".to_owned()]),
        &custom_lifecycle,
        NOW,
        10,
        &json!({}),
    )?;
    assert_eq!(result["hierarchy"][0]["children_in_progress"], 1);
    assert_eq!(result["progress"][0]["blocked"], 1);
    custom["context"] = json!({"stale_threshold_days":30});
    assert!(
        build(
            &workspace,
            &["staleness"],
            &corpus,
            &[],
            &[],
            &blocked,
            &lifecycle,
            NOW,
            0,
            &custom
        )?["staleness"]
            .as_array()
            .is_some_and(Vec::is_empty)
    );
    Ok(())
}

/// Structural ties compare every published optional field before the stable identifier.
#[test]
fn section_critical_ordering_ties() -> Result<(), Box<dyn std::error::Error>> {
    let (_directory, _workspace, lifecycle) = workspace()?;
    let a = item("sample-a", "open");
    let mut b = item("sample-b", "open");
    assert!(critical(&a, &b, &lifecycle).is_lt());
    b["updated_at"] = json!("2026-08-01T10:00:00.000Z");
    assert!(critical(&a, &b, &lifecycle).is_lt());
    b["deadline"] = json!(NOW);
    assert!(critical(&a, &b, &lifecycle).is_gt());
    b["order"] = json!(1);
    assert!(critical(&a, &b, &lifecycle).is_gt());
    b["priority"] = json!(2);
    assert!(critical(&a, &b, &lifecycle).is_lt());
    Ok(())
}

/// Compact activity retains deleted streams, deterministic ties and original filesystem failures.
#[test]
fn section_activity_storage_and_ordering() -> Result<(), Box<dyn std::error::Error>> {
    let (_directory, workspace, _lifecycle) = workspace()?;
    let history = workspace.pm_root().join("history");
    assert_eq!(activity(&workspace, 10)?, json!([]));
    fs::write(&history, "not a directory")?;
    assert!(activity(&workspace, 10).is_err());
    let error = fs::read_dir(&history).err().ok_or("filesystem refusal")?;
    assert!(activity_entry(Err(error), &history).is_err());
    fs::remove_file(&history)?;
    fs::create_dir(&history)?;
    fs::write(history.join("ignored.txt"), "not history")?;
    for (id, rows) in [
        (
            "sample-a",
            vec![
                json!({"ts":NOW,"op":"update","author":"fixture","message":""}),
                json!({"ts":NOW,"op":"create","message":"Retained evidence"}),
            ],
        ),
        (
            "sample-b",
            vec![json!({"ts":NOW}), json!({"ts":"2026-08-01T00:00:00.000Z"})],
        ),
    ] {
        fs::write(
            history.join(format!("{id}.jsonl")),
            rows.iter().fold(String::new(), |mut output, row| {
                let _ = writeln!(output, "{row}");
                output
            }),
        )?;
    }
    let rows = activity(&workspace, 10)?;
    assert_eq!(rows[0]["op"], "create");
    assert_eq!(rows[0]["msg"], "Retained evidence");
    assert_eq!(rows[2]["author"], "unknown");
    assert_eq!(activity(&workspace, 1)?.as_array().map(Vec::len), Some(1));
    fs::write(history.join("sample-b.jsonl"), "invalid JSON")?;
    assert!(activity(&workspace, 10).is_err());
    Ok(())
}
