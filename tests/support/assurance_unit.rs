//! Declaration reads retain registry and history bytes on expected refusals.
use super::*;
#[test]
/// Declaration discovery, lookup and refusal paths preserve stored registry bytes.
fn assurance_reads_are_typed_and_read_only() -> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let root = directory.path().join(".agents/pm");
    std::fs::create_dir_all(&root)?;
    std::fs::write(root.join("settings.json"), "{}")?;
    let workspace = Workspace::discover(directory.path())?;
    for kind in ["measurement", "assertion", "gate"] {
        assert_eq!(workspace.assurance_declarations(kind)?["count"], 0);
        let error = workspace.assurance_declaration(kind, "absent");
        let Err(PmRustError::CommandRefusal {
            exit_code, context, ..
        }) = error
        else {
            return Err("expected typed refusal".into());
        };
        assert_eq!(exit_code, 2);
        assert_eq!(context["reason"], "assurance_evaluation_refused");
        assert_eq!(context["changed"], false);
    }
    assert!(workspace.assurance_declarations("unknown").is_err());
    assert!(
        workspace
            .assurance_declaration("unknown", "absent")
            .is_err()
    );
    assert!(
        workspace
            .assurance_read("unknown", None, &crate::ListOptions::default())
            .is_err()
    );
    let path = root.join("assurance.json");
    for raw in ["broken", "{}"] {
        std::fs::write(&path, raw)?;
        assert!(workspace.assurance_declarations("gate").is_err());
        assert_eq!(std::fs::read_to_string(&path)?, raw);
    }
    std::fs::write(&path, r#"{"gates":[{"id":"known","title":"Known gate"}]}"#)?;
    assert_eq!(
        workspace.assurance_declaration("gate", "known")?["title"],
        "Known gate"
    );
    for id in [None, Some("known")] {
        let read = workspace.assurance_read("gate", id, &crate::ListOptions::default())?;
        assert!(read.value.is_object());
    }
    assert!(
        workspace
            .assurance_read("gate", Some("missing"), &crate::ListOptions::default())
            .is_err()
    );
    assert!(
        workspace
            .assurance_read(
                "gate",
                None,
                &crate::ListOptions {
                    output_budget: Some("invalid".to_owned()),
                    ..crate::ListOptions::default()
                }
            )
            .is_err()
    );
    std::fs::remove_file(&path)?;
    std::fs::create_dir(&path)?;
    assert!(matches!(
        workspace.assurance_declarations("gate"),
        Err(PmRustError::Io { .. })
    ));
    Ok(())
}
