//! Canonical item-document decoding and stable read projections.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use toon_format::decode_strict;

use crate::PmRustError;

/// Core pm metadata plus losslessly retained extension fields.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ItemMetadata {
    /// Stable item identifier.
    pub id: String,
    /// Human-readable item title.
    pub title: String,
    /// Human-readable item description.
    #[serde(default)]
    pub description: String,
    /// Runtime-configurable item type.
    #[serde(rename = "type")]
    pub item_type: String,
    /// Runtime-configurable lifecycle state.
    pub status: String,
    /// Priority from zero, most urgent, through four, least urgent.
    pub priority: u8,
    /// Canonical normalized tags.
    #[serde(default)]
    pub tags: Vec<String>,
    /// ISO-8601 creation timestamp.
    pub created_at: String,
    /// ISO-8601 last-update timestamp.
    pub updated_at: String,
    /// Optional parent item identifier.
    #[serde(default)]
    pub parent: Option<String>,
    /// Forward-compatible fields contributed by core or installed packages.
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

/// Fully decoded canonical item document.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ItemDocument {
    /// Item metadata stored at the TOON document root.
    #[serde(flatten)]
    pub metadata: ItemMetadata,
    /// Optional long-form Markdown body.
    #[serde(default)]
    pub body: String,
}

/// Token-efficient stable projection used by list operations.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ItemSummary {
    /// Stable item identifier.
    pub id: String,
    /// Runtime-configurable lifecycle state.
    pub status: String,
    /// Runtime-configurable item type.
    #[serde(rename = "type")]
    pub item_type: String,
    /// Human-readable item title.
    pub title: String,
    /// Item priority.
    pub priority: u8,
    /// Optional parent item identifier.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
}

impl From<&ItemDocument> for ItemSummary {
    fn from(document: &ItemDocument) -> Self {
        Self {
            id: document.metadata.id.clone(),
            status: document.metadata.status.clone(),
            item_type: document.metadata.item_type.clone(),
            title: document.metadata.title.clone(),
            priority: document.metadata.priority,
            parent: document.metadata.parent.clone(),
        }
    }
}

pub(crate) fn decode_item(path: &Path, content: &str) -> Result<ItemDocument, PmRustError> {
    if content.lines().any(|line| {
        ["<<<<<<<", "=======", ">>>>>>>"].iter().any(|marker| {
            line.strip_prefix(marker)
                .is_some_and(|suffix| suffix.is_empty() || suffix.starts_with(' '))
        })
    }) {
        return Err(PmRustError::InvalidItemDocument {
            path: path.to_path_buf(),
            reason: "merge conflict markers detected".to_owned(),
        });
    }

    let normalized = normalize_javascript_toon_dialect(content);
    let document = decode_strict::<ItemDocument>(&normalized).map_err(|error| {
        PmRustError::InvalidItemDocument {
            path: path.to_path_buf(),
            reason: format!("TOON decode failed: {error}"),
        }
    })?;

    validate_required(path, "id", &document.metadata.id)?;
    validate_required(path, "title", &document.metadata.title)?;
    validate_required(path, "type", &document.metadata.item_type)?;
    validate_required(path, "status", &document.metadata.status)?;
    validate_required(path, "created_at", &document.metadata.created_at)?;
    validate_required(path, "updated_at", &document.metadata.updated_at)?;
    if document.metadata.priority > 4 {
        return Err(PmRustError::InvalidItemDocument {
            path: path.to_path_buf(),
            reason: "priority must be between 0 and 4".to_owned(),
        });
    }
    Ok(document)
}

fn validate_required(path: &Path, field: &str, value: &str) -> Result<(), PmRustError> {
    if value.trim().is_empty() {
        return Err(PmRustError::InvalidItemDocument {
            path: path.to_path_buf(),
            reason: format!("required field {field} is empty"),
        });
    }
    Ok(())
}

fn normalize_javascript_toon_dialect(content: &str) -> String {
    let mut normalized = String::with_capacity(content.len());
    for line in content.lines() {
        if let Some(prefix) = line.strip_suffix(": []") {
            normalized.push_str(prefix);
            normalized.push_str("[0]:");
        } else {
            normalized.push_str(line);
        }
        normalized.push('\n');
    }
    normalized
}
