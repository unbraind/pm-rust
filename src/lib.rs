//! Rust-native readers for canonical `pm` workspaces.
//!
//! The crate exposes deterministic read operations and an explicit-ID create
//! operation backed by locking, durable journaling, recovery, and canonical
//! history. Broader mutation and merge operations remain gated on differential
//! conformance evidence.

mod error;
mod item;
mod mutation;
mod workspace;

pub use error::PmRustError;
pub use item::{ItemDocument, ItemMetadata, ItemSummary};
pub use mutation::{CreateItem, CreateResult};
pub use workspace::{ItemFilter, ListResult, Workspace};

/// Published canonical `pm` release used by this compatibility slice.
pub const COMPATIBLE_PM_VERSION: &str = "2026.8.6";
