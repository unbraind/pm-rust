//! Resource-admitted, immutable native read sessions with pull backpressure.

use crate::{ItemFilter, ListOptions, ListOutput, PmRustError, Workspace};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    fmt::Write as _,
    fs,
    io::Read,
    path::{Component, Path},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

/// Caller-controlled admission ceilings; independent of Node and Git.
#[derive(Clone, Debug)]
pub struct ReadSessionLimits {
    /// Accounted native working-set ceiling in bytes, including rendering reserve.
    pub memory_bytes: u64,
    /// Maximum rows per pull, before the existing output budget compacts them.
    pub page_items: usize,
    /// Existing renderer token ceiling for each page.
    pub output_tokens: usize,
    /// Select the existing TOON rather than JSON budgeting contract.
    pub toon: bool,
}

impl Default for ReadSessionLimits {
    /// Chooses a 512 MiB admission ceiling and agent-sized pages.
    fn default() -> Self {
        Self {
            memory_bytes: 512 * 1024 * 1024,
            page_items: 20,
            output_tokens: 4000,
            toon: false,
        }
    }
}

/// Thread-safe cooperative cancellation; no worker threads or outstanding tasks.
#[derive(Clone, Debug, Default)]
pub struct SessionCancellation(Arc<AtomicBool>);

impl SessionCancellation {
    /// Requests cancellation; the next copy checkpoint or pull releases storage.
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }

    /// Refuses work after cancellation without resetting the shared token.
    fn check(&self) -> Result<(), SessionRefusal> {
        if self.0.load(Ordering::Acquire) {
            Err(SessionRefusal::Cancelled)
        } else {
            Ok(())
        }
    }
}

/// Stable embedding refusals, each with explicit caller recovery guidance.
#[derive(Debug, thiserror::Error)]
pub enum SessionRefusal {
    /// Caller cancelled before the next admission or delivery boundary.
    #[error("read session cancelled; open a new session with a fresh cancellation token")]
    Cancelled,
    /// A required working set exceeds caller admission limits.
    #[error("read session memory ceiling exceeded; raise memory_bytes or select a smaller tracker")]
    MemoryLimit,
    /// Caller supplied a zero or unrepresentable resource control.
    #[error(
        "invalid session limits; use positive memory_bytes, page_items and output_tokens (at most 1000000)"
    )]
    InvalidLimits,
    /// A source cannot be safely frozen under this bounded contract.
    #[error(
        "unsupported session source; use regular files, at most 32 directory levels, and tracker-relative schema files"
    )]
    UnsupportedSource,
    /// The canonical reader rejected source or output data.
    #[error(
        "read session source refused: {0}; repair tracker integrity or increase output_tokens and reopen"
    )]
    Source(#[from] PmRustError),
    /// Snapshot creation, reading, or cleanup failed.
    #[error("read session storage failed: {0}; check scratch storage permissions and free space")]
    Storage(#[from] std::io::Error),
}

impl SessionRefusal {
    /// Stable machine code independent of human-readable operating-system errors.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Cancelled => "session_cancelled",
            Self::MemoryLimit => "session_memory_limit",
            Self::InvalidLimits => "session_invalid_limits",
            Self::UnsupportedSource => "session_unsupported_source",
            Self::Source(_) => "session_source_refused",
            Self::Storage(_) => "session_storage_failed",
        }
    }
}

/// Certification and resource evidence for one immutable session snapshot.
#[derive(Clone, Debug, Serialize)]
pub struct SessionReceipt {
    /// Versioned embedding contract, independent of oracle versioning.
    pub protocol: &'static str,
    /// Digest of ordered relative filenames and exact admitted bytes.
    pub snapshot_sha256: String,
    /// Fully decoded, unique source items before filtering.
    pub source_items: usize,
    /// Total frozen file bytes; snapshot disk use is bounded by this value.
    pub source_bytes: u64,
    /// Accounted peak reserve, not a measurement of process RSS.
    pub admitted_memory_bytes: u64,
    /// Caller-declared admission ceiling.
    pub memory_ceiling_bytes: u64,
    /// Pages successfully delivered so far.
    pub pages: usize,
    /// True only after the final page has been delivered.
    pub complete: bool,
}

/// One oracle-compatible page and its separate session certificate.
#[derive(Debug, Serialize)]
pub struct SessionPage {
    /// Canonical list envelope, retaining lossless JSON overrides.
    pub output: ListOutput,
    /// Session evidence; projected field omissions remain in the list envelope.
    pub receipt: SessionReceipt,
}

/// A pull-based, disk-backed read session. Drop removes private snapshot storage.
///
/// Retaining returned pages is the caller's responsibility. Cancellation is
/// cooperative between file copies and page queries; it does not preempt decoding.
#[derive(Debug)]
pub struct ReadSession {
    /// Private snapshot ownership; taken on completion, cancellation or failure.
    snapshot: Option<tempfile::TempDir>,
    /// Reader using private storage and the original query identity.
    workspace: Workspace,
    /// Existing canonical query controls; the session owns continuation.
    options: ListOptions,
    /// Exact snapshot evidence and aggregate delivery state.
    receipt: SessionReceipt,
    /// Shared cancellation token.
    cancellation: SessionCancellation,
    /// Stable read clock selected once by the caller.
    timestamp: String,
}

impl Workspace {
    /// Freezes and strictly certifies all items before returning a native session.
    ///
    /// # Errors
    /// Returns typed admission, cancellation, storage or source refusals. No Git
    /// subprocess, extensions, telemetry or writes to the source tracker occur.
    pub fn read_session(
        &self,
        limits: &ReadSessionLimits,
        cancellation: SessionCancellation,
        timestamp: &str,
    ) -> Result<ReadSession, SessionRefusal> {
        cancellation.check()?;
        if limits.memory_bytes == 0
            || limits.page_items == 0
            || limits.output_tokens == 0
            || limits.page_items > 1_000_000
            || limits.output_tokens > 1_000_000
        {
            return Err(SessionRefusal::InvalidLimits);
        }
        let reserved = 8 * 1024 * 1024 + limits.output_tokens as u64 * 256;
        let mut admission = Admission {
            ceiling: limits.memory_bytes,
            used: reserved,
            bytes: 0,
            items: 0,
            hash: Sha256::new(),
            ids: BTreeSet::new(),
            cancellation: &cancellation,
        };
        admission.charge(0)?;
        let snapshot = tempfile::tempdir()?;
        let target = snapshot.path().join(".agents/pm");
        fs::create_dir_all(&target)?;
        admission.copy(
            &self.pm_root().join("settings.json"),
            &target.join("settings.json"),
            Path::new("settings.json"),
            false,
        )?;
        let settings: serde_json::Value =
            serde_json::from_slice(&fs::read(target.join("settings.json"))?)
                .map_err(|_| SessionRefusal::UnsupportedSource)?;
        for name in ["statuses", "workflows"] {
            let fallback = format!("schema/{name}.json");
            let relative = Path::new(
                settings["schema"]["files"][name]
                    .as_str()
                    .unwrap_or(&fallback),
            );
            if relative
                .components()
                .any(|part| !matches!(part, Component::Normal(_)))
            {
                return Err(SessionRefusal::UnsupportedSource);
            }
            let mut schema_source = self.pm_root().to_path_buf();
            for component in relative.components() {
                schema_source.push(component);
                if schema_source.is_symlink() {
                    return Err(SessionRefusal::UnsupportedSource);
                }
            }
            if schema_source.exists() {
                admission.copy(&schema_source, &target.join(relative), relative, false)?;
            }
        }
        admission.walk(self.pm_root(), &target, Path::new(""), 0)?;
        let mut snapshot_sha256 = String::with_capacity(64);
        for byte in admission.hash.finalize() {
            let _ = write!(snapshot_sha256, "{byte:02x}");
        }
        let receipt = SessionReceipt {
            protocol: "pm-rust/read-session/v1",
            snapshot_sha256,
            source_items: admission.items,
            source_bytes: admission.bytes,
            admitted_memory_bytes: admission.used,
            memory_ceiling_bytes: limits.memory_bytes,
            pages: 0,
            complete: false,
        };
        let workspace = self.with_read_snapshot(target);
        Ok(ReadSession {
            snapshot: Some(snapshot),
            workspace,
            options: ListOptions {
                all: true,
                brief: true,
                limit: Some(limits.page_items.to_string()),
                output_budget: Some(limits.output_tokens.to_string()),
                toon: limits.toon,
                ..ListOptions::default()
            },
            receipt,
            cancellation,
            timestamp: timestamp.to_owned(),
        })
    }
}

impl ReadSession {
    /// Reads the current resource receipt without consuming or materializing a page.
    #[must_use]
    pub fn receipt(&self) -> &SessionReceipt {
        &self.receipt
    }

    /// Cancels this session synchronously and releases its private snapshot.
    ///
    /// # Errors
    /// Returns storage cleanup failures; the session remains terminal.
    pub fn cancel(&mut self) -> Result<(), SessionRefusal> {
        self.cancellation.cancel();
        self.cleanup()
    }

    /// Removes snapshot storage exactly once, including on terminal failures.
    fn cleanup(&mut self) -> Result<(), SessionRefusal> {
        if let Some(snapshot) = self.snapshot.take() {
            snapshot.close()?;
        }
        Ok(())
    }

    /// Pulls one bounded canonical list page; `None` means successful exhaustion.
    ///
    /// # Errors
    /// Cancellation, source and output errors terminate the session and release
    /// resources. A caller must drop each page to keep caller-retained memory bounded.
    pub fn next_page(&mut self) -> Result<Option<SessionPage>, SessionRefusal> {
        if let Err(error) = self.cancellation.check() {
            self.cleanup()?;
            return Err(error);
        }
        if self.snapshot.is_none() {
            return Ok(None);
        }
        let result = self
            .workspace
            .list_page(&ItemFilter::default(), &self.options, &self.timestamp)
            .map_err(SessionRefusal::from);
        let output = match result {
            Ok(output) => output,
            Err(error) => {
                self.cleanup()?;
                return Err(error);
            }
        };
        #[cfg(test)]
        tests::AFTER_QUERY_HOOK.with(|hook| hook.get()(&self.cancellation));
        if let Err(error) = self.cancellation.check() {
            self.cleanup()?;
            return Err(error);
        }
        self.options.after = output["next_cursor"].as_str().map(str::to_owned);
        self.options.output_cursor = None;
        if output["continuation_kind"] == "output_cursor" {
            self.options.output_cursor = self.options.after.take();
        }
        self.receipt.pages += 1;
        self.receipt.complete =
            self.options.after.is_none() && self.options.output_cursor.is_none();
        if self.receipt.complete {
            self.cleanup()?;
        }
        Ok(Some(SessionPage {
            output,
            receipt: self.receipt.clone(),
        }))
    }
}

/// Conservative admission ledger for private source storage and existing readers.
struct Admission<'a> {
    /// Caller ceiling.
    ceiling: u64,
    /// Accounted reserve.
    used: u64,
    /// Exact source bytes.
    bytes: u64,
    /// Validated item count.
    items: usize,
    /// Ordered snapshot digest.
    hash: Sha256,
    /// Strict global identity uniqueness.
    ids: BTreeSet<String>,
    /// Cooperative cancellation boundary.
    cancellation: &'a SessionCancellation,
}

impl Admission<'_> {
    /// Reserves before allocating file bytes or adding index entries.
    fn charge(&mut self, bytes: u64) -> Result<(), SessionRefusal> {
        self.used = self.used.saturating_add(bytes);
        if self.used > self.ceiling {
            Err(SessionRefusal::MemoryLimit)
        } else {
            Ok(())
        }
    }

    /// Copies one bounded regular file, validating items with the canonical decoder.
    fn copy(
        &mut self,
        source: &Path,
        target: &Path,
        relative: &Path,
        item: bool,
    ) -> Result<(), SessionRefusal> {
        self.cancellation.check()?;
        let metadata = fs::symlink_metadata(source)?;
        if !metadata.is_file() {
            return Err(SessionRefusal::UnsupportedSource);
        }
        let size = metadata.len();
        self.charge(
            size.saturating_mul(128)
                .saturating_add(8192 + relative.as_os_str().len() as u64 * 16),
        )?;
        #[cfg(test)]
        tests::BEFORE_COPY_HOOK.with(|hook| hook.get()(source, self.cancellation));
        let mut bytes = Vec::new();
        fs::File::open(source)?
            .take(size.saturating_add(1))
            .read_to_end(&mut bytes)?;
        if bytes.len() as u64 != size {
            return Err(SessionRefusal::UnsupportedSource);
        }
        self.cancellation.check()?;
        if item {
            let text =
                std::str::from_utf8(&bytes).map_err(|_| SessionRefusal::UnsupportedSource)?;
            let document = crate::item::decode_item(source, text)?;
            if !self.ids.insert(document.metadata.id.clone()) {
                return Err(SessionRefusal::Source(PmRustError::DuplicateItemId {
                    id: document.metadata.id,
                    first: source.to_path_buf(),
                    second: source.to_path_buf(),
                }));
            }
            self.items += 1;
        }
        fs::create_dir_all(target.parent().ok_or(SessionRefusal::UnsupportedSource)?)?;
        fs::write(target, &bytes)?;
        self.hash.update(relative.to_string_lossy().as_bytes());
        self.hash.update([0]);
        self.hash.update(size.to_le_bytes());
        self.hash.update(&bytes);
        self.bytes += size;
        Ok(())
    }

    /// Visits deterministic source entries without retaining the corpus contents.
    fn walk(
        &mut self,
        source: &Path,
        target: &Path,
        relative: &Path,
        depth: usize,
    ) -> Result<(), SessionRefusal> {
        self.cancellation.check()?;
        if depth > 32 {
            return Err(SessionRefusal::UnsupportedSource);
        }
        let mut entries = Vec::new();
        for entry in fs::read_dir(source)? {
            self.cancellation.check()?;
            self.charge(1024)?;
            entries.push(entry?);
        }
        entries.sort_by_key(std::fs::DirEntry::file_name);
        for entry in entries {
            let name = entry.file_name();
            let kind = entry.file_type()?;
            if depth == 0
                && crate::workspace::NON_ITEM_DIRECTORIES.contains(&name.to_string_lossy().as_ref())
            {
                continue;
            }
            if kind.is_symlink() {
                return Err(SessionRefusal::UnsupportedSource);
            }
            let relative = relative.join(&name);
            if kind.is_dir() {
                self.walk(&entry.path(), &target.join(&name), &relative, depth + 1)?;
            } else if depth > 0 && entry.path().extension().is_some_and(|ext| ext == "toon") {
                self.copy(&entry.path(), &target.join(name), &relative, true)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "../tests/support/read_session_unit.rs"]
mod tests;
