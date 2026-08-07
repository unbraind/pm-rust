//! Rust-native readers for canonical `pm` workspaces.
//!
//! The crate deliberately exposes read-only operations in its first release
//! slice. Mutation APIs will not be added until locking, transaction, history,
//! recovery, and merge behavior pass differential conformance tests.

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
