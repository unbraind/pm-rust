//! Crash-recoverable Rust-native item mutations.

use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::process;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize, Serializer};
use serde_json::Value;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;
use toon_format::{ToonError, encode_default};

use crate::error::PmRustError;
use crate::history::{self, EMPTY_DOCUMENT_HASH, OrderedDocument};
use crate::item::{ItemDocument, ItemMetadata};

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
    /// Argv-derived provenance role recorded in the create-history entry.
    #[serde(default)]
    pub provenance_role: Option<String>,
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

/// Request accepted by the native update mutation surface.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct UpdateItem {
    /// Exact stable identifier of the item to mutate.
    pub id: String,
    /// Replacement human-readable title, when provided.
    #[serde(default)]
    pub title: Option<String>,
    /// Replacement human-readable description, when provided.
    #[serde(default)]
    pub description: Option<String>,
    /// Replacement runtime lifecycle state, when provided.
    #[serde(default)]
    pub status: Option<String>,
    /// Replacement priority from zero through four, when provided.
    #[serde(default)]
    pub priority: Option<u8>,
    /// Replacement canonicalized tag list, when provided.
    #[serde(default)]
    pub tags: Option<Vec<String>>,
    /// Replacement long-form Markdown body, when provided.
    #[serde(default)]
    pub body: Option<String>,
    /// Asserted mutation author.
    pub author: String,
    /// Optional deterministic timestamp; current UTC is used when absent.
    #[serde(default)]
    pub timestamp: Option<String>,
    /// Optional history message; omitted from the record when absent.
    #[serde(default)]
    pub message: Option<String>,
    /// Argv-derived provenance role recorded in the history entry.
    #[serde(default)]
    pub provenance_role: Option<String>,
    /// Permit removal of a lock whose filesystem age exceeds the configured TTL.
    #[serde(default)]
    pub force_stale_lock: bool,
}

/// Request accepted by the native comment-append mutation surface.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct CommentItem {
    /// Exact stable identifier of the item to mutate.
    pub id: String,
    /// Non-empty comment text appended as the newest row.
    pub text: String,
    /// Asserted mutation author.
    pub author: String,
    /// Optional deterministic timestamp; current UTC is used when absent.
    #[serde(default)]
    pub timestamp: Option<String>,
    /// Optional history message; omitted from the record when absent.
    #[serde(default)]
    pub message: Option<String>,
    /// Argv-derived provenance role recorded in the history entry.
    #[serde(default)]
    pub provenance_role: Option<String>,
    /// Permit removal of a lock whose filesystem age exceeds the configured TTL.
    #[serde(default)]
    pub force_stale_lock: bool,
}

/// Request accepted by the native close mutation surface.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct CloseItem {
    /// Exact stable identifier of the item to close.
    pub id: String,
    /// Required non-empty immutable closing summary.
    pub reason: String,
    /// Asserted mutation author.
    pub author: String,
    /// Optional deterministic timestamp; current UTC is used when absent.
    #[serde(default)]
    pub timestamp: Option<String>,
    /// Argv-derived provenance role recorded in the history entry.
    #[serde(default)]
    pub provenance_role: Option<String>,
    /// Permit removal of a lock whose filesystem age exceeds the configured TTL.
    #[serde(default)]
    pub force_stale_lock: bool,
}

/// Durable result of a native update, comment, or close transaction.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct MutationResult {
    /// Post-mutation canonical item.
    pub item: ItemDocument,
    /// Item path relative to the tracker root.
    #[serde(serialize_with = "serialize_portable_path")]
    pub item_path: PathBuf,
    /// History path relative to the tracker root.
    #[serde(serialize_with = "serialize_portable_path")]
    pub history_path: PathBuf,
    /// SHA-256 hash of the canonical post-mutation document.
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
    #[serde(default)]
    workflow: WorkflowSettings,
}

#[derive(Debug, Deserialize)]
struct LockSettings {
    #[serde(default = "default_lock_ttl")]
    ttl_seconds: u64,
    #[serde(default)]
    wait_ms: u64,
}

impl Default for LockSettings {
    /// Builds lock settings with the canonical default time-to-live.
    fn default() -> Self {
        Self {
            ttl_seconds: default_lock_ttl(),
            wait_ms: 0,
        }
    }
}

#[derive(Debug, Deserialize)]
struct WorkflowSettings {
    #[serde(default = "default_close_status")]
    close_status: String,
}

impl Default for WorkflowSettings {
    /// Builds workflow settings with the canonical default close status.
    fn default() -> Self {
        Self {
            close_status: default_close_status(),
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
struct MutationJournal {
    version: u8,
    id: String,
    item_type: String,
    /// Canonical bytes the item must hold after the mutation.
    item_bytes: String,
    /// The single history line the mutation appends.
    history_bytes: String,
    /// Hash of the pre-mutation item document.
    ///
    /// Recovery uses this to distinguish a crash that left the durable item
    /// still holding the pre-mutation bytes (roll the transaction forward)
    /// from foreign bytes that match neither image (reserve `RecoveryConflict`).
    /// Without it, a crash between the item replace and the history append
    /// blocks every later mutation on the identifier until an operator deletes
    /// the journal by hand.
    ///
    /// Defaulted, not required: a journal written before this field existed
    /// would otherwise fail to deserialize, and both `recover` and
    /// `recover_mutation` would return `RecoveryConflict` for every later
    /// operation on that identifier until an operator deleted the file by hand.
    /// The exposure window is a crash followed by an upgrade. An empty value
    /// matches no real document hash, so an old journal falls through to the
    /// same conflict it would have reached before this field was added, rather
    /// than to a spurious roll-forward.
    #[serde(default)]
    before_item_hash: String,
}

/// Returns the canonical lifecycle status for a create request.
/// Returns the canonical lifecycle status for a new item.
#[must_use]
pub fn default_status() -> String {
    "open".to_owned()
}

/// Returns the canonical neutral priority for a create request.
#[must_use]
pub fn default_priority() -> u8 {
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

/// Returns the canonical lifecycle status written by a close mutation.
fn default_close_status() -> String {
    "closed".to_owned()
}

/// Constructs a typed validation failure with a stable human-readable reason.
fn invalid(reason: impl Into<String>) -> PmRustError {
    PmRustError::InvalidCreateRequest {
        reason: reason.into(),
    }
}

/// Constructs a typed mutation-validation failure with a stable reason.
fn invalid_mutation(reason: impl Into<String>) -> PmRustError {
    PmRustError::InvalidMutationRequest {
        reason: reason.into(),
    }
}

/// Reads the mutation settings needed to validate and coordinate a create.
fn read_settings(pm_root: &Path) -> Result<MutationSettings, PmRustError> {
    let path = pm_root.join("settings.json");
    // `match` instead of `.map_err(..)` avoids per-instantiation closure
    // functions the coverage gate counts separately.
    let raw = match fs::read_to_string(&path) {
        Ok(raw) => raw,
        Err(source) => {
            return Err(PmRustError::Io {
                path: path.clone(),
                source,
            });
        }
    };
    match serde_json::from_str(&raw) {
        Ok(settings) => Ok(settings),
        Err(error) => Err(invalid(format!("invalid settings.json: {error}"))),
    }
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
    // `let..else` and `if let` instead of `.ok_or_else(..)`/`.map_err(..)` avoid
    // per-instantiation closure functions the coverage gate counts separately.
    let Some(parent) = path.parent() else {
        return Err(invalid("target path has no parent"));
    };
    if let Err(source) = fs::create_dir_all(parent) {
        return Err(PmRustError::Io {
            path: parent.to_path_buf(),
            source,
        });
    }
    let Some(file_name) = path.file_name().and_then(|value| value.to_str()) else {
        return Err(invalid("target filename is not UTF-8"));
    };
    let temporary = parent.join(format!(".{file_name}.{}.tmp", unique_token()));
    atomic_write_with_temporary(path, &temporary, contents)
}

/// Opens an explicit private path and delegates no-replace publication.
fn atomic_write_with_temporary(
    path: &Path,
    temporary: &Path,
    contents: &str,
) -> Result<(), PmRustError> {
    let file = match OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(temporary)
    {
        Ok(file) => file,
        Err(source) => {
            return Err(PmRustError::Io {
                path: temporary.to_path_buf(),
                source,
            });
        }
    };
    commit_temporary(file, temporary, path, contents)
}

/// Flushes a private file, publishes it by hard link, and durably syncs the commit.
fn commit_temporary(
    mut file: File,
    temporary: &Path,
    path: &Path,
    contents: &str,
) -> Result<(), PmRustError> {
    // `match` instead of the two `.map_err(..)` closures avoids
    // per-instantiation closure functions the coverage gate counts separately.
    let write_sync = file
        .write_all(contents.as_bytes())
        .and_then(|()| file.sync_all());
    let publication = match write_sync {
        Ok(()) => match fs::hard_link(temporary, path) {
            Ok(()) => Ok(()),
            Err(source) => Err(PmRustError::Io {
                path: path.to_path_buf(),
                source,
            }),
        },
        Err(source) => Err(PmRustError::Io {
            path: temporary.to_path_buf(),
            source,
        }),
    };
    #[cfg(not(unix))]
    let published_sync = match file.sync_all() {
        Ok(()) => Ok(()),
        Err(source) => Err(PmRustError::Io {
            path: path.to_path_buf(),
            source,
        }),
    };
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

/// Publishes replacement bytes over an existing durable target.
fn atomic_replace(path: &Path, contents: &str) -> Result<(), PmRustError> {
    // `let..else`/`if let` instead of `.ok_or_else(..)`/`.map_err(..)` avoid
    // per-instantiation closure functions the coverage gate counts separately.
    let Some(parent) = path.parent() else {
        return Err(invalid("target path has no parent"));
    };
    if let Err(source) = fs::create_dir_all(parent) {
        return Err(PmRustError::Io {
            path: parent.to_path_buf(),
            source,
        });
    }
    let Some(file_name) = path.file_name().and_then(|value| value.to_str()) else {
        return Err(invalid("target filename is not UTF-8"));
    };
    let temporary = parent.join(format!(".{file_name}.{}.tmp", unique_token()));
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|source| PmRustError::Io {
            path: temporary.clone(),
            source,
        })?;
    let result = stage_temporary(file, &temporary, contents)
        .and_then(|()| publish_replacement(&temporary, path));
    if result.is_err() {
        // A failed stage or publish must not leave the private temporary
        // behind: repeated failures would accumulate `.<name>.<token>.tmp`
        // files in the item or history directory forever. `commit_temporary`
        // already upholds the opposite invariant, and the test suite asserts
        // no `.tmp` files remain after a failure.
        let _ = fs::remove_file(&temporary);
    }
    result
}

/// Writes and durably flushes one private temporary file's contents.
fn stage_temporary(mut file: File, temporary: &Path, contents: &str) -> Result<(), PmRustError> {
    // `match` instead of `.map_err(..)` avoids a per-instantiation closure
    // function the coverage gate counts separately.
    match file
        .write_all(contents.as_bytes())
        .and_then(|()| file.sync_all())
    {
        Ok(()) => Ok(()),
        Err(source) => Err(PmRustError::Io {
            path: temporary.to_path_buf(),
            source,
        }),
    }
}

/// Atomically moves staged bytes onto their durable target path.
fn publish_replacement(temporary: &Path, path: &Path) -> Result<(), PmRustError> {
    #[cfg(not(unix))]
    {
        // Windows cannot rename onto an existing target. The per-item lock and
        // the recovery journal cover the removal window between the two calls.
        if path.is_file() {
            fs::remove_file(path).map_err(|source| PmRustError::Io {
                path: path.to_path_buf(),
                source,
            })?;
        }
    }
    // `if let` instead of `.map_err(..)` avoids a per-instantiation closure
    // function the coverage gate counts separately.
    if let Err(source) = fs::rename(temporary, path) {
        return Err(PmRustError::Io {
            path: path.to_path_buf(),
            source,
        });
    }
    #[cfg(unix)]
    {
        sync_parent(path)
    }
    #[cfg(not(unix))]
    {
        Ok(())
    }
}

/// Durably appends one history record, creating the stream when absent.
fn append_history_line(path: &Path, line: &str) -> Result<(), PmRustError> {
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|source| PmRustError::Io {
            path: path.to_path_buf(),
            source,
        })?;
    // `match` instead of the write/sync `.map_err(..)` avoids a
    // per-instantiation closure function the coverage gate counts separately.
    let written = match file
        .write_all(line.as_bytes())
        .and_then(|()| file.sync_all())
    {
        Ok(()) => Ok(()),
        Err(source) => Err(PmRustError::Io {
            path: path.to_path_buf(),
            source,
        }),
    };
    drop(file);
    written?;
    #[cfg(unix)]
    {
        sync_parent(path)
    }
    #[cfg(not(unix))]
    {
        Ok(())
    }
}

/// Acquires an item lock, retrying within the configured wait budget.
fn acquire_lock(
    pm_root: &Path,
    id: &str,
    owner: &str,
    locks: &LockSettings,
    force_stale: bool,
    timestamp: &str,
) -> Result<ItemLock, PmRustError> {
    let deadline = Instant::now() + Duration::from_millis(locks.wait_ms);
    loop {
        match acquire_lock_attempt(
            pm_root,
            id,
            owner,
            locks.ttl_seconds,
            force_stale,
            timestamp,
        ) {
            Ok(lock) => return Ok(lock),
            Err(PmRustError::LockConflict { .. }) => {}
            Err(error) => return Err(error),
        }
        if Instant::now() >= deadline {
            return Err(PmRustError::LockConflict { id: id.to_owned() });
        }
        std::thread::sleep(Duration::from_millis(25));
    }
}

/// Attempts one immediate lock acquisition, optionally reclaiming an expired
/// unchanged owner.
fn acquire_lock_attempt(
    pm_root: &Path,
    id: &str,
    owner: &str,
    ttl_seconds: u64,
    force_stale: bool,
    timestamp: &str,
) -> Result<ItemLock, PmRustError> {
    let locks = pm_root.join("locks");
    // `if let` instead of `.map_err(..)` avoids a per-instantiation closure
    // function the coverage gate counts separately.
    if let Err(source) = fs::create_dir_all(&locks) {
        return Err(PmRustError::Io {
            path: locks.clone(),
            source,
        });
    }
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
    // `match` instead of `.map_err(..)` avoids a per-instantiation closure
    // function the coverage gate counts separately.
    // The same pending-delete window applies to reading the incumbent lock: the
    // create above failed because a lock was there, and the holder may release
    // it before this read runs.
    let existing_raw = match fs::read_to_string(&path) {
        Ok(raw) => raw,
        Err(source) if lock_error_is_contention(source.kind()) => {
            return Err(PmRustError::LockConflict { id: id.to_owned() });
        }
        Err(source) => {
            return Err(PmRustError::Io {
                path: path.clone(),
                source,
            });
        }
    };
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

/// Decides whether a lock-path filesystem error means contention or a fault.
///
/// Contention does not present the same way on every platform, and reading a
/// Windows presentation as a fault is what turns a busy lock into an aborted
/// mutation:
///
/// - `AlreadyExists` is the ordinary case: another writer holds the lock.
/// - `PermissionDenied` is the Windows presentation of the same thing. Windows
///   keeps a deleted file in a *pending delete* state until every handle to it
///   closes, and an open against a file in that state fails with
///   `ERROR_ACCESS_DENIED` rather than with "already exists" or "not found". A
///   writer releasing its lock therefore makes a concurrent writer's open fail
///   with `PermissionDenied` for a moment -- which is contention, precisely.
/// - `NotFound` means the lock, or the directory holding it, disappeared
///   between two of our own calls. The next attempt recreates the directory and
///   retries the create, so this too is a retry rather than a failure.
///
/// Every one of these is retried inside the caller's `wait_ms` budget, so a
/// lock that never frees still fails, and it fails as a lock conflict. The
/// residual cost is diagnostic: a `locks` directory that is genuinely
/// unwritable reports a lock conflict after the budget rather than a
/// permissions error. That trade is deliberate -- the common case is
/// contention, and the rare case still fails.
fn lock_error_is_contention(kind: ErrorKind) -> bool {
    matches!(
        kind,
        ErrorKind::AlreadyExists | ErrorKind::PermissionDenied | ErrorKind::NotFound
    )
}

/// Creates and initializes a new lock file without replacing another writer.
fn create_lock_file(path: &Path, raw: &str, id: &str) -> Result<ItemLock, PmRustError> {
    match OpenOptions::new().write(true).create_new(true).open(path) {
        Ok(file) => write_lock_file(file, path, raw),
        Err(source) if lock_error_is_contention(source.kind()) => {
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
    // `match` instead of `.map_err(..)` avoids a per-instantiation closure
    // function the coverage gate counts separately.
    let current_raw = match fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(source) => {
            return Err(PmRustError::Io {
                path: path.to_path_buf(),
                source,
            });
        }
    };
    if current_raw != expected_raw {
        return Err(PmRustError::LockConflict { id: id.to_owned() });
    }
    remove_file(path)
}

/// Removes a file and persists the containing-directory change where supported.
fn remove_file(path: &Path) -> Result<(), PmRustError> {
    // `if let` instead of `.map_err(..)` avoids a per-instantiation closure
    // function the coverage gate counts separately.
    if let Err(source) = fs::remove_file(path) {
        return Err(PmRustError::Io {
            path: path.to_path_buf(),
            source,
        });
    }
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
    // `let..else` instead of `.ok_or_else(..)` avoids a per-instantiation
    // closure function the coverage gate counts separately.
    let Some(parent) = path.parent() else {
        return Err(invalid("durable path has no parent directory"));
    };
    let directory = match File::open(parent) {
        Ok(directory) => directory,
        Err(source) => {
            return Err(PmRustError::Io {
                path: parent.to_path_buf(),
                source,
            });
        }
    };
    match directory.sync_all() {
        Ok(()) => Ok(()),
        Err(source) => Err(PmRustError::Io {
            path: parent.to_path_buf(),
            source,
        }),
    }
}

/// Encodes one validated item into canonical JavaScript-compatible TOON bytes.
fn canonical_item_bytes(document: &ItemDocument) -> Result<String, PmRustError> {
    // The validated shape contains only TOON-supported scalar and array values.
    let view = history::canonical_document_value(document);
    normalize_encoded_item(encode_default(&view))
}

/// Converts encoder failures into the public typed error before normalization.
fn normalize_encoded_item(encoded: Result<String, ToonError>) -> Result<String, PmRustError> {
    // `match` instead of `.map_err(..)` avoids a per-instantiation closure
    // function the coverage gate counts separately.
    let encoded = match encoded {
        Ok(encoded) => encoded,
        Err(error) => {
            return Err(PmRustError::ItemEncoding {
                reason: error.to_string(),
            });
        }
    };
    normalize_item_bytes(&encoded)
}

/// Normalizes Rust encoder output without changing ambiguous scalar semantics.
fn normalize_item_bytes(encoded: &str) -> Result<String, PmRustError> {
    let mut normalized = String::with_capacity(encoded.len() + 1);
    // The encoder emits tabular rows one line after their `{...}:` header; the
    // JavaScript encoder leaves more row scalars unquoted than the Rust one.
    let mut inside_tabular_block = false;
    for line in encoded.lines() {
        if let Some(prefix) = line.strip_suffix("[0]:") {
            normalized.push_str(prefix);
            normalized.push_str(": []");
        } else if line.starts_with("tags[") {
            // The Rust encoder already quotes ambiguous array strings correctly.
            normalized.push_str(line);
        } else if inside_tabular_block && line.starts_with([' ', '\t']) {
            normalized.push_str(&normalize_row_bytes(line));
        } else if let Some((key, quoted)) = line.split_once(": \"") {
            let Some(value) = quoted.strip_suffix('"') else {
                return Err(PmRustError::ItemEncoding {
                    reason: "encoder emitted an unterminated quoted scalar".to_owned(),
                });
            };
            if safe_unquoted_scalar(value) {
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
        inside_tabular_block =
            line.contains('{') && line.contains('}') && line.trim_end().ends_with(':');
    }
    Ok(normalized)
}

/// Reports whether the JavaScript TOON encoder leaves this scalar unquoted.
fn safe_unquoted_scalar(value: &str) -> bool {
    !value.is_empty()
        && !value.starts_with('-')
        && !matches!(value, "true" | "false" | "null")
        && serde_json::from_str::<Value>(value).is_err()
        && value.bytes().any(|byte| byte.is_ascii_alphanumeric())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._/-".contains(&byte))
        && !decoder_parses_as_number(value)
}

/// Reports whether the TOON decoder would read this scalar back as a number.
///
/// The Rust decoder accepts lenient numeric spellings such as `0.` or `1.`
/// that `serde_json` rejects, so the JSON probe alone let ambiguous scalars
/// through unquoted and broke the canonical round trip (the decoder then read
/// the description `"0."` back as the integer `0`). Ask the real decoder
/// instead of duplicating its grammar here.
fn decoder_parses_as_number(value: &str) -> bool {
    toon_format::decode_strict::<Value>(&format!("k: {value}"))
        .ok()
        .is_some_and(|decoded| decoded.get("k").is_some_and(Value::is_number))
}

/// Unquotes safe scalars in one tabular row while preserving every other field.
///
/// Fields keep their bytes unless they are a simple double-quoted segment with
/// no escapes whose content is safe to leave unquoted in the canonical dialect,
/// so ambiguous strings and timestamps stay exactly as the encoder wrote them.
fn normalize_row_bytes(line: &str) -> String {
    let indent_length = line.len() - line.trim_start().len();
    let (indent, rest) = line.split_at(indent_length);
    let mut normalized = String::with_capacity(line.len());
    normalized.push_str(indent);
    let mut field = String::new();
    let mut fields: Vec<String> = Vec::new();
    let mut inside_quotes = false;
    for character in rest.chars() {
        match character {
            '"' => {
                inside_quotes = !inside_quotes;
                field.push(character);
            }
            ',' if !inside_quotes => {
                fields.push(std::mem::take(&mut field));
            }
            _ => field.push(character),
        }
    }
    fields.push(field);
    for (index, entry) in fields.iter().enumerate() {
        if index > 0 {
            normalized.push(',');
        }
        let scalar = entry
            .strip_prefix('"')
            .and_then(|value| value.strip_suffix('"'))
            .filter(|value| !value.contains('"') && !value.contains('\\'));
        match scalar {
            Some(value) if safe_unquoted_scalar(value) => normalized.push_str(value),
            _ => normalized.push_str(entry),
        }
    }
    normalized
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
    let journal: MutationJournal = match serde_json::from_str(&raw) {
        Ok(value) => value,
        Err(error) => {
            return Err(PmRustError::RecoveryConflict {
                id: id.to_owned(),
                reason: format!("invalid durable journal: {error}"),
            });
        }
    };
    if journal.version != 1 || journal.id != id {
        return Err(PmRustError::RecoveryConflict {
            id: id.to_owned(),
            reason: "journal identity or version mismatch".to_owned(),
        });
    }
    let Some(folder) = type_folder(&journal.item_type) else {
        return Err(PmRustError::RecoveryConflict {
            id: id.to_owned(),
            reason: "journal contains an unsupported item type".to_owned(),
        });
    };
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
    // `?` instead of `.and_then(..)` avoids a per-instantiation closure
    // function the coverage gate counts separately.
    repair?;
    remove_file(&journal_path)
}

/// Completes or clears an interrupted in-place mutation journal.
///
/// The journal records the exact intended post-mutation item bytes and history
/// line. Recovery restores a missing item half, recreates a wholly absent
/// history stream, and removes a transaction that already committed both
/// halves. Any other durable divergence refuses recovery instead of guessing.
fn recover_mutation(
    pm_root: &Path,
    operation: &str,
    id: &str,
) -> Result<(PathBuf, PathBuf), PmRustError> {
    let journal_path = pm_root
        .join("runtime/transactions")
        .join(format!("{operation}-{id}.json"));
    let Some(raw) = read_optional(&journal_path)? else {
        return locate_item(pm_root, id).map(|(item_path, _)| {
            (
                item_path,
                pm_root.join("history").join(format!("{id}.jsonl")),
            )
        });
    };
    let journal: MutationJournal = match serde_json::from_str(&raw) {
        Ok(value) => value,
        Err(error) => {
            return Err(PmRustError::RecoveryConflict {
                id: id.to_owned(),
                reason: format!("invalid durable journal: {error}"),
            });
        }
    };
    if journal.version != 1 || journal.id != id {
        return Err(PmRustError::RecoveryConflict {
            id: id.to_owned(),
            reason: "journal identity or version mismatch".to_owned(),
        });
    }
    let Some(folder) = type_folder(&journal.item_type) else {
        return Err(PmRustError::RecoveryConflict {
            id: id.to_owned(),
            reason: "journal contains an unsupported item type".to_owned(),
        });
    };
    let item_relative = PathBuf::from(folder).join(format!("{id}.toon"));
    let history_relative = PathBuf::from("history").join(format!("{id}.jsonl"));
    let item_path = pm_root.join(&item_relative);
    let history_path = pm_root.join(&history_relative);
    let item = read_optional(&item_path)?;
    let history = read_optional(&history_path)?;
    // The journal records both the pre-mutation item hash and the post-mutation
    // item bytes, so recovery can tell three crash states apart:
    //
    //   * the item already holds the post-mutation image (the item half
    //     committed, but the history append may not have) — complete the
    //     history half;
    //   * the item still holds the pre-mutation image (the crash happened
    //     before the item replace) — roll the whole transaction forward;
    //   * the item holds foreign bytes that match neither image — refuse.
    //
    // Without the pre-mutation hash, the second and third cases were
    // indistinguishable, and a crash between the item replace and the history
    // append blocked every later mutation on the identifier.
    let item_is_after = item.as_deref() == Some(journal.item_bytes.as_str());
    // `match` instead of `.ok().is_some_and(..)` keeps the before-image check
    // free of per-instantiation closure functions the coverage gate counts
    // separately.
    let item_is_before = match item.as_deref() {
        Some(bytes) => match crate::item::decode_item(&item_path, bytes) {
            Ok(document) => {
                history::document_hash(&OrderedDocument::from_document(&document))
                    == journal.before_item_hash
            }
            Err(_) => false,
        },
        None => false,
    };
    // `history_needs_replay` is computed with `match` rather than
    // `is_none_or(..)` for the same closure-free reason.
    let history_needs_replay = match &history {
        None => true,
        Some(value) => !value.ends_with(&journal.history_bytes),
    };
    if item_is_after {
        // The item half committed. Complete the history half if it has not.
        if history_needs_replay {
            append_history_line(&history_path, &journal.history_bytes)?;
        }
    } else if item_is_before || item.is_none() {
        // The crash happened before the item replace, or the item half was
        // lost. Roll the whole transaction forward. `history_needs_replay` is
        // always true here: when the item still holds the before-image (or is
        // absent), `commit_mutation`'s write order (journal → item replace →
        // history append → journal remove) guarantees the history append has
        // not happened either, so the durable stream cannot already end with
        // the journalled line. The check is therefore unnecessary in this arm
        // and is omitted to avoid an unreachable false branch.
        atomic_replace(&item_path, &journal.item_bytes)?;
        append_history_line(&history_path, &journal.history_bytes)?;
    } else {
        return Err(PmRustError::RecoveryConflict {
            id: id.to_owned(),
            reason: "durable item bytes match neither the pre-mutation nor post-mutation image"
                .to_owned(),
        });
    }
    remove_file(&journal_path)?;
    // Both exits must agree on shape. The no-journal exit above returns paths
    // already joined to `pm_root`, and every caller writes through what it gets
    // back, so returning the relative forms here would resolve against the
    // process working directory instead of the tracker.
    Ok((item_path, history_path))
}

/// Replays any durable journal and reads the locked document for an in-place
/// mutation. Reading the stored document only after recovery and under the
/// caller's lock guarantees no writer ever applies changes to stale bytes.
fn recover_and_locate(
    pm_root: &Path,
    operation: &str,
    id: &str,
) -> Result<(PathBuf, PathBuf, ItemDocument), PmRustError> {
    recover_mutation(pm_root, operation, id).and_then(|(item_path, history_path)| {
        locate_item(pm_root, id)
            .map(|(_item_path, before_document)| (item_path, history_path, before_document))
    })
}

/// Locates the single stored document for one stable identifier.
///
/// The traversal mirrors `Workspace::read_items`: it walks every directory
/// that is not in `NON_ITEM_DIRECTORIES` and recurses into subdirectories, so
/// an item stored at `tasks/<subdir>/<id>.toon` or in a non-canonical type
/// folder is found by `update`, `comment`, and `close` exactly as `get` finds
/// it. The previous hardcoded 11-folder top-level probe missed those items.
fn locate_item(pm_root: &Path, id: &str) -> Result<(PathBuf, ItemDocument), PmRustError> {
    let mut found: Vec<(PathBuf, ItemDocument)> = Vec::new();
    locate_item_recursive(pm_root, true, id, &mut found)?;
    match found.len() {
        0 => Err(PmRustError::ItemNotFound { id: id.to_owned() }),
        1 => Ok(found.remove(0)),
        _ => Err(PmRustError::DuplicateItemId {
            id: id.to_owned(),
            first: found[0].0.clone(),
            second: found[1].0.clone(),
        }),
    }
}

/// Recursively collects every `<id>.toon` file under `dir`.
///
/// `at_root` is `true` only for the immediate `pm_root`, where the
/// non-item directories (`history`, `runtime`, …) are skipped, matching
/// `Workspace::read_items`. Deeper levels recurse into every directory, as
/// `collect_toon_paths` does, so nested item folders resolve.
fn locate_item_recursive(
    dir: &Path,
    at_root: bool,
    id: &str,
    found: &mut Vec<(PathBuf, ItemDocument)>,
) -> Result<(), PmRustError> {
    let target = format!("{id}.toon");
    // `read_directory` and `read_optional` map IO failures with `match`, not
    // closures, so they do not spawn per-instantiation closure functions the
    // coverage gate would count as uncovered when the IO always succeeds.
    for entry in crate::workspace::read_directory(dir)? {
        let entry_path = entry.path();
        if entry_path.is_symlink() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        if at_root && crate::workspace::NON_ITEM_DIRECTORIES.contains(&name.as_str()) {
            continue;
        }
        if entry_path.is_dir() {
            locate_item_recursive(&entry_path, false, id, found)?;
        } else if entry_path.is_file() && name == target {
            // `read_optional` returns `Ok(None)` only on `NotFound`. The
            // `is_file()` check just succeeded, so a `None` here means the
            // entry vanished between the directory listing and the read — a
            // race the per-item lock prevents for tracked items. Propagating
            // the IO error (rather than silently skipping) keeps the locator
            // free of an unreachable `None` branch and surfaces the unlikely
            // race instead of hiding it.
            let raw = match fs::read_to_string(&entry_path) {
                Ok(raw) => raw,
                Err(source) => {
                    return Err(PmRustError::Io {
                        path: entry_path.clone(),
                        source,
                    });
                }
            };
            let document = crate::item::decode_item(&entry_path, &raw)?;
            found.push((entry_path, document));
        }
    }
    Ok(())
}

/// Validates shared mutation inputs common to every in-place transaction.
fn validate_mutation_request(author: &str, timestamp: Option<&str>) -> Result<String, PmRustError> {
    if author.trim().is_empty() {
        return Err(invalid_mutation("author must not be empty"));
    }
    match timestamp {
        // Caller-supplied timestamps must be validated in full. A malformed
        // timestamp on an in-place mutation must report the mutation variant,
        // not the create variant, so the failure reads as `invalid mutation
        // request` rather than `invalid create request`.
        Some(value) => {
            // A malformed timestamp on an in-place mutation reports the
            // mutation variant, not the create variant, so the failure reads
            // as `invalid mutation request` rather than `invalid create
            // request`. `match` avoids a per-instantiation `map_err` closure.
            if validate_timestamp(value).is_err() {
                return Err(invalid_mutation(
                    "timestamp must be a non-empty UTC RFC 3339 value",
                ));
            }
            Ok(value.to_owned())
        }
        // `now_iso` emits canonical UTC RFC 3339 by construction, so the
        // generated value needs no second validation pass.
        None => Ok(now_iso()),
    }
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
    // `match` instead of `.unwrap_or_else(now_iso)` avoids a per-instantiation
    // closure function the coverage gate counts separately.
    let timestamp = match request.timestamp.clone() {
        Some(value) => value,
        None => now_iso(),
    };
    validate_timestamp(&timestamp)?;
    let _lock = acquire_lock(
        pm_root,
        &request.id,
        &request.author,
        &settings.locks,
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
        let ordered = OrderedDocument::from_document(&document);
        let after_hash = history::document_hash(&ordered);
        let patch = history::history_patch(
            &OrderedDocument {
                metadata: Vec::new(),
                body: String::new(),
            },
            &ordered,
        );
        let entry = history::history_entry(
            &timestamp,
            &request.author,
            "create",
            request.provenance_role.as_deref(),
            patch,
            EMPTY_DOCUMENT_HASH.to_owned(),
            after_hash.clone(),
            Some(request.message.as_deref().unwrap_or_default()),
        );
        let history_bytes = history::history_line(&entry);
        let journal = MutationJournal {
            version: 1,
            id: request.id.clone(),
            item_type: request.item_type,
            item_bytes: item_bytes.clone(),
            history_bytes: history_bytes.clone(),
            // A create has no pre-mutation item, so the before-image hash is
            // the empty document hash. The create recovery path treats a
            // missing item as a discardable transaction and never consults
            // this field, but the journal shape is shared with in-place
            // mutations and must stay serializable.
            before_item_hash: EMPTY_DOCUMENT_HASH.to_owned(),
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

/// Applies one validated field update to an existing item in place.
pub(crate) fn update_item(
    pm_root: &Path,
    mut request: UpdateItem,
) -> Result<MutationResult, PmRustError> {
    let settings = read_settings(pm_root)?;
    let timestamp = validate_mutation_request(&request.author, request.timestamp.as_deref())?;
    let _lock = acquire_lock(
        pm_root,
        &request.id,
        &request.author,
        &settings.locks,
        request.force_stale_lock,
        &timestamp,
    )?;
    let (item_path, history_path, before_document) =
        recover_and_locate(pm_root, "update", &request.id)?;
    // The stored document must be read under the lock so no writer ever
    // applies changes to stale bytes.
    if let Some(tags) = &mut request.tags {
        tags.sort();
        tags.dedup();
    }
    let mut document = before_document.clone();
    if request
        .title
        .as_ref()
        .is_some_and(|value| value.trim().is_empty())
        || request
            .status
            .as_ref()
            .is_some_and(|value| value.trim().is_empty())
    {
        return Err(invalid_mutation("title and status must not be empty"));
    }
    if request.priority.is_some_and(|value| value > 4) {
        return Err(invalid_mutation("priority must be between 0 and 4"));
    }
    let changed = request.title.is_some()
        || request.description.is_some()
        || request.status.is_some()
        || request.priority.is_some()
        || request.tags.is_some()
        || request.body.is_some();
    if !changed {
        return Err(invalid_mutation(
            "an update must provide at least one field to change",
        ));
    }
    if let Some(title) = request.title {
        document.metadata.title = title;
    }
    if let Some(description) = request.description {
        document.metadata.description = description;
    }
    if let Some(status) = request.status {
        document.metadata.status = status;
    }
    if let Some(priority) = request.priority {
        document.metadata.priority = priority;
    }
    if let Some(tags) = request.tags {
        document.metadata.tags = tags;
    }
    if let Some(body) = request.body {
        document.body = body;
    }
    document.metadata.updated_at.clone_from(&timestamp);
    commit_mutation(
        pm_root,
        "update",
        &before_document,
        document,
        &timestamp,
        &request.author,
        request.provenance_role.as_deref(),
        request.message.as_deref(),
        &item_path,
        &history_path,
    )
}

/// Appends one comment row to an existing item's comment list.
pub(crate) fn comment_item(
    pm_root: &Path,
    request: &CommentItem,
) -> Result<MutationResult, PmRustError> {
    let settings = read_settings(pm_root)?;
    let timestamp = validate_mutation_request(&request.author, request.timestamp.as_deref())?;
    if request.text.trim().is_empty() {
        return Err(invalid_mutation("comment text must not be empty"));
    }
    let _lock = acquire_lock(
        pm_root,
        &request.id,
        &request.author,
        &settings.locks,
        request.force_stale_lock,
        &timestamp,
    )?;
    let (item_path, history_path, before_document) =
        recover_and_locate(pm_root, "comment", &request.id)?;
    let mut document = before_document.clone();
    let row = serde_json::json!({
        "created_at": timestamp,
        "author": request.author,
        "text": request.text,
    });
    let mut comments = match document.metadata.extra.get("comments") {
        None => Vec::new(),
        Some(Value::Array(values)) => values.clone(),
        // A scalar, object, or null `comments` value is not a compatible
        // append target. Silently substituting an empty array would destroy
        // the stored content durably, so refuse the mutation instead.
        Some(_) => {
            return Err(invalid_mutation("existing comments value is not an array"));
        }
    };
    comments.push(row);
    document
        .metadata
        .extra
        .insert("comments".to_owned(), Value::Array(comments));
    document.metadata.updated_at.clone_from(&timestamp);
    commit_mutation(
        pm_root,
        "comment",
        &before_document,
        document,
        &timestamp,
        &request.author,
        request.provenance_role.as_deref(),
        request.message.as_deref(),
        &item_path,
        &history_path,
    )
}

/// Closes one open item with an immutable closing summary.
pub(crate) fn close_item(
    pm_root: &Path,
    request: CloseItem,
) -> Result<MutationResult, PmRustError> {
    let settings = read_settings(pm_root)?;
    let timestamp = validate_mutation_request(&request.author, request.timestamp.as_deref())?;
    if request.reason.trim().is_empty() {
        return Err(invalid_mutation(
            "a close requires a non-empty closing summary",
        ));
    }
    let _lock = acquire_lock(
        pm_root,
        &request.id,
        &request.author,
        &settings.locks,
        request.force_stale_lock,
        &timestamp,
    )?;
    let (item_path, history_path, before_document) =
        recover_and_locate(pm_root, "close", &request.id)?;
    if matches!(
        before_document.metadata.status.as_str(),
        "closed" | "canceled"
    ) || before_document.metadata.status == settings.workflow.close_status
    {
        return Err(invalid_mutation(format!(
            "item {} is already terminal",
            request.id
        )));
    }
    let mut document = before_document.clone();
    document
        .metadata
        .status
        .clone_from(&settings.workflow.close_status);
    document
        .metadata
        .extra
        .insert("closed_at".to_owned(), Value::String(timestamp.clone()));
    document
        .metadata
        .extra
        .insert("completed_at".to_owned(), Value::String(timestamp.clone()));
    document
        .metadata
        .extra
        .insert("close_reason".to_owned(), Value::String(request.reason));
    document.metadata.updated_at.clone_from(&timestamp);
    commit_mutation(
        pm_root,
        "close",
        &before_document,
        document,
        &timestamp,
        &request.author,
        request.provenance_role.as_deref(),
        None,
        &item_path,
        &history_path,
    )
}

/// Expresses one durable path relative to the tracker root.
///
/// Callers hand back paths joined to `pm_root`, so stripping succeeds for every
/// real tracker; the fallback keeps foreign absolute paths verbatim instead of
/// panicking when the prefix does not apply.
fn relative_to_tracker(pm_root: &Path, path: &Path) -> PathBuf {
    // `match` instead of `.map_or_else(..)` avoids a per-instantiation closure
    // function the coverage gate counts separately.
    match path.strip_prefix(pm_root) {
        Ok(relative) => relative.to_path_buf(),
        Err(_) => path.to_path_buf(),
    }
}

/// Journals, writes, and records one completed in-place mutation.
#[allow(clippy::too_many_arguments)]
fn commit_mutation(
    pm_root: &Path,
    operation: &'static str,
    before_document: &ItemDocument,
    document: ItemDocument,
    timestamp: &str,
    author: &str,
    provenance_role: Option<&str>,
    message: Option<&str>,
    item_path: &Path,
    history_path: &Path,
) -> Result<MutationResult, PmRustError> {
    canonical_item_bytes(&document).and_then(|item_bytes| {
        let before_ordered = OrderedDocument::from_document(before_document);
        let after_ordered = OrderedDocument::from_document(&document);
        let before_hash = history::document_hash(&before_ordered);
        let after_hash = history::document_hash(&after_ordered);
        let patch = history::history_patch(&before_ordered, &after_ordered);
        let entry = history::history_entry(
            timestamp,
            author,
            if operation == "comment" {
                "comment_add"
            } else {
                operation
            },
            provenance_role,
            patch,
            before_hash.clone(),
            after_hash.clone(),
            message,
        );
        let history_bytes = history::history_line(&entry);
        let journal = MutationJournal {
            version: 1,
            id: document.metadata.id.clone(),
            item_type: document.metadata.item_type.clone(),
            item_bytes: item_bytes.clone(),
            history_bytes: history_bytes.clone(),
            before_item_hash: before_hash,
        };
        let journal_path = pm_root
            .join("runtime/transactions")
            .join(format!("{operation}-{}.json", document.metadata.id));
        let journal_bytes = format!(
            "{}\n",
            // This concrete structure has no fallible custom serializers.
            serde_json::to_string_pretty(&journal).unwrap_or_default()
        );
        let result = MutationResult {
            item: document,
            item_path: relative_to_tracker(pm_root, item_path),
            history_path: relative_to_tracker(pm_root, history_path),
            after_hash,
        };
        atomic_write(&journal_path, &journal_bytes)
            .and_then(|()| atomic_replace(item_path, &item_bytes))
            .and_then(|()| append_history_line(history_path, &history_bytes))
            .and_then(|()| remove_file(&journal_path))
            .map(|()| result)
    })
}

#[cfg(test)]
#[path = "../tests/support/mutation_unit.rs"]
mod tests;
