//! Crash-recoverable Rust-native item creation.

use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::process;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize, Serializer};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;
use toon_format::{ToonError, encode_default};

use crate::{ItemDocument, ItemMetadata, PmRustError};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Input accepted by the first native mutation surface.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct CreateItem {
    /// Explicit stable identifier, including the configured project prefix.
    pub id: String,
    /// Human-readable title.
    pub title: String,
    /// Optional human-readable description.
    #[serde(default)]
    pub description: String,
    /// Canonical built-in item type.
    #[serde(rename = "type")]
    pub item_type: String,
    /// Runtime lifecycle state.
    #[serde(default = "default_status")]
    pub status: String,
    /// Priority from zero through four.
    #[serde(default = "default_priority")]
    pub priority: u8,
    /// Tags, canonicalized by sorting and deduplication.
    #[serde(default)]
    pub tags: Vec<String>,
    /// Long-form Markdown body.
    #[serde(default)]
    pub body: String,
    /// Asserted mutation author.
    pub author: String,
    /// Optional deterministic timestamp; current UTC is used when absent.
    #[serde(default)]
    pub timestamp: Option<String>,
    /// Optional history message.
    #[serde(default)]
    pub message: Option<String>,
    /// Permit removal of a lock whose filesystem age exceeds the configured TTL.
    #[serde(default)]
    pub force_stale_lock: bool,
}

/// Durable result of a native create transaction.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct CreateResult {
    /// Created canonical item.
    pub item: ItemDocument,
    /// Item path relative to the tracker root.
    #[serde(serialize_with = "serialize_portable_path")]
    pub item_path: PathBuf,
    /// History path relative to the tracker root.
    #[serde(serialize_with = "serialize_portable_path")]
    pub history_path: PathBuf,
    /// SHA-256 hash of the canonical post-create document.
    pub after_hash: String,
}

/// Serializes a relative result path with platform-independent separators.
fn serialize_portable_path<S>(path: &Path, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_str(&path.to_string_lossy().replace('\\', "/"))
}

#[derive(Debug, Deserialize)]
struct MutationSettings {
    #[serde(default)]
    id_prefix: String,
    #[serde(default = "default_item_format")]
    item_format: String,
    #[serde(default)]
    locks: LockSettings,
}

#[derive(Debug, Deserialize)]
struct LockSettings {
    #[serde(default = "default_lock_ttl")]
    ttl_seconds: u64,
}

impl Default for LockSettings {
    /// Builds lock settings with the canonical default time-to-live.
    fn default() -> Self {
        Self {
            ttl_seconds: default_lock_ttl(),
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
struct LockPayload {
    id: String,
    pid: u32,
    owner: String,
    created_at: String,
    ttl_seconds: u64,
    token: String,
}

struct ItemLock {
    path: PathBuf,
    raw: String,
}

impl Drop for ItemLock {
    /// Removes the lock only when its ownership token is still unchanged.
    fn drop(&mut self) {
        if fs::read_to_string(&self.path).is_ok_and(|raw| raw == self.raw) {
            let _ = fs::remove_file(&self.path);
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
struct CreateJournal {
    version: u8,
    id: String,
    item_type: String,
    item_bytes: String,
    history_bytes: String,
}

#[derive(Serialize)]
struct HistoryPatch {
    op: &'static str,
    path: String,
    value: Value,
}

#[derive(Serialize)]
struct HistoryEntry<'a> {
    ts: &'a str,
    author: &'a str,
    author_source: &'static str,
    op: &'static str,
    patch: Vec<HistoryPatch>,
    before_hash: &'static str,
    after_hash: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<&'a str>,
}

/// Returns the canonical lifecycle status for a create request.
fn default_status() -> String {
    "open".to_owned()
}

/// Returns the canonical neutral priority for a create request.
const fn default_priority() -> u8 {
    2
}

/// Returns the only item storage format supported by this compatibility slice.
fn default_item_format() -> String {
    "toon".to_owned()
}

/// Returns the canonical lock lifetime in seconds.
const fn default_lock_ttl() -> u64 {
    1_800
}

/// Constructs a typed validation failure with a stable human-readable reason.
fn invalid(reason: impl Into<String>) -> PmRustError {
    PmRustError::InvalidCreateRequest {
        reason: reason.into(),
    }
}

/// Reads the mutation settings needed to validate and coordinate a create.
fn read_settings(pm_root: &Path) -> Result<MutationSettings, PmRustError> {
    let path = pm_root.join("settings.json");
    let raw = fs::read_to_string(&path).map_err(|source| PmRustError::Io {
        path: path.clone(),
        source,
    })?;
    serde_json::from_str(&raw).map_err(|error| invalid(format!("invalid settings.json: {error}")))
}

/// Maps a canonical built-in item type to its tracker directory.
fn type_folder(item_type: &str) -> Option<&'static str> {
    match item_type {
        "Epic" => Some("epics"),
        "Feature" => Some("features"),
        "Task" => Some("tasks"),
        "Chore" => Some("chores"),
        "Issue" => Some("issues"),
        "Decision" => Some("decisions"),
        "Event" => Some("events"),
        "Reminder" => Some("reminders"),
        "Milestone" => Some("milestones"),
        "Meeting" => Some("meetings"),
        "Plan" => Some("plans"),
        _ => None,
    }
}

/// Validates the deliberately narrow native create contract.
fn validate_request(
    request: &CreateItem,
    settings: &MutationSettings,
) -> Result<&'static str, PmRustError> {
    if request.id.is_empty()
        || !request
            .id
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        || request.id.starts_with('-')
        || request.id.ends_with('-')
    {
        return Err(invalid(
            "id must use lowercase ASCII letters, digits, and interior hyphens",
        ));
    }
    if !settings.id_prefix.is_empty() && !request.id.starts_with(&settings.id_prefix) {
        return Err(invalid(format!(
            "id must start with configured prefix {}",
            settings.id_prefix
        )));
    }
    for (field, value) in [
        ("title", request.title.as_str()),
        ("type", request.item_type.as_str()),
        ("status", request.status.as_str()),
        ("author", request.author.as_str()),
    ] {
        if value.trim().is_empty() {
            return Err(invalid(format!("{field} must not be empty")));
        }
    }
    if request.priority > 4 {
        return Err(invalid("priority must be between 0 and 4"));
    }
    let folder = type_folder(&request.item_type).ok_or_else(|| {
        invalid("the first mutation slice supports canonical built-in item types only")
    })?;
    if settings.item_format != "toon" {
        return Err(invalid(
            "the first mutation slice supports item_format toon only",
        ));
    }
    Ok(folder)
}

/// Formats the current UTC instant in canonical millisecond RFC 3339 form.
fn now_iso() -> String {
    let value = OffsetDateTime::now_utc();
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
        value.year(),
        u8::from(value.month()),
        value.day(),
        value.hour(),
        value.minute(),
        value.second(),
        value.millisecond()
    )
}

/// Requires a non-empty RFC 3339 timestamp expressed explicitly in UTC.
fn validate_timestamp(value: &str) -> Result<(), PmRustError> {
    if value.trim().is_empty()
        || !value.ends_with('Z')
        || OffsetDateTime::parse(value, &Rfc3339).is_err()
    {
        return Err(invalid("timestamp must be a non-empty UTC RFC 3339 value"));
    }
    Ok(())
}

/// Generates a process-local collision-resistant suffix for private files.
fn unique_token() -> String {
    let nanos = OffsetDateTime::now_utc().unix_timestamp_nanos();
    format!(
        "{}-{nanos}-{}",
        process::id(),
        TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)
    )
}

/// Publishes new durable bytes without replacing an existing target.
fn atomic_write(path: &Path, contents: &str) -> Result<(), PmRustError> {
    let parent = path
        .parent()
        .ok_or_else(|| invalid("target path has no parent"))?;
    fs::create_dir_all(parent).map_err(|source| PmRustError::Io {
        path: parent.to_path_buf(),
        source,
    })?;
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| invalid("target filename is not UTF-8"))?;
    let temporary = parent.join(format!(".{file_name}.{}.tmp", unique_token()));
    atomic_write_with_temporary(path, &temporary, contents)
}

/// Opens an explicit private path and delegates no-replace publication.
fn atomic_write_with_temporary(
    path: &Path,
    temporary: &Path,
    contents: &str,
) -> Result<(), PmRustError> {
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(temporary)
        .map_err(|source| PmRustError::Io {
            path: temporary.to_path_buf(),
            source,
        })?;
    commit_temporary(file, temporary, path, contents)
}

/// Flushes a private file, publishes it by hard link, and durably syncs the commit.
fn commit_temporary(
    mut file: File,
    temporary: &Path,
    path: &Path,
    contents: &str,
) -> Result<(), PmRustError> {
    let publication = file
        .write_all(contents.as_bytes())
        .and_then(|()| file.sync_all())
        .map_err(|source| PmRustError::Io {
            path: temporary.to_path_buf(),
            source,
        })
        .and_then(|()| {
            fs::hard_link(temporary, path).map_err(|source| PmRustError::Io {
                path: path.to_path_buf(),
                source,
            })
        });
    #[cfg(not(unix))]
    let published_sync = file.sync_all().map_err(|source| PmRustError::Io {
        path: path.to_path_buf(),
        source,
    });
    // The target hard link is the commit point. Close the original handle before
    // removing its private name because Windows does not permit unlinking it open.
    drop(file);
    let _ = fs::remove_file(temporary);
    publication?;
    #[cfg(unix)]
    {
        sync_parent(path)
    }
    #[cfg(not(unix))]
    {
        published_sync
    }
}

/// Acquires an item lock, optionally reclaiming an expired unchanged owner.
fn acquire_lock(
    pm_root: &Path,
    id: &str,
    owner: &str,
    ttl_seconds: u64,
    force_stale: bool,
    timestamp: &str,
) -> Result<ItemLock, PmRustError> {
    let locks = pm_root.join("locks");
    fs::create_dir_all(&locks).map_err(|source| PmRustError::Io {
        path: locks.clone(),
        source,
    })?;
    let path = locks.join(format!("{id}.lock"));
    let payload = LockPayload {
        id: id.to_owned(),
        pid: process::id(),
        owner: owner.to_owned(),
        created_at: timestamp.to_owned(),
        ttl_seconds,
        token: unique_token(),
    };
    let raw = format!(
        "{}\n",
        // This concrete structure has no fallible custom serializers.
        serde_json::to_string_pretty(&payload).unwrap_or_default()
    );
    match create_lock_file(&path, &raw, id) {
        Ok(lock) => return Ok(lock),
        Err(PmRustError::LockConflict { .. }) => {}
        Err(error) => return Err(error),
    }
    let existing_raw = fs::read_to_string(&path).map_err(|source| PmRustError::Io {
        path: path.clone(),
        source,
    })?;
    let modified = fs::metadata(&path)
        .and_then(|metadata| metadata.modified())
        .unwrap_or(UNIX_EPOCH);
    let stale = SystemTime::now()
        .duration_since(modified)
        .unwrap_or_default()
        > Duration::from_secs(ttl_seconds);
    if !stale || !force_stale {
        return Err(PmRustError::LockConflict { id: id.to_owned() });
    }
    let gate = locks.join(format!("{id}.lock.stale-cleanup"));
    if fs::create_dir(&gate).is_err() {
        let abandoned = fs::metadata(&gate)
            .and_then(|metadata| metadata.modified())
            .ok()
            .and_then(|modified| SystemTime::now().duration_since(modified).ok())
            .is_some_and(|age| age > Duration::from_secs(ttl_seconds));
        if !abandoned {
            return Err(PmRustError::LockConflict { id: id.to_owned() });
        }
        if fs::remove_dir(&gate)
            .and_then(|()| fs::create_dir(&gate))
            .is_err()
        {
            return Err(PmRustError::LockConflict { id: id.to_owned() });
        }
    }
    let cleanup_result = remove_stale_lock(&path, &existing_raw, id);
    let _ = fs::remove_dir(&gate);
    cleanup_result.and_then(|()| create_lock_file(&path, &raw, id))
}

/// Creates and initializes a new lock file without replacing another writer.
fn create_lock_file(path: &Path, raw: &str, id: &str) -> Result<ItemLock, PmRustError> {
    match OpenOptions::new().write(true).create_new(true).open(path) {
        Ok(file) => write_lock_file(file, path, raw),
        Err(source) if source.kind() == ErrorKind::AlreadyExists => {
            Err(PmRustError::LockConflict { id: id.to_owned() })
        }
        Err(source) => Err(PmRustError::Io {
            path: path.to_path_buf(),
            source,
        }),
    }
}

/// Flushes a newly created lock payload before returning its ownership guard.
fn write_lock_file(mut file: File, path: &Path, raw: &str) -> Result<ItemLock, PmRustError> {
    if let Err(source) = file
        .write_all(raw.as_bytes())
        .and_then(|()| file.sync_all())
    {
        drop(file);
        let _ = fs::remove_file(path);
        return Err(PmRustError::Io {
            path: path.to_path_buf(),
            source,
        });
    }
    Ok(ItemLock {
        path: path.to_path_buf(),
        raw: raw.to_owned(),
    })
}

/// Removes an expired lock only when its bytes still match the observed owner.
fn remove_stale_lock(path: &Path, expected_raw: &str, id: &str) -> Result<(), PmRustError> {
    let current_raw = fs::read_to_string(path).map_err(|source| PmRustError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    if current_raw != expected_raw {
        return Err(PmRustError::LockConflict { id: id.to_owned() });
    }
    remove_file(path)
}

/// Removes a file and persists the containing-directory change where supported.
fn remove_file(path: &Path) -> Result<(), PmRustError> {
    fs::remove_file(path).map_err(|source| PmRustError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    #[cfg(unix)]
    {
        sync_parent(path)
    }
    #[cfg(not(unix))]
    {
        Ok(())
    }
}

#[cfg(unix)]
/// Flushes the parent directory so a publication or deletion survives a crash.
fn sync_parent(path: &Path) -> Result<(), PmRustError> {
    let parent = path
        .parent()
        .ok_or_else(|| invalid("durable path has no parent directory"))?;
    let directory = File::open(parent).map_err(|source| PmRustError::Io {
        path: parent.to_path_buf(),
        source,
    })?;
    directory.sync_all().map_err(|source| PmRustError::Io {
        path: parent.to_path_buf(),
        source,
    })
}

/// Encodes one validated item into canonical JavaScript-compatible TOON bytes.
fn canonical_item_bytes(document: &ItemDocument) -> Result<String, PmRustError> {
    // The validated create shape contains only TOON-supported scalar and array values.
    normalize_encoded_item(encode_default(document))
}

/// Converts encoder failures into the public typed error before normalization.
fn normalize_encoded_item(encoded: Result<String, ToonError>) -> Result<String, PmRustError> {
    let encoded = encoded.map_err(|error| PmRustError::ItemEncoding {
        reason: error.to_string(),
    })?;
    normalize_item_bytes(&encoded)
}

/// Normalizes Rust encoder output without changing ambiguous scalar semantics.
fn normalize_item_bytes(encoded: &str) -> Result<String, PmRustError> {
    let mut normalized = String::with_capacity(encoded.len() + 1);
    for line in encoded.lines() {
        if let Some(prefix) = line.strip_suffix("[0]:") {
            normalized.push_str(prefix);
            normalized.push_str(": []");
        } else if line.starts_with("tags[") {
            // The Rust encoder already quotes ambiguous array strings correctly.
            normalized.push_str(line);
        } else if let Some((key, quoted)) = line.split_once(": \"") {
            let value = quoted
                .strip_suffix('"')
                .ok_or_else(|| PmRustError::ItemEncoding {
                    reason: "encoder emitted an unterminated quoted scalar".to_owned(),
                })?;
            let safe_unquoted = !value.is_empty()
                && !value.starts_with('-')
                && !matches!(value, "true" | "false" | "null")
                && serde_json::from_str::<Value>(value).is_err()
                && value.bytes().any(|byte| byte.is_ascii_alphanumeric())
                && value
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b"._/-".contains(&byte));
            if safe_unquoted {
                normalized.push_str(key);
                normalized.push_str(": ");
                normalized.push_str(value);
            } else {
                normalized.push_str(line);
            }
        } else {
            normalized.push_str(line);
        }
        normalized.push('\n');
    }
    Ok(normalized)
}

/// Appends recursively key-sorted compact JSON for stable hashing.
fn stable_json(value: &Value, output: &mut String) {
    match value {
        Value::Array(entries) => {
            output.push('[');
            for (index, entry) in entries.iter().enumerate() {
                if index > 0 {
                    output.push(',');
                }
                stable_json(entry, output);
            }
            output.push(']');
        }
        Value::Object(entries) => {
            output.push('{');
            let mut keys = entries.keys().collect::<Vec<_>>();
            keys.sort();
            for (index, key) in keys.into_iter().enumerate() {
                if index > 0 {
                    output.push(',');
                }
                output.push_str(&serde_json::to_string(key).unwrap_or_default());
                output.push(':');
                stable_json(&entries[key], output);
            }
            output.push('}');
        }
        scalar => output.push_str(&serde_json::to_string(scalar).unwrap_or_default()),
    }
}

/// Projects item metadata into the canonical history and hashing shape.
fn metadata_value(document: &ItemDocument) -> serde_json::Map<String, Value> {
    let mut metadata = serde_json::Map::new();
    metadata.insert("id".to_owned(), Value::String(document.metadata.id.clone()));
    metadata.insert(
        "title".to_owned(),
        Value::String(document.metadata.title.clone()),
    );
    metadata.insert(
        "description".to_owned(),
        Value::String(document.metadata.description.clone()),
    );
    metadata.insert(
        "type".to_owned(),
        Value::String(document.metadata.item_type.clone()),
    );
    metadata.insert(
        "status".to_owned(),
        Value::String(document.metadata.status.clone()),
    );
    metadata.insert(
        "priority".to_owned(),
        Value::from(document.metadata.priority),
    );
    metadata.insert(
        "tags".to_owned(),
        Value::Array(
            document
                .metadata
                .tags
                .iter()
                .cloned()
                .map(Value::String)
                .collect(),
        ),
    );
    metadata.insert(
        "created_at".to_owned(),
        Value::String(document.metadata.created_at.clone()),
    );
    metadata.insert(
        "updated_at".to_owned(),
        Value::String(document.metadata.updated_at.clone()),
    );
    if let Some(parent) = &document.metadata.parent {
        metadata.insert("parent".to_owned(), Value::String(parent.clone()));
    }
    metadata.extend(document.metadata.extra.clone());
    metadata
}

/// Computes the canonical SHA-256 document hash used by pm history.
fn document_hash(document: &ItemDocument) -> String {
    let front_matter = Value::Object(metadata_value(document));
    let canonical = json!({"front_matter": front_matter, "body": document.body});
    let mut raw = String::new();
    stable_json(&canonical, &mut raw);
    format!("{:x}", Sha256::digest(raw.as_bytes()))
}

/// Builds the canonical first create-history entry as one JSONL record.
fn history_bytes(
    document: &ItemDocument,
    timestamp: &str,
    author: &str,
    message: Option<&str>,
    after_hash: &str,
) -> String {
    let object = metadata_value(document);
    let mut patch = vec![HistoryPatch {
        op: "replace",
        path: "/body".to_owned(),
        value: Value::String(document.body.clone()),
    }];
    for key in [
        "id",
        "title",
        "description",
        "type",
        "status",
        "priority",
        "tags",
        "created_at",
        "updated_at",
        "author",
    ] {
        patch.push(HistoryPatch {
            op: "add",
            path: format!("/metadata/{key}"),
            value: object[key].clone(),
        });
    }
    let entry = HistoryEntry {
        ts: timestamp,
        author,
        author_source: if author == "unknown" {
            "unknown"
        } else {
            "asserted"
        },
        op: "create",
        patch,
        before_hash: "3cc22dff72be7b14824654a7a64ea62b04799939b2fee54c1b5f52ca60bf6df0",
        after_hash,
        message,
    };
    serde_json::to_string(&entry)
        .map(|line| format!("{line}\n"))
        .unwrap_or_default()
}

/// Reads UTF-8 state while treating an absent path as an expected empty value.
fn read_optional(path: &Path) -> Result<Option<String>, PmRustError> {
    match fs::read_to_string(path) {
        Ok(raw) => Ok(Some(raw)),
        Err(source) if source.kind() == ErrorKind::NotFound => Ok(None),
        Err(source) => Err(PmRustError::Io {
            path: path.to_path_buf(),
            source,
        }),
    }
}

/// Completes or clears an interrupted create whose durable journal still matches.
fn recover(pm_root: &Path, id: &str) -> Result<(), PmRustError> {
    let journal_path = pm_root
        .join("runtime/transactions")
        .join(format!("create-{id}.json"));
    let Some(raw) = read_optional(&journal_path)? else {
        return Ok(());
    };
    let journal: CreateJournal =
        serde_json::from_str(&raw).map_err(|error| PmRustError::RecoveryConflict {
            id: id.to_owned(),
            reason: format!("invalid durable journal: {error}"),
        })?;
    if journal.version != 1 || journal.id != id {
        return Err(PmRustError::RecoveryConflict {
            id: id.to_owned(),
            reason: "journal identity or version mismatch".to_owned(),
        });
    }
    let folder = type_folder(&journal.item_type).ok_or_else(|| PmRustError::RecoveryConflict {
        id: id.to_owned(),
        reason: "journal contains an unsupported item type".to_owned(),
    })?;
    let item_path = pm_root.join(folder).join(format!("{id}.toon"));
    let history_path = pm_root.join("history").join(format!("{id}.jsonl"));
    let item = read_optional(&item_path)?;
    let history = read_optional(&history_path)?;
    if item
        .as_ref()
        .is_some_and(|value| value != &journal.item_bytes)
        || history
            .as_ref()
            .is_some_and(|value| value != &journal.history_bytes)
    {
        return Err(PmRustError::RecoveryConflict {
            id: id.to_owned(),
            reason: "durable item or history bytes differ from the transaction journal".to_owned(),
        });
    }
    let repair = match (item.is_some(), history.is_some()) {
        (true, false) => atomic_write(&history_path, &journal.history_bytes),
        (false, true) => atomic_write(&item_path, &journal.item_bytes),
        (false, false) | (true, true) => Ok(()),
    };
    repair.and_then(|()| remove_file(&journal_path))
}

/// Executes one validated, locked, journaled, no-overwrite create transaction.
pub(crate) fn create_item(
    pm_root: &Path,
    mut request: CreateItem,
) -> Result<CreateResult, PmRustError> {
    let settings = read_settings(pm_root)?;
    let folder = validate_request(&request, &settings)?;
    request.tags.sort();
    request.tags.dedup();
    let timestamp = request.timestamp.clone().unwrap_or_else(now_iso);
    validate_timestamp(&timestamp)?;
    let _lock = acquire_lock(
        pm_root,
        &request.id,
        &request.author,
        settings.locks.ttl_seconds,
        request.force_stale_lock,
        &timestamp,
    )?;
    recover(pm_root, &request.id)?;
    let item_relative = PathBuf::from(folder).join(format!("{}.toon", request.id));
    let history_relative = PathBuf::from("history").join(format!("{}.jsonl", request.id));
    let item_path = pm_root.join(&item_relative);
    let history_path = pm_root.join(&history_relative);
    if item_path.exists() || history_path.exists() {
        return Err(PmRustError::ItemAlreadyExists { id: request.id });
    }
    let mut extra = BTreeMap::new();
    extra.insert("author".to_owned(), Value::String(request.author.clone()));
    let document = ItemDocument {
        metadata: ItemMetadata {
            id: request.id.clone(),
            title: request.title,
            description: request.description,
            item_type: request.item_type.clone(),
            status: request.status,
            priority: request.priority,
            tags: request.tags,
            created_at: timestamp.clone(),
            updated_at: timestamp.clone(),
            parent: None,
            extra,
        },
        body: request.body,
    };
    canonical_item_bytes(&document).and_then(|item_bytes| {
        let after_hash = document_hash(&document);
        let history_bytes = history_bytes(
            &document,
            &timestamp,
            &request.author,
            request.message.as_deref(),
            &after_hash,
        );
        let journal = CreateJournal {
            version: 1,
            id: request.id.clone(),
            item_type: request.item_type,
            item_bytes: item_bytes.clone(),
            history_bytes: history_bytes.clone(),
        };
        let journal_path = pm_root
            .join("runtime/transactions")
            .join(format!("create-{}.json", request.id));
        let journal_bytes = format!(
            "{}\n",
            // This concrete structure has no fallible custom serializers.
            serde_json::to_string_pretty(&journal).unwrap_or_default()
        );
        let result = CreateResult {
            item: document,
            item_path: item_relative,
            history_path: history_relative,
            after_hash,
        };
        atomic_write(&journal_path, &journal_bytes)
            .and_then(|()| atomic_write(&item_path, &item_bytes))
            .and_then(|()| atomic_write(&history_path, &history_bytes))
            .and_then(|()| remove_file(&journal_path))
            .map(|()| result)
    })
}

#[cfg(test)]
#[path = "../tests/support/mutation_unit.rs"]
mod tests;
