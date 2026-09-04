//! Canonical pm history construction shared by every native mutation.
//!
//! The published pm 2026.9.4 release stores one JSON line per mutation in
//! `.agents/pm/history/<id>.jsonl`. Every record carries the canonical
//! recursively key-sorted document hashes, a JSON-patch diff computed over
//! canonically ordered metadata, the `item_hash_version` epoch marker, and a
//! record-integrity envelope covering attribution and event classification.
//! This module reproduces those bytes exactly without invoking another
//! runtime.

use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::item::{ItemDocument, ItemMetadata};

/// Canonical metadata key order used by the published storage contract.
///
/// Known keys are stored and diffed in this order; keys outside the list are
/// appended after it in lexicographic order, mirroring the published
/// `orderObject` behavior for unknown fields.
pub(crate) const CANONICAL_METADATA_KEY_ORDER: [&str; 72] = [
    "id",
    "title",
    "description",
    "type",
    "pm_format_version",
    "source_type",
    "type_options",
    "status",
    "priority",
    "tags",
    "created_at",
    "updated_at",
    "deadline",
    "reminders",
    "events",
    "closed_at",
    "completed_at",
    "assignee",
    "claim_principal",
    "source_owner",
    "author",
    "estimated_minutes",
    "acceptance_criteria",
    "design",
    "external_ref",
    "definition_of_ready",
    "order",
    "goal",
    "objective",
    "value",
    "impact",
    "outcome",
    "why_now",
    "parent",
    "reviewer",
    "risk",
    "confidence",
    "sprint",
    "release",
    "blocked_by",
    "blocked_reason",
    "unblock_note",
    "reporter",
    "severity",
    "environment",
    "repro_steps",
    "resolution",
    "expected_result",
    "actual_result",
    "affected_version",
    "fixed_version",
    "component",
    "regression",
    "customer_impact",
    "dependencies",
    "comments",
    "notes",
    "learnings",
    "files",
    "tests",
    "test_runs",
    "docs",
    "close_reason",
    "duplicate_of",
    "plan_mode",
    "plan_scope",
    "plan_harness",
    "plan_resume_context",
    "plan_validation",
    "plan_decisions",
    "plan_discoveries",
    "plan_steps",
];

/// Hash of the canonical empty document that opens every create-history chain.
/// Epoch marker stamped on every history record this implementation writes.
///
/// The published CLI moved from `2` to `3` in its 2026.8.24 release. The hash
/// bytes this implementation computes are already byte-identical to the
/// published CLI's under that epoch — a differential run over the same input
/// sequence agrees on `before_hash` and `after_hash` and differed on this field
/// alone. Stamping `2` while writing v3 hashes is the exact ambiguity
/// unbraind/pm-cli#1135 describes: two writers claiming one epoch while their
/// bytes belong to different ones, which leaves a consumer unable to tell from
/// the record which computation produced it.
pub(crate) const ITEM_HASH_VERSION: u8 = 3;

pub(crate) const EMPTY_DOCUMENT_HASH: &str =
    "3cc22dff72be7b14824654a7a64ea62b04799939b2fee54c1b5f52ca60bf6df0";

/// One JSON-patch operation inside a history record.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub(crate) struct HistoryPatch {
    /// Patch operation kind: `add`, `replace`, or `remove`.
    pub op: &'static str,
    /// JSON pointer to the mutated document location.
    pub path: String,
    /// Post-mutation value, absent for `remove` operations.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<Value>,
}

/// Agent provenance recorded alongside argv-derived mutation roles.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub(crate) struct AgentProvenance<'a> {
    /// Role inferred from the invoked command name.
    pub role: ProvenanceRole<'a>,
}

/// One argv-derived provenance value with its detection source.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub(crate) struct ProvenanceRole<'a> {
    /// Canonical role value, for example `implementer`.
    pub value: &'a str,
    /// Always `argv`: the only supported detection source in this slice.
    pub source: &'static str,
}

/// One complete history record ready for JSON serialization.
#[derive(Serialize)]
pub(crate) struct HistoryEntry<'a> {
    /// Mutation timestamp in canonical UTC RFC 3339 form.
    pub ts: &'a str,
    /// Asserted mutation author.
    pub author: &'a str,
    /// Author resolution source; `asserted` for every explicit native author.
    pub author_source: &'static str,
    /// Argv-derived provenance, absent when the command maps to no role.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_provenance: Option<AgentProvenance<'a>>,
    /// Canonical mutation operation name.
    pub op: &'static str,
    /// Canonical JSON-patch diff between the before and after documents.
    pub patch: Vec<HistoryPatch>,
    /// Hash of the document before the mutation.
    pub before_hash: String,
    /// Hash of the document after the mutation.
    pub after_hash: String,
    /// Hash-epoch marker written by every current published release.
    pub item_hash_version: u8,
    /// Optional free-form history message.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<&'a str>,
    /// Whether the event advances working context or records maintenance.
    pub event_class: &'static str,
    /// Version of the canonical immutable-record integrity contract.
    pub record_hash_version: u8,
    /// SHA-256 of all recursively sorted record fields except this digest.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub record_hash: String,
}

/// Classifies the published operation and patch contract without hiding unknown work.
pub(crate) fn classify_history_event(operation: &str, patch: &[HistoryPatch]) -> &'static str {
    /// Published operations whose semantics are always maintenance.
    const MAINTENANCE_OPERATIONS: &[&str] = &[
        "docs_add",
        "files_add",
        "history:author-acknowledge",
        "history_author_acknowledge",
        "history_compact",
        "history_compact_baseline",
        "history_redact",
        "history_repair",
        "normalize",
        "release",
        "test_run_track",
        "tests_add",
        "tests_remove",
        "update_audit",
        "update_ownership_bypass",
    ];
    /// Top-level update fields classified as bookkeeping by policy version one.
    const MAINTENANCE_FIELDS: &[&str] = &[
        "actual_result",
        "affected_version",
        "assignee",
        "blocked_by",
        "blocked_reason",
        "close_reason",
        "confidence",
        "customer_impact",
        "definition_of_ready",
        "dependencies",
        "docs",
        "environment",
        "expected_result",
        "files",
        "fixed_version",
        "goal",
        "impact",
        "learnings",
        "notes",
        "objective",
        "order",
        "outcome",
        "parent",
        "release",
        "reminders",
        "reporter",
        "repro_steps",
        "resolution",
        "reviewer",
        "severity",
        "sprint",
        "tests",
        "test_runs",
        "unblock_note",
        "updated_at",
        "value",
        "why_now",
    ];
    if MAINTENANCE_OPERATIONS.contains(&operation) {
        return "maintenance";
    }
    if operation == "update"
        && !patch.is_empty()
        && patch.iter().all(|change| {
            let mut segments = change.path.split('/').filter(|part| !part.is_empty());
            matches!(segments.next(), Some("metadata" | "front_matter"))
                && segments
                    .next()
                    .is_some_and(|field| MAINTENANCE_FIELDS.contains(&field))
        })
    {
        "maintenance"
    } else {
        "substantive"
    }
}

/// An ordered metadata-plus-body view used for diffs, hashes, and encoding.
#[derive(Clone, Debug)]
pub(crate) struct OrderedDocument {
    /// Metadata entries in canonical storage order.
    pub metadata: Vec<(String, Value)>,
    /// Long-form Markdown body.
    pub body: String,
}

impl OrderedDocument {
    /// Projects one decoded item into its canonical ordered representation.
    #[must_use]
    pub fn from_document(document: &ItemDocument) -> Self {
        Self {
            metadata: canonical_metadata_pairs(&document.metadata),
            body: document.body.clone(),
        }
    }

    /// Returns the value stored at one metadata key.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.metadata
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value)
    }

    /// Returns whether one metadata key is present.
    #[must_use]
    pub fn contains(&self, key: &str) -> bool {
        self.metadata.iter().any(|(name, _)| name == key)
    }
}

/// Returns the canonical position rank of one metadata key, if it is known.
fn canonical_rank(key: &str) -> Option<usize> {
    CANONICAL_METADATA_KEY_ORDER
        .iter()
        .position(|candidate| *candidate == key)
}

/// Projects item metadata into the canonical ordered key-value sequence.
#[must_use]
pub fn canonical_metadata_pairs(metadata: &ItemMetadata) -> Vec<(String, Value)> {
    let mut values: Vec<(String, Value)> = vec![
        ("id".to_owned(), Value::String(metadata.id.clone())),
        ("title".to_owned(), Value::String(metadata.title.clone())),
        (
            "description".to_owned(),
            Value::String(metadata.description.clone()),
        ),
        ("type".to_owned(), Value::String(metadata.item_type.clone())),
        ("status".to_owned(), Value::String(metadata.status.clone())),
        ("priority".to_owned(), Value::from(metadata.priority)),
        (
            "tags".to_owned(),
            Value::Array(metadata.tags.iter().cloned().map(Value::String).collect()),
        ),
        (
            "created_at".to_owned(),
            Value::String(metadata.created_at.clone()),
        ),
        (
            "updated_at".to_owned(),
            Value::String(metadata.updated_at.clone()),
        ),
    ];
    if let Some(parent) = &metadata.parent {
        values.push(("parent".to_owned(), Value::String(parent.clone())));
    }
    for (key, value) in &metadata.extra {
        values.push((key.clone(), value.clone()));
    }
    // A stable sort over `Some(rank)` places every known key at its canonical
    // position while unknown keys (ranked last) keep their retained order.
    values.sort_by_key(|(key, _)| {
        let rank = canonical_rank(key).map_or(usize::MAX, |value| value);
        (rank, key.clone())
    });
    values
}

/// Computes the canonical JSON-patch diff between two ordered documents.
///
/// The published runtime diffs canonically ordered documents by walking the
/// before-document keys in reverse for `replace` and `remove` operations and
/// then the after-document keys in order for `add` operations, comparing the
/// body before the metadata map at the document root. This reproduces that
/// exact operation ordering.
#[must_use]
pub fn history_patch(before: &OrderedDocument, after: &OrderedDocument) -> Vec<HistoryPatch> {
    let mut patch = Vec::new();
    if before.body != after.body {
        patch.push(HistoryPatch {
            op: "replace",
            path: "/body".to_owned(),
            value: Some(Value::String(after.body.clone())),
        });
    }
    for (key, value) in before.metadata.iter().rev() {
        match after.get(key) {
            None => patch.push(HistoryPatch {
                op: "remove",
                path: format!("/metadata/{key}"),
                value: None,
            }),
            Some(updated) if updated != value => patch.push(HistoryPatch {
                op: "replace",
                path: format!("/metadata/{key}"),
                value: Some(updated.clone()),
            }),
            Some(_) => {}
        }
    }
    for (key, value) in &after.metadata {
        if !before.contains(key) {
            patch.push(HistoryPatch {
                op: "add",
                path: format!("/metadata/{key}"),
                value: Some(value.clone()),
            });
        }
    }
    patch
}

/// Lowercase hexadecimal digits, indexed by the nibble they encode.
const HEX_DIGITS: [u8; 16] = *b"0123456789abcdef";

/// Appends recursively key-sorted compact JSON for stable hashing.
fn stable_json(value: &Value, output: &mut String) {
    match value {
        Value::Array(entries) => {
            output.push('[');
            for (index, entry) in entries.iter().enumerate() {
                if index > 0 {
                    output.push(',');
                }
                stable_json(entry, output);
            }
            output.push(']');
        }
        Value::Object(entries) => {
            output.push('{');
            let mut keys: Vec<&String> = entries.keys().collect();
            keys.sort();
            for (index, key) in keys.into_iter().enumerate() {
                if index > 0 {
                    output.push(',');
                }
                let encoded = serde_json::to_string(key).unwrap_or_default();
                output.push_str(&encoded);
                output.push(':');
                stable_json(&entries[key], output);
            }
            output.push('}');
        }
        Value::Number(number) => output.push_str(&javascript_number(number)),
        scalar => {
            let encoded = serde_json::to_string(scalar).unwrap_or_default();
            output.push_str(&encoded);
        }
    }
}

/// Renders one JSON number the way `JSON.stringify` renders it.
///
/// The hash must match the published JavaScript CLI byte for byte, and the two
/// runtimes disagree about numbers that a TOON document decodes as `f64`.
/// `serde_json` writes `30.0` and `1e21`; `JSON.stringify` writes `30` and
/// `1e+21`. Either difference changes `before_hash` and `after_hash` for any
/// item carrying a float in its metadata, which reads downstream as a corrupt
/// history chain rather than as a formatting disagreement.
///
/// Integers are left to `serde_json`, which already agrees. Floats are rendered
/// by JavaScript's own two rules: a finite value whose fractional part is zero
/// and whose magnitude is below `1e21` prints without a decimal point, and an
/// exponent always carries an explicit sign.
///
/// @param number - The decoded JSON number.
/// @returns The number formatted as `JSON.stringify` would format it.
pub(crate) fn javascript_number(number: &serde_json::Number) -> String {
    let Some(float) = number
        .as_f64()
        .filter(|_| number.as_i64().is_none() && number.as_u64().is_none())
    else {
        return number.to_string();
    };
    if float.fract() == 0.0 && float.abs() < 1e21 {
        return format!("{float:.0}");
    }
    // Exponent form needs no adjustment: `serde_json` already writes the sign
    // that `JSON.stringify` writes, `1e+21` and `1e-7` alike. Only the
    // whole-valued case above actually diverges, and a branch here for the
    // unsigned exponent `serde_json` never emits would be unreachable code
    // dressed as a safeguard.
    number.to_string()
}

/// Computes the canonical SHA-256 document hash used by pm history.
///
/// The digest is hex-encoded a nibble at a time rather than through `{:x}` on
/// the digest value itself, which keeps the encoding identical across `sha2`
/// releases. The output feeds every history record, so a change in encoding
/// would invalidate history already on disk.
#[must_use]
pub fn document_hash(document: &OrderedDocument) -> String {
    let front_matter = Value::Object(metadata_object(document));
    let canonical = serde_json::json!({"front_matter": front_matter, "body": document.body});
    let mut raw = String::new();
    stable_json(&canonical, &mut raw);
    sha256_digest(raw.as_bytes())
}

/// Builds one insertion-ordered metadata object for hashing inputs.
fn metadata_object(document: &OrderedDocument) -> serde_json::Map<String, Value> {
    // Hashing re-sorts keys recursively, so insertion order is irrelevant here;
    // a plain object preserves every value exactly.
    document
        .metadata
        .iter()
        .cloned()
        .collect::<serde_json::Map<String, Value>>()
}

/// Computes the SHA-256 digest of one byte slice as lowercase hexadecimal.
#[must_use]
pub fn sha256_digest(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut hex = String::with_capacity(digest.len() * 2);
    for byte in digest {
        hex.push(char::from(HEX_DIGITS[usize::from(byte >> 4)]));
        hex.push(char::from(HEX_DIGITS[usize::from(byte & 0x0f)]));
    }
    hex
}

/// Assembles one complete history record for a mutation.
#[allow(clippy::too_many_arguments)]
pub(crate) fn history_entry<'a>(
    ts: &'a str,
    author: &'a str,
    op: &'static str,
    provenance_role: Option<&'a str>,
    patch: Vec<HistoryPatch>,
    before_hash: String,
    after_hash: String,
    message: Option<&'a str>,
) -> HistoryEntry<'a> {
    let event_class = classify_history_event(op, &patch);
    let mut entry = HistoryEntry {
        ts,
        author,
        author_source: if author == "unknown" {
            "unknown"
        } else {
            "asserted"
        },
        agent_provenance: provenance_role.map(|role| AgentProvenance {
            role: ProvenanceRole {
                value: role,
                source: "argv",
            },
        }),
        op,
        patch,
        before_hash,
        after_hash,
        item_hash_version: ITEM_HASH_VERSION,
        message,
        event_class,
        record_hash_version: 1,
        record_hash: String::new(),
    };
    // Every field is a JSON-compatible scalar, collection, or Value; the
    // temporary empty digest is omitted from the canonical hash input.
    let canonical = serde_json::to_value(&entry).unwrap_or_default();
    let mut serialized = String::new();
    stable_json(&canonical, &mut serialized);
    entry.record_hash = sha256_digest(serialized.as_bytes());
    entry
}

/// Serializes one history record into its final JSON line including newline.
///
/// The published stream is written by `JSON.stringify`, whose output this
/// reproduces field-for-field because the record structure declares the exact
/// key order.
#[must_use]
pub fn history_line(entry: &HistoryEntry) -> String {
    let line = serde_json::to_string(entry).unwrap_or_default();
    format!("{line}\n")
}

/// Builds one TOON-serializable document with canonically ordered metadata.
///
/// The returned JSON object preserves insertion order (`serde_json` runs with
/// `preserve_order`), so the encoder emits fields in canonical storage order
/// with the body serialized last exactly like the published dialect.
#[must_use]
pub fn canonical_document_value(document: &ItemDocument) -> Value {
    let mut object = serde_json::Map::new();
    for (key, value) in canonical_metadata_pairs(&document.metadata) {
        object.insert(key, value);
    }
    object.insert("body".to_owned(), Value::String(document.body.clone()));
    Value::Object(object)
}

#[cfg(test)]
#[path = "../tests/support/history_unit.rs"]
mod tests;
