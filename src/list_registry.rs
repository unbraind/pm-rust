//! Configured lifecycle classification for native list selection.

use std::{collections::BTreeMap, fs, path::Path};

use serde_json::{Value, json};

use crate::PmRustError;

/// Lifecycle definitions and workflow selected for one complete list read.
pub(crate) struct Lifecycle {
    /// Normalized status definitions, keyed by canonical identity.
    definitions: BTreeMap<String, Value>,
    /// Alias tokens mapped to canonical status identities.
    aliases: BTreeMap<String, String>,
    /// Configured in-progress status used by lifecycle buckets.
    progress: String,
    /// Configured fallback active status when no definition carries an active role.
    open: String,
    /// Hierarchy definitions captured by the same settings read as lifecycle roles.
    pub(crate) hierarchy: Hierarchy,
}

impl Lifecycle {
    /// Loads embedded statuses, then optional schema-file additions and overrides.
    pub(crate) fn load(root: &Path) -> Result<Self, PmRustError> {
        let settings = crate::ownership::read_json(&root.join("settings.json"))?;
        let mut schema = settings["schema"].clone();
        if !schema.is_object() {
            schema = json!({});
        }
        let mut statuses = schema["statuses"].as_array().cloned().unwrap_or_default();
        if statuses.is_empty() {
            statuses = defaults();
        }
        let external = optional_schema(root, &schema, "statuses")?;
        if let Some(entries) = external.as_array().or_else(|| {
            external["statuses"]
                .as_array()
                .or(external["definitions"].as_array())
        }) {
            statuses.extend(entries.iter().cloned());
        }
        let mut definitions = BTreeMap::new();
        for definition in statuses {
            if let Some(id) = definition["id"].as_str() {
                definitions.insert(token(id), definition);
            }
        }
        let mut aliases = BTreeMap::new();
        for (id, definition) in &definitions {
            aliases.insert(id.clone(), id.clone());
            for alias in definition["aliases"].as_array().into_iter().flatten() {
                if let Some(alias) = alias.as_str() {
                    aliases.insert(token(alias), id.clone());
                }
            }
        }
        let workflow = optional_schema(root, &schema, "workflows")?;
        let workflow = workflow.get("workflow").unwrap_or(&workflow);
        let progress = workflow["in_progress_status"]
            .as_str()
            .or_else(|| schema["workflow"]["in_progress_status"].as_str())
            .unwrap_or("in_progress");
        Ok(Self {
            hierarchy: Hierarchy::new(&settings["schema"]["relationship_kinds"]),
            definitions,
            aliases,
            progress: token(progress),
            open: token(
                workflow["open_status"]
                    .as_str()
                    .or_else(|| schema["workflow"]["open_status"].as_str())
                    .unwrap_or("open"),
            ),
        })
    }

    /// Resolves a status token or alias to its canonical registry identity.
    pub(crate) fn canonical(&self, status: &str) -> String {
        let normalized = token(status);
        self.aliases.get(&normalized).cloned().unwrap_or(normalized)
    }

    /// Tests a runtime role without treating an unknown status as built-in.
    pub(crate) fn role(&self, status: &str, role: &str) -> bool {
        self.definitions
            .get(&self.canonical(status))
            .is_some_and(|definition| {
                definition["roles"]
                    .as_array()
                    .is_some_and(|roles| roles.iter().any(|entry| entry == role))
            })
    }

    /// Classifies all three published terminal roles.
    pub(crate) fn terminal(&self, status: &str) -> bool {
        ["terminal", "terminal_done", "terminal_canceled"]
            .iter()
            .any(|role| self.role(status, role))
    }

    /// Matches actionability's fallback open status when the active role set is empty.
    pub(crate) fn active_for_blocking(&self, status: &str) -> bool {
        self.role(status, "active")
            || (!self.definitions.keys().any(|id| self.role(id, "active"))
                && self.canonical(status) == self.open)
    }

    /// Applies the named open/progress buckets, retaining exact custom selectors.
    pub(crate) fn matches(&self, status: &str, selected: &str) -> bool {
        match selected {
            "open" => self.role(status, "active") && self.canonical(status) != self.progress,
            "in_progress" => self.canonical(status) == self.progress,
            _ => self.canonical(status) == self.canonical(selected),
        }
    }
}

/// Returns the canonical built-in roles when no statuses are configured.
fn defaults() -> Vec<Value> {
    [
        ("draft", vec!["draft"]),
        ("open", vec!["active", "default_open"]),
        ("in_progress", vec!["active"]),
        ("blocked", vec!["blocked"]),
        ("closed", vec!["terminal", "terminal_done", "default_close"]),
        (
            "canceled",
            vec!["terminal", "terminal_canceled", "default_cancel"],
        ),
    ]
    .into_iter()
    .map(|(id, roles)| json!({"id":id,"roles":roles}))
    .collect()
}

/// Normalizes case, whitespace and hyphen runs as the published status registry does.
fn token(value: &str) -> String {
    value
        .trim()
        .to_lowercase()
        .split(|c: char| c == '-' || c.is_whitespace())
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("_")
}

/// Loads optional runtime schema data without requiring the scaffold to exist.
fn optional_schema(root: &Path, schema: &Value, name: &str) -> Result<Value, PmRustError> {
    let fallback = format!("schema/{name}.json");
    let path = root.join(schema["files"][name].as_str().unwrap_or(&fallback));
    match fs::read(&path) {
        Ok(bytes) => Ok(serde_json::from_slice(&bytes).unwrap_or(Value::Null)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(if name == "statuses" {
            json!({"statuses": defaults()})
        } else {
            json!({"workflow":{"draft_status":"draft","open_status":"open","in_progress_status":"in_progress","blocked_status":"blocked","close_status":"closed","canceled_status":"canceled"}})
        }),
        Err(source) => Err(PmRustError::Io { path, source }),
    }
}

#[cfg(test)]
#[path = "../tests/support/list_registry_unit.rs"]
mod tests;

/// Declarative hierarchy direction indexed by canonical kind and aliases.
pub(crate) struct Hierarchy(BTreeMap<String, bool>);

impl Hierarchy {
    /// Builds the built-in registry with explicit custom hierarchy definitions.
    pub(crate) fn new(definitions: &Value) -> Self {
        let mut kinds = BTreeMap::new();
        for (kind, source_parent) in [
            ("parent", false),
            ("child_of", false),
            ("epic", false),
            ("child", true),
            ("parent_child", true),
            ("task", true),
        ] {
            kinds.insert(kind.to_owned(), source_parent);
        }
        for definition in definitions.as_array().into_iter().flatten() {
            if definition["hierarchy"] != true {
                continue;
            }
            if let Some(kind) = definition["kind"].as_str() {
                let source_parent = definition["hierarchyDirection"] == "source_parent";
                kinds.insert(token(kind), source_parent);
                for alias in definition["aliases"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                {
                    kinds.insert(token(alias), source_parent);
                }
            }
        }
        Self(kinds)
    }

    /// Resolves a relationship token to whether its source is the parent.
    pub(crate) fn source_parent(&self, kind: &str) -> Option<bool> {
        self.0.get(&token(kind)).copied()
    }
}
