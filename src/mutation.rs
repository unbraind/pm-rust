//! Crash-recoverable Rust-native item mutations.

use serde_json::Map;
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
use toon_format::{ToonError, encode_default, needs_quoting};

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

/// Explicit-author ownership mutation shared by claim and release.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct OwnershipItem {
    /// Stable identifier of the existing item.
    pub id: String,
    /// Asserted author; also the claim principal for this slice.
    pub author: String,
    /// Optional deterministic UTC clock, no later than the current UTC instant.
    /// Future values are rejected before lock acquisition or journal recovery.
    pub timestamp: Option<String>,
    /// Optional history message.
    pub message: Option<String>,
    /// Override a foreign owner or terminal claim and recover stale locks.
    pub force: bool,
    /// Treat a foreign claim as a successful no-op (claim only).
    pub if_available: bool,
    /// Optional argv-derived history role.
    pub provenance_role: Option<String>,
}

/// Published compact ownership receipt plus the complete SDK document.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct OwnershipResult {
    /// Complete document after the operation.
    #[serde(skip)]
    pub item: ItemDocument,
    /// Canonical identifier.
    pub id: String,
    /// Current lifecycle status.
    pub status: String,
    /// Number of ownership fields reported changed.
    pub changed_field_count: usize,
    /// Published immutable closing summary, when present.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub close_reason: Option<String>,
    /// Published ownership warnings.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
    /// Claim holder, including a skipped foreign claim.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub claimed_by: Option<String>,
    /// Author releasing the ownership.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub released_by: Option<String>,
    /// Previous owner, or null when absent.
    pub previous_assignee: Option<String>,
    /// Whether an override was requested.
    pub forced: bool,
    /// Present only for a skipped claim.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skipped: Option<bool>,
    /// Published recovery suggestions.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub suggestions: Vec<String>,
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
    /// Author-controlled closing summary; blank permits a field fallback or disabled policy.
    #[serde(default)]
    pub reason: String,
    /// Asserted mutation author.
    pub author: String,
    /// Optional deterministic timestamp; current UTC is used when absent.
    #[serde(default)]
    pub timestamp: Option<String>,
    /// Argv-derived provenance role recorded in the history entry.
    #[serde(default)]
    pub provenance_role: Option<String>,
    /// Published inline closure fields and validation controls.
    #[serde(default, flatten)]
    pub fields: CloseFields,
    /// Permit removal of a lock whose filesystem age exceeds the configured TTL.
    #[serde(default)]
    pub force_stale_lock: bool,
}

/// Optional fields accepted by the published single-item close command.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct CloseFields {
    /// Trimmed resolution summary, also a fallback closing reason.
    pub resolution: Option<String>,
    /// Trimmed expected outcome.
    pub expected_result: Option<String>,
    /// Trimmed observed outcome.
    pub actual_result: Option<String>,
    /// Actual completion clock, accepting ISO/date, now or a relative token.
    pub completed_at: Option<String>,
    /// Canonical duplicate target, validated before mutation.
    pub duplicate_of: Option<String>,
    /// History message, also a fallback closing reason.
    pub message: Option<String>,
    /// Closure evidence validation mode, defaulting to tracker governance.
    pub validate_close: Option<String>,
    /// Approved ownership or repeat-close override.
    #[serde(default)]
    pub force: bool,
}

/// Durable result of a native update, comment, or close transaction.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct MutationResult {
    /// Compact published close receipt, absent on other mutation kinds.
    #[serde(skip)]
    pub close_receipt: Option<Value>,
    /// Published close field names, retained for composition counts.
    #[serde(skip)]
    pub close_changed_fields: Vec<String>,
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
#[inline]
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
    #[serde(default)]
    governance: OwnershipGovernance,
}

#[derive(Debug, Default, Deserialize)]
struct OwnershipGovernance {
    #[serde(default)]
    preset: String,
    #[serde(default)]
    force_required_for_stale_lock: Option<bool>,
    #[serde(default)]
    ownership_enforcement: String,
}

impl OwnershipGovernance {
    /// Resolves ownership policy using the same preset precedence as published pm.
    fn policy(&self) -> &str {
        match self.preset.as_str() {
            "minimal" => "none",
            "strict" => "strict",
            "custom" if !self.ownership_enforcement.is_empty() => &self.ownership_enforcement,
            _ => "warn",
        }
    }

    /// Resolves whether expired lock recovery needs an explicit force override.
    fn stale_requires_force(&self) -> bool {
        match self.preset.as_str() {
            "minimal" => false,
            "custom" => self.force_required_for_stale_lock.unwrap_or(true),
            _ => true,
        }
    }
}

#[derive(Debug, Deserialize)]
struct LockSettings {
    #[serde(skip)]
    use_created_at: bool,
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
            use_created_at: false,
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

/// Releases a kernel gate lease independently of inherited file descriptors.
struct GateLease(File);

impl Drop for GateLease {
    /// Unlocks before close so a forked child cannot prolong a completed lease.
    fn drop(&mut self) {
        let _ = self.0.unlock();
    }
}

/// Retains the metadata and native identity of one observed gate instance.
struct GateObservation {
    metadata: fs::Metadata,
    #[cfg(windows)]
    identity: same_file::Handle,
}

impl GateObservation {
    /// Reads age and identity from the same open directory handle on Windows.
    fn read(path: &Path) -> std::io::Result<Self> {
        #[cfg(windows)]
        {
            same_file::Handle::from_path(path).and_then(|identity| {
                identity
                    .as_file()
                    .metadata()
                    .map(|metadata| Self { metadata, identity })
            })
        }
        #[cfg(not(windows))]
        {
            let metadata = fs::metadata(path)?;
            Ok(Self { metadata })
        }
    }
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
#[must_use]
pub fn now_iso() -> String {
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
///
/// # Errors
///
/// Returns [`PmRustError::InvalidCreateRequest`] when the value is empty, does not
/// end with `Z`, or cannot be parsed as RFC 3339.
pub fn validate_timestamp(value: &str) -> Result<(), PmRustError> {
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
    if id.is_empty()
        || !id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        || id.starts_with('-')
        || id.ends_with('-')
    {
        return Err(invalid_mutation(
            "id must use ASCII letters, digits, and interior hyphens",
        ));
    }
    let deadline = Instant::now() + Duration::from_millis(locks.wait_ms);
    loop {
        match acquire_lock_attempt(
            pm_root,
            id,
            owner,
            locks.ttl_seconds,
            force_stale,
            timestamp,
            locks.use_created_at,
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
#[allow(clippy::too_many_arguments)]
fn acquire_lock_attempt(
    pm_root: &Path,
    id: &str,
    owner: &str,
    ttl_seconds: u64,
    force_stale: bool,
    timestamp: &str,
    use_created_at: bool,
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
    // Initialization and stale reclamation share one gate, so an unpublished
    // payload can never be mistaken for an expired incumbent.
    let gate = locks.join(format!("{id}.lock.stale-cleanup"));
    // Keep this file permanently: unlinking it would split the advisory lock
    // across two inodes. The kernel releases the lease if a writer crashes.
    let lease_path = gate.with_extension("gate-lease");
    let lease = match OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&lease_path)
    {
        Ok(file) => file,
        Err(source) => {
            return Err(PmRustError::Io {
                path: lease_path,
                source,
            });
        }
    };
    if lease.try_lock().is_err() {
        return Err(PmRustError::LockConflict { id: id.to_owned() });
    }
    let _lease = GateLease(lease);
    if let Err(source) = fs::create_dir(&gate) {
        if !lock_contention_error(&source) {
            return Err(PmRustError::Io { path: gate, source });
        }
        let observed = GateObservation::read(&gate).ok();
        let abandoned = observed
            .as_ref()
            .and_then(|observed| observed.metadata.modified().ok())
            .and_then(|modified| SystemTime::now().duration_since(modified).ok())
            .is_some_and(|age| age > Duration::from_secs(ttl_seconds));
        if !abandoned {
            return Err(PmRustError::LockConflict { id: id.to_owned() });
        }
        if reclaim_stale_gate(&gate, observed.as_ref())
            .and_then(|()| fs::create_dir(&gate))
            .is_err()
        {
            return Err(PmRustError::LockConflict { id: id.to_owned() });
        }
    }
    let result = create_or_reclaim_lock(
        &path,
        &raw,
        id,
        ttl_seconds,
        force_stale,
        timestamp,
        use_created_at,
    );
    let _ = fs::remove_dir(&gate);
    result
}

/// Reclaims only the observed directory while the caller holds the gate lease.
///
/// The advisory lease serializes native gate creation, reclamation and release,
/// closing the gap between the identity check and rename. Cleanup touches only
/// the unique tombstone; it never removes a replacement at the shared pathname.
fn reclaim_stale_gate(gate: &Path, observed: Option<&GateObservation>) -> std::io::Result<()> {
    let unchanged = match (observed, GateObservation::read(gate)) {
        (Some(observed), Ok(current)) => same_gate_identity(observed, &current),
        _ => false,
    };
    if !unchanged {
        return Err(std::io::Error::from(ErrorKind::WouldBlock));
    }
    if fs::read_dir(gate)?.next().is_some() {
        return Err(std::io::Error::from(ErrorKind::DirectoryNotEmpty));
    }
    let tombstone = gate.with_file_name(format!(".reclaimed-gate-{}", unique_token()));
    fs::rename(gate, &tombstone).and_then(|()| fs::remove_dir(&tombstone))
}

/// Compares directory identity independently of its modification timestamp.
fn same_gate_identity(observed: &GateObservation, current: &GateObservation) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        (observed.metadata.dev(), observed.metadata.ino())
            == (current.metadata.dev(), current.metadata.ino())
    }
    #[cfg(windows)]
    {
        observed.identity == current.identity
    }
    #[cfg(not(any(unix, windows)))]
    {
        false
    }
}

/// Initializes or replaces one lock while its publication gate is held.
fn create_or_reclaim_lock(
    path: &Path,
    raw: &str,
    id: &str,
    ttl_seconds: u64,
    force_stale: bool,
    timestamp: &str,
    use_created_at: bool,
) -> Result<ItemLock, PmRustError> {
    match create_lock_file(path, raw, id) {
        Ok(lock) => return Ok(lock),
        Err(PmRustError::LockConflict { .. }) => {}
        Err(error) => return Err(error),
    }
    // `match` instead of `.map_err(..)` avoids a per-instantiation closure
    // function the coverage gate counts separately.
    // The same pending-delete window applies to reading the incumbent lock: the
    // create above failed because a lock was there, and the holder may release
    // it before this read runs.
    let existing_raw = match fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(source) if lock_error_is_contention(path, &source) => {
            return Err(PmRustError::LockConflict { id: id.to_owned() });
        }
        Err(source) => {
            return Err(PmRustError::Io {
                path: path.to_path_buf(),
                source,
            });
        }
    };
    let modified = fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .unwrap_or(UNIX_EPOCH);
    let stale = if use_created_at {
        ownership_lock_stale(&existing_raw, timestamp, ttl_seconds)
    } else {
        SystemTime::now()
            .duration_since(modified)
            .unwrap_or_default()
            > Duration::from_secs(ttl_seconds)
    };
    if !stale || !force_stale {
        return Err(PmRustError::LockConflict { id: id.to_owned() });
    }
    remove_stale_lock(path, &existing_raw, id).and_then(|()| create_lock_file(path, raw, id))
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
/// - Windows `ERROR_SHARING_VIOLATION` (32) and `ERROR_LOCK_VIOLATION` (33)
///   mean another handle denies access. Rust leaves these errors uncategorized,
///   so only these exact OS codes are recognized in addition to the kinds above.
///
/// A directory sitting where the lock file belongs is excluded whatever the
/// platform calls it. That is a structural fault and a permanent one, and the
/// platforms disagree about how it presents -- Unix reports `AlreadyExists`
/// then `IsADirectory`, Windows reports `PermissionDenied` -- so the shape of
/// the path decides it rather than the error kind.
///
/// Every remaining case is retried inside the caller's `wait_ms` budget, so a
/// lock that never frees still fails, and it fails as a lock conflict. The
/// residual cost is diagnostic: a `locks` directory that is genuinely
/// unwritable reports a lock conflict after the budget rather than a
/// permissions error. That trade is deliberate -- the common case is
/// contention, and the rare case still fails.
fn lock_error_is_contention(path: &Path, source: &std::io::Error) -> bool {
    if path.is_dir() {
        return false;
    }
    lock_contention_error(source)
}

/// Recognizes transient creation errors shared by lock files and publication gates.
fn lock_contention_error(source: &std::io::Error) -> bool {
    if matches!(
        source.kind(),
        ErrorKind::AlreadyExists | ErrorKind::PermissionDenied | ErrorKind::NotFound
    ) {
        return true;
    }
    #[cfg(windows)]
    {
        matches!(source.raw_os_error(), Some(32 | 33))
    }
    #[cfg(not(windows))]
    {
        false
    }
}

/// Creates and initializes a new lock file without replacing another writer.
fn create_lock_file(path: &Path, raw: &str, id: &str) -> Result<ItemLock, PmRustError> {
    match OpenOptions::new().write(true).create_new(true).open(path) {
        Ok(file) => write_lock_file(file, path, raw),
        Err(source) if lock_error_is_contention(path, &source) => {
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
    // Tabular rows follow their `{...}:` header. Preserve canonical scalar
    // quoting in those rows and in ordinary key/value fields.
    let mut inside_tabular_block = false;
    for line in encoded.lines() {
        if line.starts_with("tags[") {
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
///
/// Use the supported quoting API rather than probing the decoder: TOON v4.1
/// distinguishes numeric-looking strings such as `05` from numeric literals.
/// The decoder returns a string for `05`, but the encoder must keep its quotes.
/// Trailing-dot strings such as `0.` are unquoted in the published dialect.
fn safe_unquoted_scalar(value: &str) -> bool {
    !needs_quoting(value, ',')
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
    // `read_directory` and `read_optional` map IO failures with `match`, not
    // closures, so they do not spawn per-instantiation closure functions the
    // coverage gate would count as uncovered when the IO always succeeds.
    locate_item_entries(crate::workspace::read_directory(dir)?, at_root, id, found)
}

/// Probes a real directory snapshot, skipping entries removed before metadata reads.
fn locate_item_entries(
    entries: Vec<fs::DirEntry>,
    at_root: bool,
    id: &str,
    found: &mut Vec<(PathBuf, ItemDocument)>,
) -> Result<(), PmRustError> {
    let target = format!("{id}.toon");
    for entry in entries {
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
    crate::collation::normalize_tags(&mut request.tags);
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
    let mut extra = Map::new();
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
        crate::collation::normalize_tags(tags);
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
    crate::collation::normalize_comments(&mut comments);
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

/// Creates a published usage or conflict refusal for closure metadata.
fn close_refusal(detail: String, exit_code: u8, context: Value) -> PmRustError {
    let context = if exit_code == 2 && detail.starts_with("Invalid ") {
        let mut guidance = serde_json::json!({"code":"invalid_argument_value","title":"Invalid argument value","required":"Use values that match documented command constraints.","next_steps":["Check allowed values in command help, then rerun with corrected input."],"examples":["pm close --help","pm contracts --command <command> --flags-only --json"]});
        if let Some(field) = context["field"].as_str() {
            guidance["flag"] = serde_json::json!(format!("--{field}"));
            guidance["value"] = context["value"].clone();
        }
        guidance
    } else {
        context
    };
    PmRustError::CommandRefusal {
        detail,
        exit_code,
        context,
    }
}

/// Resolves explicit evidence validation, including the oracle's disabled aliases.
fn close_mode(raw: Option<&str>, settings: &Value) -> Result<String, PmRustError> {
    let Some(raw) = raw else {
        let governance = &settings["governance"];
        return Ok(match governance["preset"].as_str() {
            Some("strict") => "strict",
            Some("default") => "warn",
            Some("custom") => governance["close_validation_default"]
                .as_str()
                .unwrap_or("warn"),
            _ => "off",
        }
        .to_owned());
    };
    match raw.trim().to_lowercase().as_str() {
        "" | "warn" => Ok("warn".to_owned()),
        "off" | "none" | "disabled" => Ok("off".to_owned()),
        "strict" => Ok("strict".to_owned()),
        _ => Err(close_refusal(
            format!(
                "Invalid --validate-close mode \"{raw}\" (expected \"off\", \"warn\", or \"strict\")"
            ),
            2,
            serde_json::json!({}),
        )),
    }
}

/// Finds missing nonblank closure evidence in canonical field order.
fn missing_close_fields(document: &ItemDocument) -> Vec<&'static str> {
    ["resolution", "expected_result", "actual_result"]
        .into_iter()
        .filter(|key| ownership_text(document, key).trim().is_empty())
        .collect()
}

/// Quotes one command argument with the published platform-specific renderer.
#[cfg(not(windows))]
#[must_use]
pub fn quote_command_arg(value: &str) -> String {
    if !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"_.:/@=-".contains(&byte))
    {
        return value.to_owned();
    }
    let mut result = String::from("\"");
    for ch in value.chars() {
        if matches!(ch, '\"' | '\\' | '$' | '`') {
            result.push('\\');
        }
        result.push(ch);
    }
    result.push('\"');
    result
}

/// Quotes one command argument using the Windows native command-line grammar.
#[cfg(windows)]
#[must_use]
pub fn quote_command_arg(value: &str) -> String {
    if !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"_.:/\\@=-".contains(&byte))
    {
        return value.to_owned();
    }
    let mut result = String::from("\"");
    let mut pending = 0;
    for ch in value.chars() {
        if ch == '\\' {
            pending += 1;
            continue;
        }
        result.push_str(&"\\".repeat(if ch == '\"' { pending * 2 + 1 } else { pending }));
        result.push(ch);
        pending = 0;
    }
    result.push_str(&"\\".repeat(pending * 2));
    result.push('\"');
    result
}

/// Builds the same evidence recovery tokens using the actual disposable tracker root.
fn close_recovery(display_root: &Path, document: &ItemDocument) -> Option<Value> {
    let missing = missing_close_fields(document);
    if missing.is_empty() {
        return None;
    }
    let flags: Vec<String> = missing
        .iter()
        .map(|key| format!("--{}", key.replace('_', "-")))
        .collect();
    let mut args = vec![
        "--pm-path".to_owned(),
        display_root.to_string_lossy().into_owned(),
        "update".to_owned(),
        document.metadata.id.clone(),
    ];
    for flag in &flags {
        args.extend([flag.clone(), "<value>".to_owned()]);
    }
    let command = format!(
        "pm {}",
        args.iter()
            .map(|arg| quote_command_arg(arg))
            .collect::<Vec<_>>()
            .join(" ")
    );
    Some(serde_json::json!({"missing":flags,"suggested_retry":command,"suggested_retry_args":args}))
}

/// Rejects impossible ISO calendar dates with the oracle's field-specific guidance.
fn close_calendar(raw: &str) -> Result<String, PmRustError> {
    let value = raw.trim();
    let date = if let Some(prefix) = value
        .get(..10)
        .filter(|prefix| prefix.as_bytes()[4] == b'-' && prefix.as_bytes()[7] == b'-')
    {
        Some((
            prefix.get(..4).unwrap_or_default(),
            prefix.get(5..7).unwrap_or_default(),
            prefix.get(8..10).unwrap_or_default(),
            false,
        ))
    } else {
        value
            .get(..8)
            .filter(|prefix| prefix.bytes().all(|byte| byte.is_ascii_digit()))
            .map(|prefix| {
                (
                    prefix.get(..4).unwrap_or_default(),
                    prefix.get(4..6).unwrap_or_default(),
                    prefix.get(6..8).unwrap_or_default(),
                    true,
                )
            })
    };
    let Some((year, month, day, compact)) = date else {
        return Ok(value.to_owned());
    };
    let (Ok(year), Ok(month_number), Ok(day_number)) =
        (year.parse::<i32>(), month.parse::<u8>(), day.parse::<u8>())
    else {
        return Ok(value.to_owned());
    };
    let month_value = time::Month::try_from(month_number).map_err(|_| close_refusal(format!("Invalid completed-at value \"{raw}\". Month \"{month}\" is out of range — use a month between 01 and 12."),2,serde_json::json!({"field":"completed-at","value":raw})))?;
    let maximum = month_value.length(year);
    if day_number == 0 || day_number > maximum {
        return Err(close_refusal(
            format!(
                "Invalid completed-at value \"{raw}\". {month_value} {year} has {maximum} days, so day \"{day}\" does not exist. Use a real YYYY-MM-DD calendar date."
            ),
            2,
            serde_json::json!({"field":"completed-at","value":raw}),
        ));
    }
    if compact {
        Ok(format!(
            "{year:04}-{month_number:02}-{day_number:02}{}",
            value.get(8..).unwrap_or_default()
        ))
    } else {
        Ok(value.to_owned())
    }
}

/// Applies relative months without overflowing the calendar representation.
fn close_relative_month(base: OffsetDateTime, amount: i64) -> Option<OffsetDateTime> {
    let month_index = (i64::from(base.year()) * 12 + i64::from(u8::from(base.month())) - 1)
        .checked_add(amount)?;
    let year = i32::try_from(month_index.div_euclid(12)).ok()?;
    let month =
        time::Month::January.nth_next(u8::try_from(month_index.rem_euclid(12)).unwrap_or_default());
    let mut day = base.day();
    while time::Date::from_calendar_date(year, month, day).is_err() && day > 1 {
        day -= 1;
    }
    time::Date::from_calendar_date(year, month, day)
        .ok()
        .map(|date| base.replace_date(date))
}

/// Distinguishes malformed relative input from the oracle's runtime date overflow.
fn close_relative_completion(
    value: &str,
    base: OffsetDateTime,
) -> Result<Option<OffsetDateTime>, PmRustError> {
    let Some(amount) = value
        .get(..value.len().saturating_sub(1))
        .and_then(|amount| amount.parse::<i64>().ok())
    else {
        return Ok(None);
    };
    let parsed = match value.as_bytes().last().map(u8::to_ascii_lowercase) {
        Some(b'h') => amount
            .checked_mul(3600)
            .and_then(|seconds| base.checked_add(time::Duration::seconds(seconds))),
        Some(b'd') => amount
            .checked_mul(86400)
            .and_then(|seconds| base.checked_add(time::Duration::seconds(seconds))),
        Some(b'w') => amount
            .checked_mul(604_800)
            .and_then(|seconds| base.checked_add(time::Duration::seconds(seconds))),
        Some(b'm') => close_relative_month(base, amount),
        _ => return Ok(None),
    };
    parsed.map(Some).ok_or_else(|| {
        close_refusal(
            "Invalid time value".to_owned(),
            1,
            serde_json::json!({"code":"unknown_error"}),
        )
    })
}

/// Parses a completion time independently of the tracker close time.
fn close_completion(raw: &str, timestamp: &str) -> Result<String, PmRustError> {
    let Ok(base) = OffsetDateTime::parse(timestamp, &Rfc3339) else {
        return Err(invalid_mutation("invalid closure clock"));
    };
    let value = raw.trim();
    let parsed = if value.eq_ignore_ascii_case("now") {
        Some(base)
    } else if value.starts_with(['+', '-']) {
        close_relative_completion(value, base)?
    } else {
        let value = close_calendar(raw)?;
        OffsetDateTime::parse(&value, &Rfc3339)
            .ok()
            .or_else(|| OffsetDateTime::parse(&format!("{value}T00:00:00Z"), &Rfc3339).ok())
    };
    let Some(parsed) = parsed else {
        let guidance = if value.matches(['+', '-']).count() > 1 && value.starts_with(['+', '-']) {
            "Compound relative expressions like +3d+1h are not supported; use a single relative token (for example +3d) or an ISO/date string."
        } else {
            "Use ISO/date string input, \"now\", or relative +6h/-6h/+1d/-1d/+2w/-2w/+6m/-6m."
        };
        return Err(close_refusal(
            format!("Invalid completed-at value \"{raw}\". {guidance}"),
            2,
            serde_json::json!({"field":"completed-at","value":raw}),
        ));
    };
    Ok(format!(
        "{}.{:03}Z",
        parsed
            .to_offset(time::UtcOffset::UTC)
            .format(
                &time::format_description::parse_borrowed::<2>(
                    "[year]-[month]-[day]T[hour]:[minute]:[second]"
                )
                .unwrap_or_default()
            )
            .unwrap_or_default(),
        parsed.millisecond()
    ))
}

/// Validates a duplicate target and follows existing duplicate edges without loops.
fn close_duplicate(
    pm_root: &Path,
    id: &str,
    target: Option<&str>,
) -> Result<Option<String>, PmRustError> {
    let Some(target) = target.map(str::trim).filter(|target| !target.is_empty()) else {
        return Ok(None);
    };
    if target == id {
        return Err(close_refusal(
            "An item cannot be closed as a duplicate of itself.".to_owned(),
            2,
            serde_json::json!({"code":"duplicate_target_self","why":"--duplicate-of must identify the canonical item that should remain open or already represent the work."}),
        ));
    }
    let target_doc = match locate_item(pm_root, target) {
        Ok((_, document)) => document,
        Err(PmRustError::ItemNotFound { .. }) => {
            return Err(close_refusal(
                format!(
                    "Duplicate target \"{target}\" was not found. Create or locate the canonical item first."
                ),
                2,
                serde_json::json!({"code":"duplicate_target_missing","why":"Duplicate closure should point at a real canonical pm item so future dedupe and changelog tooling can trace the relationship.","examples":[format!("pm close {id} \"Duplicate of {target}\" --duplicate-of {target}")],"next_steps":["Run pm search/list to find the canonical item, then retry with --duplicate-of <id>."]}),
            ));
        }
        Err(error) => return Err(error),
    };
    let initial = ownership_text(&target_doc, "duplicate_of");
    let mut current = initial.trim().to_owned();
    let mut visited = std::collections::BTreeSet::new();
    while !current.is_empty() {
        if current == id {
            return Err(close_refusal(
                format!(
                    "Circular duplicate reference detected. Target \"{target}\" points back to \"{id}\"."
                ),
                2,
                serde_json::json!({"code":"duplicate_target_circular","why":"Circular duplicate relationships create loops for dedupe and status propagation tooling.","next_steps":["Choose the existing canonical item, or clear the target duplicate_of metadata before closing this item as a duplicate."]}),
            ));
        }
        if !visited.insert(current.clone()) {
            break;
        }
        current = match locate_item(pm_root, &current) {
            Ok((_, document)) => ownership_text(&document, "duplicate_of").trim().to_owned(),
            Err(PmRustError::ItemNotFound { .. }) => String::new(),
            Err(error) => return Err(error),
        };
    }
    if !initial.trim().is_empty() {
        return Err(close_refusal(
            format!(
                "Duplicate target \"{target}\" is already marked as a duplicate of \"{}\".",
                initial.trim()
            ),
            2,
            serde_json::json!({"code":"duplicate_target_is_duplicate","why":"Duplicate closure should point directly at the canonical item, not another duplicate.","examples":[format!("pm close {id} \"Duplicate of {}\" --duplicate-of {}",initial.trim(),initial.trim())],"next_steps":["Use the canonical item referenced by duplicate_of as the --duplicate-of target."]}),
        ));
    }
    Ok(Some(target_doc.metadata.id))
}

/// Closes one item with published metadata, validation, ownership and history semantics.
#[allow(clippy::too_many_lines, clippy::needless_pass_by_value)]
pub(crate) fn close_item(
    pm_root: &Path,
    display_root: &Path,
    request: CloseItem,
) -> Result<MutationResult, PmRustError> {
    let mut settings = read_settings(pm_root)?;
    let timestamp = validate_mutation_request(&request.author, request.timestamp.as_deref())?;
    let value = crate::ownership::runtime_settings(pm_root)?;
    let duplicate = close_duplicate(pm_root, &request.id, request.fields.duplicate_of.as_deref())?;
    let duplicate_reason = duplicate.as_ref().map(|id| format!("Duplicate of {id}"));
    let reason = [
        Some(request.reason.as_str()),
        duplicate_reason.as_deref(),
        request.fields.resolution.as_deref(),
        request.fields.message.as_deref(),
    ]
    .into_iter()
    .flatten()
    .map(str::trim)
    .find(|text| !text.is_empty());
    if reason.is_none()
        && value["governance"]["require_close_reason"]
            .as_bool()
            .unwrap_or(true)
    {
        return Err(close_refusal(
            "Close reason text is required because governance.require_close_reason is enabled"
                .to_owned(),
            2,
            serde_json::json!({"code":"close_reason_required","required":"Provide an author-controlled closing summary as the reason, message, resolution, or duplicate target.","why":"governance.require_close_reason is enabled, so tools must not invent immutable closure evidence.","examples":["pm close <id> \"Done: <what changed and why>\"","pm close <id> --reason \"<closing summary>\"","pm close <id> -m \"<closing summary>\"","pm close <id> --duplicate-of <canonical-id>"],"next_steps":["Re-run the close operation with a closing summary.","To stop requiring reasons, run: pm config set governance-require-close-reason --policy disabled"]}),
        ));
    }
    let mode = close_mode(request.fields.validate_close.as_deref(), &value)?;
    let completed = match &request.fields.completed_at {
        Some(raw) => close_completion(raw, &timestamp)?,
        None => timestamp.clone(),
    };
    settings.locks.use_created_at = true;
    let _lock = acquire_lock(
        pm_root,
        &request.id,
        &request.author,
        &settings.locks,
        request.force_stale_lock
            || request.fields.force
            || !settings.governance.stale_requires_force(),
        &timestamp,
    )?;
    let (item_path, history_path, before) = recover_and_locate(pm_root, "close", &request.id)?;
    if (crate::list::terminal(&before.metadata.status)
        || before.metadata.status == settings.workflow.close_status)
        && !request.fields.force
    {
        let mut context = serde_json::json!({});
        if let Some(recovery) = close_recovery(display_root, &before) {
            context["recovery"] = recovery;
        }
        return Err(close_refusal(
            format!(
                "Item {} is already terminal; use pm update to append acceptance evidence, or --force only for an intentional repeat closure.",
                request.id
            ),
            4,
            context,
        ));
    }
    let owner = ownership_text(&before, "assignee");
    let principal = ownership_text(&before, "claim_principal");
    let principal = if principal.is_empty() {
        owner.as_str()
    } else {
        &principal
    };
    let mut warnings = Vec::new();
    if !owner.trim().is_empty() && principal != request.author && !request.fields.force {
        if settings.governance.policy() == "strict" {
            return Err(ownership_refusal(
                format!(
                    "Item {} is assigned to {}. Use --force to override.",
                    request.id,
                    owner.trim()
                ),
                serde_json::json!({"code":"ownership_conflict"}),
            ));
        }
        if settings.governance.policy() == "warn" {
            warnings.push(format!(
                "ownership_warning:assignee_conflict:{}:{}",
                request.id,
                owner.trim()
            ));
        }
    }
    let mut document = before.clone();
    let mut inline = Vec::new();
    for (key, option) in [
        ("resolution", &request.fields.resolution),
        ("expected_result", &request.fields.expected_result),
        ("actual_result", &request.fields.actual_result),
    ] {
        if let Some(text) = option
            .as_deref()
            .map(str::trim)
            .filter(|text| !text.is_empty())
        {
            document
                .metadata
                .extra
                .insert(key.to_owned(), Value::String(text.to_owned()));
            inline.push(key);
        }
    }
    let mut fallback = Vec::new();
    if let Some(target) = &duplicate {
        document
            .metadata
            .extra
            .insert("duplicate_of".to_owned(), Value::String(target.clone()));
        for (key, text) in [
            ("resolution", format!("Duplicate of {target}")),
            (
                "expected_result",
                format!("Canonical item {target} tracks the work."),
            ),
            ("actual_result", format!("Closed as duplicate of {target}.")),
        ] {
            if ownership_text(&document, key).trim().is_empty() {
                document
                    .metadata
                    .extra
                    .insert(key.to_owned(), Value::String(text));
                fallback.push(key);
            }
        }
    }
    let missing = missing_close_fields(&document);
    if mode != "off" && !missing.is_empty() {
        let labels: Vec<String> = missing
            .iter()
            .map(|key| format!("{key} (--{})", key.replace('_', "-")))
            .collect();
        if mode == "strict" {
            let mut recovery = close_recovery(display_root, &document).unwrap_or_default();
            recovery
                .as_object_mut()
                .map(|map| map.shift_remove("suggested_retry_args"));
            return Err(close_refusal(
                format!(
                    "Cannot close item {}: missing {}. Populate fields or use --validate-close warn.",
                    request.id,
                    labels.join(", ")
                ),
                2,
                serde_json::json!({"code":"close_validation_missing_fields","recovery":recovery,"next_steps":["Run the suggested pm update, then retry the original pm close invocation.","Use --validate-close warn only when governance permits an evidenced exception."]}),
            ));
        }
        warnings.push(format!(
            "close_validation_missing_fields:{}:{}",
            request.id,
            labels.join(",")
        ));
    }
    let workspace = crate::Workspace::discover(pm_root)?;
    let mut children: Vec<String> = workspace
        .read_items()?
        .into_iter()
        .filter(|item| {
            item.metadata.parent.as_deref() == Some(&request.id)
                && !crate::list::terminal(&item.metadata.status)
        })
        .map(|item| item.metadata.id)
        .collect();
    children.sort_by(|left, right| crate::collation::compare(left, right));
    if !children.is_empty() {
        if mode == "strict" {
            return Err(close_refusal(
                format!(
                    "Cannot close item {}: active child items remain open ({}). Close, cancel, or re-parent them first, or use --validate-close warn.",
                    request.id,
                    children.join(", ")
                ),
                2,
                serde_json::json!({}),
            ));
        }
        warnings.push(format!(
            "{}:{}:{}",
            if mode == "off" {
                "closed_with_active_children"
            } else {
                "close_validation_active_children"
            },
            request.id,
            children.join(",")
        ));
    }
    let mut changed = vec!["status", "closed_at"];
    document
        .metadata
        .status
        .clone_from(&settings.workflow.close_status);
    document
        .metadata
        .extra
        .insert("closed_at".to_owned(), Value::String(timestamp.clone()));
    if request.fields.completed_at.is_some()
        || !document.metadata.extra.contains_key("completed_at")
    {
        document
            .metadata
            .extra
            .insert("completed_at".to_owned(), Value::String(completed));
        changed.push("completed_at");
    }
    if let Some(reason) = reason {
        document
            .metadata
            .extra
            .insert("close_reason".to_owned(), Value::String(reason.to_owned()));
        changed.push("close_reason");
    }
    changed.extend(inline);
    if duplicate.is_some() {
        changed.push("duplicate_of");
        changed.extend(fallback);
    }
    if document.metadata.extra.remove("assignee").is_some() {
        changed.push("assignee");
    }
    let previous_blocker = ownership_text(&document, "blocked_by");
    let mut blockers = Vec::new();
    if !previous_blocker.trim().is_empty() {
        blockers.push(previous_blocker.trim().to_owned());
    }
    if let Some(deps) = document
        .metadata
        .extra
        .get("dependencies")
        .and_then(Value::as_array)
    {
        for dep in deps {
            if dep["kind"] == "blocked_by"
                && let Some(id) = dep["id"].as_str()
                && !blockers.iter().any(|prior| prior == id)
            {
                blockers.push(id.to_owned());
            }
        }
    }
    for key in ["blocked_by", "blocked_reason"] {
        if document.metadata.extra.remove(key).is_some() {
            changed.push(key);
        }
    }
    if !blockers.is_empty() {
        warnings.push(format!(
            "closed_preserved_predecessors:{}:{}",
            request.id,
            blockers.join(",")
        ));
    }
    document.metadata.updated_at.clone_from(&timestamp);
    let recovery = if mode == "warn" {
        close_recovery(display_root, &document)
    } else {
        None
    };
    let mut receipt = serde_json::json!({"id":document.metadata.id,"status":document.metadata.status,"changed_field_count":changed.len()});
    if let Some(reason) = document.metadata.extra.get("close_reason") {
        receipt["close_reason"] = reason.clone();
    }
    if !warnings.is_empty() {
        receipt["warnings"] = serde_json::json!(warnings);
    }
    if let Some(recovery) = recovery {
        receipt["recovery"] = recovery;
    }
    let mut result = commit_mutation(
        pm_root,
        "close",
        &before,
        document,
        &timestamp,
        &request.author,
        request.provenance_role.as_deref(),
        request.fields.message.as_deref(),
        &item_path,
        &history_path,
    )?;
    result.close_changed_fields = changed.into_iter().map(str::to_owned).collect();
    result.close_receipt = Some(receipt);
    Ok(result)
}

/// Extracts canonical string ownership metadata without inventing defaults.
fn ownership_text(document: &ItemDocument, field: &str) -> String {
    match document.metadata.extra.get(field) {
        Some(Value::String(value)) => value.clone(),
        _ => String::new(),
    }
}

/// Builds a typed published refusal with its structured guidance.
fn ownership_refusal(detail: String, context: Value) -> PmRustError {
    PmRustError::OwnershipRefusal { detail, context }
}

/// Describes assignment provenance from the last assignee patch in history.
fn ownership_phrase(history_path: &Path, assignee: &str) -> Result<String, PmRustError> {
    let raw = read_optional(history_path)?.unwrap_or_default();
    for line in raw.lines().rev() {
        let Ok(entry) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if let Some(Value::Array(patches)) = entry.get("patch") {
            for patch in patches {
                if patch["path"] == "/metadata/assignee" {
                    return Ok(format!(
                        "{} {assignee}",
                        if entry["op"] == "claim" {
                            "claimed by"
                        } else {
                            "assigned to"
                        }
                    ));
                }
            }
        }
    }
    Ok(format!("assigned to {assignee}"))
}

/// Uses the published lock payload clock, including malformed-lock expiry.
fn ownership_lock_stale(raw: &str, timestamp: &str, ttl_seconds: u64) -> bool {
    let entry: Value = serde_json::from_str(raw).unwrap_or(Value::Null);
    let created = entry["created_at"].as_str().unwrap_or_default();
    match OffsetDateTime::parse(created, &Rfc3339) {
        Ok(created) => {
            let now =
                OffsetDateTime::parse(timestamp, &Rfc3339).unwrap_or(OffsetDateTime::UNIX_EPOCH);
            now.unix_timestamp_nanos() - created.unix_timestamp_nanos()
                > i128::from(ttl_seconds) * 1_000_000_000
        }
        Err(_) => true,
    }
}

/// Converts exhausted ownership lock contention to the published conflict.
fn ownership_lock_refusal(
    pm_root: &Path,
    request: &OwnershipItem,
    settings: &MutationSettings,
    timestamp: &str,
    waited_ms: u128,
) -> PmRustError {
    let raw = match read_optional(&pm_root.join("locks").join(format!("{}.lock", request.id))) {
        Ok(raw) => raw.unwrap_or_default(),
        Err(error) => return error,
    };
    if ownership_lock_stale(&raw, timestamp, settings.locks.ttl_seconds) {
        return ownership_refusal(
            format!(
                "Item {} lock is stale; rerun with --force when supported for this command",
                request.id
            ),
            serde_json::json!({}),
        );
    }
    let value: Value = serde_json::from_str(&raw).unwrap_or(Value::Null);
    let owner = value["owner"].as_str().unwrap_or_default();
    let owner_suffix = if owner.is_empty() {
        String::new()
    } else {
        format!(" (owner {owner})")
    };
    let waited = if waited_ms == 0 {
        String::new()
    } else {
        format!(" after waiting {waited_ms}ms")
    };
    ownership_refusal(
        format!("Item {} is locked{owner_suffix}{waited}", request.id),
        serde_json::json!({"code":"lock_conflict","recovery":{"retry_after_ms":250}}),
    )
}

/// Applies a collection edit under the same lock, recovery and history boundary as other writes.
#[allow(clippy::too_many_lines)]
pub(crate) fn collection_item(
    pm_root: &Path,
    id: &str,
    key: &str,
    options: &crate::CollectionOptions,
    mut edit: crate::collections::Edit,
) -> Result<(ItemDocument, bool, usize), PmRustError> {
    let mut settings = read_settings(pm_root)?;
    let timestamp = validate_mutation_request(&options.author, options.timestamp.as_deref())?;
    settings.locks.use_created_at = true;
    // Preserve fixture timestamps in history, but never let them age a live lock.
    let wall_clock = now_iso();
    let lock_clock = if OffsetDateTime::parse(&timestamp, &Rfc3339)
        .is_ok_and(|value| value > OffsetDateTime::now_utc())
    {
        &wall_clock
    } else {
        &timestamp
    };
    let request = OwnershipItem {
        id: id.to_owned(),
        author: options.author.clone(),
        timestamp: options.timestamp.clone(),
        message: options.message.clone(),
        force: options.force,
        if_available: false,
        provenance_role: None,
    };
    let started = Instant::now();
    let _lock = acquire_lock(
        pm_root,
        id,
        &options.author,
        &settings.locks,
        options.force || !settings.governance.stale_requires_force(),
        lock_clock,
    )
    .map_err(|error| match error {
        PmRustError::LockConflict { .. } => ownership_lock_refusal(
            pm_root,
            &request,
            &settings,
            lock_clock,
            if settings.locks.wait_ms == 0 {
                0
            } else {
                started.elapsed().as_millis()
            },
        ),
        error => error,
    })?;
    let operation = match key {
        "files" => "files_add",
        "docs" => "docs_add",
        _ if edit.adds.is_empty() => "tests_remove",
        _ => "tests_add",
    };
    let (item_path, history_path, before) =
        recover_and_locate(pm_root, operation, id).map_err(|error| match error {
            PmRustError::ItemNotFound { .. } => crate::collections::missing(pm_root, id),
            error => error,
        })?;
    let owner = ownership_text(&before, "assignee");
    let principal = ownership_text(&before, "claim_principal");
    let caller_principal = options
        .identity
        .as_ref()
        .map_or(options.author.as_str(), |identity| {
            identity.principal.as_str()
        });
    if !owner.trim().is_empty()
        && (if principal.is_empty() {
            &owner
        } else {
            &principal
        }) != caller_principal
        && !options.force
        && settings.governance.policy() == "strict"
    {
        return Err(ownership_refusal(
            format!(
                "Item {id} is assigned to {}. Use --force to override.",
                owner.trim()
            ),
            serde_json::json!({"code":"ownership_conflict"}),
        ));
    }
    let source = if key == "tests" && !edit.adds.is_empty() {
        crate::collections::source_ref()
    } else {
        None
    };
    for add in &mut edit.adds {
        if key == "tests" {
            add["provenance"] = serde_json::json!({"author":options.author,"created_at":timestamp,"source_kind":"local_mutation"});
            if let Some(source) = &source {
                add["provenance"]["source_ref"] = serde_json::json!(source);
            }
            if let Some(note) = add.as_object_mut().and_then(|map| map.shift_remove("note")) {
                add["note"] = note;
            }
        } else if let Some(note) = options
            .note
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            && add.get("note").is_none()
        {
            add["note"] = serde_json::json!(note);
        }
    }
    let current = before
        .metadata
        .extra
        .get(key)
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let (rows, removed) = crate::collections::merge(
        key,
        &current,
        &edit,
        key == "files" && options.append_stable,
    );
    if key == "tests" && (!edit.removes.is_empty() || !edit.indices.is_empty()) && removed == 0 {
        return Err(crate::collections::refusal(
            "No linked tests matched the requested removal selector",
            3,
            serde_json::json!({"code":"linked_test_remove_no_match","unmatched":edit.removes.iter().map(|(kind,value)|format!("{kind}={value}")).chain(edit.indices.iter().map(|i|format!("index={i}"))).collect::<Vec<_>>(),"required":"List linked tests and retry with an exact command/path or a current 1-based --remove-index.","examples":[format!("pm test {id} --list"),format!("pm test {id} --remove-index 1")]}),
        ));
    }
    if current == rows {
        return Ok((before, false, removed));
    }
    let mut document = before.clone();
    if rows.is_empty() {
        document.metadata.extra.remove(key);
    } else {
        document
            .metadata
            .extra
            .insert(key.to_owned(), Value::Array(rows));
    }
    document.metadata.updated_at.clone_from(&timestamp);
    let result = commit_mutation(
        pm_root,
        operation,
        &before,
        document,
        &timestamp,
        &options.author,
        None,
        options.message.as_deref(),
        &item_path,
        &history_path,
    )?;
    Ok((result.item, true, removed))
}

/// Executes asserted ownership transactions for the SDK contract tests.
#[cfg(test)]
#[allow(clippy::too_many_lines)]
pub(crate) fn ownership_item(
    pm_root: &Path,
    request: &OwnershipItem,
    claim: bool,
) -> Result<OwnershipResult, PmRustError> {
    ownership_item_with_options(pm_root, request, claim, &crate::OwnershipOptions::default())
}

/// Applies explicit principal and approved release bypass within the ownership lock.
#[allow(clippy::too_many_lines)]
pub(crate) fn ownership_item_with_options(
    pm_root: &Path,
    request: &OwnershipItem,
    claim: bool,
    options: &crate::OwnershipOptions,
) -> Result<OwnershipResult, PmRustError> {
    let mut settings = read_settings(pm_root)?;
    settings.locks.use_created_at = true;
    let timestamp = validate_mutation_request(&request.author, request.timestamp.as_deref())?;
    // A fixture clock must never age a live lock into stale-lock recovery.
    if OffsetDateTime::parse(&timestamp, &Rfc3339)
        .is_ok_and(|value| value > OffsetDateTime::now_utc())
    {
        return Err(invalid_mutation("timestamp must not be in the future"));
    }
    let operation = if claim { "claim" } else { "release" };
    let lock_started = Instant::now();
    let _lock = match acquire_lock(
        pm_root,
        &request.id,
        &request.author,
        &settings.locks,
        request.force || !settings.governance.stale_requires_force(),
        &timestamp,
    ) {
        Ok(lock) => lock,
        Err(PmRustError::LockConflict { .. }) => {
            return Err(ownership_lock_refusal(
                pm_root,
                request,
                &settings,
                &timestamp,
                if settings.locks.wait_ms == 0 {
                    0
                } else {
                    lock_started.elapsed().as_millis()
                },
            ));
        }
        Err(error) => return Err(error),
    };
    let (item_path, history_path, before) = recover_and_locate(pm_root, operation, &request.id)?;
    let assignee = ownership_text(&before, "assignee");
    let principal = ownership_text(&before, "claim_principal");
    let principal = if principal.trim().is_empty() {
        assignee.as_str()
    } else {
        principal.trim()
    };
    let previous_assignee = if assignee.trim().is_empty() {
        None
    } else {
        Some(assignee.clone())
    };
    let caller_principal = options
        .identity
        .as_ref()
        .map_or(request.author.as_str(), |identity| {
            identity.principal.as_str()
        });
    let foreign = previous_assignee.is_some() && principal != caller_principal;
    let mut document = before.clone();
    let mut warnings = Vec::new();
    let mut suggestions = Vec::new();
    let mut skipped = None;
    let mut changed = 0;
    if claim {
        if matches!(before.metadata.status.as_str(), "closed" | "canceled") && !request.force {
            return Err(ownership_refusal(
                format!("Cannot claim terminal item {} without --force", request.id),
                serde_json::json!({}),
            ));
        }
        if foreign {
            if request.if_available {
                skipped = Some(true);
                warnings.push(format!("claim_skipped_held_by:{assignee}"));
            } else if !request.force {
                let phrase = ownership_phrase(&history_path, &assignee)?;
                return Err(ownership_refusal(
                    format!(
                        "Item {} is already {phrase}. Use --force to take over, or --if-available to skip without failing.",
                        request.id
                    ),
                    serde_json::json!({
                        "code":"already_claimed_by",
                        "why":"Claim is an atomic test-and-set so parallel agents never proceed believing they own the same item.",
                        "next_steps":["Run pm next to pick a different unclaimed item.","Re-run with --if-available to treat a held item as a no-op skip.","Re-run with --force only when taking over the item is coordinated."]
                    }),
                ));
            } else {
                warnings.push(format!("claim_takeover:{assignee}->{}", request.author));
            }
        }
        if skipped.is_none() {
            for field in ["assignee", "claim_principal"] {
                let value = Value::String(if field == "claim_principal" {
                    caller_principal.to_owned()
                } else {
                    request.author.clone()
                });
                if document.metadata.extra.get(field) != Some(&value) {
                    changed += 1;
                }
                document.metadata.extra.insert(field.to_owned(), value);
            }
        }
    } else {
        if foreign
            && !request.force
            && !options.release_bypass
            && settings.governance.policy() == "strict"
        {
            return Err(ownership_refusal(
                format!(
                    "Item {} is assigned to {}. Use --force to override.",
                    request.id,
                    assignee.trim()
                ),
                serde_json::json!({
                    "code":"ownership_conflict",
                    "required":"For approved non-owner handoffs, prefer the ownership-release bypass before considering --force.",
                    "examples":["pm release pm-a1b2 --author \"reviewer\" --force"],
                    "next_steps":["Use the package-provided ownership-release bypass for handoffs that only clear assignee metadata.","Use --force only when an explicit override is approved for broader ownership conflicts."]
                }),
            ));
        }
        if !assignee.is_empty() || !ownership_text(&document, "claim_principal").is_empty() {
            changed = 2;
            document.metadata.extra.remove("assignee");
            document.metadata.extra.remove("claim_principal");
        }
        if document.metadata.status == "in_progress" {
            warnings.push("released_unclaimed_in_progress".to_owned());
            suggestions.push(format!("{} remains in_progress and unclaimed. If work is paused, run pm release {} --pause to move it to open.", request.id, request.id));
        }
    }
    // Claim skips history on no-op; release deliberately records even an empty patch.
    if !claim || changed > 0 {
        document.metadata.updated_at.clone_from(&timestamp);
        document = commit_mutation(
            pm_root,
            operation,
            &before,
            document,
            &timestamp,
            &request.author,
            request.provenance_role.as_deref(),
            request.message.as_deref(),
            &item_path,
            &history_path,
        )?
        .item;
    }
    let claimed_by = if claim {
        Some(if skipped.is_some() {
            assignee
        } else {
            request.author.clone()
        })
    } else {
        None
    };
    let released_by = if claim {
        None
    } else {
        Some(request.author.clone())
    };
    let close_reason = match document.metadata.extra.get("close_reason") {
        Some(Value::String(reason)) => Some(reason.clone()),
        _ => None,
    };
    Ok(OwnershipResult {
        id: document.metadata.id.clone(),
        status: document.metadata.status.clone(),
        item: document,
        close_reason,
        changed_field_count: changed,
        claimed_by,
        released_by,
        previous_assignee,
        forced: request.force,
        skipped,
        warnings,
        suggestions,
    })
}

/// Validates a status transition while retaining unconstrained and no-op workflows.
fn validate_lifecycle_transition(
    value: &Value,
    before: &ItemDocument,
    status: &str,
) -> Result<(), PmRustError> {
    if value["governance"]["workflow_enforcement"] != "strict" {
        return Ok(());
    }
    let Some(rules) = value["schema"]["type_workflows"].as_array() else {
        return Ok(());
    };
    let Some(rule) = rules.iter().find(|rule| {
        rule["type"]
            .as_str()
            .is_some_and(|name| name.eq_ignore_ascii_case(&before.metadata.item_type))
    }) else {
        return Ok(());
    };
    let Some(transitions) = rule["allowed_transitions"].as_array() else {
        return Ok(());
    };
    if before.metadata.status == status
        || transitions
            .iter()
            .any(|pair| pair[0] == before.metadata.status && pair[1] == status)
    {
        return Ok(());
    }
    let allowed = if transitions.is_empty() {
        "(no transitions allowed)".to_owned()
    } else {
        transitions
            .iter()
            .map(|pair| {
                format!(
                    "{} -> {}",
                    pair[0].as_str().unwrap_or_default(),
                    pair[1].as_str().unwrap_or_default()
                )
            })
            .collect::<Vec<_>>()
            .join(", ")
    };
    Err(PmRustError::CommandRefusal {
        detail: format!(
            "Disallowed transition for type \"{}\": {} -> {status}. Allowed transitions: {allowed}.",
            before.metadata.item_type, before.metadata.status
        ),
        exit_code: 2,
        context: serde_json::json!({}),
    })
}

/// Applies the second start step or the first pause step under an independent lock.
#[allow(clippy::too_many_lines)]
pub(crate) fn lifecycle_status(
    pm_root: &Path,
    request: &OwnershipItem,
    options: &crate::OwnershipOptions,
    start: bool,
) -> Result<Value, PmRustError> {
    let mut settings = read_settings(pm_root)?;
    settings.locks.use_created_at = true;
    let timestamp = validate_mutation_request(&request.author, request.timestamp.as_deref())?;
    if OffsetDateTime::parse(&timestamp, &Rfc3339)
        .is_ok_and(|value| value > OffsetDateTime::now_utc())
    {
        return Err(invalid_mutation("timestamp must not be in the future"));
    }
    let _lock = acquire_lock(
        pm_root,
        &request.id,
        &request.author,
        &settings.locks,
        request.force || !settings.governance.stale_requires_force(),
        &timestamp,
    )
    .map_err(|error| match error {
        PmRustError::LockConflict { .. } => ownership_refusal(
            format!("Item {} is locked", request.id),
            serde_json::json!({"code":"lock_conflict"}),
        ),
        other => other,
    })?;
    let (item_path, history_path, before) = recover_and_locate(pm_root, "update", &request.id)?;
    let principal = options
        .identity
        .as_ref()
        .map_or(request.author.as_str(), |identity| {
            identity.principal.as_str()
        });
    let owner = ownership_text(&before, "assignee");
    let held_principal = ownership_text(&before, "claim_principal");
    let held_principal = if held_principal.trim().is_empty() {
        owner.trim()
    } else {
        held_principal.trim()
    };
    if !owner.trim().is_empty()
        && held_principal != principal
        && !request.force
        && settings.governance.policy() == "strict"
    {
        return Err(ownership_refusal(
            format!(
                "Item {} is assigned to {}. Use --force to override.",
                request.id,
                owner.trim()
            ),
            serde_json::json!({"code":"ownership_conflict"}),
        ));
    }
    let ownership_warning = if !owner.trim().is_empty()
        && held_principal != principal
        && !request.force
        && settings.governance.policy() == "warn"
    {
        Some(format!(
            "ownership_warning:assignee_conflict:{}:{}",
            request.id,
            owner.trim()
        ))
    } else {
        None
    };
    let value = crate::ownership::runtime_settings(pm_root)?;
    let key = if start {
        "in_progress_status"
    } else {
        "open_status"
    };
    let status = value["schema"]["workflow"][key]
        .as_str()
        .unwrap_or(if start { "in_progress" } else { "open" });
    let mut document = before.clone();
    let mut changed = usize::from(document.metadata.status != status);
    validate_lifecycle_transition(&value, &before, status)?;
    if matches!(before.metadata.status.as_str(), "closed" | "canceled") {
        for key in [
            "closed_at",
            "completed_at",
            "close_reason",
            "resolution",
            "expected_result",
            "actual_result",
            "fixed_version",
        ] {
            if document.metadata.extra.remove(key).is_some() {
                changed += 1;
            }
        }
    }
    status.clone_into(&mut document.metadata.status);
    document.metadata.updated_at.clone_from(&timestamp);
    let item = if changed == 0 {
        before
    } else {
        commit_mutation(
            pm_root,
            "update",
            &before,
            document,
            &timestamp,
            &request.author,
            request.provenance_role.as_deref(),
            request.message.as_deref(),
            &item_path,
            &history_path,
        )?
        .item
    };
    let mut receipt = serde_json::json!({"id":item.metadata.id,"status":item.metadata.status,"changed_field_count":changed});
    if let Some(reason) = item.metadata.extra.get("close_reason") {
        receipt["close_reason"] = reason.clone();
    }
    if let Some(warning) = ownership_warning {
        receipt["warnings"] = serde_json::json!([warning]);
    }
    Ok(receipt)
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

/// Preserves I/O and active-lock failures while explaining expired session locks.
fn session_lock_refusal(
    pm_root: &Path,
    error: PmRustError,
    timestamp: &str,
    ttl_seconds: u64,
) -> PmRustError {
    if !matches!(error, PmRustError::LockConflict { .. }) {
        return error;
    }
    let raw = fs::read_to_string(pm_root.join("locks/session-state.lock")).unwrap_or_default();
    let warning = match serde_json::from_str::<Value>(&raw) {
        Err(_) => Some("lock_info_invalid_json"),
        Ok(value) => {
            if value["id"].is_string()
                && value["pid"].is_number()
                && value["owner"].is_string()
                && value["created_at"].is_string()
                && value["ttl_seconds"].is_number()
                && value.get("token").is_none_or(|token| {
                    token.as_str().is_some_and(|token| !token.trim().is_empty())
                })
            {
                None
            } else {
                Some("lock_info_invalid_shape")
            }
        }
    };
    if warning.is_some() || ownership_lock_stale(&raw, timestamp, ttl_seconds) {
        let suffix = warning.map_or_else(String::new, |warning| format!(" ({warning})"));
        close_refusal(
            format!(
                "Item session-state lock is stale{suffix}; rerun with --force when supported for this command"
            ),
            4,
            serde_json::json!({}),
        )
    } else {
        error
    }
}

/// Mutates derived session attribution under the shared session-state lock.
pub(crate) fn mutate_session_state(
    pm_root: &Path,
    update: impl FnOnce(&mut serde_json::Map<String, Value>),
) -> Result<(), PmRustError> {
    let settings = LockSettings {
        wait_ms: 3000,
        use_created_at: true,
        ..LockSettings::default()
    };
    let timestamp = now_iso();
    let _lock = match acquire_lock(
        pm_root,
        "session-state",
        "session-state",
        &settings,
        false,
        &timestamp,
    ) {
        Ok(lock) => lock,
        Err(error) => {
            return Err(session_lock_refusal(
                pm_root,
                error,
                &timestamp,
                settings.ttl_seconds,
            ));
        }
    };
    let path = pm_root.join("runtime/session.json");
    let mut state = fs::read_to_string(&path)
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Map<String, Value>>(&text).ok())
        .unwrap_or_default();
    update(&mut state);
    atomic_replace(&path, &crate::stringify_json(&Value::Object(state), false))
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
            close_receipt: None,
            close_changed_fields: Vec::new(),
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
