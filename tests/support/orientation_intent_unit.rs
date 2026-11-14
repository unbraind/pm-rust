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
        // proof-behavior
        assert_eq!(positive(raw), None);
    }
    // proof-behavior
    assert_eq!(positive(Some(" 0256 ")), Some(256));
    // proof-behavior
    assert!(prepare(&workspace, &mut ContextOptions::default(), None)?.is_none());
    for name in ["orient", "handoff"] {
        let mut options = ContextOptions {
            intent: Some(name.into()),
            ..Default::default()
        };
        let prepared_intent = prepare(&workspace, &mut options, None)?;
        // proof-behavior
        assert!(
            prepared_intent.is_some(),
            "core intent admission must produce a declaration"
        );
        let intent = prepared_intent.ok_or("missing intent")?;
        // proof-behavior
        assert_eq!(
            options.limit.as_deref(),
            Some(if name == "orient" { "3" } else { "2" })
        );
        let result = intent.attach(json!({"filters":{}}).into(), &ContextOptions::default());
        // proof-behavior
        assert_eq!(result["context_intent"]["intent"], name);
        // proof-behavior
        assert_eq!(result["context_intent"]["degradation"], "bounded_sections");
        let omitted = intent.attach(
            json!({"filters":{},"payload":vec!["retained evidence"; 2000]}).into(),
            &ContextOptions::default(),
        );
        // proof-behavior
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
                // proof-behavior
                assert_eq!(prepared.limit.as_deref(), Some(effective));
                let receipt = intent.attach(json!({"filters":{}}).into(), &original);
                // proof-behavior
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
            // proof-behavior
            assert_eq!(result["context_intent"]["intent"], name);
            if raw == "256" {
                // proof-behavior
                assert_eq!(
                    result["context_intent"]["result_omitted"],
                    result["context_intent"]["estimated_tokens"]
                        .as_u64()
                        .ok_or("intent estimate")?
                        > 256
                );
            }
        }
    }
    let mut options = ContextOptions {
        intent: Some("orient".into()),
        token_budget: Some("255".into()),
        ..Default::default()
    };
    // proof-behavior
    assert!(prepare(&workspace, &mut options, None).is_err());
    options.token_budget = None;
    options.intent = Some("custom".into());
    // proof-behavior
    assert!(prepare(&workspace, &mut options, None).is_err());
    options.intent = Some("orient".into());
    fs::write(pm.join("context-intents.json"), "[]")?;
    // proof-behavior
    assert!(prepare(&workspace, &mut options, None).is_err());
    // proof-behavior
    assert!(prepare(&workspace, &mut ContextOptions::default(), None)?.is_none());
    fs::remove_file(pm.join("context-intents.json"))?;
    fs::create_dir(pm.join("extensions"))?;
    fs::write(pm.join("extensions/ignored.json"), "{}")?;
    // proof-behavior
    assert!(prepare(&workspace, &mut options, None)?.is_some());
    fs::create_dir(pm.join("extensions/custom"))?;
    // proof-behavior
    assert!(prepare(&workspace, &mut options, None).is_err());
    fs::remove_dir_all(pm.join("extensions"))?;
    fs::write(pm.join("extensions"), "not a directory")?;
    // proof-behavior
    assert!(prepare(&workspace, &mut options, None).is_err());
    let invalid = workspace.with_read_snapshot(std::path::PathBuf::from("invalid\0root"));
    // proof-behavior
    assert!(prepare(&invalid, &mut options, None).is_err());
    let mut broken = std::iter::once(Err(std::io::Error::other("directory iteration refused")));
    // proof-behavior
    assert!(check_extension_entries(&mut broken, &pm).is_err());
    fs::remove_file(pm.join("extensions"))?;
    fs::create_dir(pm.join("extensions"))?;
    fs::write(pm.join("extensions/disappeared"), "metadata")?;
    let entries = fs::read_dir(pm.join("extensions"))?.collect::<Vec<_>>();
    fs::remove_file(pm.join("extensions/disappeared"))?;
    // proof-behavior
    assert!(check_extension_entries(&mut entries.into_iter(), &pm).is_err());
    let global = root.path().join("global");
    fs::create_dir_all(global.join("extensions/custom"))?;
    // proof-behavior
    assert!(prepare(&workspace, &mut options, Some(&global)).is_err());
    Ok(())
}

/// Removing a temporary row contract recomputes feasibility across each declared ceiling.
#[test]
fn retained_context_intent_feasibility_matches_projection() -> Result<(), Box<dyn std::error::Error>>
{
    let root = tempfile::tempdir()?;
    let pm = root.path().join(".agents/pm");
    fs::create_dir_all(pm.join("tasks"))?;
    fs::write(pm.join("settings.json"), "{}")?;
    let workspace = Workspace::discover(root.path())?;
    for name in ["orient", "handoff"] {
        for toon in [false, true] {
            let original = ContextOptions {
                intent: Some(name.into()),
                token_budget: Some("50000".into()),
                toon,
                ..Default::default()
            };
            let intent = prepare(&workspace, &mut original.clone(), None)?.ok_or("intent")?;
            let mut feasible = 0;
            let mut infeasible = 0;
            for count in 40..100 {
                let result = intent.attach(
                    json!({"filters":{},"payload":vec!["x".repeat(130); count]}).into(),
                    &original,
                );
                let measured = read_output::measure_intent(&result, toon);
                // proof-behavior
                assert_eq!(result["context_intent"]["estimated_tokens"], measured);
                let expected = measured <= intent.declared;
                // proof-behavior
                assert_eq!(
                    result["context_intent"]["declaration_feasible"], expected,
                    "{name}, toon={toon}, count={count}"
                );
                if expected {
                    feasible += 1;
                } else {
                    infeasible += 1;
                }
                // proof-behavior
                assert!(result.get("row_contract").is_none());
            }
            // proof-behavior
            assert!(
                feasible > 0 && infeasible > 0,
                "must cross the declaration boundary"
            );
        }
    }
    Ok(())
}
