//! Native session orientation reads and their explicit compatibility boundaries.

use crate::collections::refusal;
use crate::{ListOutput, PmRustError, Workspace};
use serde_json::{Value, json};
use std::fs;

/// Brief context selection and output controls.
#[derive(Clone, Debug, Default)]
pub struct ContextOptions {
    /// Maximum focus rows; absent retains the oracle's per-group default.
    pub limit: Option<String>,
    /// Focus packing token ceiling.
    pub token_budget: Option<String>,
    /// Explicit supported brief depth.
    pub depth: Option<String>,
    /// Omit focus tags.
    pub no_tags: bool,
    /// Omit installed extension diagnostics.
    pub no_extension_health: bool,
    /// Measure default TOON output instead of JSON.
    pub toon: bool,
}

/// Append-only history read controls.
// These booleans mirror independent published projection, verification and encoding flags.
#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Debug, Default)]
pub struct HistoryOptions {
    /// Latest record count.
    pub limit: Option<String>,
    /// Verify immutable records, replay and the current item.
    pub verify: bool,
    /// Retain full JSON patches instead of compact timeline rows.
    pub full: bool,
    /// Disclose an explicitly requested compact projection when budgeting binds.
    pub compact: bool,
    /// Measure default TOON output instead of JSON.
    pub toon: bool,
}

/// Dependency traversal and projection controls.
#[derive(Clone, Debug, Default)]
pub struct DepsOptions {
    /// Tree or graph output.
    pub format: Option<String>,
    /// Maximum traversal depth.
    pub max_depth: Option<String>,
    /// Collapse repeated nodes.
    pub collapse: Option<String>,
    /// Counts without tree payload.
    pub summary: bool,
    /// Explicit full projection.
    pub full: bool,
    /// Maximum delivered nodes.
    pub node_limit: Option<String>,
    /// Maximum delivered edges.
    pub edge_limit: Option<String>,
    /// Complete response ceiling.
    pub token_budget: Option<String>,
    /// Measure the default TOON rendering instead of JSON.
    pub toon: bool,
}

/// Returns the shared complete-projection receipt.
pub(crate) fn complete_receipt() -> Value {
    json!({"has_omissions":false,"omitted_field_group_count":0,"omitted_field_groups":[]})
}

/// Reports invalid values with the published usage error classification.
pub(crate) fn invalid(detail: impl Into<String>) -> PmRustError {
    refusal(detail, 2, json!({"code":"invalid_argument_value"}))
}

/// Parses JavaScript numeric option forms without lossy integer casts.
pub(crate) fn numeric(
    raw: &str,
    integer: bool,
    minimum: usize,
    detail: String,
) -> Result<usize, PmRustError> {
    let normalized = raw.trim();
    let number = if normalized.is_empty() {
        0.0
    } else if let Some((digits, radix)) = [
        ("0x", 16u32),
        ("0X", 16),
        ("0b", 2),
        ("0B", 2),
        ("0o", 8),
        ("0O", 8),
    ]
    .iter()
    .find_map(|(prefix, radix)| {
        normalized
            .strip_prefix(prefix)
            .map(|digits| (digits, *radix))
    }) {
        if digits.is_empty() {
            f64::NAN
        } else {
            digits
                .chars()
                .try_fold(0.0, |total, digit| {
                    digit
                        .to_digit(radix)
                        .map(|digit| total * f64::from(radix) + f64::from(digit))
                })
                .unwrap_or(f64::NAN)
        }
    } else {
        normalized.parse::<f64>().unwrap_or(f64::NAN)
    };
    #[allow(clippy::cast_precision_loss)]
    let minimum = minimum as f64;
    if !number.is_finite() || number < minimum || (integer && number.fract() != 0.0) {
        return Err(invalid(detail));
    }
    if number > 9_007_199_254_740_991.0 {
        return Err(refusal(
            "Native orientation numeric controls must fit the JavaScript safe integer range; use the published pm CLI for larger values.",
            2,
            json!({"code":"unsupported_native_value"}),
        ));
    }
    // Valid native numeric controls fit every supported 64-bit target.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    Ok(number.floor() as usize)
}

/// Reads raw JSONL without changing or auto-creating a stream.
pub(crate) fn records(workspace: &Workspace, id: &str) -> Result<Vec<Value>, PmRustError> {
    if id.contains(['/', '\\', ':', '\0']) || matches!(id, "." | "..") {
        return Err(refusal(
            "Invalid history item id: expected a single filename.",
            2,
            json!({}),
        ));
    }
    let path = workspace
        .pm_root()
        .join("history")
        .join(format!("{id}.jsonl"));
    let raw = match fs::read_to_string(&path) {
        Ok(raw) => raw,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(source) => return Err(PmRustError::Io { path, source }),
    };
    let mut result = Vec::new();
    for (index, line) in raw.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if ["<<<<<<<", "=======", ">>>>>>>"].iter().any(|marker| {
            line.strip_prefix(marker)
                .is_some_and(|suffix| suffix.is_empty() || suffix.starts_with(' '))
        }) {
            let marker = line.split_whitespace().next().unwrap_or_default();
            return Err(refusal(
                format!(
                    "History for {id} contains merge conflict markers at line {} ({marker}). Resolve <<<<<<< ======= >>>>>>> markers and retry.",
                    index + 1
                ),
                1,
                json!({"code":"history_merge_conflict_markers_detected","required":"Repair the history stream by resolving merge-conflict markers.","why":"Conflict markers break JSONL parsing and invalidate deterministic audit history.","examples":[format!("pm history {id}"),format!("pm restore {id} <timestamp-or-version>")],"next_steps":["Resolve or restore the history file, then rerun the command."]}),
            ));
        }
        result.push(serde_json::from_str(line).map_err(|_| refusal(format!("History for {id} contains invalid JSON at line {}. Repair or restore the history stream and retry.",index+1),1,json!({"code":"history_json_invalid","next_steps":[format!("Run pm history-repair {id} --salvage-tail --dry-run to check whether only an invalid suffix is damaged.")]})))?);
    }
    Ok(result)
}

impl Workspace {
    /// Reads item history, retaining every immutable record field and insertion order.
    ///
    /// # Errors
    /// Fails on missing identities, invalid numeric limits, malformed JSONL or I/O errors.
    pub fn history(&self, id: &str, options: &HistoryOptions) -> Result<ListOutput, PmRustError> {
        let limit = options
            .limit
            .as_ref()
            .map(|raw| {
                numeric(
                    raw,
                    false,
                    0,
                    "--limit must be a non-negative number".to_owned(),
                )
            })
            .transpose()?;
        let settings = crate::ownership::runtime_settings(self.pm_root())?;
        let prefix = settings["id_prefix"].as_str().unwrap_or_default();
        let normalized = if id.contains('-') || prefix.is_empty() {
            id.to_owned()
        } else {
            format!("{}-{id}", prefix.trim_end_matches('-'))
        };
        let corpus = self.read_items()?;
        let item = corpus
            .iter()
            .find(|item| item.metadata.id.eq_ignore_ascii_case(&normalized));
        let resolved = item.map_or(normalized.as_str(), |item| item.metadata.id.as_str());
        let history_path = self
            .pm_root()
            .join("history")
            .join(format!("{resolved}.jsonl"));
        if item.is_none() && !history_path.exists() {
            return Err(refusal(
                format!("Item {id} not found"),
                3,
                json!({"code":"item_not_found","id":id}),
            ));
        }
        if item.is_some()
            && !history_path.exists()
            && settings["history"]["missing_stream"] == "strict_error"
        {
            return Err(refusal(
                format!(
                    "Missing history stream for {resolved}. history requires history streams when settings.history.missing_stream is strict_error."
                ),
                3,
                json!({}),
            ));
        }
        let entries = records(self, resolved)?;
        let start = entries.len().saturating_sub(limit.unwrap_or(entries.len()));
        let mut result = json!({"id":resolved,"history":entries[start..],"compact":false,"projection":{"mode":"full","row_key":"history"},"omission_receipt":complete_receipt(),"count":entries.len()-start,"limit":limit});
        if !options.full {
            let compact: Vec<_> = entries.iter().enumerate().skip(start).map(|(index,entry)| {
                let patch: Vec<_> = entry["patch"].as_array().into_iter().flatten().filter(|op| op["op"].is_string() && op["path"].is_string()).collect();
                let mut fields: Vec<_> = patch.iter().flat_map(|op| [op["path"].as_str(),op["from"].as_str()].into_iter().flatten()).map(changed_field).collect();
                fields.sort_by(|a,b| crate::collation::compare(a,b)); fields.dedup();
                json!({"index":index+1,"ts":entry["ts"],"op":entry["op"],"author":entry["author"],"patch_ops":patch.len(),"changed_fields":fields})
            }).collect();
            result = json!({"id":resolved,"compact_history":compact,"compact":true,"projection":{"mode":"compact","row_key":"compact_history"},"omission_receipt":{"has_omissions":true,"omitted_field_group_count":1,"omitted_field_groups":[{"name":"raw_history","restore_with":"--full"}]},"count":entries.len()-start,"limit":limit});
        }
        if entries
            .first()
            .is_some_and(|entry| entry["op"] == "history_compact_baseline")
        {
            let offset = entries[0]["context"]["history_compaction"]["version_offset"]
                .as_u64()
                .filter(|offset| {
                    offset.saturating_add(entries.len() as u64) <= 9_007_199_254_740_991
                });
            result["version_addressing"] = json!({"offset":offset,"first_version":offset.map(|n|n+1),"last_version":offset.map(|n|n+entries.len() as u64)});
        }
        if options.verify {
            result["verification"] = verify_records(&entries, item);
        }
        crate::read_output::apply_orientation(
            result.into(),
            &crate::ListOptions {
                full: options.full,
                brief: options.compact,
                limit: options.limit.clone(),
                toon: options.toon,
                ..Default::default()
            },
            "history",
            if options.full {
                &["history"]
            } else {
                &["compact_history"]
            },
        )
    }
}

/// Computes a recursively sorted canonical JSON digest.
fn digest(value: &Value) -> String {
    let mut serialized = String::new();
    crate::history::stable_json(value, &mut serialized);
    crate::history::sha256_digest(serialized.as_bytes())
}

/// Verifies the record envelope before applying any untrusted patch.
fn record_error(entry: &Value) -> Option<&'static str> {
    if entry
        .get("hash_algorithm")
        .is_some_and(|value| value != "sha256" && value != "sha512")
    {
        return Some("unsupported_history_hash_algorithm");
    }
    match (entry.get("record_hash_version"), entry.get("record_hash")) {
        (None, None) => None,
        (Some(version), Some(hash)) => {
            if version.as_f64() != Some(1.0) {
                return Some("unsupported_record_hash_version");
            }
            let mut canonical = entry.clone();
            canonical.as_object_mut()?.shift_remove("record_hash");
            (history_digest(
                &canonical,
                entry["hash_algorithm"].as_str().unwrap_or("sha256"),
            ) != *hash)
                .then_some("record_hash_mismatch")
        }
        _ => Some("incomplete_record_hash_envelope"),
    }
}

/// Recognizes a lowercase digest with one of the retained envelope lengths.
fn hex_digest(value: &Value, lengths: &[usize]) -> bool {
    value.as_str().is_some_and(|text| {
        lengths.contains(&text.len())
            && text
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    })
}
/// Validates retained RFC 6902 evidence before attempting reconstruction.
fn patch_shape(value: &Value) -> bool {
    value.as_array().is_some_and(|changes| {
        changes.iter().all(|change| {
            change.is_object()
                && change["path"].is_string()
                && match change["op"].as_str() {
                    Some("move" | "copy") => change["from"].is_string(),
                    Some("add" | "replace" | "test") => change.get("value").is_some(),
                    Some("remove") => true,
                    _ => false,
                }
        })
    })
}
/// Validates and reconstructs retained immutable rewrite evidence without changing a stream.
fn rewrite_error(entry: &Value) -> Option<&'static str> {
    let evidence = entry.get("reanchor_evidence")?;
    let Some(evidence) = evidence.as_array() else {
        return Some("rewrite_evidence_invalid");
    };
    for row in evidence {
        let envelope = row.get("record_hash_version").is_some() == row.get("record_hash").is_some();
        let record_valid = row.get("record").is_none_or(|record| {
            record.is_object()
                && ["ts", "author", "op", "before_hash", "after_hash"]
                    .iter()
                    .all(|key| record[key].is_string())
                && patch_shape(&record["patch"])
        });
        let valid = row.is_object()
            && envelope
            && row["before_hash"].is_string()
            && row["after_hash"].is_string()
            && hex_digest(&row["patch_hash"], &[64])
            && row.get("patch").is_none_or(patch_shape)
            && row.get("record_hash_version").is_none_or(Value::is_number)
            && row
                .get("record_hash")
                .is_none_or(|hash| hex_digest(hash, &[64, 128]))
            && row
                .get("hash_algorithm")
                .is_none_or(|value| value == "sha256" || value == "sha512")
            && record_valid;
        if !valid {
            return Some("rewrite_evidence_invalid");
        }
    }
    let mut reconstructed = entry.clone();
    for (index, row) in evidence.iter().enumerate().rev() {
        if let Some(patch) = row.get("patch")
            && digest(patch) != row["patch_hash"]
        {
            return Some("rewrite_evidence_patch_hash_mismatch");
        }
        if let Some(record) = row.get("record") {
            let matches = [
                "before_hash",
                "after_hash",
                "item_hash_version",
                "hash_algorithm",
                "record_hash_version",
                "record_hash",
            ]
            .iter()
            .all(|key| record.get(key) == row.get(key))
                && digest(&record["patch"]) == row["patch_hash"];
            if !matches {
                return Some("rewrite_evidence_record_hash_mismatch");
            }
            reconstructed = record.clone();
        } else {
            let patch = row.get("patch").unwrap_or(&reconstructed["patch"]).clone();
            if digest(&patch) != row["patch_hash"] {
                return None;
            }
            reconstructed["before_hash"] = row["before_hash"].clone();
            reconstructed["after_hash"] = row["after_hash"].clone();
            reconstructed["patch"] = patch;
            for key in [
                "hash_algorithm",
                "item_hash_version",
                "record_hash_version",
                "record_hash",
            ] {
                if let Some(value) = row.get(key) {
                    reconstructed[key] = value.clone();
                } else if let Some(record) = reconstructed.as_object_mut() {
                    record.shift_remove(key);
                }
            }
            if index == 0 {
                reconstructed
                    .as_object_mut()
                    .map(|record| record.shift_remove("reanchor_evidence"));
            } else {
                reconstructed["reanchor_evidence"] = json!(evidence[..index]);
            }
            row.get("record_hash_version")?;
        }
        if record_error(&reconstructed).is_some() {
            return Some("rewrite_evidence_record_hash_mismatch");
        }
    }
    None
}

/// Normalizes only the historical document-root metadata namespace.
fn history_pointer(pointer: &str) -> String {
    pointer
        .strip_prefix("/front_matter")
        .filter(|suffix| suffix.is_empty() || suffix.starts_with('/'))
        .map_or_else(|| pointer.to_owned(), |suffix| format!("/metadata{suffix}"))
}
/// Applies strict JSON-patch operations to an isolated replay value.
fn apply_patch(document: &mut Value, changes: &Value) -> Option<()> {
    for change in changes.as_array().into_iter().flatten() {
        let (Some(op), Some(pointer)) = (change["op"].as_str(), change["path"].as_str()) else {
            continue;
        };
        let pointer = history_pointer(pointer);
        if matches!(op, "copy" | "move") {
            let from = history_pointer(change["from"].as_str()?);
            let value = document.pointer(&from)?.clone();
            if op == "move" {
                apply_patch(document, &json!([{"op":"remove","path":from}]))?;
            }
            apply_patch(
                document,
                &json!([{"op":"add","path":pointer,"value":value}]),
            )?;
            continue;
        }
        if pointer.is_empty() {
            match op {
                "add" | "replace" => *document = change.get("value")?.clone(),
                "test" => {
                    if document != change.get("value")? {
                        return None;
                    }
                }
                _ => return None,
            }
            continue;
        }
        if pointer.split('/').any(|part| part == "__proto__") {
            return None;
        }
        let (parent, key) = pointer.rsplit_once('/')?;
        let parent = document.pointer_mut(parent)?;
        let key = key.replace("~1", "/").replace("~0", "~");
        match parent {
            Value::Object(map) => match op {
                "add" => {
                    map.insert(key, change.get("value")?.clone());
                }
                "replace" => {
                    *map.get_mut(&key)? = change.get("value")?.clone();
                }
                "remove" => {
                    map.shift_remove(&key)?;
                }
                "test" => {
                    if map.get(&key)? != change.get("value")? {
                        return None;
                    }
                }
                _ => return None,
            },
            Value::Array(rows) => {
                let index = if key == "-" {
                    rows.len()
                } else {
                    if key.len() > 1 && key.starts_with('0') {
                        return None;
                    }
                    key.parse::<usize>().ok()?
                };
                match op {
                    "add" if index <= rows.len() => {
                        rows.insert(index, change.get("value")?.clone());
                    }
                    "replace" => *rows.get_mut(index)? = change.get("value")?.clone(),
                    "remove" if index < rows.len() => {
                        rows.remove(index);
                    }
                    _ => return None,
                }
            }
            _ => return None,
        }
    }
    (document["metadata"].is_object() && document["body"].is_string()).then_some(())
}

/// Computes the selected published history digest algorithm.
fn history_digest(value: &Value, algorithm: &str) -> String {
    if algorithm == "sha512" {
        use sha2::Digest;
        let mut serialized = String::new();
        crate::history::stable_json(value, &mut serialized);
        let mut hex = String::with_capacity(128);
        for byte in sha2::Sha512::digest(serialized.as_bytes()) {
            use std::fmt::Write;
            let _ = write!(hex, "{byte:02x}");
        }
        hex
    } else {
        digest(value)
    }
}

/// Hashes canonical replay metadata for one retained history epoch.
fn replay_hash(document: &Value, version: u64, legacy: bool, algorithm: &str) -> String {
    let mut metadata = document["metadata"].clone();
    if metadata
        .as_object()
        .is_some_and(|metadata| !metadata.is_empty())
    {
        if let Ok(parsed) = serde_json::from_value::<crate::ItemMetadata>(metadata.clone()) {
            metadata = Value::Object(
                crate::history::canonical_metadata_pairs(&parsed)
                    .into_iter()
                    .collect(),
            );
        } else {
            return history_digest(
                &json!({"replay_fallback":true,"metadata":metadata,"body":document["body"]}),
                algorithm,
            );
        }
    }
    if version == 1 || (version == 2 && legacy) {
        if let Some(deps) = metadata["dependencies"].as_array_mut() {
            for dep in deps {
                if let Some(id) = dep["id"].as_str() {
                    dep["id"] = json!(id.to_lowercase());
                }
            }
        }
        if let Some(tests) = metadata["tests"].as_array_mut() {
            for test in tests.iter_mut() {
                if let Some(test) = test.as_object_mut() {
                    for key in ["workspace_context_mode", "provenance", "provenance_invalid"] {
                        test.shift_remove(key);
                    }
                }
            }
            if version == 1 {
                tests.sort_by(|a, b| {
                    ["command", "path", "scope"]
                        .iter()
                        .map(|key| {
                            crate::collation::compare(
                                a[key].as_str().unwrap_or_default(),
                                b[key].as_str().unwrap_or_default(),
                            )
                        })
                        .find(|order| !order.is_eq())
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
            }
        }
        if let Some(runs) = metadata["test_runs"].as_array_mut() {
            for run in runs {
                if let Some(run) = run.as_object_mut() {
                    run.shift_remove("executions");
                }
            }
        }
    }
    history_digest(
        &json!({"front_matter":metadata,"body":document["body"]}),
        algorithm,
    )
}

/// Verifies immutable record hashes and sequential replay without modifying storage.
#[allow(clippy::too_many_lines)]
fn verify_records(entries: &[Value], current: Option<&crate::ItemDocument>) -> Value {
    let mut replay = json!({"metadata":{},"body":""});
    let mut errors = Vec::new();
    let mut verified = true;
    let mut detected = None;
    let mut authoritative = None;
    let mut prior_genesis = entries
        .first()
        .filter(|entry| entry["op"] == "history_compact_baseline")
        .map(|_| 1);
    for (index, entry) in entries.iter().enumerate() {
        if entry["op"] == "create" {
            if let Some(prior) = prior_genesis {
                errors.push(format!(
                    "verify_failed:duplicate_create:entry_{}:prior_{prior}",
                    index + 1
                ));
            }
            prior_genesis.get_or_insert(index + 1);
        }
    }
    for (index, entry) in entries.iter().enumerate() {
        let version = (1u8..=3)
            .find(|version| Some(f64::from(*version)) == entry["item_hash_version"].as_f64())
            .map(u64::from);
        let unsupported = entry.get("item_hash_version").is_some() && version.is_none();
        if unsupported {
            errors.push(format!(
                "verify_failed:unsupported_item_hash_version:{}:entry_{}",
                entry["item_hash_version"].as_str().map_or_else(
                    || entry["item_hash_version"].to_string(),
                    ToString::to_string
                ),
                index + 1
            ));
        }
        let before = replay.clone();
        let input_error = record_error(entry)
            .or_else(|| rewrite_error(entry))
            .or_else(|| {
                apply_patch(&mut replay, &entry["patch"])
                    .is_none()
                    .then_some("patch_apply_failed")
            });
        if let Some(error) = input_error {
            errors.push(format!("verify_failed:{error}:entry_{}", index + 1));
            verified = false;
            break;
        }
        if unsupported {
            detected = None;
            authoritative = None;
            continue;
        }
        let candidates = version
            .or(authoritative)
            .map_or_else(|| vec![1, 2, 3], |version| vec![version]);
        let algorithm = entry["hash_algorithm"].as_str().unwrap_or("sha256");
        let mut before_matches = false;
        let mut matched = None;
        for version in candidates {
            for legacy in [false, true] {
                if legacy && version != 2 {
                    continue;
                }
                let before_hash = replay_hash(&before, version, legacy, algorithm);
                let after_hash = replay_hash(&replay, version, legacy, algorithm);
                before_matches |= before_hash == entry["before_hash"];
                if before_hash == entry["before_hash"] && after_hash == entry["after_hash"] {
                    matched = Some(version);
                    break;
                }
            }
            if matched.is_some() {
                break;
            }
        }
        if let Some(version) = matched {
            detected = Some(version);
            authoritative = entry
                .get("item_hash_version")
                .map(|_| version)
                .or(authoritative);
        } else {
            errors.push(format!(
                "verify_failed:{}_hash_mismatch:entry_{}",
                if before_matches { "after" } else { "before" },
                index + 1
            ));
            verified = false;
            break;
        }
    }
    let latest = entries
        .last()
        .and_then(|entry| entry.get("after_hash"))
        .cloned();
    let current_hash=current.map(|item|{
        if verified && let Some(version)=detected {
            let replay=json!({"metadata":Value::Object(crate::history::canonical_metadata_pairs(&item.metadata).into_iter().collect()),"body":item.body});
            let algorithm=entries.last().and_then(|entry|entry["hash_algorithm"].as_str()).unwrap_or("sha256");
            let primary=replay_hash(&replay,version,false,algorithm);
            if version==2{let legacy=replay_hash(&replay,version,true,algorithm);if latest.as_ref()==Some(&json!(legacy)){return legacy;}}
            return primary;
        }
        crate::history::document_hash(&crate::history::OrderedDocument::from_document(item))
    });
    let matches = current_hash.as_ref().map(|hash| {
        Value::String(hash.clone())
            == latest
                .clone()
                .unwrap_or_else(|| json!(crate::history::EMPTY_DOCUMENT_HASH))
    });
    if matches == Some(false) {
        errors.push("verify_failed:current_item_hash_mismatch".to_owned());
    }
    let mut result =
        json!({"ok":verified && errors.is_empty(),"entries":entries.len(),"errors":errors});
    if let Some(latest) = latest {
        result["latest_after_hash"] = latest;
    }
    if let Some(hash) = current_hash {
        result["current_item_hash"] = json!(hash);
    }
    if let Some(matches) = matches {
        result["current_matches_latest"] = json!(matches);
    }
    result
}

/// Maps JSON patch pointers to the published changed-field names.
fn changed_field(path: &str) -> String {
    if path == "/body" || path.starts_with("/body/") {
        return "body".to_owned();
    }
    for prefix in ["/metadata", "/front_matter"] {
        if let Some(suffix) = path
            .strip_prefix(prefix)
            .filter(|suffix| suffix.is_empty() || suffix.starts_with('/'))
        {
            let segment = suffix
                .strip_prefix('/')
                .unwrap_or(suffix)
                .split('/')
                .next()
                .unwrap_or_default();
            return if segment.is_empty() {
                "metadata".to_owned()
            } else {
                segment.replace("~1", "/").replace("~0", "~")
            };
        }
    }
    let segment = path
        .strip_prefix('/')
        .unwrap_or(path)
        .split('/')
        .next()
        .unwrap_or_default();
    if segment.is_empty() {
        "root".to_owned()
    } else {
        segment.replace("~1", "/").replace("~0", "~")
    }
}

#[cfg(test)]
#[path = "../tests/support/orientation_unit.rs"]
mod tests;
