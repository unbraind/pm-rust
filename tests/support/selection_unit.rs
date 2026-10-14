//! Readiness and ranking invariants over a real disposable tracker.

use super::*;
use crate::{CreateItem, ItemMetadata};

const CLOCK: &str = "2026-08-22T10:00:00.000Z";

/// Builds an actionable document for pure filtering and ordering checks.
fn item(id: &str) -> ItemDocument {
    ItemDocument {
        metadata: ItemMetadata {
            id: id.to_owned(),
            title: id.to_owned(),
            description: String::new(),
            item_type: "Task".to_owned(),
            status: "open".to_owned(),
            priority: 2,
            tags: vec!["native".to_owned()],
            created_at: CLOCK.to_owned(),
            updated_at: CLOCK.to_owned(),
            parent: None,
            extra: serde_json::Map::new(),
        },
        body: String::new(),
    }
}

/// Creates the persisted settings and history used by ranking and ownership.
fn tracker() -> Result<(tempfile::TempDir, Workspace), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let root = directory.path().join(".agents/pm");
    fs::create_dir_all(&root)?;
    fs::write(root.join("settings.json"), "{}")?;
    Ok((directory, Workspace::discover(&root)?))
}

/// Seeds the real item shared by ownership and validation scenarios.
fn seed(workspace: &Workspace) -> Result<(), Box<dyn std::error::Error>> {
    workspace.create(CreateItem {
        id: "sample-own".to_owned(),
        title: "Ready".to_owned(),
        description: String::new(),
        item_type: "Task".to_owned(),
        status: "open".to_owned(),
        priority: 2,
        tags: vec![],
        body: String::new(),
        author: "alpha".to_owned(),
        timestamp: Some(CLOCK.to_owned()),
        message: None,
        provenance_role: None,
        force_stale_lock: false,
    })?;
    Ok(())
}

/// Constructs an explicit native ownership request.
fn request() -> OwnershipItem {
    OwnershipItem {
        id: "sample-own".to_owned(),
        author: "alpha".to_owned(),
        timestamp: Some(CLOCK.to_owned()),
        message: None,
        force: false,
        if_available: false,
        provenance_role: None,
    }
}

/// Missing schema roles use defaults; explicit roles support custom workflows.
#[test]
fn lifecycle_roles_and_filters_respect_schema_and_ownership() {
    let mut document = item("sample-own");
    assert!(role(&document, &json!({}), "active"));
    assert!(!role(&document, &json!({}), "terminal"));
    assert!(!role(&document, &json!({}), "other"));
    document.metadata.status = "done".to_owned();
    let settings = json!({"schema":{"statuses":[{"id":"done","roles":["terminal"]}, {"id":"ready", "roles":["active"]}]}});
    assert!(role(&document, &settings, "terminal"));
    assert!(!role(&document, &settings, "active"));
    document.metadata.extra.extend([
        ("sprint".to_owned(), json!("S1")),
        ("release".to_owned(), json!("R1")),
        ("assignee".to_owned(), json!("alpha")),
    ]);
    let selection = SelectionOptions {
        item_type: Some("task".to_owned()),
        tag: Some("native".to_owned()),
        priority: Some(2),
        sprint: Some("S1".to_owned()),
        release: Some("R1".to_owned()),
        assignee_filter: Some("assigned".to_owned()),
        ..SelectionOptions::default()
    };
    assert!(matches(&document, &selection));
    let unassigned = SelectionOptions {
        assignee_filter: Some("unassigned".to_owned()),
        ..SelectionOptions::default()
    };
    assert!(!matches(&document, &unassigned));
    document.metadata.extra.remove("assignee");
    assert!(matches(&document, &unassigned));
    assert!(!matches(&document, &selection));
}

/// Cycles terminate, and inverse blocks are combined with direct prerequisites.
#[test]
fn dependencies_and_descendants_are_cycle_safe() {
    let mut first = item("sample-a");
    first.metadata.parent = Some("sample-b".to_owned());
    first
        .metadata
        .extra
        .insert("blocked_by".to_owned(), json!(" SAMPLE-C "));
    first.metadata.extra.insert(
        "dependencies".to_owned(),
        json!([
            {"id":"SAMPLE-D", "kind":"blocked_by"},
            {"id":"sample-e", "kind":"related"},
            {"kind":"blocked_by"}
        ]),
    );
    let mut second = item("sample-b");
    second.metadata.parent = Some("sample-a".to_owned());
    second.metadata.extra.insert(
        "dependencies".to_owned(),
        json!([
            {"id":"SAMPLE-A", "kind":"blocks"}
        ]),
    );
    let corpus = vec![first.clone(), second];
    assert_eq!(
        descendants("SAMPLE-A", &corpus),
        BTreeSet::from(["sample-a".to_owned(), "sample-b".to_owned()])
    );
    assert_eq!(
        blockers(&first, &corpus),
        BTreeSet::from([
            "sample-b".to_owned(),
            "sample-c".to_owned(),
            "sample-d".to_owned()
        ])
    );
    first
        .metadata
        .extra
        .insert("blocked_by".to_owned(), json!("no-active-blocker"));
    first.metadata.extra.remove("dependencies");
    assert!(blockers(&first, &[]).is_empty());
}

/// Baseline ordering is total across every optional structural sort key.
#[test]
fn structural_order_keeps_missing_values_last() {
    let mut earlier = item("sample-a");
    let mut later = item("sample-b");
    let settings = json!({});
    assert_eq!(baseline(&earlier, &later, &settings), Ordering::Less);
    earlier.metadata.updated_at = "2026-08-21T10:00:00.000Z".to_owned();
    assert_eq!(baseline(&earlier, &later, &settings), Ordering::Greater);
    earlier
        .metadata
        .extra
        .insert("deadline".to_owned(), json!("2026-08-23T10:00:00Z"));
    assert_eq!(baseline(&earlier, &later, &settings), Ordering::Less);
    assert_eq!(baseline(&later, &earlier, &settings), Ordering::Greater);
    later
        .metadata
        .extra
        .insert("deadline".to_owned(), json!("2026-08-24T10:00:00Z"));
    assert_eq!(baseline(&earlier, &later, &settings), Ordering::Less);
    later.metadata.extra.insert("order".to_owned(), json!(1));
    assert_eq!(baseline(&earlier, &later, &settings), Ordering::Greater);
    assert_eq!(baseline(&later, &earlier, &settings), Ordering::Less);
    earlier.metadata.extra.insert("order".to_owned(), json!(0));
    assert_eq!(baseline(&earlier, &later, &settings), Ordering::Less);
    later.metadata.priority = 0;
    assert_eq!(baseline(&earlier, &later, &settings), Ordering::Greater);
    earlier.metadata.status = "in_progress".to_owned();
    assert_eq!(baseline(&earlier, &later, &settings), Ordering::Less);
}

/// Persisted history and graph/activity signals distinguish equally ready work.
#[test]
fn ranking_uses_history_activity_and_ready_descendants() -> Result<(), Box<dyn std::error::Error>> {
    let (_directory, workspace) = tracker()?;
    let mut parent = item("sample-parent");
    parent.metadata.item_type = "Epic".to_owned();
    let mut active = item("sample-active");
    active.metadata.status = "in_progress".to_owned();
    active.metadata.parent = Some("sample-parent".to_owned());
    active.metadata.extra.extend([
        ("risk".to_owned(), json!("high")),
        ("deadline".to_owned(), json!("2026-08-22T10:00:00Z")),
        ("comments".to_owned(), json!([{}, {}, {}, {}, {}, {}])),
        (
            "dependencies".to_owned(),
            json!([{"kind":"related", "id":"sample-parent"}]),
        ),
    ]);
    let mut terminal = item("sample-terminal");
    terminal.metadata.status = "closed".to_owned();
    let mut ready = item("sample-ready");
    ready
        .metadata
        .extra
        .insert("blocked_by".to_owned(), json!("sample-terminal"));
    ready
        .metadata
        .extra
        .insert("risk".to_owned(), json!("medium"));
    ready
        .metadata
        .extra
        .insert("deadline".to_owned(), json!("2026-09-22T10:00:00Z"));
    let mut blocked = item("sample-blocked");
    blocked
        .metadata
        .extra
        .insert("blocked_by".to_owned(), json!("sample-absent"));
    let mut foreign = item("sample-foreign");
    foreign
        .metadata
        .extra
        .insert("assignee".to_owned(), json!("beta"));
    let mut waiting = item("sample-waiting");
    waiting
        .metadata
        .extra
        .insert("blocked_by".to_owned(), json!("sample-active"));
    let corpus = vec![parent, active, terminal, ready, blocked, foreign, waiting];
    let selected = candidates(
        &workspace,
        &corpus,
        &json!({}),
        &request(),
        &SelectionOptions::default(),
    )?;
    assert_eq!(
        selected
            .iter()
            .map(|item| item.metadata.id.as_str())
            .collect::<Vec<_>>(),
        ["sample-active", "sample-ready"]
    );
    let missing = SelectionOptions {
        parent: Some("sample-absent".to_owned()),
        ..SelectionOptions::default()
    };
    assert!(matches!(
        candidates(&workspace, &corpus, &json!({}), &request(), &missing),
        Err(PmRustError::CommandRefusal { exit_code: 3, .. })
    ));
    let family = SelectionOptions {
        parent: Some("sample-parent".to_owned()),
        include_containers: true,
        ..SelectionOptions::default()
    };
    assert_eq!(
        candidates(&workspace, &corpus, &json!({}), &request(), &family)?.len(),
        2
    );
    let filtered = SelectionOptions {
        priority: Some(1),
        ..SelectionOptions::default()
    };
    assert_eq!(
        candidates(&workspace, &corpus, &json!({}), &request(), &filtered)?,
        Vec::<ItemDocument>::new()
    );
    fs::create_dir_all(workspace.pm_root().join("history"))?;
    fs::write(
        workspace.pm_root().join("history/sample-active.jsonl"),
        "invalid\n{\"event_class\":\"substantive\",\"ts\":\"2026-08-20T10:00:00Z\"}\n{\"event_class\":\"maintenance\"}\n",
    )?;
    assert_eq!(recency(&workspace, &corpus[1]), "2026-08-20T10:00:00Z");
    assert_eq!(recency(&workspace, &corpus[3]), CLOCK);
    Ok(())
}

/// Bad settings and invalid budgets fail before attempting an ownership mutation.
#[test]
fn selection_validation_and_storage_errors_preserve_items() -> Result<(), Box<dyn std::error::Error>>
{
    let (_directory, workspace) = tracker()?;
    let options = OwnershipOptions::default();
    for max_attempts in [0, 101] {
        assert!(matches!(
            workspace.claim_next(
                &request(),
                &options,
                &SelectionOptions {
                    max_attempts: Some(max_attempts),
                    ..SelectionOptions::default()
                }
            ),
            Err(PmRustError::CommandRefusal { exit_code: 2, .. })
        ));
    }
    seed(&workspace)?;
    let before = fs::read(workspace.pm_root().join("tasks/sample-own.toon"))?;
    assert!(matches!(
        workspace.claim_next(
            &request(),
            &options,
            &SelectionOptions {
                assignee_filter: Some("mine".to_owned()),
                ..SelectionOptions::default()
            }
        ),
        Err(PmRustError::CommandRefusal { exit_code: 2, .. })
    ));
    let corrupt = workspace.pm_root().join("tasks/sample-corrupt.toon");
    fs::write(&corrupt, "invalid")?;
    assert!(
        workspace
            .claim_next(&request(), &options, &SelectionOptions::default())
            .is_err()
    );
    fs::remove_file(corrupt)?;
    assert_eq!(
        workspace.claim_next(&request(), &options, &SelectionOptions::default())?["id"],
        "sample-own"
    );
    let explicit = OwnershipOptions {
        identity: Some(crate::AuthorIdentity::resolve(
            Some("alpha"),
            None,
            &BTreeMap::new(),
        )),
        release_bypass: false,
    };
    assert_eq!(
        workspace.claim_next(&request(), &explicit, &SelectionOptions::default())?["id"],
        "sample-own"
    );
    assert_eq!(
        workspace.claim_next(
            &request(),
            &explicit,
            &SelectionOptions {
                assignee_filter: Some("assigned".to_owned()),
                ..SelectionOptions::default()
            }
        )?["id"],
        "sample-own"
    );
    workspace.release(&request())?;
    assert!(matches!(
        workspace.claim_next(
            &request(),
            &options,
            &SelectionOptions {
                parent: Some("sample-absent".to_owned()),
                ..SelectionOptions::default()
            }
        ),
        Err(PmRustError::CommandRefusal { exit_code: 3, .. })
    ));
    fs::write(workspace.pm_root().join("settings.json"), "invalid")?;
    assert!(matches!(
        workspace.claim_next(&request(), &options, &SelectionOptions::default()),
        Err(PmRustError::InvalidReadRequest { .. })
    ));
    fs::remove_file(workspace.pm_root().join("settings.json"))?;
    assert!(matches!(
        workspace.claim_next(&request(), &options, &SelectionOptions::default()),
        Err(PmRustError::Io { .. })
    ));
    assert_eq!(
        fs::read(workspace.pm_root().join("tasks/sample-own.toon"))?,
        before
    );
    Ok(())
}

/// A stale ranking retries only competing owners and preserves lock refusals.
#[test]
fn ranked_snapshot_exhaustion_and_lock_refusals_are_distinct()
-> Result<(), Box<dyn std::error::Error>> {
    let (_directory, workspace) = tracker()?;
    seed(&workspace)?;
    let snapshot = workspace.read_items()?;
    let options = OwnershipOptions::default();
    assert!(matches!(
        workspace.claim_ranked(&request(), &options, &[], 1),
        Err(PmRustError::CommandRefusal { exit_code: 3, .. })
    ));
    let mut owner = request();
    owner.author = "beta".to_owned();
    workspace.claim(&owner)?;
    assert!(matches!(
        workspace.claim_ranked(&request(), &options, &snapshot, 1),
        Err(PmRustError::CommandRefusal { exit_code: 4, .. })
    ));
    let mut skip = request();
    skip.if_available = true;
    let receipt = workspace.claim_ranked(&skip, &options, &snapshot, 1)?;
    assert_eq!(receipt["attempts"], 1);
    assert_eq!(receipt["available"], false);
    workspace.release(&owner)?;
    fs::write(
        workspace.pm_root().join("locks/sample-own.lock"),
        crate::stringify_json(
            &json!({"created_at":crate::current_timestamp(), "author":"other"}),
            false,
        ),
    )?;
    assert!(
        matches!(workspace.claim_ranked(&request(), &options, &snapshot, 1), Err(PmRustError::OwnershipRefusal {context,..}) if context["code"]=="lock_conflict")
    );
    Ok(())
}

/// Completed containers are usable only when no actionable leaf remains.
#[test]
fn completed_containers_and_ownership_ranking_remain_actionable()
-> Result<(), Box<dyn std::error::Error>> {
    let (_directory, workspace) = tracker()?;
    let parent = item("sample-parent");
    let mut child = item("sample-child");
    child.metadata.parent = Some("sample-parent".to_owned());
    child.metadata.status = "closed".to_owned();
    let mut low = item("sample-low");
    low.metadata.extra.insert("risk".to_owned(), json!("low"));
    let options = SelectionOptions::default();
    assert_eq!(
        candidates(
            &workspace,
            &[parent.clone(), child.clone()],
            &json!({}),
            &request(),
            &options
        )?[0]
            .metadata
            .id,
        "sample-parent"
    );
    assert_eq!(
        candidates(
            &workspace,
            &[parent, child, low],
            &json!({}),
            &request(),
            &options
        )?[0]
            .metadata
            .id,
        "sample-low"
    );
    let mut first = item("sample-a");
    let mut second = item("sample-b");
    first.metadata.created_at = "2026-08-23T10:00:00Z".to_owned();
    second
        .metadata
        .extra
        .insert("assignee".to_owned(), json!("alpha"));
    let result = candidates(
        &workspace,
        &[first, second],
        &json!({}),
        &request(),
        &options,
    )?;
    assert_eq!(result.len(), 2);
    let mut decision = item("sample-decision");
    decision.metadata.item_type = "Decision".to_owned();
    let mut gate = item("sample-gate");
    gate.metadata.item_type = "Milestone".to_owned();
    let corpus = [decision, gate];
    assert_eq!(
        candidates(&workspace, &corpus, &json!({}), &request(), &options)?,
        Vec::<ItemDocument>::new()
    );
    let inclusive = SelectionOptions {
        include_decisions: true,
        include_gates: true,
        ..SelectionOptions::default()
    };
    assert_eq!(
        candidates(&workspace, &corpus, &json!({}), &request(), &inclusive)?.len(),
        2
    );
    Ok(())
}
