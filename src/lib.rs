//! Rust-native readers and mutation writers for canonical `pm` workspaces.
//!
//! The crate exposes deterministic read operations plus explicit-ID create,
//! field-update, comment-append, and close transactions, each backed by
//! per-item locking with a wait budget, durable journaling, recovery, and
//! canonical `item_hash_version: 2` history compatible with the published
//! `pm` 2026.8.21 release. Merge operations remain gated on differential
//! conformance evidence.

mod error;
mod history;
mod item;
mod mutation;
mod workspace;

pub use error::PmRustError;
pub use history::canonical_metadata_pairs;
pub use item::{ItemDocument, ItemMetadata, ItemSummary};
pub use mutation::{CloseItem, CommentItem, CreateItem, CreateResult, MutationResult, UpdateItem};
pub use workspace::{ItemFilter, ListResult, Workspace};

/// Published canonical `pm` release used by this compatibility slice.
pub const COMPATIBLE_PM_VERSION: &str = "2026.8.21";
