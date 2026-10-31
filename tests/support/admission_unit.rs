//! Strict admission boundaries, shared native preparation and non-writing batch semantics.
use super::*;
use serde_json::json;

/// Fallible unit-test return.
type TestResult = Result<(), Box<dyn std::error::Error>>;

/// Uses real canonical settings and native item files in a plain directory.
fn fixture() -> Result<(tempfile::TempDir, Workspace), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let root = directory.path().join(".agents/pm");
    fs::create_dir_all(root.join("tasks"))?;
    fs::write(
        root.join("settings.json"),
        include_bytes!("../fixtures/list-settings.json"),
    )?;
    Ok((directory, Workspace::discover(&root)?))
}

/// Canonical request producer whose output also passes the strict input schema.
fn batch(mutations: &Value) -> Result<AdmissionBatch, AdmissionRefusal> {
    AdmissionBatch::from_json(
        &serde_json::to_vec(&json!({"author":"fixture-agent","mutations":mutations}))
            .map_err(|_| invalid())?,
    )
}

/// Complete progressive creation, including explicit scope assertions.
fn create(id: &str) -> Value {
    json!({"op":"create","id":id,"options":{"title":"Fixture","type":"Task","status":"open","priority":2,"createMode":"progressive","parent":"none","assignee":"none","allowDuplicate":true}})
}

/// Builds a native item through the SDK rather than duplicating normalization.
fn stored(workspace: &Workspace) -> Result<ItemDocument, PmRustError> {
    workspace
        .create(CreateItem {
            id: "demo-existing".to_owned(),
            title: "Before".to_owned(),
            description: String::new(),
            item_type: "Task".to_owned(),
            status: "open".to_owned(),
            priority: 2,
            tags: vec![],
            body: String::new(),
            author: "fixture-agent".to_owned(),
            timestamp: Some("2026-10-21T00:00:00.000Z".to_owned()),
            message: None,
            provenance_role: None,
            force_stale_lock: false,
        })
        .map(|r| r.item)
}

/// A strict schema rejects extremes, duplicate keys, nulls and sparse or malformed arrays.
#[test]
fn admission_strict_input_and_limits() -> TestResult {
    let valid = batch(&json!([create("demo-new")]))?;
    assert!(batch(&json!([{"op":"delete","id":"demo-new","options":{}}])).is_err());
    for text in [
        "null",
        "[]",
        "{}",
        "{\"author\":\"one\",\"author\":\"two\"}",
        "{\"author\":\"a\",\"mutations\":[,]}",
    ] {
        assert!(AdmissionBatch::from_json(text.as_bytes()).is_err());
    }
    assert!(AdmissionBatch::from_json(&vec![b' '; ADMISSION_LIMITS.input_bytes + 1]).is_err());
    for author in [String::new(), " ".to_owned(), "a".repeat(129)] {
        let mut bad = valid.clone();
        bad.author = author;
        assert!(bad.validate().is_err());
    }
    for count in [0, 101] {
        let mut bad = valid.clone();
        bad.mutations = vec![valid.mutations[0].clone(); count];
        assert!(bad.validate().is_err());
    }
    for id in [
        String::new(),
        "x".repeat(129),
        "-first".to_owned(),
        "last-".to_owned(),
        "UPPER".to_owned(),
        "a/b".to_owned(),
        "a_b".to_owned(),
    ] {
        let mut bad = valid.clone();
        bad.mutations[0].id = id;
        assert!(bad.validate().is_err());
    }
    for value in [
        Value::Null,
        json!(-1),
        json!(256),
        json!(1.5),
        json!("Infinity"),
        json!([]),
        json!({}),
    ] {
        let mut intent = create("demo-new");
        intent["options"]["priority"] = value;
        assert!(batch(&json!([intent])).is_err());
    }
    for text in ["1e309", "NaN", "Infinity"] {
        assert!(AdmissionBatch::from_json(format!("{{\"author\":\"fixture-agent\",\"mutations\":[{{\"op\":\"update\",\"id\":\"demo-x\",\"options\":{{\"priority\":{text}}}}}]}}").as_bytes()).is_err());
    }
    for field in [
        "title",
        "description",
        "type",
        "status",
        "priority",
        "tags",
        "body",
        "createMode",
        "parent",
        "assignee",
        "allowDuplicate",
    ] {
        let mut intent = create("demo-new");
        intent["options"][field] = Value::Null;
        assert!(batch(&json!([intent])).is_err());
    }
    let mut unknown = create("demo-new");
    unknown["options"]["untrusted"] = json!(true);
    assert!(batch(&json!([unknown])).is_err());
    assert!(batch(&json!([{"op":"update","id":"demo-x","options":{"tags":[]}}])).is_err());
    assert!(serde_json::to_vec(&valid)?.len() < ADMISSION_LIMITS.input_bytes);
    assert!(refuse("test", "retry").to_string().contains("retry"));
    assert!(valid.validate().is_ok());
    Ok(())
}

/// Required fields and conflict rows fail alone; later same-target work is suppressed.
#[test]
#[allow(clippy::too_many_lines)]
fn admission_ordered_verdicts_and_required_fields() -> TestResult {
    let (_directory, workspace) = fixture()?;
    let before = stored(&workspace)?;
    let mut missing = create("demo-missing");
    missing["options"]
        .as_object_mut()
        .ok_or("options")?
        .remove("title");
    let request = batch(&json!([
        create("demo-new"), {"op":"update","id":"demo-new","options":{"title":"Revised"}},
        missing, {"op":"update","id":"demo-missing","options":{"body":"depends"}},
        {"op":"update","id":"demo-absent","options":{"title":"Missing"}},
        create("demo-existing"), {"op":"update","id":"demo-existing","options":{"title":"Before"}},
        {"op":"update","id":"demo-existing","options":{"description":"","body":""}}
    ]))?;
    let preview = workspace.preview_item_mutations(&request)?;
    assert!(!preview.ok);
    assert_eq!(
        preview
            .diagnostics
            .iter()
            .map(|r| r.state)
            .collect::<Vec<_>>(),
        [
            "admitted",
            "admitted",
            "refused",
            "skipped",
            "refused",
            "unchanged",
            "unchanged",
            "unchanged"
        ]
    );
    assert_eq!(preview.diagnostics[2].field, Some("title"));
    assert_eq!(preview.diagnostics[4].code, "item_not_found");
    assert_eq!(workspace.get("demo-existing")?, before);
    assert!(workspace.get("demo-new").is_err());
    for field in [
        "title",
        "type",
        "status",
        "priority",
        "createMode",
        "parent",
        "assignee",
        "allowDuplicate",
    ] {
        let mut intent = create("demo-new");
        intent["options"]
            .as_object_mut()
            .ok_or("options")?
            .remove(field);
        let preview = workspace.preview_item_mutations(&batch(&json!([intent]))?)?;
        assert_eq!(preview.diagnostics[0].field, Some(field));
    }
    for (field, value) in [
        ("title", json!(" \r\n ")),
        ("type", json!(" ")),
        ("createMode", json!("strict")),
        ("parent", json!("demo-parent")),
        ("assignee", json!("other")),
        ("allowDuplicate", json!(false)),
    ] {
        let mut intent = create("demo-new");
        intent["options"][field] = value;
        assert_eq!(
            workspace
                .preview_item_mutations(&batch(&json!([intent]))?)?
                .diagnostics[0]
                .code,
            "required_field"
        );
    }
    for (options, code) in [
        (json!({"title":" "}), "required_field"),
        (json!({"priority":5}), "invalid_fields"),
        (json!({}), "invalid_fields"),
        (json!({"type":"Issue"}), "unsupported_options"),
        (json!({"createMode":"progressive"}), "unsupported_options"),
        (json!({"parent":"none"}), "unsupported_options"),
        (json!({"assignee":"none"}), "unsupported_options"),
        (json!({"allowDuplicate":true}), "unsupported_options"),
    ] {
        let preview = workspace.preview_item_mutations(&batch(
            &json!([{"op":"update","id":"demo-existing","options":options}]),
        )?)?;
        assert_eq!(preview.diagnostics[0].code, code);
    }
    for (field, value) in [("priority", json!(5)), ("type", json!("Unknown"))] {
        let mut intent = create("demo-new");
        intent["options"][field] = value;
        assert_eq!(
            workspace
                .preview_item_mutations(&batch(&json!([intent]))?)?
                .diagnostics[0]
                .code,
            "invalid_fields"
        );
    }
    let mut large = batch(&json!([create("demo-new")]))?;
    large.mutations[0].options.body = Some("x".repeat(ADMISSION_LIMITS.input_bytes));
    assert_eq!(
        workspace
            .preview_item_mutations(&large)
            .err()
            .ok_or("refusal")?
            .code,
        "admission_invalid_request"
    );
    Ok(())
}

/// Native producer and preview share normalization, validation and complete documents.
#[test]
fn admission_preparation_matches_native_apply_and_producers_validate() -> TestResult {
    let (_directory, workspace) = fixture()?;
    let before = stored(&workspace)?;
    let (source, _) = Source::load(workspace.pm_root())?;
    let request = batch(
        &json!([create("demo-new"),{"op":"update","id":"demo-existing","options":{"title":" After ","description":" \r\n detail \r\n","status":"in_progress","priority":4,"tags":" B,a,B ","body":" \r\n body  "}}]),
    )?;
    for intent in &request.mutations {
        let mut prepared =
            prepare(&source, intent, &request.author, Some(&before)).map_err(|e| e.0)?;
        let actual = match intent.op {
            AdmissionKind::Create => {
                workspace
                    .create(CreateItem {
                        id: intent.id.clone(),
                        title: "Fixture".to_owned(),
                        description: String::new(),
                        item_type: "Task".to_owned(),
                        status: "open".to_owned(),
                        priority: 2,
                        tags: vec![],
                        body: String::new(),
                        author: request.author.clone(),
                        timestamp: Some(prepared.metadata.updated_at.clone()),
                        message: None,
                        provenance_role: None,
                        force_stale_lock: false,
                    })?
                    .item
            }
            _ => {
                workspace
                    .update(UpdateItem {
                        id: intent.id.clone(),
                        title: Some(" After ".to_owned()),
                        description: Some(" \r\n detail \r\n".to_owned()),
                        status: Some("in_progress".to_owned()),
                        priority: Some(4),
                        tags: Some(vec![" B".to_owned(), "a".to_owned(), "B ".to_owned()]),
                        body: Some(" \r\n body  ".to_owned()),
                        author: request.author.clone(),
                        timestamp: Some(prepared.metadata.updated_at.clone()),
                        message: None,
                        provenance_role: None,
                        force_stale_lock: false,
                    })?
                    .item
            }
        };
        prepared
            .metadata
            .updated_at
            .clone_from(&actual.metadata.updated_at);
        assert_eq!(prepared, actual);
        assert_eq!(workspace.get(&intent.id)?, actual);
        assert_eq!(
            crate::item::decode_item(
                Path::new("producer.toon"),
                &crate::mutation::canonical_item_bytes(&actual)?
            )?,
            actual
        );
    }
    let mut unsupported = request.mutations[0].clone();
    unsupported.op = AdmissionKind::Unsupported;
    assert_eq!(
        prepare(&source, &unsupported, "fixture-agent", None)
            .err()
            .ok_or("refusal")?
            .0,
        "unsupported_intent"
    );
    let mut source = Source::default();
    assert_eq!(
        prepare(&source, &request.mutations[0], "fixture-agent", None)
            .err()
            .ok_or("refusal")?
            .0,
        "invalid_schema"
    );
    source.lifecycle = Some(crate::list_registry::Lifecycle::from_values(
        &json!({}),
        &Value::Null,
        &Value::Null,
    ));
    assert_eq!(
        prepare(&source, &request.mutations[0], "fixture-agent", None)
            .err()
            .ok_or("refusal")?
            .0,
        "invalid_schema"
    );
    Ok(())
}

/// Runtime roles, rather than status spelling, bound lifecycle admission.
#[test]
fn admission_custom_registry_and_schema_refusals() -> TestResult {
    let (_directory, workspace) = fixture()?;
    let mut before = stored(&workspace)?;
    for status in ["unknown", "OPEN", "closed", "blocked", ""] {
        let mut intent = create("demo-new");
        intent["options"]["status"] = json!(status);
        assert_eq!(
            workspace
                .preview_item_mutations(&batch(&json!([intent]))?)?
                .diagnostics[0]
                .code,
            "unsupported_status"
        );
    }
    fs::create_dir_all(workspace.pm_root().join("schema"))?;
    fs::write(workspace.pm_root().join("schema/statuses.json"),json!({"statuses":[{"id":"closed","roles":["active"]},{"id":"work","roles":["terminal_done"]},{"id":"halt","roles":["blocked"]}]}).to_string())?;
    let mut intent = create("demo-new");
    intent["options"]["status"] = json!("closed");
    assert_eq!(
        workspace
            .preview_item_mutations(&batch(&json!([intent]))?)?
            .diagnostics[0]
            .code,
        "close_reason_required"
    );
    for status in ["work", "halt"] {
        let mut intent = create("demo-new");
        intent["options"]["status"] = json!(status);
        assert_eq!(
            workspace
                .preview_item_mutations(&batch(&json!([intent]))?)?
                .diagnostics[0]
                .code,
            "unsupported_status"
        );
    }
    for (status, extra) in [
        ("work", None),
        ("halt", None),
        ("open", Some("dependencies")),
        ("open", Some("blocked_by")),
    ] {
        before.metadata.status = status.to_owned();
        before.metadata.extra.clear();
        if let Some(key) = extra {
            before.metadata.extra.insert(key.to_owned(), json!([]));
        }
        let (source, _) = Source::load(workspace.pm_root())?;
        let request =
            batch(&json!([{"op":"update","id":"demo-existing","options":{"status":"open"}}]))?;
        assert_eq!(
            prepare(
                &source,
                &request.mutations[0],
                "fixture-agent",
                Some(&before)
            )
            .err()
            .ok_or("refusal")?
            .0,
            "unsupported_transition"
        );
    }
    for (name, key) in [
        ("fields", "fields"),
        ("types", "definitions"),
        ("workflows", "type_workflows"),
    ] {
        let file = workspace.pm_root().join(format!("schema/{name}.json"));
        fs::write(&file, json!({key:[{"key":"custom"}]}).to_string())?;
        assert_eq!(
            workspace
                .preview_item_mutations(&batch(&json!([create("demo-new")]))?)?
                .diagnostics[0]
                .code,
            "unsupported_schema"
        );
        fs::remove_file(file)?;
    }
    fs::write(workspace.pm_root().join("schema/fields.json"), "[]")?;
    assert!(
        workspace
            .preview_item_mutations(&batch(&json!([create("demo-new")]))?)?
            .ok
    );
    fs::write(
        workspace.pm_root().join("schema/fields.json"),
        "[{\"key\":\"custom\"}]",
    )?;
    assert!(
        !workspace
            .preview_item_mutations(&batch(&json!([create("demo-new")]))?)?
            .ok
    );
    Ok(())
}

/// Source safety, byte/row/depth limits and preexisting data remain observable.
#[test]
fn admission_source_boundaries() -> TestResult {
    let (_directory, workspace) = fixture()?;
    let mut source = Source::default();
    for path in ["../outside", "/outside", "schema/../outside"] {
        assert!(source.read(workspace.pm_root(), Path::new(path)).is_err());
    }
    assert!(
        source
            .read(workspace.pm_root(), Path::new("missing"))
            .is_err()
    );
    assert!(
        source
            .read(workspace.pm_root(), Path::new("tasks"))
            .is_err()
    );
    assert!(
        source
            .json(workspace.pm_root(), Path::new("settings.json"))
            .is_ok()
    );
    assert!(
        source
            .walk(workspace.pm_root(), Path::new("tasks"), 33)
            .is_err()
    );
    source.entries = ADMISSION_LIMITS.source_entries;
    assert!(source.walk(workspace.pm_root(), Path::new(""), 0).is_err());
    let huge = workspace.pm_root().join("huge");
    fs::File::create(&huge)?.set_len(ADMISSION_LIMITS.source_bytes + 1)?;
    assert_eq!(
        Source::default()
            .read(workspace.pm_root(), Path::new("huge"))
            .err()
            .ok_or("refusal")?
            .code,
        "admission_source_limit"
    );
    fs::remove_file(huge)?;
    fs::write(workspace.pm_root().join("tasks/bad.toon"), [0xff])?;
    assert!(Source::load(workspace.pm_root()).is_err());
    fs::write(
        workspace.pm_root().join("tasks/bad.toon"),
        "title: Missing core fields\n",
    )?;
    assert!(Source::load(workspace.pm_root()).is_err());
    fs::remove_file(workspace.pm_root().join("tasks/bad.toon"))?;
    stored(&workspace)?;
    fs::copy(
        workspace.pm_root().join("tasks/demo-existing.toon"),
        workspace.pm_root().join("tasks/duplicate.toon"),
    )?;
    assert!(Source::load(workspace.pm_root()).is_err());
    fs::remove_file(workspace.pm_root().join("tasks/duplicate.toon"))?;
    let mut source = Source::default();
    let existing = workspace.get("demo-existing")?;
    for i in 0..ADMISSION_LIMITS.source_rows {
        source.items.insert(i.to_string(), existing.clone());
    }
    assert_eq!(
        source
            .walk(workspace.pm_root(), Path::new("tasks"), 1)
            .err()
            .ok_or("refusal")?
            .code,
        "admission_source_limit"
    );
    fs::write(workspace.pm_root().join("settings.json"), "{")?;
    assert!(Source::load(workspace.pm_root()).is_err());
    fs::write(
        workspace.pm_root().join("settings.json"),
        json!({"schema":{"files":{"fields":"../outside"}}}).to_string(),
    )?;
    assert!(Source::load(workspace.pm_root()).is_err());
    Ok(())
}

/// Creates a directory link using the existing native test policy on each OS.
fn directory_link(target: &Path, link: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(target, link)
    }
    #[cfg(windows)]
    {
        let output = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(link)
            .arg(target)
            .output()?;
        if !output.status.success() {
            return Err(std::io::Error::other("directory junction fixture failed"));
        }
        Ok(())
    }
}

/// The Windows pin denies only ancestor replacement: child links and a leaf rename replacement proceed.
#[cfg(windows)]
#[test]
fn windows_source_pin_allows_child_links_and_leaf_replacement() -> TestResult {
    use std::os::windows::fs::OpenOptionsExt as _;
    let directory = tempfile::tempdir()?;
    let tasks = directory.path().join("tasks");
    fs::create_dir(&tasks)?;
    let leaf = tasks.join("item.toon");
    fs::write(&leaf, b"original")?;
    let ancestor = fs::OpenOptions::new()
        .read(true)
        .share_mode(PIN_DIRECTORY_SHARE)
        .custom_flags(0x0220_0000)
        .open(&tasks)?;
    fs::write(directory.path().join("staged"), b"linked")?;
    fs::hard_link(directory.path().join("staged"), tasks.join("linked.toon"))?;
    assert!(
        fs::rename(&tasks, directory.path().join("moved")).is_err(),
        "a pinned ancestor cannot be renamed"
    );
    drop(ancestor);
    let mut pinned = open_source(directory.path(), Path::new("tasks/item.toon"))?;
    fs::write(directory.path().join("replacement"), b"replacement")?;
    fs::rename(directory.path().join("replacement"), &leaf)?;
    let mut bytes = Vec::new();
    pinned.read_to_end(&mut bytes)?;
    assert_eq!(bytes, b"original");
    assert_eq!(fs::read(&leaf)?, b"replacement");
    Ok(())
}

/// Real Unix links and Windows junctions cannot add external source payloads.
#[test]
fn admission_refuses_links_and_special_files() -> TestResult {
    let (directory, workspace) = fixture()?;
    let outside = directory.path().join("external");
    fs::create_dir(&outside)?;
    fs::write(outside.join("fields.json"), "{}")?;
    directory_link(&outside, &workspace.pm_root().join("tasks/link"))?;
    assert!(Source::load(workspace.pm_root()).is_err());
    assert!(
        Source::default()
            .read(workspace.pm_root(), Path::new("tasks/link/fields.json"))
            .is_err()
    );
    directory_link(&outside, &workspace.pm_root().join("schema"))?;
    assert!(Source::load(workspace.pm_root()).is_err());
    #[cfg(unix)]
    {
        let (_directory, workspace) = fixture()?;
        fs::create_dir(workspace.pm_root().join("schema"))?;
        std::os::unix::fs::symlink("missing", workspace.pm_root().join("schema/fields.json"))?;
        assert!(Source::load(workspace.pm_root()).is_err());
    }
    Ok(())
}

/// The shared native history reader refuses directory aliases on every supported OS.
#[test]
fn admission_shared_history_reader_refuses_directory_aliases() -> TestResult {
    let (directory, workspace) = fixture()?;
    let document = stored(&workspace)?;
    let history = workspace.pm_root().join("history");
    let retained = directory.path().join("retained-history");
    let original = fs::read(history.join("demo-existing.jsonl"))?;
    fs::rename(&history, &retained)?;
    directory_link(&retained, &history)?;
    let adapter = crate::changes::LocalChangeAdapter::new(
        workspace.clone(),
        crate::SessionCancellation::default(),
    );
    assert!(matches!(
        adapter.pull(&crate::changes::ChangeRequest::default()),
        Err(crate::changes::ChangeRefusal::Source)
    ));
    assert_eq!(workspace.get("demo-existing")?, document);
    assert_eq!(fs::read(retained.join("demo-existing.jsonl"))?, original);
    Ok(())
}

/// Five hundred seeded operation sequences compare preview with actual native apply verdicts.
#[test]
fn admission_seeded_preview_equals_native_apply() -> TestResult {
    use proptest::{
        prelude::*,
        test_runner::{Config, RngSeed, TestRunner},
    };
    let (_directory, workspace) = fixture()?;
    stored(&workspace)?;
    let strategy = prop::collection::vec(("[ a-zA-Z0-9]{0,16}", 0_u8..7, any::<bool>()), 2..5);
    let mut runner = TestRunner::new(Config {
        cases: 500,
        rng_seed: RngSeed::Fixed(20_261_030),
        failure_persistence: None,
        ..Config::default()
    });
    runner.run(&strategy, |operations| {
        for (title, priority, body) in operations {
            let options = if body {
                json!({"body":format!(" \r\n{title}\r\n "),"priority":priority})
            } else {
                json!({"title":title,"priority":priority})
            };
            let batch = batch(&json!([{"op":"update","id":"demo-existing","options":options}]))
                .map_err(|e| TestCaseError::fail(e.to_string()))?;
            let verdict = workspace
                .preview_item_mutations(&batch)
                .map_err(|e| TestCaseError::fail(e.to_string()))?;
            let result = workspace.update(UpdateItem {
                id: "demo-existing".to_owned(),
                title: if body { None } else { Some(title.clone()) },
                description: None,
                status: None,
                priority: Some(priority),
                tags: None,
                body: if body {
                    Some(format!(" \r\n{title}\r\n "))
                } else {
                    None
                },
                author: "fixture-agent".to_owned(),
                timestamp: None,
                message: None,
                provenance_role: None,
                force_stale_lock: false,
            });
            prop_assert_eq!(verdict.ok, result.is_ok());
            if let Ok(result) = result {
                // The reference model adopts the producer's result, then requires SDK read-back.
                prop_assert_eq!(
                    workspace
                        .get("demo-existing")
                        .map_err(|e| TestCaseError::fail(e.to_string()))?,
                    result.item
                );
            }
        }
        Ok(())
    })?;
    Ok(())
}

thread_local! {
    /// Monotonic observation boundary count within one unit-test thread.
    static STEP: std::cell::Cell<usize> = const {std::cell::Cell::new(0)};
    /// A competing writer fires at exactly this observation boundary.
    static INJECT_AT: std::cell::Cell<usize> = const {std::cell::Cell::new(usize::MAX)};
    /// One-shot directory replacement exactly between the path check and open.
    static INJECT_PARENT_SWAP: std::cell::RefCell<Option<(PathBuf, PathBuf, PathBuf)>> = const {std::cell::RefCell::new(None)};
    /// Optional real replacement for aliased-schema interleavings.
    static INJECT_REPLACEMENT: std::cell::RefCell<Option<(PathBuf, Vec<u8>)>> = const {std::cell::RefCell::new(None)};
}

/// A genuine competing file replacement; no SDK or filesystem mocks.
pub(super) fn checkpoint(root: &Path) {
    INJECT_PARENT_SWAP.with(|swap| {
        if let Some((parent, retained, target)) = swap.take() {
            assert!(fs::rename(&parent, &retained).is_ok());
            assert!(directory_link(&target, &parent).is_ok());
        }
    });
    STEP.with(|step| {
        let next = step.get() + 1;
        step.set(next);
        if INJECT_AT.with(std::cell::Cell::get) == next {
            if INJECT_REPLACEMENT.with(|replacement| {
                replacement.borrow().as_ref().is_some_and(|(path, bytes)| {
                    assert!(fs::write(root.join(path), bytes).is_ok());
                    true
                })
            }) {
                return;
            }
            // Mutation retains valid JSON but changes source bytes; the final observation must refuse.
            let path = root.join("settings.json");
            if let Ok(mut bytes) = fs::read(&path) {
                if INJECT_INVALID.with(std::cell::Cell::get) {
                    bytes = b"{".to_vec();
                } else {
                    bytes.push(b' ');
                }
                assert!(fs::write(path, bytes).is_ok());
            }
        }
    });
}

/// Every observed transition boundary either sees the new state initially or refuses change.
#[test]
fn admission_interleaving_at_every_read_and_candidate_boundary() -> TestResult {
    let (_directory, workspace) = fixture()?;
    stored(&workspace)?;
    let path = workspace.pm_root().join("settings.json");
    let original = fs::read(&path)?;
    let request = batch(
        &json!([create("demo-new"),{"op":"update","id":"demo-existing","options":{"title":"Updated"}}]),
    )?;
    STEP.with(|step| step.set(0));
    assert!(workspace.preview_item_mutations(&request)?.ok);
    let steps = STEP.with(std::cell::Cell::get);
    assert!(steps >= 10);
    for boundary in 1..=steps {
        fs::write(&path, &original)?;
        STEP.with(|step| step.set(0));
        INJECT_AT.with(|step| step.set(boundary));
        let result = workspace.preview_item_mutations(&request);
        INJECT_AT.with(|step| step.set(usize::MAX));
        if boundary == 1 || boundary >= steps - 2 {
            assert!(result?.ok);
        } else {
            assert_eq!(
                result
                    .err()
                    .ok_or("expected visible interleaving refusal")?
                    .code,
                "admission_source_changed"
            );
        }
        assert_eq!(workspace.get("demo-existing")?.metadata.title, "Before");
        assert!(workspace.get("demo-new").is_err());
    }
    fs::write(path, original)?;
    Ok(())
}

/// Fuzz both the strict boundary and its own producer; no accepted numbers become nonfinite.
#[test]
fn admission_boundary_fuzz_and_output_limits() -> TestResult {
    use proptest::{
        prelude::*,
        test_runner::{Config, RngSeed, TestRunner},
    };
    let mut runner = TestRunner::new(Config {
        cases: 500,
        rng_seed: RngSeed::Fixed(20_261_031),
        failure_persistence: None,
        ..Config::default()
    });
    runner.run(&prop::collection::vec(any::<u8>(), 0..2048), |bytes| {
        if let Ok(request) = AdmissionBatch::from_json(&bytes) {
            let emitted =
                serde_json::to_vec(&request).map_err(|e| TestCaseError::fail(e.to_string()))?;
            prop_assert!(AdmissionBatch::from_json(&emitted).is_ok());
        }
        Ok(())
    })?;
    let (_directory, workspace) = fixture()?;
    let intents = (0..100)
        .map(|i| json!({"op":"create","id":format!("demo-{}-{i}","x".repeat(110)),"options":{}}))
        .collect::<Vec<_>>();
    let preview = workspace.preview_item_mutations(&batch(&json!(intents))?)?;
    let output = crate::ListOutput::from(serde_json::json!(&preview));
    assert!(crate::stringify_json(&preview, false).len() <= ADMISSION_LIMITS.output_bytes);
    assert!(crate::stringify_toon(&output)?.len() <= ADMISSION_LIMITS.output_bytes);
    assert_eq!(preview.diagnostics.len(), 100);
    let example = AdmissionBatch::from_json(include_bytes!("../../examples/item-admission.json"))?;
    assert!(workspace.preview_item_mutations(&example)?.ok);
    Ok(())
}

/// Read/enumeration faults and growth are refused through the same bounded source helpers.
#[test]
fn admission_io_growth_and_render_boundaries() -> TestResult {
    /// A stream that fails after admission, representing a source read fault.
    struct ReadFailure;
    impl Read for ReadFailure {
        fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
            Err(std::io::Error::other("read failure"))
        }
    }
    let (_directory, workspace) = fixture()?;
    let mut source = Source::default();
    assert_eq!(
        source
            .read_bytes(&mut ReadFailure)
            .err()
            .ok_or("refusal")?
            .code,
        "admission_source"
    );
    source.bytes = ADMISSION_LIMITS.source_bytes - 1;
    assert_eq!(
        source
            .read_bytes(&mut &b"ab"[..])
            .err()
            .ok_or("growth refusal")?
            .code,
        "admission_source_limit"
    );
    let mut source = Source::default();
    assert!(
        source
            .walk(workspace.pm_root(), Path::new("missing"), 1)
            .is_err()
    );
    assert!(
        source
            .walk_entries(
                workspace.pm_root(),
                Path::new(""),
                0,
                &mut [Err(std::io::Error::other("enumeration failure"))].into_iter()
            )
            .is_err()
    );
    let entry = fs::read_dir(workspace.pm_root())?.next().ok_or("entry")??;
    assert!(
        source
            .visit_entry(
                workspace.pm_root(),
                Path::new(""),
                1,
                &entry,
                Err(std::io::Error::other("entry type failure"))
            )
            .is_err()
    );
    std::io::Write::flush(&mut InputCounter::default())?;
    for (json, toon) in [(0, 0), (65_536, 65_536), (65_537, 0), (0, 65_537)] {
        assert_eq!(
            check_output_lengths(json, toon).is_ok(),
            json <= 65_536 && toon <= 65_536
        );
    }
    Ok(())
}

/// Candidate encoders and existing-target adoption preserve explicit diagnostics.
#[test]
fn admission_candidate_encoding_and_adoption() -> TestResult {
    let (_directory, workspace) = fixture()?;
    assert_eq!(
        native_error(&PmRustError::ItemEncoding {
            reason: "encoder refusal".to_owned()
        })
        .0,
        "invalid_document"
    );
    let mut intent = create("demo-new");
    intent["options"]["description"] = json!(" \r\n description ");
    intent["options"]["body"] = json!(" \r\n body ");
    intent["options"]["tags"] = json!(" B,a,B ");
    let request = batch(&json!([intent]))?;
    assert!(workspace.preview_item_mutations(&request)?.ok);
    let (mut source, _) = Source::load(workspace.pm_root())?;
    source
        .files
        .insert(PathBuf::from("settings.json"), b"null".to_vec());
    assert_eq!(
        prepare(&source, &request.mutations[0], "fixture-agent", None)
            .err()
            .ok_or("bad settings")?
            .0,
        "invalid_fields"
    );
    let mut item = stored(&workspace)?;
    for reason in [
        Value::Null,
        json!("Ordinary closure"),
        json!("Compensated: interrupted bulk transaction example"),
    ] {
        item.metadata
            .extra
            .insert("close_reason".to_owned(), reason.clone());
        let (mut source, _) = Source::load(workspace.pm_root())?;
        source.items.insert("demo-new".to_owned(), item.clone());
        let (rows, ok) = evaluate(&source, &request, false);
        assert_eq!(
            ok,
            !reason
                .as_str()
                .is_some_and(|s| s.starts_with("Compensated: interrupted bulk transaction "))
        );
        assert_eq!(rows[0].state, if ok { "unchanged" } else { "refused" });
    }
    Ok(())
}

/// Malformed configuration and every optional-source outcome refuse conservatively.
#[test]
fn admission_contract_configuration_boundaries() -> TestResult {
    let (_directory, workspace) = fixture()?;
    assert!(
        optional_source(Err(std::io::Error::from(
            std::io::ErrorKind::PermissionDenied
        )))
        .is_err()
    );
    assert!(!optional_source(Err(std::io::Error::from(
        std::io::ErrorKind::NotFound
    )))?);
    assert!(optional_source(fs::metadata(workspace.pm_root()))?);
    for value in [Value::Null, json!([]), json!([{}]), json!({}), json!(false)] {
        assert_eq!(
            declares_contract(&value),
            !value.is_null() && value.as_array().is_none_or(|a| !a.is_empty())
        );
    }
    fs::write(workspace.pm_root().join("tasks/ignored.txt"), "ignored")?;
    fs::write(workspace.pm_root().join("tasks/extensionless"), "ignored")?;
    let settings = workspace.pm_root().join("settings.json");
    let original = fs::read(&settings)?;
    for bytes in [
        b"null".as_slice(),
        b"{\"locks\":{\"ttl_seconds\":\"bad\"}}".as_slice(),
    ] {
        fs::write(&settings, bytes)?;
        assert!(Source::load(workspace.pm_root()).is_err());
    }
    fs::write(&settings, &original)?;
    let mut changed: Value = serde_json::from_slice(&original)?;
    changed["schema"]["fields"] = json!({"unexpected":"declaration"});
    fs::write(&settings, serde_json::to_vec(&changed)?)?;
    assert_eq!(
        workspace
            .preview_item_mutations(&batch(&json!([create("demo-new")]))?)?
            .diagnostics[0]
            .code,
        "unsupported_schema"
    );
    fs::write(&settings, &original)?;
    fs::create_dir_all(workspace.pm_root().join("schema"))?;
    let fields = workspace.pm_root().join("schema/fields.json");
    for value in [Value::Null, json!(3), json!("invalid")] {
        fs::write(&fields, serde_json::to_vec(&value)?)?;
        assert!(Source::load(workspace.pm_root()).is_err());
    }
    fs::write(&fields, "{\"fields\":{\"invalid\":true}}")?;
    assert_eq!(
        workspace
            .preview_item_mutations(&batch(&json!([create("demo-new")]))?)?
            .diagnostics[0]
            .code,
        "unsupported_schema"
    );
    fs::write(&fields, "{\"fields\":[]}")?;
    fs::write(
        workspace.pm_root().join("schema/statuses.json"),
        "{\"statuses\":[{\"id\":\"roleless\",\"roles\":[]},{\"id\":\"draft_custom\",\"roles\":[\"draft\"]}]}",
    )?;
    for (status, admitted) in [("roleless", false), ("draft_custom", true)] {
        let mut intent = create("demo-new");
        intent["options"]["status"] = json!(status);
        assert_eq!(
            workspace
                .preview_item_mutations(&batch(&json!([intent]))?)?
                .ok,
            admitted
        );
    }
    Ok(())
}

thread_local! {
    /// Inject a recoverable schema metadata fault after a real filesystem probe.
    static SCHEMA_FAULT: std::cell::Cell<bool> = const {std::cell::Cell::new(false)};
    /// Replace configuration with malformed JSON at one competing-writer boundary.
    static INJECT_INVALID: std::cell::Cell<bool> = const {std::cell::Cell::new(false)};
}

/// Preserves real metadata except for one explicitly requested I/O-failure scenario.
pub(super) fn schema_metadata(
    result: std::io::Result<fs::Metadata>,
) -> std::io::Result<fs::Metadata> {
    if SCHEMA_FAULT.with(std::cell::Cell::get) {
        Err(std::io::ErrorKind::PermissionDenied.into())
    } else {
        result
    }
}

/// Public SDK callers receive failures from every phase without partial diagnostics.
#[test]
fn admission_public_error_propagation() -> TestResult {
    let (_directory, workspace) = fixture()?;
    let valid = batch(&json!([create("demo-new")]))?;
    let mut invalid = valid.clone();
    invalid.author.clear();
    assert!(workspace.preview_item_mutations(&invalid).is_err());
    let settings = workspace.pm_root().join("settings.json");
    let original = fs::read(&settings)?;
    fs::write(&settings, "{")?;
    assert!(workspace.preview_item_mutations(&valid).is_err());
    fs::write(&settings, &original)?;
    SCHEMA_FAULT.with(|flag| flag.set(true));
    let result = workspace.preview_item_mutations(&valid);
    SCHEMA_FAULT.with(|flag| flag.set(false));
    assert_eq!(result.err().ok_or("schema fault")?.code, "admission_source");
    let item = stored(&workspace)?;
    let file = workspace.pm_root().join("tasks/demo-existing.toon");
    let entry = fs::read_dir(workspace.pm_root().join("tasks"))?
        .next()
        .ok_or("entry")??;
    let kind = entry.file_type();
    fs::remove_file(file)?;
    assert!(
        Source::default()
            .visit_entry(workspace.pm_root(), Path::new("tasks"), 1, &entry, kind)
            .is_err()
    );
    assert_eq!(item.metadata.id, "demo-existing");
    STEP.with(|step| step.set(0));
    assert!(workspace.preview_item_mutations(&valid)?.ok);
    let steps = STEP.with(std::cell::Cell::get);
    // The final observation has just the two settings-file checkpoints now.
    STEP.with(|step| step.set(0));
    INJECT_AT.with(|step| step.set(steps - 2));
    INJECT_INVALID.with(|flag| flag.set(true));
    let result = workspace.preview_item_mutations(&valid);
    INJECT_AT.with(|step| step.set(usize::MAX));
    INJECT_INVALID.with(|flag| flag.set(false));
    fs::write(&settings, original)?;
    assert_eq!(
        result.err().ok_or("final observation fault")?.code,
        "admission_source"
    );
    Ok(())
}

/// Preview helpers retain apply's author and timestamp validation before producing a document.
#[test]
fn admission_native_preview_clock_and_author_refusals() -> TestResult {
    let (_directory, workspace) = fixture()?;
    let before = stored(&workspace)?;
    for (author, timestamp) in [("", None), ("fixture-agent", Some("invalid".to_owned()))] {
        let request = UpdateItem {
            id: "demo-existing".to_owned(),
            title: Some("Updated".to_owned()),
            description: None,
            status: None,
            priority: None,
            tags: None,
            body: None,
            author: author.to_owned(),
            timestamp,
            message: None,
            provenance_role: None,
            force_stale_lock: false,
        };
        assert!(
            crate::mutation::prepare_update_preview(
                &fs::read(workspace.pm_root().join("settings.json"))?,
                &crate::list_registry::Lifecycle::load(workspace.pm_root())?,
                &before,
                request
            )
            .is_err()
        );
    }
    let request = UpdateItem {
        id: "demo-existing".into(),
        title: Some("Title".into()),
        description: None,
        status: None,
        priority: None,
        tags: None,
        body: None,
        author: "fixture-agent".into(),
        timestamp: None,
        message: None,
        provenance_role: None,
        force_stale_lock: false,
    };
    assert!(
        crate::mutation::prepare_update_preview(
            b"{",
            &crate::list_registry::Lifecycle::load(workspace.pm_root())?,
            &before,
            request
        )
        .is_err()
    );
    assert_eq!(before, workspace.get("demo-existing")?);
    Ok(())
}

/// Random policies, principals and workflow roles cannot bypass shared apply guards.
#[test]
fn admission_governance_matches_apply_for_every_scalar_field() -> TestResult {
    let (_directory, workspace) = fixture()?;
    let original = stored(&workspace)?;
    let settings_path = workspace.pm_root().join("settings.json");
    let base: Value = serde_json::from_slice(&fs::read(&settings_path)?)?;
    let path = workspace.pm_root().join("tasks/demo-existing.toon");
    let mut seed = 0x0132_2026_1029_u64;
    for index in 0..500 {
        seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
        let preset = ["strict", "minimal", "custom", "default"][index % 4];
        let strict = preset == "strict" || preset == "custom";
        let owner = ["", "fixture-agent", "other-agent", "  other-agent  "]
            [usize::try_from((seed >> 8) % 4)?];
        let principal =
            ["", "fixture-agent", "other-principal", "  "][usize::try_from((seed >> 16) % 4)?];
        let mut settings = base.clone();
        settings["governance"] =
            json!({"preset":preset,"ownership_enforcement":"strict","require_close_reason":true});
        let status = format!("stage_{index}");
        settings["schema"]["workflow"]["close_status"] = json!(status);
        fs::write(&settings_path, serde_json::to_vec(&settings)?)?;
        fs::create_dir_all(workspace.pm_root().join("schema"))?;
        fs::write(workspace.pm_root().join("schema/statuses.json"), json!({"statuses":[{"id":status,"roles":[if index % 2 == 0 {"active"} else {"draft"}]}]}).to_string())?;
        fs::write(
            workspace.pm_root().join("schema/workflows.json"),
            json!({"workflow":settings["schema"]["workflow"]}).to_string(),
        )?;
        let mut before = original.clone();
        before
            .metadata
            .extra
            .insert("assignee".into(), json!(owner));
        before
            .metadata
            .extra
            .insert("claim_principal".into(), json!(principal));
        fs::write(&path, crate::mutation::canonical_item_bytes(&before)?)?;
        let fields = [
            json!({"title":"Changed"}),
            json!({"description":"Changed"}),
            json!({"body":"Changed"}),
            json!({"priority":0}),
            json!({"tags":"one,two"}),
            json!({"status":"in_progress"}),
        ];
        let options = fields[index % fields.len()].clone();
        let intent = batch(&json!([{"op":"update","id":"demo-existing","options":options}]))?;
        let preview = workspace.preview_item_mutations(&intent)?;
        let held = if principal.trim().is_empty() {
            owner.trim()
        } else {
            principal.trim()
        };
        let denied = strict && !owner.trim().is_empty() && held != "fixture-agent";
        assert_eq!(preview.ok, !denied, "case {index}");
        let request = UpdateItem {
            id: "demo-existing".into(),
            title: intent.mutations[0].options.title.clone(),
            description: intent.mutations[0].options.description.clone(),
            body: intent.mutations[0].options.body.clone(),
            priority: intent.mutations[0].options.priority,
            tags: intent.mutations[0]
                .options
                .tags
                .as_ref()
                .map(|s| s.split(',').map(str::to_owned).collect()),
            status: intent.mutations[0].options.status.clone(),
            author: "fixture-agent".into(),
            timestamp: None,
            message: None,
            provenance_role: None,
            force_stale_lock: false,
        };
        assert_eq!(
            workspace.update(request).is_ok(),
            preview.ok,
            "apply {index}"
        );
        if denied {
            assert_eq!(workspace.get("demo-existing")?, before);
        }
        fs::write(&path, crate::mutation::canonical_item_bytes(&before)?)?;
        let comment = crate::CommentItem {
            id: "demo-existing".into(),
            text: "Annotation".into(),
            author: "fixture-agent".into(),
            timestamp: None,
            message: None,
            provenance_role: None,
            force_stale_lock: false,
        };
        assert_eq!(
            workspace.comment(&comment).is_ok(),
            !denied,
            "comment {index}"
        );
        assert_closure_readiness(&workspace, &before, &status, &mut settings)?;
    }
    Ok(())
}

/// Registry I/O failures in native apply cannot bypass preparation or write state.
#[test]
fn admission_apply_registry_failures_preserve_state() -> TestResult {
    let (_directory, workspace) = fixture()?;
    let before = stored(&workspace)?;
    let history = fs::read(workspace.pm_root().join("history/demo-existing.jsonl"))?;
    fs::create_dir_all(workspace.pm_root().join("schema/statuses.json"))?;
    let create = CreateItem {
        id: "demo-new".into(),
        title: "Fixture".into(),
        description: String::new(),
        item_type: "Task".into(),
        status: "open".into(),
        priority: 2,
        tags: vec![],
        body: String::new(),
        author: "fixture-agent".into(),
        timestamp: None,
        message: None,
        provenance_role: None,
        force_stale_lock: false,
    };
    assert!(workspace.create(create).is_err());
    let update = UpdateItem {
        id: "demo-existing".into(),
        title: Some("Changed".into()),
        description: None,
        status: None,
        priority: None,
        tags: None,
        body: None,
        author: "fixture-agent".into(),
        timestamp: None,
        message: None,
        provenance_role: None,
        force_stale_lock: false,
    };
    assert!(workspace.update(update).is_err());
    assert_eq!(workspace.get("demo-existing")?, before);
    assert_eq!(
        fs::read(workspace.pm_root().join("history/demo-existing.jsonl"))?,
        history
    );
    assert!(!workspace.pm_root().join("tasks/demo-new.toon").exists());
    Ok(())
}

/// Exercises closure readiness against both captured preparation and native SDK apply.
fn assert_closure_readiness(
    workspace: &Workspace,
    before: &ItemDocument,
    status: &str,
    settings: &mut Value,
) -> TestResult {
    let path = workspace.pm_root().join("tasks/demo-existing.toon");
    // Closing by scalar update has the same requirement as creation.
    let mut unassigned = before.clone();
    unassigned.metadata.extra.remove("assignee");
    fs::write(&path, crate::mutation::canonical_item_bytes(&unassigned)?)?;
    let closing_update =
        batch(&json!([{"op":"update","id":"demo-existing","options":{"status":status}}]))?;
    assert_eq!(
        workspace
            .preview_item_mutations(&closing_update)?
            .diagnostics[0]
            .code,
        "close_reason_required"
    );
    let mut closing = create("demo-new");
    closing["options"]["status"] = json!(status);
    let preview = workspace.preview_item_mutations(&batch(&json!([closing]))?)?;
    assert_eq!(preview.diagnostics[0].code, "close_reason_required");
    let (source, _) = Source::load(workspace.pm_root())?;
    let mut request = CreateItem {
        id: "demo-new".into(),
        title: "Fixture".into(),
        description: String::new(),
        item_type: "Task".into(),
        status: status.to_owned(),
        priority: 2,
        tags: vec![],
        body: String::new(),
        author: "fixture-agent".into(),
        timestamp: None,
        message: None,
        provenance_role: None,
        force_stale_lock: false,
    };
    assert!(
        crate::mutation::prepare_create_preview(
            b"{",
            source.lifecycle.as_ref().ok_or("lifecycle")?,
            request.clone()
        )
        .is_err()
    );
    assert!(workspace.create(request.clone()).is_err());
    request.message = Some(" \r\n ".into());
    assert!(
        crate::mutation::prepare_create_preview(
            &source.files[Path::new("settings.json")],
            source.lifecycle.as_ref().ok_or("lifecycle")?,
            request.clone()
        )
        .is_err()
    );
    // Both explicit policy opt-out and a caller-controlled reason remain usable.
    request.message = Some("Done with evidence".into());
    assert!(
        crate::mutation::prepare_create_preview(
            &source.files[Path::new("settings.json")],
            source.lifecycle.as_ref().ok_or("lifecycle")?,
            request.clone()
        )
        .is_ok()
    );
    settings["governance"]["require_close_reason"] = json!(false);
    request.message = None;
    assert!(
        crate::mutation::prepare_create_preview(
            &serde_json::to_vec(&settings)?,
            source.lifecycle.as_ref().ok_or("lifecycle")?,
            request
        )
        .is_ok()
    );
    Ok(())
}

/// Every logical schema reader must use the bytes retained by its first observation.
#[test]
fn admission_snapshot_aliases_freeze_every_derived_view() -> TestResult {
    let (_directory, workspace) = fixture()?;
    fs::create_dir_all(workspace.pm_root().join("schema"))?;
    let relative = Path::new("schema/shared.json");
    for sequence in 0..500 {
        let initial = json!({"statuses":[{"id":format!("stage_{sequence}"),"roles":[if sequence % 2 == 0 {"active"} else {"draft"}]}],"workflow":{"close_status":"closed"},"definitions":[],"fields":[],"type_workflows":[]});
        let first = serde_json::to_vec(&initial)?;
        fs::write(workspace.pm_root().join(relative), &first)?;
        let mut source = Source::default();
        assert_eq!(source.json(workspace.pm_root(), relative)?, initial);
        let changed = json!({"statuses":[{"id":format!("stage_{sequence}"),"roles":["terminal_done"]}],"workflow":{"close_status":format!("stage_{sequence}")}});
        fs::write(
            workspace.pm_root().join(relative),
            serde_json::to_vec(&changed)?,
        )?;
        for _role in ["types", "fields", "statuses", "workflows"] {
            assert_eq!(source.json(workspace.pm_root(), relative)?, initial);
        }
        assert_eq!(source.files[relative], first);
        assert_eq!(source.bytes, u64::try_from(first.len())?);
        let mut current = Source::default();
        assert_eq!(current.json(workspace.pm_root(), relative)?, changed);
        assert_ne!(source.files, current.files);
    }
    Ok(())
}

/// A role change after status capture cannot be hidden by an aliased workflow read.
#[test]
fn admission_shared_schema_interleaving_refuses_stale_roles() -> TestResult {
    let (_directory, workspace) = fixture()?;
    stored(&workspace)?;
    fs::create_dir_all(workspace.pm_root().join("schema"))?;
    let path = workspace.pm_root().join("settings.json");
    let original: Value = serde_json::from_slice(&fs::read(&path)?)?;
    for shared_roles in [
        vec!["statuses", "workflows"],
        vec!["types", "fields", "statuses", "workflows"],
    ] {
        let mut settings = original.clone();
        for name in &shared_roles {
            settings["schema"]["files"][name] = json!("schema/shared.json");
        }
        fs::write(&path, serde_json::to_vec(&settings)?)?;
        let active = json!({"statuses":[{"id":"stage","roles":["active"]}],"workflow":{},"definitions":[],"fields":[],"type_workflows":[]});
        let terminal = json!({"statuses":[{"id":"stage","roles":["terminal_done"]}],"workflow":{},"definitions":[],"fields":[],"type_workflows":[]});
        fs::write(
            workspace.pm_root().join("schema/shared.json"),
            serde_json::to_vec(&active)?,
        )?;
        let mut intent = create("demo-new");
        intent["options"]["status"] = json!("stage");
        let request = batch(&json!([intent]))?;
        assert!(workspace.preview_item_mutations(&request)?.ok);
        STEP.with(|step| step.set(0));
        INJECT_AT.with(|step| step.set(4));
        let replacement_bytes = serde_json::to_vec(&terminal)?;
        INJECT_REPLACEMENT.with(|replacement| {
            *replacement.borrow_mut() =
                Some((PathBuf::from("schema/shared.json"), replacement_bytes));
        });
        let result = workspace.preview_item_mutations(&request);
        INJECT_AT.with(|step| step.set(usize::MAX));
        INJECT_REPLACEMENT.with(|replacement| *replacement.borrow_mut() = None);
        assert!(
            result.is_err(),
            "stale shared schema must not return a preview: {result:?}"
        );
        assert_eq!(
            result
                .err()
                .ok_or("expected shared-source transition refusal")?
                .code,
            "admission_source_changed"
        );
        assert!(workspace.get("demo-new").is_err());
        // Stable terminal roles refuse; stable active roles remain supported.
        assert!(!workspace.preview_item_mutations(&request)?.ok);
        fs::write(
            workspace.pm_root().join("schema/shared.json"),
            serde_json::to_vec(&active)?,
        )?;
        assert!(workspace.preview_item_mutations(&request)?.ok);
        let (source, _) = Source::load(workspace.pm_root())?;
        assert_eq!(
            source.bytes,
            source.files.values().map(|b| b.len() as u64).sum::<u64>()
        );
    }
    Ok(())
}

/// SDK lookup, preview and all scalar writes agree on physical target identity.
#[test]
fn admission_filename_identity_matches_sdk_for_500_paths() -> TestResult {
    let (_directory, workspace) = fixture()?;
    let original = stored(&workspace)?;
    let bytes = crate::mutation::canonical_item_bytes(&original)?;
    assert!(crate::item::validate_file_identity(Path::new(""), &original).is_err());
    let canonical = workspace.pm_root().join("tasks/demo-existing.toon");
    let history_path = workspace.pm_root().join("history/demo-existing.jsonl");
    let history = fs::read(&history_path)?;
    fs::remove_file(&canonical)?;
    let request =
        batch(&json!([{"op":"update","id":"demo-existing","options":{"title":"Changed"}}]))?;
    for sequence in 0..500 {
        let directory = if sequence % 7 == 0 {
            workspace.pm_root().join("tasks")
        } else {
            workspace
                .pm_root()
                .join(format!("tasks/nested-{}", sequence % 7))
        };
        fs::create_dir_all(&directory)?;
        let matches = sequence % 3 == 0;
        let stem = if matches {
            "demo-existing".to_owned()
        } else {
            format!("demo-alias-{sequence}")
        };
        let path = directory.join(format!("{stem}.toon"));
        fs::write(&path, &bytes)?;
        assert_eq!(workspace.get("demo-existing").is_ok(), matches);
        let result = workspace.preview_item_mutations(&request);
        if matches {
            assert!(result?.ok);
        } else {
            assert_eq!(
                result.err().ok_or("unresolvable target must refuse")?.code,
                "admission_source"
            );
            assert!(workspace.get(&stem).is_err());
            let update = UpdateItem {
                id: stem.clone(),
                title: Some("Must not write".into()),
                description: None,
                status: None,
                priority: None,
                tags: None,
                body: None,
                author: "fixture-agent".into(),
                timestamp: None,
                message: None,
                provenance_role: None,
                force_stale_lock: false,
            };
            assert!(workspace.update(update).is_err());
            assert!(
                workspace
                    .comment(&crate::CommentItem {
                        id: stem,
                        text: "Must not append".into(),
                        author: "fixture-agent".into(),
                        timestamp: None,
                        message: None,
                        provenance_role: None,
                        force_stale_lock: false
                    })
                    .is_err()
            );
        }
        assert_eq!(fs::read(&path)?, bytes.as_bytes());
        assert_eq!(fs::read(&history_path)?, history);
        // Existing list/export reads must keep legacy documents inspectable.
        assert_eq!(workspace.read_items_impl()?.len(), 1);
        fs::remove_file(path)?;
    }
    fs::write(&canonical, bytes)?;
    let nested = workspace.pm_root().join("tasks/duplicate");
    fs::create_dir_all(&nested)?;
    fs::copy(&canonical, nested.join("demo-existing.toon"))?;
    assert!(Source::load(workspace.pm_root()).is_err());
    Ok(())
}

/// Settings reused as a schema file cannot overwrite the image used to derive roles.
#[test]
fn admission_settings_alias_interleaving_refuses_stale_registry() -> TestResult {
    let (_directory, workspace) = fixture()?;
    stored(&workspace)?;
    let path = workspace.pm_root().join("settings.json");
    let mut active: Value = serde_json::from_slice(&fs::read(&path)?)?;
    active["schema"]["statuses"] = json!([{"id":"stage","roles":["active"]}]);
    for role in ["types", "fields", "statuses", "workflows"] {
        active["schema"]["files"][role] = json!("settings.json");
    }
    fs::write(&path, serde_json::to_vec(&active)?)?;
    let mut terminal = active.clone();
    terminal["schema"]["statuses"][0]["roles"] = json!(["terminal_done"]);
    let mut intent = create("demo-new");
    intent["options"]["status"] = json!("stage");
    let request = batch(&json!([intent]))?;
    assert!(workspace.preview_item_mutations(&request)?.ok);
    STEP.with(|step| step.set(0));
    INJECT_AT.with(|step| step.set(2));
    let replacement_bytes = serde_json::to_vec(&terminal)?;
    INJECT_REPLACEMENT.with(|replacement| {
        *replacement.borrow_mut() = Some((PathBuf::from("settings.json"), replacement_bytes));
    });
    let result = workspace.preview_item_mutations(&request);
    INJECT_AT.with(|step| step.set(usize::MAX));
    INJECT_REPLACEMENT.with(|replacement| *replacement.borrow_mut() = None);
    assert!(
        result.is_err(),
        "stale settings must not return a preview: {result:?}"
    );
    assert_eq!(
        result.err().ok_or("settings transition refusal")?.code,
        "admission_source_changed"
    );
    assert!(!workspace.preview_item_mutations(&request)?.ok);
    assert!(workspace.get("demo-new").is_err());
    Ok(())
}

/// A parent swapped after validation cannot redirect an opened admission source.
#[test]
fn admission_parent_component_swap_is_refused() -> TestResult {
    let (directory, workspace) = fixture()?;
    let root = workspace.pm_root();
    let parent = root.join("nested");
    let retained = directory.path().join("retained");
    let target = directory.path().join("external");
    fs::create_dir(&parent)?;
    fs::create_dir(&target)?;
    fs::write(parent.join("fields.json"), "{}")?;
    fs::write(target.join("fields.json"), "{}")?;
    let relative = Path::new("nested/fields.json");
    assert_eq!(Source::default().read(root, relative)?, b"{}");
    INJECT_PARENT_SWAP.with(|swap| swap.replace(Some((parent, retained, target))));
    let result = Source::default().read(root, relative);
    assert_eq!(
        result.err().ok_or("parent swap followed")?.code,
        "admission_source"
    );
    Ok(())
}
