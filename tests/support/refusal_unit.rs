//! Recovery redaction for stored claim holders.

use super::{RESTORE, redact};
use serde_json::json;

/// The stored-holder exception preserves only clean invocation recovery.
#[test]
fn stored_holder_recovery_preserves_only_clean_replays() {
    let holder = "https://fixture:synthetic-secret@example.invalid";
    let clean = json!({"attempted_command":"pm claim sample","normalized_args":["claim","sample"],"replay_ready":true});
    let mut public = json!({
        "code": "already_claimed_by",
        "claimed_by": "https://example.invalid/@scope/package",
        "detail": "Assigned to https://example.invalid/@scope/package",
        "recovery": clean
    });
    let before = public.clone();
    redact(&mut public, &["claim".to_owned(), "sample".to_owned()]);
    assert_eq!(public, before);
    redact(&mut public, &[holder.to_owned()]);
    assert_eq!(public["claimed_by"], before["claimed_by"]);
    assert_eq!(public["recovery"], clean);
    assert_eq!(public["next_steps"][0], RESTORE);
    for (recovery, raw, allowed) in [
        (clean.clone(), vec!["claim", "sample"], true),
        (
            json!({"suggested_retry":"pm claim sample --force","replay_ready":true}),
            vec![],
            false,
        ),
        (
            json!({"suggested_retry_args":["claim","sample","--force"],"replay_ready":true}),
            vec![],
            false,
        ),
        (
            json!({"attempted_command":holder,"normalized_args":[holder],"replay_ready":true}),
            vec![],
            false,
        ),
        (clean.clone(), vec!["claim", "sample", holder], false),
    ] {
        let mut payload = json!({"code":"already_claimed_by","claimed_by":holder,"detail":format!("Assigned to {holder}"),"recovery":recovery});
        let raw = raw.into_iter().map(str::to_owned).collect::<Vec<_>>();
        redact(&mut payload, &raw);
        assert_eq!(payload["claimed_by"], "https://[redacted]@example.invalid");
        assert!(!payload.to_string().contains("synthetic-secret"));
        if allowed {
            assert_eq!(payload["recovery"], clean);
            assert!(payload.get("examples").is_none());
            assert!(
                !payload["next_steps"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .any(|step| step.as_str().is_some_and(|step| step.contains(RESTORE)))
            );
        } else {
            assert_eq!(payload["next_steps"][0], RESTORE);
            assert!(payload["recovery"].get("suggested_retry").is_none());
            assert!(payload["recovery"].get("suggested_retry_args").is_none());
        }
    }
}

/// Standalone command syntax survives even when a credential matches the verb.
#[test]
fn recovery_steps_project_arguments_and_preserve_command_syntax() {
    let args = ["get", "get", "--fields", "title"];
    let mut payload = json!({"next_steps":[
        crate::render_pm_command(args),
        crate::render_pm_rust_command(args),
        "pm get \"<id>",
        17
    ]});
    redact(
        &mut payload,
        &["https://get:fixture-secret@example.invalid".to_owned()],
    );
    let safe = ["get", "[redacted]", "--fields", "title"];
    assert_eq!(
        payload["next_steps"],
        json!([
            RESTORE,
            crate::render_pm_command(safe),
            crate::render_pm_rust_command(safe),
            "pm [redacted] \"<id>",
            17
        ])
    );
}

/// Projection preserves spacing and multiline guidance when its arguments stay public.
#[test]
fn public_recovery_steps_keep_their_original_text() {
    let steps = ["pm get    first", "pm get first\npm get second"];
    let mut payload = json!({
        "code":"already_claimed_by",
        "claimed_by":"https://fixture:synthetic-secret@example.invalid",
        "next_steps":steps
    });
    redact(&mut payload, &[]);
    assert_eq!(payload["next_steps"], json!(steps));

    let multiline = "pm get first\npm get https://fixture:synthetic-secret@example.invalid";
    let mut payload = json!({"next_steps":[multiline]});
    redact(&mut payload, &[]);
    assert_eq!(
        payload["next_steps"],
        json!([
            RESTORE,
            "pm get first\npm get https://[redacted]@example.invalid"
        ])
    );
}

/// A credential may coincide with delimiter syntax or an option value.
#[test]
fn recovery_projection_preserves_only_the_parsed_delimiter() {
    let delimiter = ["get", "--", "sample-own"];
    let value = ["files", "sample-own", "--add", "--", "--", "--"];
    let mut payload = json!({
        "next_steps":[crate::render_pm_command(delimiter), crate::render_pm_command(value)],
        "recovery":{"normalized_args": value}
    });
    redact(
        &mut payload,
        &["https://fixture:--@example.invalid".to_owned()],
    );
    let safe = [
        "files",
        "sample-own",
        "--add",
        "[redacted]",
        "--",
        "[redacted]",
    ];
    assert_eq!(
        payload["next_steps"],
        json!([
            RESTORE,
            crate::render_pm_command(delimiter),
            crate::render_pm_command(safe),
        ])
    );
    assert_eq!(payload["recovery"]["normalized_args"], json!(safe));
}

/// Encoded rejection spans stay scoped and preserve the native command renderer.
#[test]
fn encoded_identity_projection_preserves_native_recovery() {
    let refused = || {
        json!({
            "recovery":{
                "normalized_args":["files","sample-own","--artifact-cwd",".","--message","fixture-only-password public text"],
                "attempted_command":"pm-rust files sample-own --artifact-cwd . --message \"fixture-only-password public text\""
            }
        })
    };
    super::with_cli_projection(|| {
        assert!(
            crate::AuthorIdentity::validate_credentials(
                "https://fixture:fixture-only-password%40example.invalid",
                "--author"
            )
            .is_err()
        );
        let mut payload = refused();
        redact(&mut payload, &[]);
        let recovery = &payload["recovery"];
        assert_eq!(recovery["normalized_args"][5], "[redacted] public text");
        assert_eq!(
            recovery["attempted_command"],
            "pm-rust files sample-own --artifact-cwd . --message \"[redacted] public text\""
        );
    });
    let mut payload = refused();
    redact(&mut payload, &[]);
    assert_eq!(payload, refused());
}

/// Attached and separate author values remain subject to credential projection.
#[test]
fn credential_recovery_preserves_author_option_but_redacts_values() {
    let source = "https://author@example.invalid";
    let redacted = "https://[redacted]@example.invalid";
    for (supplied, expected) in [
        (
            vec!["--author".to_owned(), source.to_owned()],
            vec!["--author".to_owned(), redacted.to_owned()],
        ),
        (
            vec![format!("--author={source}")],
            vec![format!("--author={redacted}")],
        ),
    ] {
        let mut args = vec!["claim".to_owned(), "sample-own".to_owned()];
        args.extend(supplied);
        let mut expected_args = vec!["claim".to_owned(), "sample-own".to_owned()];
        expected_args.extend(expected);
        let mut payload = json!({
            "details": {"normalized_args": ["--author"]},
            "recovery": {
                "normalized_args": args,
                "attempted_command": crate::render_pm_command(&args),
                "provided_fields": ["--author", "author"],
            }
        });
        redact(&mut payload, &args);
        assert_eq!(payload["recovery"]["normalized_args"], json!(expected_args));
        assert_eq!(
            payload["recovery"]["provided_fields"],
            json!(["--author", "[redacted]"])
        );
        assert_eq!(
            payload["details"]["normalized_args"],
            json!(["--[redacted]"])
        );
        assert_eq!(
            payload["recovery"]["attempted_command"],
            crate::render_pm_command(&expected_args)
        );
    }
}

/// Fixed command spellings survive while credentials in arguments and other examples are masked.
#[test]
fn stored_holder_projection_preserves_command_spelling_and_warns_about_altered_examples() {
    let command = "pm claim sample";
    for (example, projected, warn) in [
        (command, command, false),
        (
            "pm release sample --author claim",
            "pm release sample --author [redacted]",
            true,
        ),
    ] {
        let raw = ["claim".to_owned(), "sample".to_owned()];
        let mut payload = json!({
            "code": "already_claimed_by",
            "claimed_by": "custom:claim%40example.invalid",
            "detail": "Assigned to custom:claim%40example.invalid",
            "examples": [command, example],
            "recovery": {"attempted_command":command,"normalized_args":raw}
        });
        redact(&mut payload, &raw);
        assert_eq!(payload["claimed_by"], "custom:[redacted]%40example.invalid");
        assert_eq!(payload["recovery"]["normalized_args"], json!(raw));
        assert_eq!(payload["recovery"]["attempted_command"], command);
        assert_eq!(payload["examples"], json!([command, projected]));
        assert_eq!(payload["next_steps"][0] == RESTORE, warn);
    }

    let raw = ["item", "files", "sample", "--message", "files"].map(str::to_owned);
    let mut payload = json!({
        "code": "already_claimed_by",
        "claimed_by": "custom:files%40example.invalid",
        "recovery": {"attempted_command":crate::render_pm_command(&raw),"normalized_args":raw}
    });
    redact(&mut payload, &raw);
    assert_eq!(
        payload["recovery"]["normalized_args"],
        json!(["item", "files", "sample", "--message", "[redacted]"])
    );
    assert_eq!(payload["next_steps"][0], RESTORE);
}
