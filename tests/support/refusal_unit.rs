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
        } else {
            assert_eq!(payload["next_steps"][0], RESTORE);
            assert!(payload["recovery"].get("suggested_retry").is_none());
            assert!(payload["recovery"].get("suggested_retry_args").is_none());
        }
    }
}
