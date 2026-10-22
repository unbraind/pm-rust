//! Readiness and ranking invariants over a real disposable tracker.

use super::*;
use crate::{CreateItem, ItemMetadata};

thread_local! {
    /// Counts whole in-memory corpus passes on the current test thread.
    static CORPUS_SCANS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Observes actual corpus iteration without replacing documents or results.
pub(super) fn corpus_items(corpus: &[ItemDocument]) -> std::slice::Iter<'_, ItemDocument> {
    CORPUS_SCANS.with(|count| count.set(count.get() + 1));
    corpus.iter()
}

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
    let index = CorpusIndex::new(&corpus, &json!({}));
    assert_eq!(
        descendants("SAMPLE-A", &index),
        BTreeSet::from(["sample-a".to_owned(), "sample-b".to_owned()])
    );
    assert_eq!(
        blockers(&first, &index),
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
    assert!(blockers(&first, &CorpusIndex::new(&[], &json!({}))).is_empty());
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

/// Readiness indexes once and filters once, independent of corpus size and depth.
#[test]
fn selection_readiness_scans_corpus_once() -> Result<(), Box<dyn std::error::Error>> {
    let (_directory, workspace) = tracker()?;
    for size in [52, 208] {
        let mut corpus = Vec::new();
        for level in 0..size {
            let mut document = item(&format!("sample-{level:03}"));
            if level > 0 {
                document.metadata.parent = Some(format!("sample-{:03}", level - 1));
            }
            document.metadata.extra.insert(
                "dependencies".to_owned(),
                json!([
                    {"kind":"blocks", "id":format!("sample-{:03}", (level + 1) % size)}
                ]),
            );
            corpus.push(document);
        }
        let mut leaf = item("sample-leaf");
        leaf.metadata
            .extra
            .insert("blocked_by".to_owned(), json!("sample-done"));
        let mut done = item("sample-done");
        done.metadata.status = "closed".to_owned();
        corpus.extend([leaf, done]);
        let before = CORPUS_SCANS.get();
        let selected = candidates(
            &workspace,
            &corpus,
            &json!({}),
            &request(),
            &SelectionOptions::default(),
        )?;
        // One indexing pass and one candidate pass; no readiness lookup scans it.
        assert_eq!(CORPUS_SCANS.get() - before, 2);
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].metadata.id, "sample-leaf");
    }
    Ok(())
}

/// Witness propagation preserves cyclic self inclusion, terminal intermediates and cycles.
#[test]
fn readiness_index_preserves_case_cycles_and_existential_roles() {
    let mut first = item("sample-a");
    first.metadata.parent = Some("SAMPLE-B".to_owned());
    let mut second = item("sample-b");
    second.metadata.parent = Some("sample-a".to_owned());
    second.metadata.status = "closed".to_owned();
    let mut self_cycle = item("sample-self");
    self_cycle.metadata.parent = Some("sample-self".to_owned());
    let mut duplicate = item("SAMPLE-A");
    duplicate.metadata.status = "closed".to_owned();
    let mut blocker = item("sample-blocker");
    blocker.metadata.extra.insert(
        "dependencies".to_owned(),
        json!([
            {"kind":"blocks", "id":" SAMPLE-A "}, {"kind":"blocks"},
            {"kind":"related", "id":"sample-a"}
        ]),
    );
    let corpus = [first, second, self_cycle, duplicate, blocker];
    let index = CorpusIndex::new(&corpus, &json!({}));
    assert!(index.has_active_descendants("sample-a"));
    assert!(index.has_active_descendants("sample-b"));
    assert!(index.has_active_descendants("sample-self"));
    assert!(index.is_terminal("sample-a", &json!({})));
    assert!(!index.is_terminal("sample-self", &json!({})));
    assert!(!index.is_terminal("sample-absent", &json!({})));
    assert!(blockers(&corpus[0], &index).is_empty());
    assert!(index.exact_parents.contains("SAMPLE-B"));
    assert!(!index.exact_parents.contains("sample-b"));
    // Another nonterminal witness in the cycle makes both ancestors containers.
    let mut extra = item("sample-extra");
    extra.metadata.parent = Some("sample-b".to_owned());
    let mut extended = corpus.to_vec();
    extended.push(extra);
    let index = CorpusIndex::new(&extended, &json!({}));
    assert!(index.has_active_descendants("sample-a"));
    assert!(index.has_active_descendants("sample-b"));
}

thread_local! {
    /// Injects actual competing mutations after a captured ranking in this test thread only.
    static CONTEND_AFTER_RANKING: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Advances real ownership state after ranking without replacing any production result.
pub(super) fn contend_after_ranking(
    workspace: &Workspace,
    ranked: &[ItemDocument],
) -> Result<(), PmRustError> {
    let count = CONTEND_AFTER_RANKING.replace(0);
    for document in ranked.iter().take(count) {
        let mut competing = request();
        competing.id.clone_from(&document.metadata.id);
        competing.author = "competing-agent".to_owned();
        crate::mutation::ownership_item(workspace.pm_root(), &competing, true)?;
    }
    Ok(())
}

/// The actual claim-next entry point reads the corpus once, rather than once per lost race.
/// Set `PM_RUST_CONTENTION_ITEMS=50000` with nocapture for a large-tracker diagnostic benchmark.
#[test]
fn contended_claim_next_does_not_rescan_rejected_candidates()
-> Result<(), Box<dyn std::error::Error>> {
    let (_directory, workspace) = tracker()?;
    let size =
        std::env::var("PM_RUST_CONTENTION_ITEMS").map_or(Ok(1000), |raw| raw.parse::<usize>())?;
    assert!(size >= 100);
    fs::create_dir_all(workspace.pm_root().join("tasks"))?;
    fs::create_dir_all(workspace.pm_root().join("history"))?;
    let mut originals = Vec::new();
    for index in 0..size {
        let mut document = item(&format!("sample-{index:05}"));
        document.body = "Synthetic retained payload. ".repeat(32);
        let bytes = toon_format::encode_default(&document)?;
        let path = workspace
            .pm_root()
            .join(format!("tasks/{}.toon", document.metadata.id));
        fs::write(&path, &bytes)?;
        if index < 100 {
            originals.push((document.metadata.id, path, bytes));
        }
    }
    for (limit, skip, contenders) in [
        (10, false, 10),
        (10, true, 10),
        (100, false, 100),
        (100, true, 100),
        (100, false, 99),
    ] {
        for (id, path, bytes) in &originals {
            fs::write(path, bytes)?;
            let history = workspace.pm_root().join(format!("history/{id}.jsonl"));
            if history.exists() {
                fs::remove_file(history)?;
            }
        }
        let mut caller = request();
        caller.if_available = skip;
        CONTEND_AFTER_RANKING.set(contenders);
        let before = crate::ownership::tests::READ_ITEMS_SCANS.get();
        let start = std::time::Instant::now();
        let result = workspace.claim_next(
            &caller,
            &OwnershipOptions::default(),
            &SelectionOptions {
                max_attempts: Some(limit),
                ..SelectionOptions::default()
            },
        );
        let elapsed = start.elapsed();
        let reads = crate::ownership::tests::READ_ITEMS_SCANS.get() - before;
        let accepted = contenders < limit;
        assert_eq!(
            reads,
            if accepted { 2 } else { 1 },
            "contended claim --next rescanned corpus for rejected candidates"
        );
        if accepted || skip {
            let receipt = result?;
            assert_eq!(receipt["available"], accepted);
            assert_eq!(receipt["attempts"], limit);
        } else {
            assert!(
                matches!(result, Err(PmRustError::CommandRefusal {exit_code:4,context,..}) if context["code"]=="no_available_next_item")
            );
        }
        println!(
            "{}",
            json!({"benchmark":"contended claim --next", "items":size,"attempts":limit,"if_available":skip,"accepted":accepted,"corpus_reads":reads,"elapsed_ms_including_competing_writes":elapsed.as_secs_f64()*1000.0})
        );
    }
    assert!(
        !workspace
            .pm_root()
            .parent()
            .and_then(std::path::Path::parent)
            .ok_or("tracker root")?
            .join(".git")
            .exists()
    );
    Ok(())
}
