//! Pull-based native mutation transport; canonical events, caller-owned cursors.

use crate::native_adapter::{
    AdapterCapability, AdapterManifest, AdapterOperation, AdapterOutcome, AdapterReply,
    AdapterRequest, NATIVE_ADAPTER_ABI, NATIVE_ADAPTER_MAX_BYTES, NativeAdapter,
    NativeAdapterClient,
};
use crate::{SessionCancellation, Workspace};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    io::{self, Read},
};

/// Maximum source bytes inspected per pull, including prefix revalidation.
const SCAN_BYTES: u64 = 67_108_864;
/// Maximum directory entries inspected per pull, including foreign entries.
const STREAMS: usize = 1024;
/// Versioned transport/cursor domain; not an event schema.
const PROTOCOL: &str = "pm-rust/changes/1";

/// One strict request schema shared by the SDK, CLI and adapter ABI.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ChangeRequest {
    /// Caller-owned opaque cursor; absent starts at retained history's beginning.
    pub cursor: Option<String>,
    /// Maximum admitted events plus skipped records, from 1 through 100.
    pub limit: u16,
    /// Complete JSON AND TOON page ceiling, from 1024 through 1048576 bytes.
    pub output_bytes: u32,
}

impl Default for ChangeRequest {
    fn default() -> Self {
        Self {
            cursor: None,
            limit: 20,
            output_bytes: 65_536,
        }
    }
}

impl ChangeRequest {
    /// Checks caller controls before any source or transport access.
    ///
    /// # Errors
    /// Refuses out-of-domain limits and oversized or empty cursor strings.
    pub fn validate(&self) -> Result<(), ChangeRefusal> {
        if !(1..=100).contains(&self.limit)
            || !(1024..=1_048_576).contains(&self.output_bytes)
            || self
                .cursor
                .as_ref()
                .is_some_and(|v| v.is_empty() || v.len() > 524_288)
        {
            return Err(ChangeRefusal::InvalidRequest);
        }
        Ok(())
    }
}

/// Canonical compact `MutationEvent` projection from the public pm SDK.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MutationEvent {
    /// Item identity or the canonical `_workspace` stream.
    pub item_id: String,
    /// One-based physical nonblank stream ordinal, independent of timestamps.
    pub version: u64,
    /// Unmodified canonical mutation timestamp.
    pub ts: String,
    /// Unmodified attributed author.
    pub author: String,
    /// Canonical mutation operation.
    #[serde(rename = "type")]
    pub operation: String,
    /// Optional authoritative history message.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    /// Number of patch operations, without transporting patch contents.
    pub patch_count: u64,
}

/// A single foreign or malformed record skipped without losing its neighbors.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SkippedChange {
    /// Physical stream identity.
    pub item_id: String,
    /// Nonblank record ordinal.
    pub version: u64,
    /// Stable explanation; source bytes are never echoed.
    pub reason: String,
}

/// One admitted page; its cursor resumes after every returned or skipped record.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ChangePage {
    /// Native transport identity, independent of core event schema versioning.
    pub protocol: String,
    /// Canonical compact mutation facts, in stream/physical order.
    pub events: Vec<MutationEvent>,
    /// Invalid or foreign records, counted against the same admission limit.
    pub skipped: Vec<SkippedChange>,
    /// Whether a complete unadmitted record was observed.
    pub has_more: bool,
    /// Opaque continuation, including for empty polls.
    pub next_cursor: String,
}

impl ChangePage {
    /// Renders UTF-8 typed fields through the canonical codec; no raw string overrides.
    #[must_use]
    pub fn stringify_toon(&self) -> String {
        // The typed JSON data model cannot produce a codec serialization error.
        toon_format::encode_default(self).unwrap_or_default()
    }

    /// Uses one producer/consumer validator, including both encoded byte ceilings.
    fn validate(&self, request: &ChangeRequest) -> Result<(), ChangeRefusal> {
        let total = self.events.len().saturating_add(self.skipped.len());
        if self.protocol != PROTOCOL
            || total > usize::from(request.limit)
            || self.next_cursor.is_empty()
            || self.next_cursor.len() > 524_288
            || self.events.iter().any(|event| {
                event.version == 0
                    || event.version > 9_007_199_254_740_991
                    || event.patch_count > 9_007_199_254_740_991
            })
            || self
                .skipped
                .iter()
                .any(|row| row.version == 0 || row.version > 9_007_199_254_740_991)
            || (total > 0 && request.cursor.as_ref() == Some(&self.next_cursor))
        {
            return Err(ChangeRefusal::InvalidPage);
        }
        let encoded_bytes = crate::stringify_json(self, false)
            .len()
            .max(self.stringify_toon().len())
            + 1;
        if encoded_bytes > request.output_bytes as usize {
            return Err(ChangeRefusal::OutputLimit);
        }
        Ok(())
    }
}

/// Typed recoverable transport refusals; failures never advance a reader cursor.
#[derive(Debug, thiserror::Error)]
pub enum ChangeRefusal {
    /// Invalid boundary input.
    #[error(
        "invalid change request; use limit 1..100, output_bytes 1024..1048576 and a cursor returned by this adapter"
    )]
    InvalidRequest,
    /// Cursor belongs to another tracker/adapter or a changed retained prefix.
    #[error("stale change cursor; reorient from a full tracker read, then start a new cursor")]
    StaleCursor,
    /// Caller cancelled cooperatively before admission.
    #[error("change pull cancelled; resume the previous cursor with a fresh cancellation token")]
    Cancelled,
    /// Fixed source work ceiling exceeded.
    #[error(
        "change scan limit exceeded; use an adapter with indexed history (at most 1024 directory entries and 64 MiB per native pull)"
    )]
    ScanLimit,
    /// A whole record plus cursor cannot fit without loss.
    #[error(
        "change output limit exceeded; increase output_bytes or reduce limit; the previous cursor remains valid"
    )]
    OutputLimit,
    /// Transport page does not meet the shared producer contract.
    #[error(
        "invalid change page; repair the adapter's canonical event/page producer and resume the previous cursor"
    )]
    InvalidPage,
    /// The source is unavailable, unsafe or not a regular local history source.
    #[error(
        "change source unavailable; use a readable local tracker with regular history files and retry the previous cursor"
    )]
    Source,
    /// Existing byte ABI admission/transport refusal, retained intact.
    #[error("{0}")]
    Adapter(#[from] crate::native_adapter::AdapterRefusal),
    /// Authoritative remote backend refusal, retained intact.
    #[error("backend refused changes: {code}: {message}; {remedy}")]
    Backend {
        /// Stable backend code.
        code: String,
        /// Unmodified authoritative explanation.
        message: String,
        /// Authoritative recovery instruction.
        remedy: String,
        /// Unmodified backend refusal details.
        details: Box<serde_json::Value>,
    },
}

impl ChangeRefusal {
    /// Stable machine code, retaining the existing backend/ABI classifications.
    #[must_use]
    pub fn code(&self) -> &str {
        match self {
            Self::InvalidRequest => "changes_invalid_request",
            Self::StaleCursor => "changes_stale_cursor",
            Self::Cancelled => "changes_cancelled",
            Self::ScanLimit => "changes_scan_limit",
            Self::OutputLimit => "changes_output_limit",
            Self::InvalidPage => "changes_invalid_page",
            Self::Source => "changes_source",
            Self::Adapter(_) => "changes_adapter_refused",
            Self::Backend { code, .. } => code,
        }
    }
}

/// Immutable prefix checkpoint; validation uses the exact stored bytes, not time.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Prefix {
    /// Byte boundary immediately after the last admitted newline.
    bytes: usize,
    /// Nonblank record ordinal, including skipped records.
    version: u64,
    /// Exact prefix digest, detecting pruning, compaction and same-size rewrites.
    hash: String,
}

/// Caller-owned checkpoint; no shared state or checkpoint writes to synchronize.
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Cursor {
    /// Native cursor domain.
    protocol: String,
    /// Tracker identity selected by the adapter, never by event input.
    tracker: String,
    /// Last admitted per-stream positions; missing streams begin at zero.
    prefixes: BTreeMap<String, Prefix>,
}

/// Opens without following replacement symlinks or blocking on a replacement FIFO.
#[cfg(unix)]
fn open_history(path: &std::path::Path) -> io::Result<fs::File> {
    use rustix::fs::{Mode, OFlags, open};
    open(
        path,
        OFlags::RDONLY | OFlags::CLOEXEC | OFlags::NOFOLLOW | OFlags::NONBLOCK,
        Mode::empty(),
    )
    .map(fs::File::from)
    .map_err(io::Error::from)
}

/// Opens the selected file itself, retaining reparse-point type information.
#[cfg(windows)]
fn open_history(path: &std::path::Path) -> io::Result<fs::File> {
    use std::os::windows::fs::OpenOptionsExt as _;
    fs::OpenOptions::new()
        .read(true)
        .custom_flags(0x0020_0000)
        .open(path)
}

/// Admits only a regular opened handle and captures its snapshot length.
fn open_regular_history(path: &std::path::Path) -> Result<(fs::File, u64), ChangeRefusal> {
    let file = open_history(path).map_err(source_error)?;
    let metadata = file.metadata();
    #[cfg(test)]
    let metadata = tests::inject_io(metadata, 1);
    let metadata = metadata.map_err(source_error)?;
    regular_history_length(&metadata).map(|length| (file, length))
}

/// Checks the opened object's kind before admitting its byte length.
fn regular_history_length(metadata: &fs::Metadata) -> Result<u64, ChangeRefusal> {
    if !metadata.is_file() {
        return Err(ChangeRefusal::Source);
    }
    Ok(metadata.len())
}

/// Classifies I/O failures without exposing arbitrary source paths or bytes.
fn source_error(_: io::Error) -> ChangeRefusal {
    ChangeRefusal::Source
}

/// Checks cooperative cancellation at every bounded processing boundary.
fn cancelled(token: &SessionCancellation) -> Result<(), ChangeRefusal> {
    token.check().map_err(|_| ChangeRefusal::Cancelled)
}

/// Read-only native history adapter. No Node, Git, shared checkpoint or mutations.
pub struct LocalChangeAdapter {
    /// Explicit backend selected by the embedding application.
    workspace: Workspace,
    /// Shared cooperative cancellation handle.
    cancellation: SessionCancellation,
}

impl LocalChangeAdapter {
    /// Selects an authorized local tracker and caller cancellation token.
    #[must_use]
    pub const fn new(workspace: Workspace, cancellation: SessionCancellation) -> Self {
        Self {
            workspace,
            cancellation,
        }
    }

    /// Pulls one bounded page through the same producer as the byte ABI.
    ///
    /// # Errors
    /// Returns typed admission, stale-prefix, cancellation and source refusals.
    pub fn pull(&self, request: &ChangeRequest) -> Result<ChangePage, ChangeRefusal> {
        request.validate()?;
        cancelled(&self.cancellation)?;
        let tracker =
            crate::history::sha256_digest(self.workspace.pm_root().as_os_str().as_encoded_bytes());
        let mut cursor = match &request.cursor {
            Some(raw) => {
                let bytes = URL_SAFE_NO_PAD
                    .decode(raw)
                    .map_err(|_| ChangeRefusal::StaleCursor)?;
                let cursor: Cursor = crate::strict_json::from_slice(&bytes)
                    .map_err(|_| ChangeRefusal::StaleCursor)?;
                if cursor.protocol != PROTOCOL
                    || cursor.tracker != tracker
                    || cursor.prefixes.len() > STREAMS
                {
                    return Err(ChangeRefusal::StaleCursor);
                }
                cursor
            }
            None => Cursor {
                protocol: PROTOCOL.into(),
                tracker,
                prefixes: BTreeMap::new(),
            },
        };
        let history = self.workspace.pm_root().join("history");
        if history.is_symlink() {
            return Err(ChangeRefusal::Source);
        }
        let mut files = BTreeMap::new();
        for (index, entry) in fs::read_dir(history).map_err(source_error)?.enumerate() {
            #[cfg(test)]
            tests::boundary(1, self);
            cancelled(&self.cancellation)?;
            if index >= STREAMS {
                return Err(ChangeRefusal::ScanLimit);
            }
            #[cfg(test)]
            let entry = tests::inject_io(entry, 2);
            let entry = entry.map_err(source_error)?;
            let path = entry.path();
            let kind = entry.file_type();
            #[cfg(test)]
            let kind = tests::inject_io(kind, 3);
            if kind.map_err(source_error)?.is_file()
                && path
                    .extension()
                    .is_some_and(|extension| extension == "jsonl")
            {
                let id = path
                    .file_stem()
                    .and_then(|id| id.to_str())
                    .ok_or(ChangeRefusal::Source)?
                    .to_owned();
                files.insert(id, path);
            }
        }
        if cursor.prefixes.keys().any(|id| !files.contains_key(id)) {
            return Err(ChangeRefusal::StaleCursor);
        }
        let mut page = ChangePage {
            protocol: PROTOCOL.into(),
            events: Vec::new(),
            skipped: Vec::new(),
            has_more: false,
            next_cursor: String::new(),
        };
        let mut remaining = SCAN_BYTES;
        for (id, path) in files {
            #[cfg(test)]
            tests::boundary(2, self);
            cancelled(&self.cancellation)?;
            let (file, length) = open_regular_history(&path)?;
            let mut raw = Vec::new();
            let read = file.take(length.min(remaining + 1)).read_to_end(&mut raw);
            #[cfg(test)]
            let read = tests::inject_io(read, 4);
            read.map_err(source_error)?;
            if raw.len() as u64 > remaining {
                return Err(ChangeRefusal::ScanLimit);
            }
            remaining -= raw.len() as u64;
            #[cfg(test)]
            tests::boundary(3, self);
            admit_stream(
                &id,
                &raw,
                &mut cursor,
                &mut page,
                request.limit,
                &self.cancellation,
            )?;
        }
        #[cfg(test)]
        tests::boundary(4, self);
        cancelled(&self.cancellation)?;
        page.next_cursor = URL_SAFE_NO_PAD.encode(crate::stringify_json(&cursor, false));
        page.validate(request)?;
        Ok(page)
    }
}

/// Validates the retained prefix, then admits only complete records after it.
fn admit_stream(
    id: &str,
    raw: &[u8],
    cursor: &mut Cursor,
    page: &mut ChangePage,
    limit: u16,
    token: &SessionCancellation,
) -> Result<(), ChangeRefusal> {
    let previous = cursor.prefixes.get(id);
    let mut bytes = previous.map_or(0, |p| p.bytes);
    let mut version = previous.map_or(0, |p| p.version);
    if let Some(prefix) = previous
        && (bytes > raw.len()
            || bytes == 0
            || raw[bytes - 1] != b'\n'
            || crate::history::sha256_digest(&raw[..bytes]) != prefix.hash
            || raw[..bytes]
                .split_inclusive(|byte| *byte == b'\n')
                .filter(|line| !blank_line(line))
                .count() as u64
                != version)
    {
        return Err(ChangeRefusal::StaleCursor);
    }
    for line in raw[bytes..].split_inclusive(|byte| *byte == b'\n') {
        cancelled(token)?;
        if line.last() != Some(&b'\n') {
            break;
        }
        if blank_line(line) {
            bytes += line.len();
            continue;
        }
        if page.events.len() + page.skipped.len() >= usize::from(limit) {
            page.has_more = true;
            break;
        }
        version += 1;
        match project_event(id, version, line) {
            Some(event) => page.events.push(event),
            None => page.skipped.push(SkippedChange {
                item_id: id.into(),
                version,
                reason: "foreign_or_invalid_history_record".into(),
            }),
        }
        bytes += line.len();
    }
    if bytes > 0 {
        cursor.prefixes.insert(
            id.into(),
            Prefix {
                bytes,
                version,
                hash: crate::history::sha256_digest(&raw[..bytes]),
            },
        );
    }
    Ok(())
}

/// Uses the canonical JavaScript whitespace domain without changing record fields.
fn record_text(line: &[u8]) -> Option<&str> {
    std::str::from_utf8(line).ok().map(|text| {
        text.trim_matches(|c: char| c == '\u{feff}' || c != '\u{0085}' && c.is_whitespace())
    })
}

/// Shared blank-line decision for ordinal production and cursor validation.
fn blank_line(line: &[u8]) -> bool {
    record_text(line).is_some_and(str::is_empty)
}

/// Projects precisely the canonical compact event fields; unknown fields survive at source.
fn project_event(id: &str, version: u64, line: &[u8]) -> Option<MutationEvent> {
    let entry: serde_json::Value = crate::strict_json::from_str(record_text(line)?).ok()?;
    Some(MutationEvent {
        item_id: id.into(),
        version,
        ts: entry.get("ts")?.as_str()?.into(),
        author: entry.get("author")?.as_str()?.into(),
        operation: entry.get("op")?.as_str()?.into(),
        message: match entry.get("message") {
            None => None,
            Some(value) => Some(value.as_str()?.into()),
        },
        patch_count: entry.get("patch")?.as_array()?.len() as u64,
    })
}

impl NativeAdapter for LocalChangeAdapter {
    fn manifest(&self) -> AdapterManifest {
        AdapterManifest {
            abi: NATIVE_ADAPTER_ABI.into(),
            capabilities: vec![AdapterCapability::Changes],
            max_frame_bytes: NATIVE_ADAPTER_MAX_BYTES,
        }
    }
    fn exchange(&mut self, bytes: &[u8], response_limit: usize) -> io::Result<Vec<u8>> {
        let request: AdapterRequest = crate::strict_json::from_slice(bytes)?;
        let AdapterOperation::Changes { request: changes } = request.operation else {
            return Err(io::Error::other("unsupported read-only capability"));
        };
        let outcome = match self.pull(&changes) {
            Ok(page) => AdapterOutcome::Success {
                output: serde_json::json!(page),
            },
            Err(error) => AdapterOutcome::Refused {
                code: error.code().into(),
                message: error.to_string(),
                remedy: error.to_string(),
                details: serde_json::Value::Null,
            },
        };
        let mut reply = AdapterReply {
            abi: request.abi,
            request_id: request.request_id,
            outcome,
        };
        let bytes = crate::stringify_json(&reply, false).into_bytes();
        let maximum = response_limit.min(NATIVE_ADAPTER_MAX_BYTES);
        if bytes.len() <= maximum {
            return Ok(bytes);
        }
        reply.outcome=AdapterOutcome::Refused {
            code:"changes_output_limit".into(), message:"change frame exceeds transport ceiling".into(),
            remedy:"reduce the page limit or raise the adapter response ceiling; retain the previous cursor".into(), details:serde_json::Value::Null,
        };
        let bytes = crate::stringify_json(&reply, false).into_bytes();
        if bytes.len() > maximum {
            return Err(io::Error::other("change refusal exceeds transport ceiling"));
        }
        Ok(bytes)
    }
}

/// Pull consumer over any authorized adapter declaring the changes capability.
///
/// One synchronous exchange per pull, no prefetch, retries, workers or polling loop.
/// Embedders must bound transport timeouts; cancellation does not preempt exchange.
/// Persist the returned cursor only after consuming the entire admitted page.
pub struct ChangeReader<A> {
    /// Existing ABI client retains correlation, frame bounds and typed refusals.
    client: NativeAdapterClient<A>,
    /// Latest successfully admitted cursor and stable page limits.
    request: ChangeRequest,
    /// Cooperative caller cancellation.
    cancellation: SessionCancellation,
}

impl<A: NativeAdapter> ChangeReader<A> {
    /// Admits controls and adapter before any dispatch.
    ///
    /// # Errors
    /// Refuses invalid limits or incompatible/unsupported adapter declarations.
    pub fn new(
        adapter: A,
        request: ChangeRequest,
        cancellation: SessionCancellation,
    ) -> Result<Self, ChangeRefusal> {
        request.validate()?;
        let client = NativeAdapterClient::new(adapter)?;
        crate::native_adapter::prepare_adapter_request(
            &client.manifest,
            &AdapterRequest {
                abi: NATIVE_ADAPTER_ABI.into(),
                request_id: "changes".into(),
                operation: AdapterOperation::Changes {
                    request: request.clone(),
                },
            },
        )?;
        Ok(Self {
            client,
            request,
            cancellation,
        })
    }

    /// Last admitted checkpoint; unchanged on any refusal or cancellation.
    #[must_use]
    pub fn cursor(&self) -> Option<&str> {
        self.request.cursor.as_deref()
    }

    /// Pulls one page and advances only after complete validation and cancellation checks.
    ///
    /// # Errors
    /// Preserves ABI/backend refusals and refuses malformed or oversized pages.
    pub fn pull(&mut self) -> Result<ChangePage, ChangeRefusal> {
        cancelled(&self.cancellation)?;
        let reply = self.client.call(&AdapterRequest {
            abi: NATIVE_ADAPTER_ABI.into(),
            request_id: "changes".into(),
            operation: AdapterOperation::Changes {
                request: self.request.clone(),
            },
        })?;
        let page: ChangePage = match reply.outcome {
            AdapterOutcome::Success { output } => {
                serde_json::from_value(output).map_err(|_| ChangeRefusal::InvalidPage)?
            }
            AdapterOutcome::Refused {
                code,
                message,
                remedy,
                details,
            } => {
                return Err(ChangeRefusal::Backend {
                    code,
                    message,
                    remedy,
                    details: Box::new(details),
                });
            }
        };
        page.validate(&self.request)?;
        cancelled(&self.cancellation)?;
        self.request.cursor = Some(page.next_cursor.clone());
        Ok(page)
    }
}

#[cfg(test)]
#[path = "../tests/support/changes_unit.rs"]
mod tests;
