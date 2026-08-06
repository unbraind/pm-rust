//! Typed failure contracts for workspace discovery and item reads.

use std::io;
use std::path::PathBuf;

/// Failures returned by the Rust-native read surface.
#[derive(Debug, thiserror::Error)]
pub enum PmRustError {
    /// A filesystem operation failed at a known path.
    #[error("filesystem operation failed at {path}: {source}")]
    Io {
        /// Path involved in the failed operation.
        path: PathBuf,
        /// Original operating-system error.
        source: io::Error,
    },
    /// No `.agents/pm/settings.json` marker exists at or above the start path.
    #[error("no pm tracker found from {start}")]
    TrackerNotFound {
        /// Discovery path supplied by the caller.
        start: PathBuf,
    },
    /// A TOON item could not be decoded or violated required core metadata.
    #[error("invalid pm item document at {path}: {reason}")]
    InvalidItemDocument {
        /// Item path that failed validation.
        path: PathBuf,
        /// Stable human-readable validation reason.
        reason: String,
    },
    /// Two stored item documents declare the same stable identifier.
    #[error("duplicate pm item id {id} in {first} and {second}")]
    DuplicateItemId {
        /// Colliding stable item identifier.
        id: String,
        /// First path carrying the identifier.
        first: PathBuf,
        /// Second path carrying the identifier.
        second: PathBuf,
    },
    /// No stored item matches an exact identifier.
    #[error("pm item not found: {id}")]
    ItemNotFound {
        /// Requested stable item identifier.
        id: String,
    },
}
