//! Read-only, bounded planning for the native list reader, without executing it.
use crate::{
    Workspace,
    read_projection::{Projection, project_text},
};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{self, Read},
    path::Path,
};

/// Admitted list decoder projection; selection and lifecycle semantics stay in list.
#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum QueryProjection {
    /// Native default list: identity, sorting and selection scalars, without bodies.
    Brief,
    /// Native full list: decode complete items including bodies, return metadata.
    Full,
}

/// Strict admission schema. No filters, cursors, status interpretation or writes.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct QueryCostRequest {
    /// Which existing native list projection to explain.
    pub projection: QueryProjection,
    /// Maximum visited directory entries, including ignored entries; 1..=1000000.
    pub max_entries: u32,
    /// Maximum total item source bytes inspected; 1..=1073741824 (one GiB).
    pub max_bytes: u64,
}

impl Default for QueryCostRequest {
    /// Bounds ordinary planning to 100000 directory entries and 64 MiB of source.
    fn default() -> Self {
        Self {
            projection: QueryProjection::Brief,
            max_entries: 100_000,
            max_bytes: 67_108_864,
        }
    }
}

/// Conditional work range for unchanged source files, never an execution promise.
#[derive(Clone, Debug, Serialize)]
pub struct WorkBound {
    /// Zero: a later read may refuse before performing this work.
    pub lower: u64,
    /// Upper limit for the observed corpus; not an allocator or wire-size estimate.
    pub upper: u64,
}

/// Compact agent plan. Counts are observations; work is bounded or unknown.
#[derive(Clone, Debug, Serialize)]
pub struct QueryCost {
    /// Versioned native planning contract.
    pub protocol: &'static str,
    /// Successful planning does not certify item validity or execute the query.
    pub ok: bool,
    /// Supported execution command.
    pub read: &'static str,
    /// Admitted decoder projection.
    pub projection: QueryProjection,
    /// Delivered metadata fields; None means the full metadata projection.
    pub fields: Option<Vec<&'static str>>,
    /// Observed regular item files, not necessarily valid or unique items.
    pub observed_candidate_files: u64,
    /// Candidate rows that execution may need before selection and sorting.
    pub candidate_rows: WorkBound,
    /// Item-file bytes a later list scan may read; excludes settings and registries.
    pub item_read_bytes: WorkBound,
    /// Sum of shared projection input lengths, not heap size or delivered bytes.
    pub decoder_input_bytes: WorkBound,
    /// Bytes actually inspected by this explanation, disclosed separately.
    pub explain_read_bytes: u64,
    /// Settings, schema and other auxiliary I/O are not estimated.
    pub auxiliary_read_bytes: Option<u64>,
    /// Output size cannot be established without query execution and rendering.
    pub output_bytes: Option<u64>,
    /// Allocator overhead and platform behavior are not modeled.
    pub heap_bytes: Option<u64>,
    /// Time depends on filesystem, hardware, contention and decoding.
    pub elapsed_ms: Option<u64>,
    /// Explicit consistency and certification limits.
    pub condition: &'static str,
    /// Available bounded delivery alternatives, with no source-scan savings claimed.
    pub alternatives: [&'static str; 2],
}

/// Typed refusals always include a concrete remedy; no partial plan is returned.
#[derive(Debug, thiserror::Error)]
pub enum QueryCostRefusal {
    /// Invalid numeric controls or a request outside the strict schema.
    #[error("invalid query-cost request; use max_entries 1..1000000 and max_bytes 1..1073741824")]
    InvalidRequest,
    /// Directory work, item bytes or traversal depth exceed admitted ceilings.
    #[error(
        "query-cost scan limit exceeded; raise max_entries/max_bytes within their domains, or use a smaller, shallower tracker (depth at most 32)"
    )]
    ScanLimit,
    /// Source I/O or UTF-8 decoding failed.
    #[error(
        "query-cost source unavailable; use readable regular UTF-8 item files and retry against a stable tracker"
    )]
    Source,
}

impl QueryCostRefusal {
    /// Stable code for machine-readable recovery.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::InvalidRequest => "query_cost_invalid_request",
            Self::ScanLimit => "query_cost_scan_limit",
            Self::Source => "query_cost_source",
        }
    }
}

impl Workspace {
    /// Explains default `list` or `list --full` without writes or query execution.
    ///
    /// Reads at most `max_bytes + 1` source bytes (one detection byte). Directory
    /// traversal is streaming, bounded by `max_entries` and depth 32. Symlinks and
    /// non-item entries are skipped like the native reader. Does not load status
    /// registries, normalize items, create snapshots, recover journals or invoke Git.
    ///
    /// # Errors
    /// Returns a typed refusal for invalid controls, exceeded ceilings or bad I/O.
    pub fn explain_list(&self, request: &QueryCostRequest) -> Result<QueryCost, QueryCostRefusal> {
        if !(1..=1_000_000).contains(&request.max_entries)
            || !(1..=1_073_741_824).contains(&request.max_bytes)
        {
            return Err(QueryCostRefusal::InvalidRequest);
        }
        let mut scan = Scan {
            request,
            entries: 0,
            files: 0,
            bytes: 0,
            projected: 0,
        };
        scan.walk(self.pm_root(), 0).map_err(|error| match error {
            ScanError::Limit => QueryCostRefusal::ScanLimit,
            ScanError::Source => QueryCostRefusal::Source,
        })?;
        // All counters are <= one GiB or one million entries. Projection retains
        // bytes plus at most one newline per source line, hence <= two GiB. Every
        // emitted integer is consequently finite and below the JS safe range.
        Ok(QueryCost {
            protocol: "pm-rust/query-cost/v1",
            ok: true,
            read: "list",
            projection: request.projection,
            fields: match request.projection {
                QueryProjection::Brief => Some(crate::list::brief_fields(None)),
                QueryProjection::Full => None,
            },
            observed_candidate_files: scan.files,
            candidate_rows: WorkBound {
                lower: 0,
                upper: scan.files,
            },
            item_read_bytes: WorkBound {
                lower: 0,
                upper: scan.bytes,
            },
            decoder_input_bytes: WorkBound {
                lower: 0,
                upper: scan.projected,
            },
            explain_read_bytes: scan.bytes,
            auxiliary_read_bytes: None,
            output_bytes: None,
            heap_bytes: None,
            elapsed_ms: None,
            condition: "Observed item files only; auxiliary I/O unknown. Bounds require unchanged sources. Live scan is not a snapshot, validity check or reservation. Full decoding includes bodies; full list returns metadata. Output limits do not bound source scanning.",
            alternatives: [
                "pm-rust list --brief --output-limit 20 --output-budget 4000",
                "pm-rust read-session --page-items 20 --output-tokens 4000 --memory-bytes 536870912",
            ],
        })
    }
}

/// Internal scanner failure, without retaining arbitrary source data in diagnostics.
enum ScanError {
    Limit,
    Source,
}

impl From<io::Error> for ScanError {
    /// Preserves failure without leaking local filenames through the public receipt.
    fn from(_: io::Error) -> Self {
        Self::Source
    }
}

/// Streaming, non-mutating inventory; never retains the corpus or item documents.
struct Scan<'a> {
    /// Once-validated admission controls.
    request: &'a QueryCostRequest,
    /// Entries visited, including non-item and ignored entries.
    entries: u32,
    /// Candidate item files.
    files: u64,
    /// Item bytes read.
    bytes: u64,
    /// Exact decoder input lengths produced by the existing projector.
    projected: u64,
}

impl Scan<'_> {
    /// Visits only native item directories; no symlink traversal or Git discovery.
    fn walk(&mut self, path: &Path, depth: u32) -> Result<(), ScanError> {
        if depth > 32 {
            return Err(ScanError::Limit);
        }
        let mut entries = fs::read_dir(path)?;
        self.walk_entries(&mut entries, depth)
    }

    /// Preserves directory iteration failures without returning a partial plan.
    fn walk_entries(
        &mut self,
        entries: &mut dyn Iterator<Item = io::Result<fs::DirEntry>>,
        depth: u32,
    ) -> Result<(), ScanError> {
        for entry in entries {
            let entry = entry?;
            self.entries += 1;
            if self.entries > self.request.max_entries {
                return Err(ScanError::Limit);
            }
            self.visit_entry(&entry, entry.file_type(), depth)?;
        }
        Ok(())
    }

    /// Uses the authoritative entry type, retaining metadata failures as refusals.
    fn visit_entry(
        &mut self,
        entry: &fs::DirEntry,
        kind: io::Result<fs::FileType>,
        depth: u32,
    ) -> Result<(), ScanError> {
        let kind = kind?;
        let path = entry.path();
        if kind.is_dir() {
            if depth == 0
                && crate::workspace::NON_ITEM_DIRECTORIES
                    .contains(&entry.file_name().to_string_lossy().as_ref())
            {
                return Ok(());
            }
            self.walk(&path, depth + 1)?;
        } else if depth > 0 && kind.is_file() && path.extension().is_some_and(|ext| ext == "toon") {
            self.inspect(&path)?;
        }
        Ok(())
    }

    /// Reads one bounded source and asks the shared projector for its decoder input.
    fn inspect(&mut self, path: &Path) -> Result<(), ScanError> {
        self.inspect_reader(&mut fs::File::open(path)?)
    }

    /// Bounds source reads and preserves failures from a caller-owned file handle.
    fn inspect_reader(&mut self, reader: &mut dyn Read) -> Result<(), ScanError> {
        let mut bytes = Vec::new();
        reader
            .take(self.request.max_bytes - self.bytes + 1)
            .read_to_end(&mut bytes)?;
        self.bytes += bytes.len() as u64;
        if self.bytes > self.request.max_bytes {
            return Err(ScanError::Limit);
        }
        let text = std::str::from_utf8(&bytes).map_err(|_| ScanError::Source)?;
        let projection = match self.request.projection {
            QueryProjection::Brief => Projection::Summary,
            QueryProjection::Full => Projection::Full,
        };
        self.projected += project_text(text, projection).len() as u64;
        self.files += 1;
        Ok(())
    }
}

#[cfg(test)]
#[path = "../tests/support/query_cost_unit.rs"]
mod tests;
