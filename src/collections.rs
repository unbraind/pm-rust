//! Native linked artifact and linked test collection contracts.

use crate::{PmRustError, Workspace};
use serde_json::{Map, Value, json};
use std::path::{Component, Path, PathBuf};

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
    /// Include stored history alongside the entries written by this mutation.
    pub full_history: bool,
    /// Artifact note conflict policy, defaulting to refusal.
    pub note_conflict: Option<String>,
}

#[cfg(test)]
#[path = "../tests/support/collections_unit.rs"]
mod tests;

/// Produces a published command failure with optional contract guidance.
pub(crate) fn refusal(detail: impl Into<String>, exit_code: u8, context: Value) -> PmRustError {
    let (detail, context) = crate::refusal::sdk_fields(detail.into(), context, None);
    PmRustError::CommandRefusal {
        detail,
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
        // Hooks can export coordinates for a different repository. Resolve
        // provenance solely from the selected workspace, including worktrees.
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE")
        .env_remove("GIT_OBJECT_DIRECTORY")
        .env_remove("GIT_ALTERNATE_OBJECT_DIRECTORIES")
        .env_remove("GIT_COMMON_DIR")
        .env_remove("GIT_NAMESPACE")
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
        crate::strict_json::from_str::<String>(value)
            .unwrap_or_else(|_| inner.replace("\\\"", "\""))
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

/// Maps external aliases into the project while retaining logical paths that point outside.
fn resolve_artifact_cwd(path: &Path, root: &Path) -> Result<PathBuf, PmRustError> {
    let absolute = std::path::absolute(path).map_err(|source| PmRustError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let mut normalized = PathBuf::new();
    for component in absolute.components() {
        if component == Component::ParentDir {
            normalized.pop();
        } else {
            normalized.push(component.as_os_str());
        }
    }
    // The nearest existing ancestor must be a directory, including on platforms
    // that report a file in an intermediate component as a missing path.
    let directory =
        normalized
            .ancestors()
            .find_map(|candidate| match std::fs::metadata(candidate) {
                Ok(metadata) => Some(metadata.is_dir()),
                Err(error) if error.kind() == std::io::ErrorKind::NotADirectory => Some(false),
                Err(_) => None,
            });
    if directory == Some(false) {
        return Err(refusal(
            "--artifact-cwd must name a directory",
            2,
            json!({}),
        ));
    }
    if normalized.starts_with(root) {
        return Ok(normalized);
    }
    // Stop canonicalizing at the first project prefix so in-project symlinks
    // remain part of the logical link, regardless of where they point.
    let ancestors: Vec<_> = normalized.ancestors().collect();
    for ancestor in ancestors.into_iter().rev() {
        if let Ok(canonical) = ancestor.canonicalize()
            && canonical.starts_with(root)
        {
            return Ok(canonical.join(normalized.strip_prefix(ancestor).unwrap_or(Path::new(""))));
        }
    }
    Ok(normalized)
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
            crate::strict_json::from_str(&crate::history::javascript_float(number.floor()))
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
    note_policy: NotePolicy,
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
        note_policy: NotePolicy::Refuse,
    })
}

/// Files and docs share one canonical path and scope identity.
fn same_artifact(a: &Value, b: &Value) -> bool {
    a["path"] == b["path"] && a["scope"] == b["scope"]
}

/// Validates the explicit repair policy before opening an item for mutation.
fn note_policy(options: &CollectionOptions, has_adds: bool) -> Result<NotePolicy, PmRustError> {
    let Some(raw) = options.note_conflict.as_deref() else {
        return Ok(NotePolicy::Refuse);
    };
    let mode = raw.trim();
    let Some((_, mode)) = [
        ("refuse", NotePolicy::Refuse),
        ("replace", NotePolicy::Replace),
        ("append", NotePolicy::Append),
    ]
    .into_iter()
    .find(|(value, _)| mode.eq_ignore_ascii_case(value)) else {
        return Err(refusal(
            format!("--note-conflict must be one of refuse, replace, append; got \"{raw}\""),
            2,
            json!({"code":"invalid_argument_value","flag":"--note-conflict","value":raw,"required":"Pass --note-conflict refuse|replace|append."}),
        ));
    };
    if !has_adds {
        return Err(refusal(
            "--note-conflict requires --add or --add-glob in the same invocation (it decides how those links treat an existing note)",
            2,
            json!({"code":"missing_required_argument","flag":"--note-conflict","required":"Pass --add or --add-glob with --note-conflict."}),
        ));
    }
    Ok(mode)
}

/// Builds retry guidance from the original linked-artifact arguments.
fn note_conflict_refusal(
    id: &str,
    key: &str,
    options: &CollectionOptions,
    conflicts: &[Value],
    retry_scope: Option<(&Path, &Path)>,
) -> PmRustError {
    let first = &conflicts[0];
    let path = first["path"].as_str().unwrap_or_default();
    let noun = if key == "files" { "file" } else { "doc" };
    let more = if conflicts.len() > 1 {
        format!(" and {} more", conflicts.len() - 1)
    } else {
        String::new()
    };
    let existing = first["existing_note"].as_str().unwrap_or_default();
    let requested = first["requested_note"].as_str().unwrap_or_default();
    let detail = if first["source"] == "stored" {
        format!(
            "{id} already links {noun} {path} with the note \"{existing}\"{more}; the add was refused so that note is kept"
        )
    } else {
        format!(
            "The add names {noun} {path} on {id} with two notes, \"{existing}\" and then \"{requested}\"{more}; the add was refused because they disagree"
        )
    };
    let mut context = json!({
        "code":"linked_artifact_note_conflict",
        "why":"A different stored note is usually another agent's rationale, so --add never overwrites it silently.",
        "required":"Re-run with --note-conflict append to keep both notes, or --note-conflict replace to overwrite the stored note."
    });
    if options
        .identity
        .as_ref()
        .is_some_and(|identity| identity.source == "detected")
    {
        context["required"] = json!(
            "Run the repair in the original detected session with --note-conflict append or replace, or retry through the SDK with the original AuthorIdentity and the chosen note conflict policy."
        );
    }
    let (binary, base_args) = if let Some((pm_root, cwd)) = retry_scope {
        let Some((path, cwd)) = pm_root.to_str().zip(cwd.to_str()) else {
            context["next_steps"] = json!([
                "Run the repair from the original directory with the same tracker selected. The tracker or invocation path cannot be encoded as UTF-8 retry arguments."
            ]);
            context["details"] = json!({"conflicts":conflicts});
            return refusal(detail, 4, context);
        };
        // Tracker selection and operand anchoring are independent.
        (
            "pm-rust",
            ["--pm-path", path, key, id, "--artifact-cwd", cwd]
                .map(str::to_owned)
                .to_vec(),
        )
    } else {
        ("pm", vec![key.to_owned(), id.to_owned()])
    };
    let native = retry_scope.is_some();
    let append = note_retry_args(base_args.clone(), options, "append", native);
    let append_command = crate::command_line::render_command(binary, &append);
    let next_steps = json!([
        append_command,
        crate::command_line::render_command(
            binary,
            note_retry_args(base_args, options, "replace", native)
        )
    ]);
    context["next_steps"] = next_steps.clone();
    context["recovery"] = json!({"suggested_retry":append_command,"suggested_retry_args":append});
    context["details"] = json!({"conflicts":conflicts});
    let (detail, mut context) = crate::refusal::sdk_fields(detail, context, None);
    if native
        && (
            &context["next_steps"],
            &context["recovery"]["suggested_retry_args"],
        ) != (&next_steps, &json!(append))
    {
        context
            .as_object_mut()
            .and_then(|fields| fields.shift_remove("recovery"));
        context["next_steps"] = json!([
            "Repeat the original request from the original directory with the same tracker selected. Supply the original note or other redacted values again and choose --note-conflict append or replace."
        ]);
    }
    PmRustError::CommandRefusal {
        detail,
        exit_code: 4,
        context,
    }
}

/// Replays the original linked-artifact edit with an explicit conflict policy.
fn note_retry_args(
    mut args: Vec<String>,
    options: &CollectionOptions,
    mode: &str,
    native: bool,
) -> Vec<String> {
    for add in &options.add {
        args.extend(["--add".to_owned(), add.clone()]);
    }
    for remove in &options.remove {
        args.extend(["--remove".to_owned(), remove.clone()]);
    }
    if let Some(note) = &options.note {
        if native && note.starts_with('-') {
            args.push(format!("--note={note}"));
        } else {
            args.extend(["--note".to_owned(), note.clone()]);
        }
    }
    args.extend(["--note-conflict".to_owned(), mode.to_owned()]);
    if options.force {
        args.push("--force".to_owned());
    }
    if options.append_stable {
        args.push("--append-stable".to_owned());
    }
    if options.full_history {
        args.push("--full-history".to_owned());
    }
    if let Some(message) = &options.message {
        if native && message.starts_with('-') {
            args.push(format!("--message={message}"));
        } else {
            args.extend(["--message".to_owned(), message.clone()]);
        }
    }
    if let Some(timestamp) = &options.timestamp {
        args.extend(["--timestamp".to_owned(), timestamp.clone()]);
    }
    if options
        .identity
        .as_ref()
        .is_none_or(|identity| identity.source != "detected")
    {
        if native && options.author.starts_with('-') {
            args.push(format!("--author={}", options.author));
        } else {
            args.extend(["--author".to_owned(), options.author.clone()]);
        }
    }
    args
}

/// Resolves note edits in request order and refuses the whole write on a conflict.
pub(crate) fn reconcile_artifact_notes(
    id: &str,
    key: &str,
    current: &[Value],
    edit: &mut Edit,
    options: &CollectionOptions,
    retry_scope: Option<(&Path, &Path)>,
) -> Result<Vec<Value>, PmRustError> {
    let mode = edit.note_policy;
    let mut working: Vec<(Value, bool)> = current
        .iter()
        .cloned()
        .map(|entry| (entry, false))
        .collect();
    let mut conflicts = Vec::new();
    for add in &mut edit.adds {
        let Some((entry, edited)) = working
            .iter_mut()
            .rev()
            .find(|(entry, _)| same_artifact(entry, add))
        else {
            working.push((add.clone(), add.get("note").is_some()));
            continue;
        };
        let Some(requested) = add.get("note").and_then(Value::as_str) else {
            continue;
        };
        let existing = entry["note"].as_str();
        if existing == Some(requested) {
            continue;
        }
        let revised = match (existing, mode) {
            (None, _) | (_, NotePolicy::Replace) => requested.to_owned(),
            (Some(existing), NotePolicy::Append) => {
                if format!("; {existing}; ").contains(&format!("; {requested}; ")) {
                    existing.to_owned()
                } else {
                    format!("{existing}; {requested}")
                }
            }
            (Some(existing), NotePolicy::Refuse) => {
                conflicts.push(json!({"path":add["path"],"scope":add["scope"],"existing_note":existing,"requested_note":requested,"source":if *edited {"this_write"} else {"stored"}}));
                continue;
            }
        };
        *edited |= existing != Some(revised.as_str());
        add["note"] = json!(revised);
        entry["note"] = add["note"].clone();
    }
    if !conflicts.is_empty() {
        return Err(note_conflict_refusal(
            id,
            key,
            options,
            &conflicts,
            retry_scope,
        ));
    }
    let mut outcomes = Vec::<Value>::new();
    for add in &edit.adds {
        if outcomes.iter().any(|outcome| same_artifact(outcome, add)) {
            continue;
        }
        let before = current.iter().rev().find(|entry| same_artifact(entry, add));
        let after = working
            .iter()
            .rev()
            .find(|(entry, _)| same_artifact(entry, add))
            .map(|(entry, _)| &entry["note"]);
        let outcome = match before {
            None => "added",
            Some(entry) if after == Some(&entry["note"]) => "unchanged",
            Some(entry) if entry["note"].is_null() => "note_added",
            Some(_) if mode == NotePolicy::Append => "note_appended",
            Some(_) => "note_replaced",
        };
        outcomes.push(json!({"path":add["path"],"scope":add["scope"],"outcome":outcome}));
    }
    Ok(outcomes)
}

/// Entries and receipts produced from the snapshot held by a collection mutation.
#[derive(Default)]
pub(crate) struct CollectionChange {
    /// Whether the persisted collection changed at all.
    pub(crate) changed: bool,
    /// Count of exact selectors that removed a row.
    pub(crate) removed: usize,
    /// Entries added or rewritten, in stored order.
    pub(crate) written: Vec<Value>,
    /// Per-argument linked artifact outcomes.
    pub(crate) outcomes: Vec<Value>,
}

impl CollectionChange {
    /// Captures changed entries using multiset equality against the previous snapshot.
    pub(crate) fn between(
        current: &[Value],
        rows: &[Value],
        removed: usize,
        outcomes: Vec<Value>,
    ) -> Self {
        let mut unmatched: Vec<_> = current.iter().collect();
        let written = rows
            .iter()
            .filter(|row| {
                if let Some(index) = unmatched.iter().position(|previous| *previous == *row) {
                    unmatched.remove(index);
                    false
                } else {
                    true
                }
            })
            .cloned()
            .collect();
        Self {
            changed: current != rows,
            removed,
            written,
            outcomes,
        }
    }
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

fn refuse_collection_option(command: &str, flag: &str, candidates: &[&str]) -> PmRustError {
    refusal(
        format!("Commander does not recognize option {flag} for this command path."),
        2,
        json!({
            "code":"unknown_option",
            "required":"Use an option declared on this path, or move the declared option to one of its accepting command paths.",
            "recovery":{"candidate_commands":candidates,"candidate_commands_total":candidates.len(),"option_scope":"declared_elsewhere"},
            "next_steps":[
                "Run command help to confirm the exact option contracts for this command path.",
                format!("{flag} is accepted by {}. Inspect those command contracts only if you intended a different operation.", candidates.join(", "))
            ],
            "title":format!("Unknown option {flag}"),
            "examples":[crate::render_pm_command(["item",command,"--help"])],
            "flag":flag,
            "value":flag
        }),
    )
}

impl Workspace {
    /// Gets the root against which project-relative artifact paths are stored.
    fn artifact_project_root(&self) -> &Path {
        let pm_root = self.pm_root();
        pm_root.parent().and_then(Path::parent).unwrap_or(pm_root)
    }

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
        let artifact_cwd = (matches!(key, "files" | "docs")
            && !(options.add.is_empty() && options.remove.is_empty()))
        .then(|| resolve_artifact_cwd(cwd, self.artifact_project_root()))
        .transpose()?;
        let retry_scope = artifact_cwd
            .as_deref()
            .map(|directory| (self.pm_root(), directory));
        self.collection_with_retry_scope(
            id,
            key,
            options,
            None,
            artifact_cwd.as_deref().unwrap_or(cwd),
            retry_scope,
        )
    }

    /// Runs a collection with retry scope projected from the original CLI invocation.
    ///
    /// # Errors
    /// Returns the same collection refusals as [`Self::collection`].
    #[doc(hidden)]
    pub fn collection_for_cli(
        &self,
        id: &str,
        key: &str,
        options: &CollectionOptions,
        cwd: &Path,
    ) -> Result<Value, PmRustError> {
        self.collection_for_cli_with_artifact_cwd(id, key, options, cwd, None)
    }

    /// Runs a CLI collection, optionally anchoring artifact operands at an SDK retry's directory.
    ///
    /// # Errors
    /// Returns the same collection refusals as [`Self::collection`].
    #[doc(hidden)]
    pub fn collection_for_cli_with_artifact_cwd(
        &self,
        id: &str,
        key: &str,
        options: &CollectionOptions,
        cwd: &Path,
        artifact_cwd: Option<&Path>,
    ) -> Result<Value, PmRustError> {
        let needs_native = artifact_cwd.is_some()
            || options.timestamp.is_some()
            || (key == "docs" && options.append_stable);
        let artifact_cwd = needs_native
            .then(|| {
                resolve_artifact_cwd(
                    &cwd.join(artifact_cwd.unwrap_or(Path::new(""))),
                    self.artifact_project_root(),
                )
            })
            .transpose()?;
        let retry_scope = artifact_cwd
            .as_deref()
            .map(|directory| (self.pm_root(), directory));
        self.collection_with_retry_scope(
            id,
            key,
            options,
            Some(cwd),
            artifact_cwd.as_deref().unwrap_or(cwd),
            retry_scope,
        )
    }

    /// Runs the shared collection operation with the caller's retry scope policy.
    fn collection_with_retry_scope(
        &self,
        id: &str,
        key: &str,
        options: &CollectionOptions,
        cli_cwd: Option<&Path>,
        artifact_cwd: &Path,
        retry_scope: Option<(&Path, &Path)>,
    ) -> Result<Value, PmRustError> {
        if !matches!(key, "files" | "docs" | "tests") {
            return Err(PmRustError::InvalidMutationRequest {
                reason: "collection must be files, docs or tests".to_owned(),
            });
        }
        if key != "tests" && !options.remove_index.is_empty() {
            return Err(refuse_collection_option(
                key,
                "--remove-index",
                &["item test"],
            ));
        }
        if key == "tests" && options.note_conflict.is_some() {
            return Err(refuse_collection_option(
                "test",
                "--note-conflict",
                &["item docs", "item files"],
            ));
        }
        let pm_root = self.pm_root();
        let root = self.artifact_project_root();
        let mut edit = prepare(key, options, root, artifact_cwd)?;
        if options.note.is_some() && edit.adds.is_empty() {
            return Err(refusal(
                "--note requires --add or --add-glob in the same invocation (the note annotates the links being added)",
                2,
                json!({}),
            ));
        }
        edit.note_policy = note_policy(options, !edit.adds.is_empty())?;
        let write = !edit.adds.is_empty() || !edit.removes.is_empty() || !edit.indices.is_empty();
        let (document, mutation) = if write {
            crate::ownership::with_identity(
                pm_root,
                &crate::OwnershipOptions {
                    identity: options.identity.clone(),
                    ..crate::OwnershipOptions::default()
                },
                || crate::mutation::collection_item(pm_root, id, key, options, edit, retry_scope),
            )?
        } else {
            (
                self.get(id).map_err(|error| match error {
                    PmRustError::ItemNotFound { .. } => {
                        refusal(format!("Item {id} not found"), 3, json!({}))
                    }
                    error => error,
                })?,
                CollectionChange::default(),
            )
        };
        let full = document
            .metadata
            .extra
            .get(key)
            .and_then(Value::as_array)
            .map_or(&[][..], Vec::as_slice);
        let total = full.len();
        let rows = if write && !options.full_history {
            &mutation.written[..]
        } else {
            full
        };
        let count = rows.len();
        let changed = mutation.changed;
        let mut result = if key == "tests" {
            let mut result = json!({"ok":true,"id":document.metadata.id,"tests":rows,"run_results":[],"changed":changed,"count":count});
            if mutation.removed > 0 {
                result["removed"] = json!(mutation.removed);
            }
            result
        } else {
            json!({"id":document.metadata.id,key:rows,"changed":changed,"count":count})
        };
        if write {
            attach_collection_counts(&mut result, total, count);
            if key != "tests" && changed && count > 0 {
                result["validation"] = validate_link_paths(root, rows);
            }
            attach_collection_receipts(
                &mut result,
                key,
                id,
                pm_root,
                cli_cwd,
                &mutation,
                count < total,
            );
        }
        Ok(result)
    }
}

/// Attaches delivered and stored counts before validation details.
fn attach_collection_counts(result: &mut Value, total: usize, count: usize) {
    result["total_count"] = json!(total);
    result["returned_count"] = json!(count);
    result["has_more"] = json!(count < total);
}

/// Reports the exact mutation and how to expand an incomplete response.
fn attach_collection_receipts(
    result: &mut Value,
    key: &str,
    id: &str,
    pm_root: &Path,
    cli_cwd: Option<&Path>,
    changed: &CollectionChange,
    withheld: bool,
) {
    let mut receipt = json!({"changed_count":changed.written.len(),"removed_count":changed.removed,"full_history_included":!withheld});
    if !changed.outcomes.is_empty() {
        receipt["link_outcomes"] = json!(changed.outcomes);
    }
    result["mutation_receipt"] = receipt;
    let groups = if withheld {
        let command = if key == "tests" { "test" } else { key };
        // Discovery canonicalizes the tracker. Resolve the invocation directory on the same
        // basis so symlinked temporary roots and Windows short paths retain relative recovery.
        let relative = cli_cwd.and_then(|cwd| {
            let canonical = cwd.canonicalize();
            pm_root
                .strip_prefix(canonical.as_deref().unwrap_or(cwd))
                .ok()
        });
        let scope = match relative {
            Some(relative) if relative.as_os_str().is_empty() => Some(".".to_owned()),
            Some(relative) => relative
                .to_str()
                .map(|path| path.replace(std::path::MAIN_SEPARATOR, "/")),
            None => pm_root.to_str().map(str::to_owned),
        };
        let name = format!("{key}_history");
        if let Some(scope) = scope {
            json!([{"name":name,"restore_with":crate::render_pm_command(["--pm-path", &scope, command, id])}])
        } else {
            json!([{"name":name,"restore_guidance":format!("Select the original tracker manually and list {key} for {id}. The tracker scope cannot be encoded as UTF-8.")}])
        }
    } else {
        json!([])
    };
    result["omission_receipt"] = json!({"has_omissions":withheld,"omitted_field_group_count":usize::from(withheld),"omitted_field_groups":groups});
}

/// Checks only the links returned by the current response.
fn validate_link_paths(root: &Path, rows: &[Value]) -> Value {
    let mut paths: Vec<_> = rows.iter().filter_map(|r| r["path"].as_str()).collect();
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
    json!({"checked":paths.len(),"existing_files":existing,"missing_paths":missing,"non_file_paths":nonfile,"remote_references":remote})
}

/// Repair policy validated before any linked-artifact mutation opens the tracker.
#[derive(Clone, Copy, PartialEq, Eq)]
enum NotePolicy {
    Refuse,
    Replace,
    Append,
}
