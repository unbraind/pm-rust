//! Bounded, read-only local agent metadata used by collection history.
use crate::AuthorIdentity;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, VecDeque},
    fs,
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
};

#[cfg(test)]
#[path = "../tests/support/identity_probes_unit.rs"]
mod tests;

/// Trims and bounds one optional metadata string.
fn text(value: &str) -> Option<String> {
    let value =
        value.trim_matches(|c: char| c == '\u{feff}' || c != '\u{0085}' && c.is_whitespace());
    (!value.is_empty()).then(|| value.chars().take(256).collect())
}

/// Reads one optional environment field without retaining its original buffer.
fn env_text(env: &BTreeMap<String, String>, key: &str) -> Option<String> {
    env.get(key).and_then(|value| text(value))
}

/// Tests whether a dimension has an actual resolved value.
fn resolved(identity: &AuthorIdentity, key: &str) -> bool {
    identity
        .provenance
        .get(key)
        .is_some_and(|value| !value.is_null())
}

/// Returns the platform's explicit home-directory environment setting.
fn home(env: &BTreeMap<String, String>) -> Option<PathBuf> {
    #[cfg(windows)]
    let names = ["USERPROFILE", "HOME"];
    #[cfg(not(windows))]
    let names = ["HOME", "USERPROFILE"];
    names
        .into_iter()
        .find_map(|key| {
            env.get(key)
                .map(String::as_str)
                .filter(|value| !value.is_empty())
        })
        .map(PathBuf::from)
}

/// Bounded local file resolution result; failure is optional metadata, never a mutation failure.
#[derive(Default)]
struct Probe {
    /// Resolver input/file availability, before optional file contents are parsed.
    status: &'static str,
    /// Values found in bounded JSONL input.
    values: BTreeMap<String, String>,
}

/// Validates the oracle's ASCII session-file identity domain.
fn session(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
}

/// Enumerates sorted regular directory entries without following symlinks.
fn entries(path: &Path, descending: bool) -> std::io::Result<Vec<fs::DirEntry>> {
    let mut rows: Vec<_> = fs::read_dir(path)?.filter_map(Result::ok).collect();
    rows.sort_by(|a, b| {
        crate::collation::compare(
            &a.file_name().to_string_lossy(),
            &b.file_name().to_string_lossy(),
        )
    });
    if descending {
        rows.reverse();
    }
    Ok(rows)
}

/// Finds the first newest Codex session suffix within four levels and 512 visited entries.
fn codex_file(root: &Path, suffix: &str, depth: usize, visited: &mut usize) -> Option<PathBuf> {
    if depth > 4 || *visited >= 512 {
        return None;
    }
    for entry in entries(root, true).unwrap_or_default() {
        *visited += 1;
        if *visited > 512 {
            return None;
        }
        let kind = entry.file_type().ok();
        if kind.as_ref().is_some_and(fs::FileType::is_file)
            && entry.file_name().to_string_lossy().ends_with(suffix)
        {
            return Some(entry.path());
        }
        if kind.as_ref().is_some_and(fs::FileType::is_dir)
            && let Some(path) = codex_file(&entry.path(), suffix, depth + 1, visited)
        {
            return Some(path);
        }
    }
    None
}

/// Classifies a regular file without following a symbolic link.
fn file_status(path: &Path) -> &'static str {
    match fs::symlink_metadata(path) {
        Ok(value) if value.is_file() => "available",
        Ok(_) => "missing",
        Err(error)
            if [
                std::io::ErrorKind::NotFound,
                std::io::ErrorKind::NotADirectory,
            ]
            .contains(&error.kind()) =>
        {
            "missing"
        }
        Err(_) => "failed",
    }
}

/// Resolves Claude's preferred cwd file, then a unique project fallback.
fn claude_file(root: &Path, id: &str, cwd: &Path) -> (Option<PathBuf>, &'static str) {
    let slug: String = cwd
        .to_string_lossy()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '-'
            }
        })
        .collect();
    let preferred = root.join(slug).join(format!("{id}.jsonl"));
    match file_status(&preferred) {
        "available" => return (Some(preferred), "available"),
        "failed" => return (None, "failed"),
        _ => {}
    }
    claude_fallback(root, id)
}

/// Scans a bounded set of project directories for a unique Claude session file.
fn claude_fallback(root: &Path, id: &str) -> (Option<PathBuf>, &'static str) {
    let directories = match entries(root, false) {
        Ok(value) => value,
        Err(error) => {
            return (
                None,
                if [
                    std::io::ErrorKind::NotFound,
                    std::io::ErrorKind::NotADirectory,
                ]
                .contains(&error.kind())
                {
                    "missing"
                } else {
                    "failed"
                },
            );
        }
    };
    let candidates = directories
        .into_iter()
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .take(512)
        .map(|directory| directory.path().join(format!("{id}.jsonl")));
    claude_candidates(candidates.collect())
}

/// Preserves failed candidate probes and ambiguity instead of selecting an arbitrary file.
fn claude_candidates(paths: Vec<PathBuf>) -> (Option<PathBuf>, &'static str) {
    let mut matches = Vec::new();
    let mut failed = false;
    for path in paths {
        match file_status(&path) {
            "available" => matches.push(path),
            "failed" => failed = true,
            _ => {}
        }
    }
    match matches.len() {
        0 => (None, if failed { "failed" } else { "missing" }),
        1 => (matches.pop(), "available"),
        _ => (None, "ambiguous"),
    }
}

/// Adds first-seen strings from a JSON object, visiting at most 64 objects and four levels.
fn claude_row(value: &Value, found: &mut BTreeMap<String, String>) {
    let mut queue = VecDeque::from([(value, 0)]);
    let mut visited = 0;
    while let Some((value, depth)) = queue.pop_front() {
        if visited >= 64 {
            break;
        }
        visited += 1;
        let Some(object) = value.as_object() else {
            continue;
        };
        for key in ["model", "version"] {
            if !found.contains_key(key)
                && let Some(value) = object.get(key).and_then(Value::as_str).and_then(text)
            {
                found.insert(key.to_owned(), value);
            }
        }
        if found.len() == 2 {
            break;
        }
        if depth < 4 {
            queue.extend(
                object
                    .values()
                    .filter(|value| value.is_object())
                    .map(|value| (value, depth + 1)),
            );
        }
    }
}

/// Parses one bounded JSONL row according to its provider's record contract.
fn row(raw: &str, codex: bool, found: &mut BTreeMap<String, String>) {
    if raw.is_empty() || raw.encode_utf16().count() > 262_144 {
        return;
    }
    let Ok(value) = serde_json::from_str::<Value>(raw) else {
        return;
    };
    if codex {
        if value["type"] != "turn_context" {
            return;
        }
        for key in ["model", "effort"] {
            if !found.contains_key(key)
                && let Some(value) = value["payload"][key].as_str().and_then(text)
            {
                found.insert(key.to_owned(), value);
            }
        }
    } else {
        claude_row(&value, found);
    }
}

/// Seekable byte source shared by real files and optional-metadata I/O boundary tests.
trait ProbeRead: Read + Seek {}
impl<T: Read + Seek> ProbeRead for T {}

/// Reads at most four MiB for Codex or one MiB for Claude, preserving provider line order.
fn read_probe(
    reader: &mut dyn ProbeRead,
    codex: bool,
) -> std::io::Result<BTreeMap<String, String>> {
    let size = usize::try_from(reader.seek(SeekFrom::End(0))?).unwrap_or(usize::MAX);
    let cap = if codex { 4_194_304 } else { 1_048_576 };
    let slices = if codex {
        if size <= cap {
            vec![(0, size)]
        } else {
            vec![(size - cap / 2, cap / 2), (0, cap / 2)]
        }
    } else {
        let amount = size.min(cap);
        let head = amount.div_ceil(2);
        vec![(0, head), (size - (amount - head), amount - head)]
    };
    let mut buffers = Vec::new();
    for (offset, length) in slices {
        reader.seek(SeekFrom::Start(offset as u64))?;
        let mut bytes = vec![0; length];
        reader.read_exact(&mut bytes)?;
        buffers.push(bytes);
    }
    if !codex && size <= cap {
        buffers = vec![buffers.concat()];
    }
    let mut found = BTreeMap::new();
    let mut remaining = 4096;
    for (index, bytes) in buffers.into_iter().enumerate() {
        let raw = String::from_utf8_lossy(&bytes);
        let rows: Vec<_> = raw.split('\n').collect();
        let selected: Vec<_> = if codex {
            rows.into_iter().rev().take(remaining).collect()
        } else if size > cap && index > 0 {
            rows.into_iter()
                .rev()
                .take(4096)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect()
        } else {
            rows.into_iter().take(4096).collect()
        };
        remaining -= selected.len().min(remaining);
        for line in selected {
            row(line, codex, &mut found);
            if found.len() == 2 {
                break;
            }
        }
        if found.len() == 2 || codex && remaining == 0 {
            break;
        }
    }
    Ok(found)
}

/// Finds and reads provider-owned session records with no network or subprocess access.
fn local_probe(harness: &str, env: &BTreeMap<String, String>, cwd: &Path) -> Probe {
    let key = if harness == "codex" {
        "CODEX_THREAD_ID"
    } else {
        "CLAUDE_CODE_SESSION_ID"
    };
    let Some(id) = env_text(env, key) else {
        return Probe {
            status: "unavailable",
            ..Probe::default()
        };
    };
    let codex = harness == "codex";
    if !session(&id) {
        return Probe {
            status: if codex { "available" } else { "unavailable" },
            ..Probe::default()
        };
    }
    let Some(home) = home(env) else {
        return Probe {
            status: if harness == "codex" {
                "available"
            } else {
                "missing"
            },
            ..Probe::default()
        };
    };
    let (path, status) = if codex {
        (
            codex_file(
                &home.join(".codex/sessions"),
                &format!("-{id}.jsonl"),
                0,
                &mut 0,
            ),
            "available",
        )
    } else {
        claude_file(&home.join(".claude/projects"), &id, cwd)
    };
    let values = path
        .and_then(|path| fs::File::open(path).ok())
        .and_then(|mut file| read_probe(&mut file, codex).ok())
        .unwrap_or_default();
    Probe { status, values }
}

/// Extracts the pinned resolver's numeric version suffix from a matching `AI_AGENT` label.
fn agent_version(raw: &str, harness: &str) -> (Option<String>, &'static str) {
    let matches = raw
        .to_lowercase()
        .chars()
        .take(512)
        .collect::<String>()
        .split(|c: char| c == '/' || c == '\\' || c.is_whitespace() || c == ':' || c == '=')
        .any(|part| part == harness);
    if !matches {
        return (None, "unavailable");
    }
    for (index, _) in raw.char_indices() {
        if index > 0
            && !raw[..index]
                .chars()
                .next_back()
                .is_some_and(|c| c == '@' || c == '/' || c.is_whitespace())
        {
            continue;
        }
        let candidate = raw[index..].strip_prefix('v').unwrap_or(&raw[index..]);
        let split = candidate.find(['-', '+']);
        let numeric = split.map_or(candidate, |index| &candidate[..index]);
        let suffix = split.map(|index| &candidate[index + 1..]);
        let components: Vec<_> = numeric.split('.').collect();
        if (2..=3).contains(&components.len())
            && components
                .iter()
                .all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()))
            && suffix.is_none_or(|suffix| {
                !suffix.is_empty()
                    && suffix
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b".-".contains(&b))
            })
        {
            return (Some(candidate.chars().take(256).collect()), "available");
        }
    }
    (None, "available")
}

/// Enriches only collection attribution, retaining exactly the resolver outcomes stored by pm.
pub(crate) fn enrich(identity: &mut AuthorIdentity, env: &BTreeMap<String, String>) {
    let harness = identity.harness.clone().unwrap_or_default();
    let resolvers: &[(&str, &str)] = match harness.as_str() {
        "codex" => &[
            ("model", "codex_session_file"),
            ("effort", "codex_session_file"),
            ("version", "ai_agent_version"),
        ],
        "claude-code" => &[
            ("model", "claude_session_file"),
            ("version", "claude_session_file"),
        ],
        _ => &[],
    };
    for (field, key) in [("model", "CLAUDE_MODEL"), ("effort", "CODEX_EFFORT")] {
        if (harness == "claude-code" && field == "model" || harness == "codex" && field == "effort")
            && !resolved(identity, field)
            && let Some(value) = env_text(env, key)
        {
            identity.provenance.insert(
                field.to_owned(),
                json!({"value":value,"source":"environment"}),
            );
        }
    }
    let enabled = !env_text(env, "PM_AGENT_PROBES")
        .is_some_and(|value| ["0", "false", "off"].contains(&value.to_lowercase().as_str()));
    let needs_file = resolvers
        .iter()
        .any(|(key, resolver)| *resolver != "ai_agent_version" && !resolved(identity, key));
    let probe = if enabled && needs_file {
        local_probe(&harness, env, &std::env::current_dir().unwrap_or_default())
    } else {
        Probe::default()
    };
    for (key, resolver) in resolvers {
        let mut status = probe.status;
        if !resolved(identity, key) && enabled {
            let value = if *resolver == "ai_agent_version" {
                let (value, state) = agent_version(
                    env.get("AI_AGENT").map_or("", String::as_str).trim(),
                    &harness,
                );
                status = state;
                value
            } else {
                probe.values.get(*key).cloned()
            };
            if let Some(value) = value {
                identity
                    .provenance
                    .insert((*key).to_owned(), json!({"value":value,"source":"probe"}));
            }
        }
        if resolved(identity, key) && identity.provenance[*key]["source"] != "probe" {
            continue;
        }
        let mut outcome = if resolved(identity, key) {
            json!({"status":"resolved"})
        } else if !enabled {
            json!({"status":"unavailable","reason":"probes_disabled"})
        } else {
            match status {
                "unavailable" => json!({"status":"unavailable","reason":"harness_unavailable"}),
                "missing" => json!({"status":"unavailable","reason":"resolver_input_missing"}),
                _ => json!({"status":"failed","reason":"resolver_failed"}),
            }
        };
        outcome["resolver"] = json!(resolver);
        outcome["rule_version"] = json!("v1");
        identity
            .resolver_outcomes
            .insert((*key).to_owned(), outcome);
    }
}
