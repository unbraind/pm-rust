//! Core intent admission and resource receipts over real non-git trackers.
use super::*;
use std::fs;

/// Intents retain static core declarations and explicitly refuse runtime overrides before reads.
#[test]
#[allow(clippy::too_many_lines)]
fn core_intent_boundaries_and_budget_receipts() -> Result<(), Box<dyn std::error::Error>> {
    let root = tempfile::tempdir()?;
    let pm = root.path().join(".agents/pm");
    fs::create_dir_all(pm.join("tasks"))?;
    fs::write(
        pm.join("settings.json"),
        r#"{"id_prefix":"sample-","item_format":"toon"}"#,
    )?;
    let workspace = Workspace::discover(root.path())?;
    for raw in [
        None,
        Some(""),
        Some("0"),
        Some("invalid"),
        Some("9007199254740992"),
        Some("999999999999999999999999999"),
    ] {
        assert_eq!(positive(raw), None);
    }
    assert_eq!(positive(Some(" 0256 ")), Some(256));
    assert!(prepare(&workspace, &mut ContextOptions::default(), None)?.is_none());
    for name in ["orient", "handoff"] {
        let mut options = ContextOptions {
            intent: Some(name.into()),
            ..Default::default()
        };
        let intent = prepare(&workspace, &mut options, None)?.ok_or("missing intent")?;
        assert_eq!(
            options.limit.as_deref(),
            Some(if name == "orient" { "3" } else { "2" })
        );
        let result = intent.attach(json!({"filters":{}}).into(), &ContextOptions::default());
        assert_eq!(result["context_intent"]["intent"], name);
        assert_eq!(result["context_intent"]["degradation"], "bounded_sections");
        let omitted = intent.attach(
            json!({"filters":{},"payload":vec!["retained evidence"; 2000]}).into(),
            &ContextOptions::default(),
        );
        assert_eq!(
            omitted["budget_exceeded"]["reason"],
            "declared_budget_infeasible"
        );
        for (raw, limit) in [
            ("256", "1"),
            ("50000", "100"),
            ("0x400", "0"),
            ("50000", "9007199254740991"),
        ] {
            let original = ContextOptions {
                intent: Some(name.into()),
                token_budget: Some(raw.into()),
                limit: Some(limit.into()),
                ..Default::default()
            };
            let mut prepared = original.clone();
            let intent = prepare(&workspace, &mut prepared, None)?.ok_or("missing intent")?;
            if raw == "50000" && limit == "100" {
                let effective = if name == "orient" { "81" } else { "98" };
                assert_eq!(prepared.limit.as_deref(), Some(effective));
                let receipt = intent.attach(json!({"filters":{}}).into(), &original);
                assert_eq!(
                    receipt["warnings"],
                    json!([format!(
                        "limit_overridden_by_intent:--limit 100->{effective}"
                    )])
                );
            }
            let result = intent.attach(
                json!({"filters":{},"payload":"long".repeat(6000)}).into(),
                &original,
            );
            assert_eq!(result["context_intent"]["intent"], name);
            if raw == "256" {
                assert_eq!(result["context_intent"]["result_omitted"], true);
            }
        }
    }
    let mut options = ContextOptions {
        intent: Some("orient".into()),
        token_budget: Some("255".into()),
        ..Default::default()
    };
    assert!(prepare(&workspace, &mut options, None).is_err());
    options.token_budget = None;
    options.intent = Some("custom".into());
    assert!(prepare(&workspace, &mut options, None).is_err());
    options.intent = Some("orient".into());
    fs::write(pm.join("context-intents.json"), "[]")?;
    assert!(prepare(&workspace, &mut options, None).is_err());
    assert!(prepare(&workspace, &mut ContextOptions::default(), None)?.is_none());
    fs::remove_file(pm.join("context-intents.json"))?;
    fs::create_dir(pm.join("extensions"))?;
    fs::write(pm.join("extensions/ignored.json"), "{}")?;
    assert!(prepare(&workspace, &mut options, None)?.is_some());
    fs::create_dir(pm.join("extensions/custom"))?;
    assert!(prepare(&workspace, &mut options, None).is_err());
    fs::remove_dir_all(pm.join("extensions"))?;
    fs::write(pm.join("extensions"), "not a directory")?;
    assert!(prepare(&workspace, &mut options, None).is_err());
    let invalid = workspace.with_read_snapshot(std::path::PathBuf::from("invalid\0root"));
    assert!(prepare(&invalid, &mut options, None).is_err());
    let mut broken = std::iter::once(Err(std::io::Error::other("directory iteration refused")));
    assert!(check_extension_entries(&mut broken, &pm).is_err());
    fs::remove_file(pm.join("extensions"))?;
    fs::create_dir(pm.join("extensions"))?;
    fs::write(pm.join("extensions/disappeared"), "metadata")?;
    let entries = fs::read_dir(pm.join("extensions"))?.collect::<Vec<_>>();
    fs::remove_file(pm.join("extensions/disappeared"))?;
    assert!(check_extension_entries(&mut entries.into_iter(), &pm).is_err());
    let global = root.path().join("global");
    fs::create_dir_all(global.join("extensions/custom"))?;
    assert!(prepare(&workspace, &mut options, Some(&global)).is_err());
    Ok(())
}
