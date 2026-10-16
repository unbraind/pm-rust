//! Rust-native readers and mutation writers for canonical `pm` workspaces.
//!
//! The crate exposes deterministic read operations plus explicit-ID create,
//! field-update, comment-append, and close transactions, each backed by
//! per-item locking with a wait budget, durable journaling, recovery, and
//! canonical `item_hash_version: 3` history compatible with the published
//! `pm` 2026.10.14 release. Merge operations remain gated on differential
//! conformance evidence.

mod collation;
mod error;
mod history;
mod item;
mod json_output;
mod list;
mod list_intent;
mod list_registry;
mod mutation;
mod ownership;
mod pagination;
mod read_output;
mod selection;
mod toon_output;
mod workspace;

pub use error::PmRustError;
pub use history::canonical_metadata_pairs;
pub use item::{ItemDocument, ItemMetadata, ItemSummary};
pub use json_output::{ListOutput, stringify_json, write_pretty_json};
pub use list::ListOptions;
pub use mutation::{
    CloseFields, CloseItem, CommentItem, CreateItem, CreateResult, MutationResult, OwnershipItem,
    OwnershipResult, UpdateItem, default_priority, default_status, validate_timestamp,
};
pub use mutation::{now_iso as current_timestamp, quote_command_arg};
pub use ownership::{
    AuthorIdentity, OwnershipOptions, SelectionOptions, partial_failure as completed_step_failure,
};
pub use toon_output::stringify_toon;
pub use workspace::{ItemFilter, ListResult, Workspace};

/// Published canonical `pm` release used by this compatibility slice.
pub const COMPATIBLE_PM_VERSION: &str = "2026.10.14";
