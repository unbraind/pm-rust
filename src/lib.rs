//! Rust-native readers and mutation writers for canonical `pm` workspaces.
//!
//! The crate exposes deterministic read operations plus explicit-ID create,
//! field-update, comment-append, and close transactions, each backed by
//! per-item locking with a wait budget, durable journaling, recovery, and
//! canonical `item_hash_version: 3` history compatible with the published
//! `pm` 2026.10.7 release. Merge operations remain gated on differential
//! conformance evidence.

mod error;
mod history;
mod item;
mod json_output;
mod list;
mod list_intent;
mod mutation;
mod pagination;
mod read_output;
mod workspace;

pub use error::PmRustError;
pub use history::canonical_metadata_pairs;
pub use item::{ItemDocument, ItemMetadata, ItemSummary};
pub use json_output::{stringify_json, write_pretty_json};
pub use list::ListOptions;
pub use mutation::now_iso as current_timestamp;
pub use mutation::{
    CloseItem, CommentItem, CreateItem, CreateResult, MutationResult, UpdateItem, default_priority,
    default_status, validate_timestamp,
};
pub use workspace::{ItemFilter, ListResult, Workspace};

/// Published canonical `pm` release used by this compatibility slice.
pub const COMPATIBLE_PM_VERSION: &str = "2026.10.7";
