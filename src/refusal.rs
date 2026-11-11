//! Credential projection shared by every native command refusal.

use std::collections::HashSet;

use serde_json::Value;

/// Published replacement token and replay instruction.
const REDACTED: &str = "[redacted]";
/// Recovery advice is withheld until the caller restores its own credentials.
const RESTORE: &str = "URL credentials in this invocation were replaced with [redacted]; restore them before replaying any command shown here.";

/// Finds URL userinfo using the published permissive token grammar.
fn userinfo(token: &str) -> Option<(usize, usize)> {
    let end = token.rfind('@')?;
    let colon = token.find(':')?;
    if colon > end {
        return None;
    }
    let mut start = colon + 1;
    while start < end && start < colon + 3 && matches!(token.as_bytes()[start], b'/' | b'\\') {
        start += 1;
    }
    (start != end).then_some((start, end))
}

/// Replaces a complete token's userinfo without parsing or normalizing its URL.
fn redact_token(token: &str) -> String {
    userinfo(token).map_or_else(
        || token.to_owned(),
        |(start, end)| format!("{}{REDACTED}{}", &token[..start], &token[end..]),
    )
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
    /// Registers only credentials explicitly supplied to this invocation.
    fn new(args: &[String]) -> Self {
        let mut spellings = HashSet::new();
        let mut carried = false;
        for token in args {
            if let Some((start, end)) = userinfo(token) {
                carried = true;
                let span = &token[start..end];
                let secret = span
                    .split_once(':')
                    .filter(|(_, secret)| !secret.is_empty())
                    .map_or(span, |(_, secret)| secret);
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
        let text = if token {
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

/// Redacts every refusal field once, before either JSON or human rendering.
pub(super) fn redact(payload: &mut Value, raw: &[String]) {
    let credentials = Credentials::new(raw);
    let original = payload.clone();
    credentials.value(payload, false, false, true);
    if credentials.carried || *payload != original {
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
            let steps = payload
                .as_object_mut()
                .and_then(|map| map.shift_remove("next_steps"));
            if let (Some(steps), Some(map)) = (steps, payload.as_object_mut()) {
                let mut ordered = serde_json::Map::new();
                for (key, value) in std::mem::take(map) {
                    if key == "exit_code" {
                        ordered.insert("next_steps".to_owned(), steps.clone());
                    }
                    ordered.insert(key, value);
                }
                if !ordered.contains_key("next_steps") {
                    ordered.insert("next_steps".to_owned(), steps);
                }
                *map = ordered;
            }
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
            "another-public-value"
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
