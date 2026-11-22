//! Registry classification and optional-file edge contracts over real trackers.

use super::*;

/// Writes one independent settings object for registry tests.
fn fixture(settings: &Value) -> Result<tempfile::TempDir, Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    fs::write(directory.path().join("settings.json"), settings.to_string())?;
    if let Some(workflow) = settings["schema"].get("workflow") {
        fs::create_dir_all(directory.path().join("schema"))?;
        fs::write(
            directory.path().join("schema/workflows.json"),
            json!({"workflow":workflow}).to_string(),
        )?;
    }
    Ok(directory)
}

/// Defaults, aliases, partial roles, non-string entries and terminal variants are classified.
#[test]
fn configured_roles_and_defaults() -> Result<(), Box<dyn std::error::Error>> {
    for schema in [Value::Null, json!({"statuses":[]})] {
        let directory = fixture(&json!({"schema":schema}))?;
        let registry = Lifecycle::load(directory.path())?;
        assert!(registry.matches("open", "open"));
        assert!(!registry.matches("in_progress", "open"));
        assert!(registry.matches("in_progress", "in_progress"));
        assert!(!registry.matches("open", "in_progress"));
        assert!(registry.terminal("closed"));
        assert!(registry.terminal("canceled"));
        assert!(!registry.terminal("open"));
    }
    let directory = fixture(&json!({"schema":{"statuses":[
        {"id":"queued","roles":["active"],"aliases":["To-do",null]},
        {"id":"working","roles":["active"]},
        {"id":"done","roles":["terminal_done"]},
        {"id":"abandoned","roles":["terminal_canceled"]},
        {"id":"archived","roles":["terminal"]},
        {"id":"roleless"}, {"roles":["active"]}
    ],"workflow":{"in_progress_status":"working"}}}))?;
    let registry = Lifecycle::load(directory.path())?;
    assert_eq!(registry.canonical("  TO-  DO "), "queued");
    assert!(registry.matches("queued", "open"));
    assert!(!registry.matches("working", "open"));
    assert!(registry.matches("working", "in_progress"));
    assert!(registry.matches("done", "done"));
    assert!(!registry.matches("done", "queued"));
    assert!(!registry.role("roleless", "active"));
    assert!(!registry.role("absent", "active"));
    assert!(registry.terminal("done"));
    assert!(registry.terminal("abandoned"));
    assert!(registry.terminal("archived"));

    let directory = fixture(&json!({"schema":{"workflow":{"in_progress_status":" "}}}))?;
    assert_eq!(
        Lifecycle::load(directory.path())?.activity_progress_status(),
        "in_progress"
    );
    Ok(())
}

/// Unset workflow preferences retain the registry's fallback order without an open role.
#[test]
fn unset_open_without_a_default_role_uses_the_registry_fallback() {
    for (statuses, expected) in [
        (
            json!([{"id":"draft","order":0},{"id":"open","order":1}]),
            "open",
        ),
        (
            json!([{"id":"later","order":2},{"id":"first","order":1}]),
            "first",
        ),
    ] {
        let settings = json!({"schema":{
            "statuses":statuses, "workflow":{"in_progress_status":" "}
        }});
        let registry = Lifecycle::from_values(&settings, &Value::Null, &Value::Null);
        assert_eq!(registry.open_status(), expected);
        assert_eq!(registry.activity_progress_status(), expected);
    }
}

/// Blank IDs cannot become defaults and unusable embedded registries retain the built-ins.
#[test]
fn unusable_status_definitions_do_not_supply_workflow_defaults() {
    let built_in_open = defaults()[1].clone();
    let blank = json!({"id":" \u{feff} ","roles":["default_open"],"order":-10});
    for statuses in [json!([built_in_open, blank]), json!([{"roles":["active"]}])] {
        let settings = json!({"schema":{"statuses":statuses}});
        let sidecar = json!([{"id":"custom","roles":["active"]}]);
        let registry = Lifecycle::from_values(&settings, &sidecar, &Value::Null);
        assert_eq!(registry.open_status(), "open");
        assert!(registry.known("custom"));
        assert!(!registry.known(""));
    }
    let settings = json!({"schema":{"statuses":[{"roles":["active"]}]}});
    let registry = Lifecycle::from_values(&settings, &Value::Null, &Value::Null);
    assert_eq!(registry.open_status(), "open");
    assert_eq!(registry.activity_progress_status(), "in_progress");
}

/// Optional arrays/objects merge over embedded definitions, while invalid JSON is ignored.
#[test]
fn schema_files_extend_and_override() -> Result<(), Box<dyn std::error::Error>> {
    for statuses in [
        json!([{"id":"done","roles":["terminal"]}]),
        json!({"statuses":[{"id":"done","roles":["terminal"]}]}),
        json!({"definitions":[{"id":"done","roles":["terminal"]}]}),
    ] {
        let directory = fixture(
            &json!({"schema":{"files":{"statuses":"statuses.json","workflows":"workflow.json"}}}),
        )?;
        fs::write(directory.path().join("statuses.json"), statuses.to_string())?;
        for workflow in [
            json!({"in_progress_status":"open"}),
            json!({"workflow":{"in_progress_status":"open"}}),
        ] {
            fs::write(directory.path().join("workflow.json"), workflow.to_string())?;
            let registry = Lifecycle::load(directory.path())?;
            assert!(registry.terminal("done"));
            assert!(registry.matches("open", "in_progress"));
        }
        fs::write(directory.path().join("statuses.json"), "invalid json")?;
        assert!(!Lifecycle::load(directory.path())?.terminal("done"));
    }
    let directory = fixture(&json!({"schema":{"files":{"statuses":"directory"}}}))?;
    fs::create_dir(directory.path().join("directory"))?;
    assert!(Lifecycle::load(directory.path()).is_err());
    Ok(())
}

/// Custom aliases and both edge directions coexist with unrelated kinds and defaults.
#[test]
fn declarative_hierarchy_registry() {
    let registry = Hierarchy::new(&json!([
        {"kind":"belongs_to","hierarchy":true,"hierarchyDirection":"target_parent","aliases":["member_of",null]},
        {"kind":"contains","hierarchy":true,"hierarchyDirection":"source_parent"},
        {"kind":"related_custom","hierarchy":false}, {"hierarchy":true}
    ]));
    assert_eq!(registry.source_parent(" MEMBER-OF "), Some(false));
    assert_eq!(registry.source_parent("contains"), Some(true));
    assert_eq!(registry.source_parent("parent"), Some(false));
    assert_eq!(registry.source_parent("child"), Some(true));
    assert_eq!(registry.source_parent("related_custom"), None);
    assert_eq!(registry.source_parent("unknown"), None);
}

/// Empty workflow files fall back to inline settings and then the built-in workflow.
#[test]
fn workflow_fallbacks_and_settings_errors() -> Result<(), Box<dyn std::error::Error>> {
    let directory = fixture(&json!({"schema":{"workflow":{"in_progress_status":"working"}}}))?;
    fs::write(directory.path().join("schema/workflows.json"), "{}")?;
    assert!(Lifecycle::load(directory.path())?.matches("working", "in_progress"));
    let directory = fixture(&json!({}))?;
    fs::create_dir(directory.path().join("schema"))?;
    fs::write(directory.path().join("schema/workflows.json"), "{}")?;
    assert!(Lifecycle::load(directory.path())?.matches("in_progress", "in_progress"));
    fs::remove_file(directory.path().join("settings.json"))?;
    assert!(Lifecycle::load(directory.path()).is_err());
    let tracker = tempfile::tempdir()?;
    let root = tracker.path().join(".agents/pm");
    fs::create_dir_all(root.join("tasks"))?;
    fs::create_dir(root.join("workflow-directory"))?;
    fs::write(
        root.join("settings.json"),
        json!({"schema":{"files":{"workflows":"workflow-directory"}}}).to_string(),
    )?;
    let workspace = crate::Workspace::discover(tracker.path())?;
    let result = workspace.list_page(
        &crate::ItemFilter::default(),
        &crate::ListOptions::default(),
        "2026-10-02T10:00:00.000Z",
    );
    // Match discovery's contract independently: Windows keeps resolved aliases,
    // while Unix uses the physical root (including macOS temp-root symlinks).
    let expected_root = if cfg!(windows) {
        std::path::absolute(&root)?
    } else {
        fs::canonicalize(&root)?
    };
    let Err(PmRustError::Io { path, source }) = result else {
        return Err("workflow directory must return a typed I/O error".into());
    };
    assert_eq!(path, expected_root.join("workflow-directory"));
    assert_ne!(source.kind(), std::io::ErrorKind::NotFound);
    Ok(())
}

/// Dependency blocking falls back to workflow open only when every active role is absent.
#[test]
fn empty_active_role_fallback() -> Result<(), Box<dyn std::error::Error>> {
    let directory = fixture(&json!({}))?;
    let registry = Lifecycle::load(directory.path())?;
    assert!(registry.active_for_blocking("open"));
    assert!(!registry.active_for_blocking("draft"));
    fs::create_dir_all(directory.path().join("schema"))?;
    fs::write(
        directory.path().join("schema/statuses.json"),
        json!({"statuses":[{"id":"open","roles":[]},{"id":"in_progress","roles":[]}]}).to_string(),
    )?;
    let registry = Lifecycle::load(directory.path())?;
    assert!(registry.active_for_blocking("open"));
    assert!(!registry.active_for_blocking("draft"));
    Ok(())
}
