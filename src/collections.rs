//! Native linked artifact and linked test collection contracts.

use crate::{PmRustError, Workspace};
use serde_json::{Map, Value, json};
use std::path::{Component, Path};

/// Collection command inputs shared by the CLI and SDK.
#[derive(Clone, Debug, Default)]
pub struct CollectionOptions {
    /// Repeated bare or structured additions.
    pub add: Vec<String>,
    /// Repeated exact value removal selectors.
    pub remove: Vec<String>,
    /// Repeated one-based linked test indices.
    pub remove_index: Vec<String>,
    /// Author asserted for a mutation.
    pub author: String,
    /// Resolved ownership principal and history attribution, when supplied by an agent caller.
    pub identity: Option<crate::AuthorIdentity>,
    /// Deterministic mutation clock, when supplied.
    pub timestamp: Option<String>,
    /// Optional history explanation.
    pub message: Option<String>,
    /// Approved ownership and stale-lock override.
    pub force: bool,
    /// Preserve insertion order for files.
    pub append_stable: bool,
    /// Standalone note applied to added artifacts.
    pub note: Option<String>,
}

#[cfg(test)]
#[path = "../tests/support/collections_unit.rs"]
mod tests;

/// Produces a published command failure with optional contract guidance.
pub(crate) fn refusal(detail: impl Into<String>, exit_code: u8, context: Value) -> PmRustError {
    PmRustError::CommandRefusal {
        detail: detail.into(),
        exit_code,
        context,
    }
}

/// Computes UTF-16 edit distance for the oracle's missing-identity suggestions.
pub(crate) fn distance(left: &str, right: &str) -> usize {
    let right: Vec<_> = right.encode_utf16().collect();
    let mut previous: Vec<_> = (0..=right.len()).collect();
    for (i, left) in left.encode_utf16().enumerate() {
        let mut next = vec![i + 1];
        for (j, right) in right.iter().enumerate() {
            next.push(
                (previous[j + 1] + 1)
                    .min(next[j] + 1)
                    .min(previous[j] + usize::from(left != *right)),
            );
        }
        previous = next;
    }
    previous[right.len()]
}

/// Missing mutation identities include the same bounded nearby-ID hints as the oracle.
pub(crate) fn missing(pm_root: &Path, id: &str) -> PmRustError {
    let mut candidates: Vec<_> = Workspace::discover(pm_root)
        .and_then(|workspace| workspace.read_items())
        .unwrap_or_default()
        .into_iter()
        .map(|document| {
            let distance = distance(&document.metadata.id, id);
            (distance, document.metadata.id)
        })
        .filter(|(distance, _)| *distance <= 3.max(id.encode_utf16().count() / 2))
        .collect();
    candidates.sort_by_key(|entry| entry.0);
    let ids: Vec<_> = candidates.into_iter().take(3).map(|(_, id)| id).collect();
    let mut steps = Vec::new();
    if !ids.is_empty() {
        steps.push(format!("Did you mean one of: {}?", ids.join(", ")));
    }
    steps.push("Confirm the active --path/PM_PATH scope, then retry with a valid id.".to_owned());
    refusal(
        format!("Item {id} not found"),
        3,
        json!({"next_steps":steps}),
    )
}

/// Resolves the same hosted or local branch provenance used by linked test additions.
pub(crate) fn source_ref(pm_root: &Path) -> Option<String> {
    let hosted = ["GITHUB_HEAD_REF", "GITHUB_REF_NAME"]
        .into_iter()
        .filter_map(|key| std::env::var(key).ok())
        .find(|s| !s.trim().is_empty());
    if let Some(hosted) = hosted.filter(|s| s.trim() != "merge") {
        return Some(hosted.trim().to_owned());
    }
    let workspace = std::env::var_os("PM_SOURCE_WORKSPACE_ROOT").map_or_else(
        || {
            pm_root
                .parent()
                .and_then(Path::parent)
                .unwrap_or(pm_root)
                .to_path_buf()
        },
        std::path::PathBuf::from,
    );
    if !workspace
        .ancestors()
        .any(|parent| parent.join(".git").exists())
    {
        return None;
    }
    let mut command = std::process::Command::new("git");
    command
        .current_dir(workspace)
        .args(["symbolic-ref", "--quiet", "--short", "HEAD"]);
    let output = command.output().ok()?;
    let value = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    (!value.is_empty()).then_some(value)
}

/// Parses quoted CSV or Markdown key/value lines without interpreting shell text.
fn pairs(raw: &str, flag: &str) -> Result<Map<String, Value>, PmRustError> {
    let raw = raw.trim();
    let raw = if raw.starts_with("```") {
        raw.split_once('\n')
            .map_or(raw, |(_, body)| body.trim_end_matches('`').trim())
    } else {
        raw
    };
    let mut parts = Vec::new();
    let mut part = String::new();
    let mut quoted = false;
    let markdown = raw.contains('\n')
        || raw.starts_with("- ")
        || raw.starts_with("* ")
        || raw.starts_with("+ ");
    let mut chars = raw.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\\' && chars.peek() == Some(&',') {
            part.push(',');
            chars.next();
        } else if ch == '\\' && chars.peek().is_some() {
            part.push(ch);
            part.push(chars.next().unwrap_or_default());
        } else if ch == '"' {
            quoted = !quoted;
            part.push(ch);
        } else if (ch == ',' && !markdown || ch == '\n') && !quoted {
            parts.push(std::mem::take(&mut part));
        } else {
            part.push(ch);
        }
    }
    if !part.trim().is_empty() {
        parts.push(part);
    }
    let mut result = Map::new();
    let mut active = String::new();
    for part in parts {
        let continuation = if markdown { part.trim() } else { part.as_str() };
        let part = part.trim();
        let part = part
            .strip_prefix("- ")
            .or_else(|| part.strip_prefix("* "))
            .or_else(|| part.strip_prefix("+ "))
            .unwrap_or(part)
            .trim();
        let Some(index) = part.find(['=', ':']) else {
            if ["command", "cmd", "note"].contains(&active.as_str()) {
                result[&active] = json!(format!(
                    "{}{separator}{continuation}",
                    result[&active].as_str().unwrap_or_default(),
                    separator = if raw.contains('\n') { "\n" } else { "," }
                ));
                continue;
            }
            return Err(invalid_pair(raw, flag));
        };
        let key = part[..index].trim().to_lowercase();
        if key.is_empty() {
            return Err(invalid_pair(raw, flag));
        }
        active.clone_from(&key);
        result.insert(key, json!(unquote(&part[index + 1..])));
    }
    Ok(result)
}

/// Renders the published malformed-pair diagnostic and its optional artifact guidance.
fn invalid_pair(raw: &str, flag: &str) -> PmRustError {
    let guidance = if flag == "--add" && raw.contains(['.', '/', '\\']) {
        "For file/doc paths use: path=<file-path>[,scope=project|global]. The scope field is optional and defaults to project (example: path=src/api.ts or path=README.md,scope=project). "
    } else {
        ""
    };
    refusal(
        format!(
            "Invalid {flag} value \"{raw}\". Expected key=value entries separated by commas. {guidance}Also accepts markdown-style key/value lines (for example \"- path: README.md\"). Use {flag} - to read piped stdin input."
        ),
        2,
        json!({}),
    )
}

/// Decodes JSON-quoted pair values, retaining the oracle's lenient quote fallback.
fn unquote(value: &str) -> String {
    let value = value.trim();
    if let Some(inner) = value
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
    {
        serde_json::from_str::<String>(value).unwrap_or_else(|_| inner.replace("\\\"", "\""))
    } else {
        value.to_owned()
    }
}

/// Recognizes structured input while leaving URL and ordinary path equals signs intact.
fn structured(raw: &str, test: bool) -> bool {
    if raw.starts_with("```") || raw.contains('\n') {
        return true;
    }
    let lead = raw.trim_start_matches(['-', '*', '+', ' ']);
    let first = lead.split(',').next().unwrap_or_default();
    let Some(index) = first.find(['=', ':']) else {
        return false;
    };
    let key = first[..index].trim().to_lowercase();
    let known = if test {
        "command,cmd,path,scope,timeout,timeout_seconds,pm_context_mode,workspace_context_mode,env_set,env_clear,shared_host_safe,assert_stdout_contains,assert_stdout_regex,assert_stderr_contains,assert_stderr_regex,assert_stdout_min_lines,assert_json_field_equals,assert_json_field_gte,note"
    } else {
        "path,scope,note"
    };
    known.split(',').any(|candidate| candidate == key)
        || !test
            && first.find('=').is_some_and(|index| {
                raw.chars().any(char::is_whitespace) || !first[..index].contains(['/', '\\'])
            })
}

/// Anchors a relative artifact reference at the invocation directory.
fn anchor(raw: &str, root: &Path, cwd: &Path) -> String {
    let raw = raw.trim();
    if raw.contains("://") {
        return raw.to_owned();
    }
    if Path::new(raw).is_absolute() || raw.as_bytes().get(1) == Some(&b':') {
        return raw.replace('\\', "/");
    }
    let base = cwd
        .strip_prefix(root)
        .ok()
        .filter(|path| !path.starts_with(".."))
        .unwrap_or_else(|| Path::new(""));
    let joined = base.join(raw);
    let mut parts = Vec::new();
    for component in joined.components() {
        match component {
            Component::ParentDir if parts.last().is_some_and(|p| *p != "..") => {
                parts.pop();
            }
            Component::CurDir => {}
            _ => parts.push(component.as_os_str().to_str().unwrap_or_default()),
        }
    }
    parts.join("/")
}

/// Parses one addition into the stored collection row shape.
#[allow(clippy::similar_names, clippy::too_many_lines)]
fn addition(raw: &str, key: &str, root: &Path, cwd: &Path) -> Result<Value, PmRustError> {
    let test = key == "tests";
    let identity = if test { "command" } else { "path" };
    if key == "docs"
        && raw.starts_with('[')
        && raw.ends_with(')')
        && let Some((label, destination)) = raw[1..raw.len() - 1].split_once("](")
    {
        return addition(
            &format!("path=\"{destination}\",note=\"{label}\""),
            key,
            root,
            cwd,
        );
    }
    if key == "docs"
        && let Some((label, destination)) = raw.split_once(',')
        && !label.trim().is_empty()
        && destination.trim().contains("://")
    {
        return Ok(json!({"path":destination.trim(),"scope":"project","note":label.trim()}));
    }
    if !test && !structured(raw, false) && raw.contains(',') && !raw.contains("://") {
        let noun = if key == "docs" { "doc" } else { "file" };
        return Err(refusal(
            format!(
                "--add received an ambiguous comma-bearing bare {noun} path. Repeat --add for multiple values, or quote or escape literal commas in a structured key=<value> entry."
            ),
            2,
            json!({"code":"bare_comma_entry_ambiguous","required":format!("Use one --add per bare {noun} path, or quote or escape literal commas in a structured key=<value> entry.")}),
        ));
    }
    let mut row = if structured(raw, test) {
        pairs(raw, "--add")?
    } else {
        Map::from_iter([(identity.to_owned(), json!(raw.trim()))])
    };
    let allowed = if test {
        "command, cmd, path, scope, timeout, timeout_seconds, pm_context_mode, workspace_context_mode, env_set, env_clear, shared_host_safe, assert_stdout_contains, assert_stdout_regex, assert_stderr_contains, assert_stderr_regex, assert_stdout_min_lines, assert_json_field_equals, assert_json_field_gte, note"
    } else {
        "path, scope, note"
    };
    let unknown: Vec<_> = row
        .keys()
        .filter(|k| !allowed.split(", ").any(|a| a == *k))
        .collect();
    if !unknown.is_empty() {
        return Err(refusal(
            format!(
                "--add does not recognize key{} {}. Allowed keys: {allowed}.",
                if unknown.len() > 1 { "s" } else { "" },
                unknown
                    .iter()
                    .map(|k| format!("\"{k}\""))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            2,
            json!({}),
        ));
    }
    if let Some(cmd) = row.shift_remove("cmd") {
        if row
            .get("command")
            .is_some_and(|value| value.as_str().map(str::trim) != cmd.as_str().map(str::trim))
        {
            return Err(refusal(
                "--add command and cmd must match when both are provided",
                2,
                json!({}),
            ));
        }
        row.entry("command".to_owned()).or_insert(cmd);
    }
    let value = row
        .get(identity)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_owned();
    if value.is_empty() {
        return Err(refusal(
            if test {
                "--add requires command=<value> or a bare command (path=<value> is optional metadata)".to_owned()
            } else {
                format!(
                    "--add requires path=<value> or a bare {} path",
                    if key == "docs" { "doc" } else { "file" }
                )
            },
            2,
            json!({}),
        ));
    }
    let scope = row
        .get("scope")
        .and_then(Value::as_str)
        .unwrap_or("project");
    if !["project", "global"].contains(&scope) {
        return Err(refusal(
            format!(
                "Invalid scope \"{scope}\"{}",
                if test {
                    ""
                } else {
                    ". Valid scopes: project, global (default: project)."
                }
            ),
            2,
            json!({}),
        ));
    }
    let mut ordered = json!({identity:if test {value} else {anchor(&value,root,cwd)}});
    if test
        && let Some(path) = row
            .get("path")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
    {
        ordered["path"] = json!(path);
    }
    ordered["scope"] = json!(scope);
    for (key, value) in row {
        if key != identity && key != "scope" && value.as_str().is_some_and(|s| !s.trim().is_empty())
        {
            ordered[&key] = json!(value.as_str().unwrap_or_default().trim());
        }
    }
    if test {
        normalize_test_fields(&mut ordered)?;
        let mut canonical = Map::new();
        for key in [
            "command",
            "path",
            "scope",
            "timeout_seconds",
            "pm_context_mode",
            "workspace_context_mode",
            "env_set",
            "env_clear",
            "shared_host_safe",
            "assert_stdout_contains",
            "assert_stdout_regex",
            "assert_stderr_contains",
            "assert_stderr_regex",
            "assert_stdout_min_lines",
            "assert_json_field_equals",
            "assert_json_field_gte",
            "note",
        ] {
            if let Some(value) = ordered.get(key) {
                canonical.insert(key.to_owned(), value.clone());
            }
        }
        ordered = Value::Object(canonical);
    }
    Ok(ordered)
}

/// Converts linked test pair values to their canonical typed metadata.
fn normalize_test_fields(row: &mut Value) -> Result<(), PmRustError> {
    if row["command"].as_str().is_some_and(|s| {
        s.split_whitespace()
            .collect::<Vec<_>>()
            .windows(2)
            .any(|p| p == ["pm", "test-all"])
    }) {
        return Err(refusal(
            "Linked test commands must not invoke \"pm test-all\"; this creates recursive orchestration.",
            2,
            json!({}),
        ));
    }
    let timeout = row
        .get("timeout_seconds")
        .or_else(|| row.get("timeout"))
        .and_then(Value::as_str)
        .map(str::to_owned);
    if let Some(raw) = timeout {
        if row.get("timeout_seconds").is_some()
            && row.get("timeout").is_some()
            && row["timeout_seconds"] != row["timeout"]
        {
            return Err(refusal(
                "--add timeout and timeout_seconds must match when both are provided",
                2,
                json!({}),
            ));
        }
        let number = raw
            .parse::<f64>()
            .ok()
            .filter(|n| n.is_finite())
            .ok_or_else(|| {
                refusal(
                    format!("Invalid timeout_seconds value \"{raw}\""),
                    2,
                    json!({}),
                )
            })?;
        if number.floor() <= 0.0 {
            return Err(refusal(
                "--add timeout_seconds must be a positive integer",
                2,
                json!({}),
            ));
        }
        row.as_object_mut().map(|map| map.shift_remove("timeout"));
        row["timeout_seconds"] =
            serde_json::from_str(&crate::history::javascript_float(number.floor()))
                .unwrap_or_default();
    }
    for (key, domain) in [
        ("pm_context_mode", "schema, tracker, auto, none"),
        ("workspace_context_mode", "source, isolated, snapshot"),
    ] {
        if let Some(raw) = row[key].as_str() {
            let value = raw.trim().to_lowercase();
            if !domain.split(", ").any(|allowed| allowed == value) {
                return Err(refusal(
                    format!("--add {key} must be one of: {domain}"),
                    2,
                    json!({}),
                ));
            }
            row[key] = json!(value);
        }
    }
    for key in [
        "assert_stdout_contains",
        "assert_stdout_regex",
        "assert_stderr_contains",
        "assert_stderr_regex",
        "env_clear",
    ] {
        if let Some(raw) = row[key].as_str() {
            row[key] = json!(
                raw.split(|character| character == ';'
                    || character == '\n'
                    || key == "env_clear" && character == ',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>()
            );
        }
    }
    if let Some(raw) = row["shared_host_safe"].as_str() {
        row["shared_host_safe"] = match raw.trim().to_lowercase().as_str() {
            "true" | "1" | "yes" => json!(true),
            "false" | "0" | "no" => json!(false),
            _ => {
                return Err(refusal(
                    "--add shared_host_safe must be one of true|false|1|0|yes|no",
                    2,
                    json!({}),
                ));
            }
        };
    }
    test_maps(row)
}

/// Parses environment and JSON assertion maps from semicolon-separated pairs.
fn test_maps(row: &mut Value) -> Result<(), PmRustError> {
    if let Some(raw) = row["assert_stdout_min_lines"].as_str() {
        let number = raw.parse::<u64>().map_err(|_| {
            refusal(
                "--add assert_stdout_min_lines must be an integer >= 0",
                2,
                json!({}),
            )
        })?;
        row["assert_stdout_min_lines"] = json!(number);
    }

    for key in [
        "env_set",
        "assert_json_field_equals",
        "assert_json_field_gte",
    ] {
        if let Some(raw) = row[key].as_str() {
            let mut values = Map::new();
            for assignment in raw
                .split([';', '\n'])
                .map(str::trim)
                .filter(|s| !s.is_empty())
            {
                let (name, value) = assignment.split_once('=').filter(|(name,_)|!name.trim().is_empty()).ok_or_else(||refusal(if key=="env_set" {"--add env_set entries must use KEY=VALUE and be separated by semicolons. Example: env_set=PORT=0;PLAYWRIGHT_BASE_URL=http://127.0.0.1:4173".to_owned()} else {format!("--add {key} entries must use path=value and be separated by semicolons")},2,json!({})))?;
                if key == "env_set" {
                    environment_name(name.trim(), key)?;
                } else if value.trim().is_empty() {
                    return Err(refusal(
                        format!("--add {key} entries must include non-empty path and value"),
                        2,
                        json!({}),
                    ));
                }
                let value = if key == "assert_json_field_gte" {
                    let number=value.trim().parse::<f64>().ok().filter(|n|n.is_finite()).ok_or_else(||refusal(format!("--add assert_json_field_gte value for \"{name}\" must be numeric"),2,json!({})))?;
                    json!(number)
                } else {
                    json!(if key == "env_set" {
                        value
                    } else {
                        value.trim()
                    })
                };
                values.insert(name.trim().to_owned(), value);
            }
            if values.is_empty() {
                return Err(refusal(
                    format!(
                        "--add {key} must include at least one {} assignment",
                        if key == "env_set" {
                            "KEY=VALUE"
                        } else {
                            "path=value"
                        }
                    ),
                    2,
                    json!({}),
                ));
            }
            row[key] = Value::Object(values);
        }
    }
    for name in row["env_clear"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
    {
        environment_name(name, "env_clear")?;
    }
    Ok(())
}

/// Enforces the linked-test environment name and protected-key contract.
fn environment_name(name: &str, field: &str) -> Result<(), PmRustError> {
    let mut characters = name.chars();
    if !characters
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        || !characters.all(|c| c.is_ascii_alphanumeric() || c == '_')
    {
        return Err(refusal(
            format!("--add {field} key \"{name}\" is invalid"),
            2,
            json!({}),
        ));
    }
    if [
        "PM_PATH",
        "PM_GLOBAL_PATH",
        "PM_SOURCE_PM_PATH",
        "PM_SOURCE_WORKSPACE_ROOT",
        "PM_BUILD_CONSUMER_LEASE",
        "PM_BUILD_CONSUMER_LEASE_ROOT",
        "PM_SOURCE_CONTEXT_ACCESS",
        "PM_ALLOW_SOURCE_CONTEXT_WRITES",
        "FORCE_COLOR",
    ]
    .contains(&name.to_uppercase().as_str())
    {
        return Err(refusal(
            format!("--add {field} key \"{name}\" is reserved for sandbox safety"),
            2,
            json!({}),
        ));
    }
    Ok(())
}

/// Parsed mutation intent, prepared before lock acquisition and applied under it.
pub(crate) struct Edit {
    /// Canonical new rows.
    pub adds: Vec<Value>,
    /// Selector kind and exact value.
    pub removes: Vec<(String, String)>,
    /// Unique one-based linked test indices.
    pub indices: Vec<usize>,
}

/// Parses collection inputs without reading a potentially stale item snapshot.
#[allow(clippy::too_many_lines)]
pub(crate) fn prepare(
    key: &str,
    options: &CollectionOptions,
    root: &Path,
    cwd: &Path,
) -> Result<Edit, PmRustError> {
    let adds = options
        .add
        .iter()
        .map(|raw| addition(raw, key, root, cwd))
        .collect::<Result<Vec<_>, _>>()?;
    let mut removes = Vec::new();
    for raw in &options.remove {
        let raw = raw.trim();
        if raw.is_empty() {
            return Err(refusal(
                if key == "tests" {
                    "--remove requires command or path value"
                } else {
                    "--remove requires a path value"
                },
                2,
                json!({}),
            ));
        }
        if key == "tests"
            && let Some((kind, value)) = raw.split_once('=')
            && ["command", "path"].contains(&kind.trim().to_lowercase().as_str())
        {
            if value.trim().is_empty() {
                return Err(refusal(
                    "--remove requires a non-empty command=<value> or path=<value>",
                    2,
                    json!({}),
                ));
            }
            removes.push((kind.trim().to_lowercase(), value.trim().to_owned()));
            continue;
        }
        if key == "tests"
            && let Some((name, value)) = raw.split_once('=')
            && !name.is_empty()
            && name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
            && !value.is_empty()
            && !value.chars().any(char::is_whitespace)
        {
            return Err(refusal(
                "--remove structured selectors require command=<value> or path=<value>",
                2,
                json!({}),
            ));
        }
        let (kind, value) = if structured(raw, key == "tests") {
            let pair = pairs(raw, "--remove")?;
            if key != "tests" {
                let unsupported: Vec<_> = pair
                    .keys()
                    .filter(|key| ["note", "scope"].contains(&key.as_str()))
                    .map(|key| format!("\"{key}\""))
                    .collect();
                if !unsupported.is_empty() {
                    return Err(refusal(
                        format!(
                            "--remove identifies a linked artifact by path only and does not accept {}. Pass just the path (path=<value>, path:<value>, or a bare path); record removal context with --message \"<why removed>\".",
                            unsupported.join(", ")
                        ),
                        2,
                        json!({}),
                    ));
                }
                let unknown: Vec<_> = pair
                    .keys()
                    .filter(|key| key.as_str() != "path")
                    .map(|key| format!("\"{key}\""))
                    .collect();
                if !unknown.is_empty() {
                    return Err(refusal(
                        format!(
                            "--remove does not recognize key{} {}. Allowed keys: path.",
                            if unknown.len() > 1 { "s" } else { "" },
                            unknown.join(", ")
                        ),
                        2,
                        json!({}),
                    ));
                }
            }
            let kind = if pair.contains_key("path") {
                "path"
            } else {
                "command"
            };
            (
                kind.to_owned(),
                pair.get(kind)
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned(),
            )
        } else {
            (
                if key == "tests" { "either" } else { "path" }.to_owned(),
                raw.to_owned(),
            )
        };
        if value.is_empty() {
            return Err(refusal(
                if key == "tests" {
                    "--remove requires command=<value> and/or path=<value>"
                } else {
                    "--remove key/value form requires path=<value>"
                },
                2,
                json!({}),
            ));
        }
        removes.push((
            kind,
            if key == "tests" {
                value
            } else {
                anchor(&value, root, cwd)
            },
        ));
    }
    let mut indices = options
        .remove_index
        .iter()
        .map(|raw| {
            raw.trim()
                .parse::<usize>()
                .ok()
                .filter(|n| *n > 0 && n.to_string() == raw.trim())
                .ok_or_else(|| refusal("--remove-index must be a positive integer", 2, json!({})))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut seen = std::collections::BTreeSet::new();
    indices.retain(|index| seen.insert(*index));
    if key != "tests" {
        let mut conflicts: Vec<_> = removes
            .iter()
            .filter(|(_, value)| adds.iter().any(|add| add["path"] == *value))
            .map(|(_, value)| value.clone())
            .collect();
        conflicts.sort_by(|a, b| crate::collation::compare(a, b));
        conflicts.dedup();
        if !conflicts.is_empty() {
            return Err(refusal(
                format!(
                    "Linked artifact mutation cannot add and remove the same normalized path in one transaction: {}. Split the replacement into separate commands so each history event has one unambiguous intent.",
                    conflicts.join(", ")
                ),
                2,
                json!({"code":"linked_artifact_mutation_conflict","reason":"same_path_added_and_removed","required":"Use distinct normalized paths for --add and --remove in one invocation.","why":"Conflicting batch intent cannot be applied deterministically without silently discarding metadata or a removal.","next_steps":["Run the removal and addition as separate commands with a --message describing each intent."]}),
            ));
        }
    }
    Ok(Edit {
        adds,
        removes,
        indices,
    })
}

/// Applies additions and removals to the latest locked collection.
pub(crate) fn merge(
    key: &str,
    current: &[Value],
    edit: &Edit,
    stable: bool,
) -> (Vec<Value>, usize) {
    let mut rows = Vec::<Value>::new();
    for row in current {
        if key != "tests"
            && let Some(previous) = rows.iter_mut().find(|previous| {
                previous["path"] == row["path"] && previous["scope"] == row["scope"]
            })
        {
            *previous = row.clone();
        } else {
            rows.push(row.clone());
        }
    }
    for add in &edit.adds {
        let existing = rows.iter_mut().find(|row| {
            if key == "tests" {
                [
                    "command",
                    "path",
                    "scope",
                    "pm_context_mode",
                    "workspace_context_mode",
                ]
                .iter()
                .all(|key| row[*key] == add[*key])
            } else {
                row["path"] == add["path"] && row["scope"] == add["scope"]
            }
        });
        if let Some(existing) = existing {
            if key != "tests" && add.get("note").is_some() {
                existing["note"] = add["note"].clone();
            }
        } else {
            rows.push(add.clone());
        }
    }
    let before = rows.len();
    rows = rows
        .into_iter()
        .enumerate()
        .filter(|(i, row)| {
            !edit.indices.contains(&(i + 1))
                && !edit.removes.iter().any(|(kind, value)| {
                    if kind == "either" {
                        row["command"] == *value || row["path"] == *value
                    } else {
                        row[kind] == *value
                    }
                })
        })
        .map(|(_, row)| row)
        .collect();
    let removed = before - rows.len();
    if key != "tests" && !stable {
        rows.sort_by(|a, b| {
            crate::collation::compare(
                a["path"].as_str().unwrap_or_default(),
                b["path"].as_str().unwrap_or_default(),
            )
            .then_with(|| {
                crate::collation::compare(
                    a["scope"].as_str().unwrap_or_default(),
                    b["scope"].as_str().unwrap_or_default(),
                )
            })
        });
    }
    (rows, removed)
}

impl Workspace {
    /// Lists or atomically edits files, docs or tests using the published receipts.
    ///
    /// # Errors
    /// Returns typed parsing, missing-item, ownership, lock or storage refusals.
    pub fn collection(
        &self,
        id: &str,
        key: &str,
        options: &CollectionOptions,
        cwd: &Path,
    ) -> Result<Value, PmRustError> {
        if !matches!(key, "files" | "docs" | "tests") {
            return Err(PmRustError::InvalidMutationRequest {
                reason: "collection must be files, docs or tests".to_owned(),
            });
        }
        if key != "tests" && !options.remove_index.is_empty() {
            return Err(refusal(
                "Commander does not recognize option --remove-index for this command path.",
                2,
                json!({"code":"unknown_option","required":"Use an option declared on this path, or move the declared option to one of its accepting command paths.","recovery":{"candidate_commands":["item test"],"candidate_commands_total":1,"option_scope":"declared_elsewhere"},"next_steps":["Run command help to confirm the exact option contracts for this command path.","--remove-index is accepted by item test. Inspect those command contracts only if you intended a different operation."],"title":"Unknown option --remove-index","examples":[format!("pm item {key} --help")],"flag":"--remove-index","value":"--remove-index"}),
            ));
        }
        let root = self
            .pm_root()
            .parent()
            .and_then(Path::parent)
            .unwrap_or(self.pm_root());
        let edit = prepare(key, options, root, cwd)?;
        if options.note.is_some() && edit.adds.is_empty() {
            return Err(refusal(
                "--note requires --add or --add-glob in the same invocation (the note annotates the links being added)",
                2,
                json!({}),
            ));
        }
        let (document, changed, removed) =
            if edit.adds.is_empty() && edit.removes.is_empty() && edit.indices.is_empty() {
                (
                    self.get(id).map_err(|error| match error {
                        PmRustError::ItemNotFound { .. } => {
                            refusal(format!("Item {id} not found"), 3, json!({}))
                        }
                        error => error,
                    })?,
                    false,
                    0,
                )
            } else {
                crate::ownership::with_identity(
                    self.pm_root(),
                    &crate::OwnershipOptions {
                        identity: options.identity.clone(),
                        ..crate::OwnershipOptions::default()
                    },
                    || crate::mutation::collection_item(self.pm_root(), id, key, options, edit),
                )?
            };
        let rows = document
            .metadata
            .extra
            .get(key)
            .cloned()
            .unwrap_or_else(|| json!([]));
        if key == "tests" {
            return Ok(json!({"ok":true,"id":document.metadata.id,"tests":rows,"run_results":[],"failure_categories":{"infra_collision":0,"trust_refusal":0,"config_refusal":0,"assertion_failure":0,"empty_run":0,"timeout":0,"max_buffer":0,"spawn_error":0,"signal":0},"changed":changed,"count":rows.as_array().map_or(0,Vec::len)}).as_object().map(|map|{let mut map=map.clone();if removed>0 {map.insert("removed".to_owned(),json!(removed));}Value::Object(map)}).unwrap_or_default());
        }
        let mut result = json!({"id":document.metadata.id,key:rows,"changed":changed,"count":rows.as_array().map_or(0,Vec::len)});
        if changed {
            let mut paths: Vec<_> = rows
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|r| r["path"].as_str())
                .collect();
            paths.sort_by(|a, b| crate::collation::compare(a, b));
            paths.dedup();
            let mut existing = Vec::new();
            let mut missing = Vec::new();
            let mut nonfile = Vec::new();
            let mut remote = Vec::new();
            for path in &paths {
                if path.contains("://") {
                    remote.push(*path);
                } else {
                    match std::fs::metadata(root.join(path)) {
                        Ok(m) if m.is_file() => existing.push(*path),
                        Err(e) if e.kind() == std::io::ErrorKind::NotFound => missing.push(*path),
                        Ok(_) | Err(_) => nonfile.push(*path),
                    }
                }
            }
            result["validation"] = json!({"checked":paths.len(),"existing_files":existing,"missing_paths":missing,"non_file_paths":nonfile,"remote_references":remote});
        }
        Ok(result)
    }
}
