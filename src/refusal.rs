//! Credential projection shared by every native command refusal.

use std::collections::HashSet;

use serde_json::Value;

/// Published replacement token and replay instruction.
const REDACTED: &str = "[redacted]";
/// Recovery advice is withheld until the caller restores its own credentials.
const RESTORE: &str = "URL credentials in this invocation were replaced with [redacted]; restore them before replaying any command shown here.";

/// Finds every explicit URL scheme without consuming neighboring URLs or their separators.
fn url_schemes(text: &str) -> Vec<(usize, usize)> {
    let mut schemes = Vec::new();
    for (colon, _) in text.match_indices(':') {
        if !text[colon..].starts_with("://") && !text[colon..].starts_with(":\\\\") {
            continue;
        }
        let scheme = text[..colon]
            .rsplit(|ch: char| !ch.is_ascii_alphanumeric() && !matches!(ch, '+' | '-' | '.'))
            .next()
            .unwrap_or_default();
        if let Some(first) = scheme.find(|ch: char| ch.is_ascii_alphabetic()) {
            schemes.push((colon - scheme.len() + first, colon + 3));
        }
    }
    schemes
}

/// Finds userinfo in every URL, allowing slash-bearing passwords under the GH-1707 rule.
fn userinfos(text: &str) -> Vec<(usize, usize)> {
    let schemes = url_schemes(text);
    let mut spans = Vec::new();
    for (index, &(scheme, start)) in schemes.iter().enumerate() {
        if spans.last().is_some_and(|&(_, end)| scheme < end) {
            continue;
        }
        let mut stop = schemes.get(index + 1).map_or(text.len(), |next| next.0);
        let prefix = &text[start..stop];
        if !prefix.contains('@')
            && !prefix.ends_with(|ch: char| whitespace(ch) || matches!(ch, ',' | ';'))
            && prefix[..prefix.find('/').unwrap_or(prefix.len())].contains(':')
            && let Some(at) = text[start..].find('@')
        {
            // A scheme inside an unresolved password is credential data, not a new URL.
            stop = schemes[index + 1..]
                .iter()
                .find(|next| next.0 > start + at)
                .map_or(text.len(), |next| next.0);
        }
        let candidate = &text[start..stop];
        let boundary = candidate.find(['/', '?', '#']).unwrap_or(candidate.len());
        let authority = &candidate[..boundary];
        let before_slash = &candidate[..candidate.find('/').unwrap_or(candidate.len())];
        let end = authority.rfind('@').or_else(|| {
            before_slash
                .contains(':')
                .then(|| candidate.rfind('@'))
                .flatten()
        });
        let Some(end) = end.filter(|end| *end > 0) else {
            continue;
        };
        let span = &candidate[..end];
        // A public URL followed by prose and an email is not URL authentication.
        if span.chars().any(whitespace)
            && !span.contains(':')
            && scheme != 0
            && !text[..scheme].ends_with('=')
        {
            continue;
        }
        spans.push((start, start + end));
    }
    spans
}

/// Replaces every URL's userinfo while preserving public URLs and surrounding text.
fn redact_token(token: &str) -> String {
    let mut output = String::new();
    let mut copied = 0;
    for (start, end) in userinfos(token) {
        output.push_str(&token[copied..start]);
        output.push_str(REDACTED);
        copied = end;
    }
    output.push_str(&token[copied..]);
    output
}

/// Lowercases without shifting JavaScript UTF-16 offsets.
fn lower(text: &str) -> Vec<u16> {
    let lowered = text.to_lowercase();
    if lowered.encode_utf16().count() == text.encode_utf16().count() {
        return lowered.encode_utf16().collect();
    }
    text.chars()
        .map(|ch| {
            let lowered: String = ch.to_lowercase().collect();
            if lowered.encode_utf16().count() == ch.len_utf16() {
                lowered
            } else {
                ch.to_string()
            }
        })
        .collect::<String>()
        .encode_utf16()
        .collect()
}

/// Matches JavaScript's whitespace grammar for token projection and short passwords.
fn whitespace(ch: char) -> bool {
    matches!(
        ch,
        '\t' | '\n' | '\u{b}' | '\u{c}' | '\r' | ' ' | '\u{a0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200a}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202f}'
                | '\u{205f}'
                | '\u{3000}'
                | '\u{feff}'
    )
}

/// Quotes a fragment with Windows backslash rules on every host.
fn windows_spelling(span: &str) -> String {
    let mut output = String::new();
    let mut pending = 0;
    for ch in span.chars() {
        if ch == '\\' {
            pending += 1;
        } else {
            output.push_str(&"\\".repeat(if ch == '"' { pending * 2 + 1 } else { pending }));
            output.push(ch);
            pending = 0;
        }
    }
    output.push_str(&"\\".repeat(pending));
    output
}

/// Bounded registered password fragments prevent re-escaped credential echoes.
struct Credentials {
    /// Whether the original invocation carried URL userinfo.
    carried: bool,
    /// Fixed-width UTF-16 windows from original and escaped passwords.
    fragments: HashSet<Vec<u16>>,
    /// Short whitespace-bearing spellings that cannot form a full window.
    short: Vec<Vec<u16>>,
    /// Window width and sampling stride share the published registration bound.
    width: usize,
}

impl Credentials {
    /// Registers credentials supplied by the invocation and persisted refusal provenance.
    fn new(args: &[String]) -> Self {
        let mut spellings = HashSet::new();
        let mut carried = false;
        for token in args {
            for (start, end) in userinfos(token) {
                carried = true;
                let span = &token[start..end];
                let secret = span
                    .split_once(':')
                    .filter(|(_, secret)| !secret.is_empty())
                    .map_or(span, |(_, secret)| secret);
                let mut secrets = vec![secret];
                if span.chars().any(whitespace) {
                    secrets.push(span);
                }
                for secret in secrets {
                    let escaped: String = secret
                        .chars()
                        .flat_map(|ch| {
                            if matches!(ch, '"' | '\\' | '$' | '`') {
                                vec!['\\', ch]
                            } else {
                                vec![ch]
                            }
                        })
                        .collect();
                    let json = pm_rust::stringify_json(&Value::String(secret.to_owned()), false);
                    for spelling in [
                        secret.to_owned(),
                        escaped,
                        windows_spelling(secret),
                        json[1..json.len() - 1].to_owned(),
                    ] {
                        spellings.insert(lower(&spelling));
                    }
                }
            }
        }
        let positions: usize = spellings
            .iter()
            .map(|spelling| spelling.len().saturating_sub(5) + 1)
            .sum();
        let stride = positions.div_ceil(65_536).max(1);
        let width = stride.max(6);
        let mut fragments = HashSet::new();
        let mut short = Vec::new();
        for spelling in spellings {
            if spelling.len() < width {
                if spelling.len() >= 6
                    || String::from_utf16_lossy(&spelling).chars().any(whitespace)
                {
                    short.push(spelling);
                }
            } else {
                for index in (0..=spelling.len() - width).step_by(stride) {
                    fragments.insert(spelling[index..index + width].to_vec());
                }
                fragments.insert(spelling[spelling.len() - width..].to_vec());
            }
        }
        Self {
            carried,
            fragments,
            short,
            width,
        }
    }

    /// Projects registered fragments before scanning whitespace-delimited URL tokens.
    fn text(&self, text: &str, token: bool) -> String {
        let text = if token || !userinfos(text).is_empty() {
            redact_token(text)
        } else {
            text.to_owned()
        };
        let folded = lower(&text);
        let units: Vec<_> = text.encode_utf16().collect();
        let mut covered = vec![false; units.len()];
        for (index, window) in folded.windows(self.width).enumerate() {
            if self.fragments.contains(window) {
                covered[index..index + self.width].fill(true);
            }
        }
        for spelling in &self.short {
            for (index, window) in folded.windows(spelling.len()).enumerate() {
                if window == spelling {
                    covered[index..index + spelling.len()].fill(true);
                }
            }
        }
        let mut projected = Vec::new();
        for (index, unit) in units.into_iter().enumerate() {
            if !covered[index] {
                projected.push(unit);
            } else if index == 0 || !covered[index - 1] {
                projected.extend(REDACTED.encode_utf16());
            }
        }
        let text = String::from_utf16_lossy(&projected);
        let mut output = String::new();
        for part in text.split_inclusive(whitespace) {
            let token = part.trim_end_matches(whitespace);
            output.push_str(&redact_token(token));
            output.push_str(&part[token.len()..]);
        }
        output
    }

    /// Recurses through typed values, preserving schema keys and fixed contract identifiers.
    fn value(&self, value: &mut Value, token: bool, producer: bool, schema: bool) {
        match value {
            Value::String(text) if text != RESTORE => *text = self.text(text, token),
            Value::Array(values) => {
                for value in values {
                    self.value(value, token, producer, schema);
                }
            }
            Value::Object(values) => {
                let mut projected = serde_json::Map::new();
                for (key, mut value) in std::mem::take(values) {
                    if schema
                        && matches!(
                            key.as_str(),
                            "code"
                                | "original_code"
                                | "type"
                                | "recovery_mode"
                                | "option_scope"
                                | "diagnostic_class"
                        )
                        && value.is_string()
                    {
                        projected.insert(key, value);
                        continue;
                    }
                    let safe_key = if producer {
                        self.text(&key, false)
                    } else {
                        key.clone()
                    };
                    let mut unique = safe_key.clone();
                    let mut suffix = 2;
                    while projected.contains_key(&unique) {
                        unique = format!("{safe_key} ({suffix})");
                        suffix += 1;
                    }
                    let token = token
                        || matches!(
                            key.as_str(),
                            "normalized_args"
                                | "suggested_retry_args"
                                | "provided_fields"
                                | "parsed_positionals"
                                | "normalizedInvocationArgs"
                                | "providedOptionFlags"
                                | "suggestedRetryArgs"
                        );
                    self.value(
                        &mut value,
                        token,
                        producer || key == "details",
                        schema && key == "recovery",
                    );
                    projected.insert(unique, value);
                }
                *values = projected;
            }
            _ => {}
        }
    }
}

/// Redacts fallback parser and runtime diagnostics with the same credential registry.
pub(super) fn diagnostic(text: &str, raw: &[String]) -> String {
    Credentials::new(raw).text(text, false)
}

/// Registers credential-bearing producer strings before any copied fragment is projected.
fn collect_credential_sources(value: &Value, sources: &mut Vec<String>) {
    match value {
        Value::String(text) => {
            if !userinfos(text).is_empty() {
                sources.push(text.clone());
            }
        }
        Value::Array(values) => {
            for value in values {
                collect_credential_sources(value, sources);
            }
        }
        Value::Object(fields) => {
            for (key, value) in fields {
                if !userinfos(key).is_empty() {
                    sources.push(key.clone());
                }
                collect_credential_sources(value, sources);
            }
        }
        _ => {}
    }
}

/// Redacts every refusal field once, before either JSON or human rendering.
pub(super) fn redact(payload: &mut Value, raw: &[String]) {
    let mut sources = raw.to_vec();
    collect_credential_sources(payload, &mut sources);
    let credentials = Credentials::new(&sources);
    let original = payload.clone();
    credentials.value(payload, false, false, true);
    // Every credential-bearing producer was registered before projection.
    if credentials.carried {
        if payload["recovery"].get("attempted_command").is_some()
            && let Some(values) = payload["recovery"]["normalized_args"].as_array()
        {
            let args = values
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect::<Vec<_>>();
            let command = super::render_recovery(&args);
            payload["recovery"]["attempted_command"] = Value::String(command.clone());
            if let Some(examples) = payload["examples"].as_array_mut() {
                for (before, after) in original["examples"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .zip(examples)
                {
                    if before == &original["recovery"]["attempted_command"] {
                        *after = Value::String(command.clone());
                    }
                }
            }
        }
        if let Some(recovery) = payload["recovery"].as_object_mut() {
            recovery.shift_remove("suggested_retry");
            recovery.shift_remove("suggested_retry_args");
        }
        let mut steps = vec![Value::String(RESTORE.to_owned())];
        steps.extend(
            payload["next_steps"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|step| step.as_str() != Some(RESTORE))
                .cloned(),
        );
        payload["next_steps"] = Value::Array(steps);
        if original.get("next_steps").is_none() {
            // The assignment above guarantees the object and steps array.
            let steps = payload["next_steps"].clone();
            let mut ordered = serde_json::Map::new();
            for (key, value) in payload.as_object().into_iter().flatten() {
                if key == "next_steps" {
                    continue;
                }
                if key == "exit_code" {
                    ordered.insert("next_steps".to_owned(), steps.clone());
                }
                ordered.insert(key.clone(), value.clone());
            }
            ordered.entry("next_steps".to_owned()).or_insert(steps);
            *payload = Value::Object(ordered);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Non-secret URL, email and plain tokens retain exact receipts and safe retry advice.
    #[test]
    fn ordinary_invocations_preserve_refusals() {
        for token in [
            "plain",
            "user@example.invalid",
            "https://example.invalid/path",
            "user@host in prose",
            "Contact: user@example.invalid",
            "https://host/path@ref",
            "https://host?ref=user@example.invalid",
            "https://host#user@example.invalid",
            "Visit https://host and contact user@example.invalid",
            "urn:public-record",
            "https://@example.invalid",
        ] {
            let mut payload = json!({"code":"lock_conflict","recovery":{"normalized_args":[token],"suggested_retry":"pm claim item","suggested_retry_args":["claim","item"]}});
            let original = payload.clone();
            redact(&mut payload, &[token.to_owned()]);
            assert_eq!(payload, original);
        }
    }

    /// Registered fragments redact nested producer keys, collisions and escaped echoes.
    #[test]
    fn credential_projection_preserves_schema_and_withholds_retry() {
        let mut payload = json!({"code":"https://fixture:test-password@example.invalid","detail":"TEST-PASSWORD","details":{"https://fixture:test-password@example.invalid":"test-password","https://other:another-public-value@example.invalid":"another-public-value"},"recovery":{"normalized_args":["--message=https://fixture:test-password@example.invalid"],"suggested_retry":"unsafe","suggested_retry_args":["unsafe"],"diagnostic_class":"test-password"}});
        redact(
            &mut payload,
            &["https://fixture:test-password@example.invalid".to_owned()],
        );
        assert_eq!(payload["detail"], REDACTED);
        assert_eq!(
            payload["code"],
            "https://fixture:test-password@example.invalid"
        );
        assert_eq!(payload["recovery"]["diagnostic_class"], "test-password");
        assert_eq!(
            payload["details"]["https://[redacted]@example.invalid"],
            REDACTED
        );
        assert_eq!(
            payload["details"]["https://[redacted]@example.invalid (2)"],
            REDACTED
        );
        assert!(payload["recovery"].get("suggested_retry").is_none());
        assert!(payload["recovery"].get("suggested_retry_args").is_none());
        assert_eq!(payload["next_steps"], json!([RESTORE]));
        let original = payload.clone();
        redact(
            &mut payload,
            &["https://fixture:test-password@example.invalid".to_owned()],
        );
        assert_eq!(payload, original);
    }

    /// Persisted credentials enroll the whole whitespace-bearing userinfo before any rendering.
    #[test]
    fn persisted_credentials_redact_every_refusal_copy() {
        for secret in [
            "stored password",
            "a b",
            "abc\tdef",
            "abc\ndef",
            "a\\b\" c$d`ef",
        ] {
            let holder = format!("https://fixture:{secret}@example.invalid");
            let mut payload = json!({"claimed_by":holder,"title":format!("Owned by {holder}"),"detail":format!("Item is assigned to {holder}. Retry."),"examples":[holder],"details":{holder.clone():holder},"recovery":{"suggested_retry":"pm claim sample --force","suggested_retry_args":["claim","sample","--force"]}});
            redact(
                &mut payload,
                &[
                    "claim".to_owned(),
                    "sample".to_owned(),
                    "--author".to_owned(),
                    "beta".to_owned(),
                ],
            );
            assert_eq!(payload["claimed_by"], "https://[redacted]@example.invalid");
            assert!(!pm_rust::stringify_json(&payload, false).contains(
                &pm_rust::stringify_json(&json!(secret), false)
                    [1..pm_rust::stringify_json(&json!(secret), false).len() - 1]
            ));
            assert_eq!(
                payload["title"],
                "Owned by https://[redacted]@example.invalid"
            );
            assert_eq!(
                payload["detail"],
                "Item is assigned to https://[redacted]@example.invalid. Retry."
            );
            assert!(payload["recovery"].get("suggested_retry").is_none());
        }
    }

    /// Whitespace-only userinfo remains private when persisted without a password separator.
    #[test]
    fn persisted_whitespace_username_is_redacted() {
        let holder = "https://stored user@example.invalid";
        let mut payload = json!({"claimed_by":holder,"detail":format!("Assigned to {holder}")});
        redact(&mut payload, &["claim".to_owned(), "sample".to_owned()]);
        assert_eq!(payload["claimed_by"], "https://[redacted]@example.invalid");
        assert!(!payload.to_string().contains("stored user"));
    }

    /// Every authority in one field is private, including later whitespace and slash passwords.
    #[test]
    fn multiple_url_credentials_are_redacted_globally() {
        for (input, expected) in [
            (
                "https://first:alpha-secret@one.invalid/p https://second:beta-secret@two.invalid/q",
                "https://[redacted]@one.invalid/p https://[redacted]@two.invalid/q",
            ),
            (
                "https://first:alpha-secret@one.invalid/p,https://second:beta-secret@two.invalid/q",
                "https://[redacted]@one.invalid/p,https://[redacted]@two.invalid/q",
            ),
            (
                "https://first:alpha-secret@one.invalid;https://second:beta-secret@two.invalid",
                "https://[redacted]@one.invalid;https://[redacted]@two.invalid",
            ),
            (
                "https://www.npmjs.com/package/@scope/x https://u:p@host",
                "https://www.npmjs.com/package/@scope/x https://[redacted]@host",
            ),
            (
                "See https://public.invalid/docs and https://fixture:abc def@example.invalid",
                "See https://public.invalid/docs and https://[redacted]@example.invalid",
            ),
            (
                "https://user:abc/def+gh==@host/x https://ghp_fixture@github.com/o/r",
                "https://[redacted]@host/x https://[redacted]@github.com/o/r",
            ),
            (
                "https://user:long-lived-secret://tail@host.invalid",
                "https://[redacted]@host.invalid",
            ),
            (
                "https://user:part://middle://tail@host.invalid;https://other:safe-secret@two.invalid",
                "https://[redacted]@host.invalid;https://[redacted]@two.invalid",
            ),
            ("https://user:pa:ss@host", "https://[redacted]@host"),
            (
                ".https://u:first-secret@one.invalid;.https://v:second-secret@two.invalid",
                ".https://[redacted]@one.invalid;.https://[redacted]@two.invalid",
            ),
            ("https://user?hint:abc@host", "https://[redacted]@host"),
        ] {
            let raw = vec![
                "claim".to_owned(),
                "sample".to_owned(),
                "--message".to_owned(),
                input.to_owned(),
            ];
            let mut payload = json!({"title":input,"detail":input,"why":input,"required":input,"examples":[input],"next_steps":[input],"claimed_by":input,"details":{input:input},"recovery":{"normalized_args":raw,"attempted_command":super::super::render_recovery(&raw),"suggested_retry":"pm claim sample"}});
            redact(&mut payload, &raw);
            for key in ["title", "detail", "why", "required", "claimed_by"] {
                assert_eq!(payload[key], expected, "{key}: {input}");
            }
            assert_eq!(payload["next_steps"][0], RESTORE);
            assert_eq!(payload["next_steps"][1], expected);
            assert_eq!(payload["examples"][0], expected);
            assert_eq!(payload["recovery"]["normalized_args"][3], expected);
            assert_eq!(payload["details"][expected], expected);
            assert!(payload["recovery"].get("suggested_retry").is_none());
            assert_eq!(redact_token(input), expected);
        }
        let raw = vec![
            "https://u:first-secret@one.invalid,https://v:second-secret@two.invalid".to_owned(),
        ];
        let mut copies = json!({"title":"first-secret second-secret","detail":"SECOND-SECRET","details":{"echo":"second-secret"}});
        redact(&mut copies, &raw);
        assert_eq!(copies["title"], "[redacted] [redacted]");
        assert_eq!(copies["detail"], "[redacted]");
        assert_eq!(copies["details"]["echo"], "[redacted]");
        let mut copies =
            json!({"title":"long-lived-secret://tail", "detail":"LONG-LIVED-SECRET://TAIL"});
        redact(
            &mut copies,
            &["https://user:long-lived-secret://tail@host.invalid".to_owned()],
        );
        assert_eq!(copies["title"], "[redacted]");
        assert_eq!(copies["detail"], "[redacted]");
        let public = "https://www.npmjs.com/package/@scope/x";
        let original = json!({"detail":public,"recovery":{"suggested_retry":public}});
        let mut payload = original.clone();
        redact(&mut payload, &[public.to_owned()]);
        assert_eq!(payload, original);
    }

    /// Replay examples are rebuilt from projected arguments, without retaining copied credentials.
    #[test]
    fn round5_refusal_rebuild_and_field_order_corpus() {
        let raw = vec![
            "claim".to_owned(),
            "https://u:long-secret@host.invalid".to_owned(),
        ];
        let command = super::super::render_recovery(&raw);
        for exit in [false, true] {
            let mut payload = json!({"title":"Refused","examples":[command,"independent"],"recovery":{"normalized_args":raw,"attempted_command":command,"suggested_retry":"retry","suggested_retry_args":["retry"]}});
            if exit {
                payload["exit_code"] = json!(4);
            }
            redact(&mut payload, &raw);
            assert_eq!(
                payload["examples"][0],
                payload["recovery"]["attempted_command"]
            );
            assert_eq!(payload["examples"][1], "independent");
            assert_eq!(payload["next_steps"][0], RESTORE);
            assert!(!payload.to_string().contains("long-secret"));
        }
        let mut payload = json!({"next_steps":[RESTORE,"Keep this"],"recovery":null});
        redact(&mut payload, &raw);
        assert_eq!(payload["next_steps"], json!([RESTORE, "Keep this"]));
        assert!(url_schemes("123://host").is_empty());
        let mut payload = json!({"detail":"https://u:long-secret@host.invalid","recovery":{"attempted_command":"echo"}});
        redact(&mut payload, &[]);
        assert!(!payload.to_string().contains("long-secret"));
    }

    /// Escaped separators, short fragments beside large passwords and nested metadata stay safe.
    #[test]
    fn round5_credential_boundary_corpus() {
        for public in [
            "https://host.invalid:443/path",
            "See https://host.invalid and user@example.invalid",
            "https://@host.invalid",
        ] {
            assert_eq!(diagnostic(public, &[]), public);
        }
        assert_eq!(
            redact_token("arg=https://stored user@host.invalid"),
            "arg=https://[redacted]@host.invalid"
        );
        assert_eq!(
            redact_token("https:\\\\u:secret@host.invalid"),
            "https:\\\\[redacted]@host.invalid"
        );
        let credentials = Credentials::new(&[
            format!("https://u:{}@host.invalid", "large".repeat(100_000)),
            "https://v:small6@other.invalid".to_owned(),
        ]);
        assert_eq!(credentials.text("small6", false), REDACTED);
        assert_eq!(credentials.text("ordinary", false), "ordinary");
        let mut payload = json!({"code":4,"details":{"normalizedInvocationArgs":[{"echo":"https://u:long-secret@host.invalid"}]}});
        redact(&mut payload, &[]);
        assert_eq!(payload["code"], 4);
        assert_eq!(
            payload["details"]["normalizedInvocationArgs"][0]["echo"],
            "https://[redacted]@host.invalid"
        );
        let raw = vec!["https://u:long-secret@host.invalid".to_owned()];
        let mut payload = json!({"recovery":{"normalized_args":raw,"attempted_command":"retry"}});
        redact(&mut payload, &raw);
        assert_eq!(payload["examples"], Value::Null);
        assert!(!payload.to_string().contains("long-secret"));
    }

    /// Persisted blocker titles register copied secrets even when the invocation has no credentials.
    #[test]
    fn round5_stored_blocker_title_registers_copied_credentials() {
        let title =
            "https://user:long-lived-secret://tail@host.invalid copied long-lived-secret://tail";
        let mut payload = json!({"code":"start_blocked_by_open_dependency","detail":"The blocker is open","details":{"open_blockers":[{"id":"gate","title":title}]},"next_steps":["Inspect long-lived-secret://tail"],"recovery":{"suggested_retry":"pm next"}});
        redact(&mut payload, &["claim".to_owned(), "sample".to_owned()]);
        assert_eq!(
            payload["details"]["open_blockers"][0]["title"],
            "https://[redacted]@host.invalid copied [redacted]"
        );
        assert!(!payload.to_string().contains("long-lived-secret"));
        assert_eq!(payload["next_steps"][1], "Inspect [redacted]");
        assert!(payload["recovery"].get("suggested_retry").is_none());
    }

    /// UTF-16 fragments and shell/JSON spellings are projected without shifting boundaries.
    #[test]
    fn unicode_and_escaped_fragments_are_redacted() {
        for secret in ["ab🚀cdefgh", "a\\b\"c$d`efgh", "a b", "İabcdef"] {
            let credentials =
                Credentials::new(&[format!("https://fixture:{secret}@example.invalid")]);
            let encoded = pm_rust::stringify_json(&json!(secret), false);
            assert_eq!(credentials.text(secret, false), REDACTED);
            assert_eq!(
                credentials.text(&encoded[1..encoded.len() - 1], false),
                REDACTED
            );
            assert_eq!(credentials.text(&windows_spelling(secret), false), REDACTED);
        }
        assert_eq!(
            lower("İabcdef"),
            "İabcdef".encode_utf16().collect::<Vec<_>>()
        );
    }
}
