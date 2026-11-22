//! Configured lifecycle classification for native list selection.

use std::{collections::BTreeMap, fs, path::Path};

use serde_json::{Value, json};

use crate::PmRustError;

/// Lifecycle definitions and workflow selected for one complete list read.
pub(crate) struct Lifecycle {
    /// Normalized status definitions, keyed by canonical identity.
    definitions: BTreeMap<String, (Value, f64)>,
    /// Alias tokens mapped to canonical status identities.
    aliases: BTreeMap<String, String>,
    /// Configured in-progress status used by lifecycle buckets.
    progress: String,
    /// Configured close target, independent of the roles assigned to it.
    close: String,
    /// Configured fallback active status when no definition carries an active role.
    open: String,
    /// Type transition rules captured through the same workflow-file discovery as statuses.
    pub(crate) type_workflows: Value,
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
        let external = optional_schema(root, &schema, "statuses")?;
        let workflow = optional_schema(root, &schema, "workflows")?;
        Ok(Self::from_values(&settings, &external, &workflow))
    }

    /// Resolves an already captured schema through the native registry normalizer.
    pub(crate) fn from_values(settings: &Value, external: &Value, workflow: &Value) -> Self {
        let schema = &settings["schema"];
        let mut definitions = BTreeMap::new();
        extend_statuses(
            &mut definitions,
            schema["statuses"].as_array().into_iter().flatten().cloned(),
        );
        if definitions.is_empty() {
            extend_statuses(&mut definitions, defaults());
        }
        if let Some(entries) = external.as_array().or_else(|| {
            external["statuses"]
                .as_array()
                .or(external["definitions"].as_array())
        }) {
            extend_statuses(&mut definitions, entries.iter().cloned());
        }
        let mut aliases = BTreeMap::new();
        for (id, (definition, _)) in &definitions {
            aliases.insert(id.clone(), id.clone());
            for alias in definition["aliases"].as_array().into_iter().flatten() {
                if let Some(alias) = alias.as_str() {
                    aliases.insert(token(alias), id.clone());
                }
            }
        }
        let type_workflows = workflow
            .get("type_workflows")
            .unwrap_or(&schema["type_workflows"])
            .clone();
        let workflow = workflow.get("workflow").unwrap_or(workflow);
        let configured_progress = workflow["in_progress_status"]
            .as_str()
            .or_else(|| schema["workflow"]["in_progress_status"].as_str())
            .unwrap_or_default();
        let progress = token(crate::history::trim_evidence_line(configured_progress));
        let configured_open = workflow["open_status"]
            .as_str()
            .or_else(|| schema["workflow"]["open_status"].as_str())
            .unwrap_or_default();
        let open = token(crate::history::trim_evidence_line(configured_open));
        let open = if open.is_empty() {
            let first = ordered_status(&definitions, "default_open");
            if let Some((id, _)) = first.filter(|(_, (value, _))| has_role(value, "default_open")) {
                id.clone()
            } else {
                ["open", "in_progress", "draft"]
                    .into_iter()
                    .find(|id| definitions.contains_key(*id))
                    .or(first.map(|(id, _)| id.as_str()))
                    .unwrap_or_default()
                    .to_owned()
            }
        } else {
            open
        };
        let progress = if progress.is_empty() {
            progress_fallback(&definitions, &open)
        } else {
            progress
        };
        Self {
            type_workflows,
            hierarchy: Hierarchy::new(&settings["schema"]["relationship_kinds"]),
            definitions,
            aliases,
            progress,
            close: token(
                workflow["close_status"]
                    .as_str()
                    .or_else(|| schema["workflow"]["close_status"].as_str())
                    .unwrap_or("closed"),
            ),
            open,
        }
    }

    /// Resolves a status token or alias to its canonical registry identity.
    pub(crate) fn canonical(&self, status: &str) -> String {
        let normalized = token(status);
        self.aliases.get(&normalized).cloned().unwrap_or(normalized)
    }

    /// Resolves the published progress fallback when workflow statuses coincide or are removed.
    pub(crate) fn start_status(&self) -> String {
        let open = self.canonical(&self.open);
        let progress = self.canonical(&self.progress);
        if progress != open && self.known(&progress) {
            progress
        } else if self.known("in_progress") {
            self.canonical("in_progress")
        } else {
            self.canonical(&self.open)
        }
    }

    /// Resolves the progress alias for activity advisories independent of list buckets.
    pub(crate) fn activity_progress_status(&self) -> String {
        self.canonical(&self.progress)
    }

    /// Returns the canonical configured open status for pause transitions.
    pub(crate) fn open_status(&self) -> String {
        self.canonical(&self.open)
    }

    /// Tests the configured close identity even when its roles are customized.
    pub(crate) fn closes(&self, status: &str) -> bool {
        self.canonical(status) == self.canonical(&self.close)
    }

    /// Recognizes a configured status token or alias for strict search filters.
    pub(crate) fn known(&self, status: &str) -> bool {
        self.definitions.contains_key(&self.canonical(status))
    }

    /// Tests a runtime role without treating an unknown status as built-in.
    pub(crate) fn role(&self, status: &str, role: &str) -> bool {
        self.definitions
            .get(&self.canonical(status))
            .is_some_and(|(definition, _)| has_role(definition, role))
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

/// Adds usable status definitions, retaining their configured or positional precedence.
fn extend_statuses(
    definitions: &mut BTreeMap<String, (Value, f64)>,
    statuses: impl IntoIterator<Item = Value>,
) {
    let offset = definitions.len();
    for (index, definition) in statuses.into_iter().enumerate() {
        let id = token(crate::history::trim_evidence_line(
            definition["id"].as_str().unwrap_or_default(),
        ));
        if id.is_empty() {
            continue;
        }
        #[allow(
            clippy::cast_precision_loss,
            reason = "status arrays fit the precise range"
        )]
        let order = definition["order"]
            .as_f64()
            .unwrap_or((offset + index) as f64);
        definitions.insert(id, (definition, order));
    }
}

/// Checks the declared roles on a status definition.
fn has_role(definition: &Value, role: &str) -> bool {
    definition["roles"].as_array().is_some_and(|roles| {
        roles
            .iter()
            .filter_map(Value::as_str)
            .any(|entry| crate::history::trim_evidence_line(entry) == role)
    })
}

/// Selects the published active fallback for an unset progress setting.
fn progress_fallback(definitions: &BTreeMap<String, (Value, f64)>, open: &str) -> String {
    if let Some(preferred) = ["in_progress", open]
        .into_iter()
        .find(|status| definitions.contains_key(*status))
    {
        return preferred.to_owned();
    }
    ordered_status(definitions, "active")
        .map(|(id, _)| id.clone())
        .unwrap_or_default()
}

/// Selects the first definition with the requested role, or the first definition overall.
fn ordered_status<'a>(
    definitions: &'a BTreeMap<String, (Value, f64)>,
    role: &str,
) -> Option<(&'a String, &'a (Value, f64))> {
    definitions.iter().min_by(
        |(left_id, (left, left_order)), (right_id, (right, right_order))| {
            has_role(right, role)
                .cmp(&has_role(left, role))
                .then_with(|| left_order.total_cmp(right_order))
                .then_with(|| crate::collation::compare(left_id, right_id))
        },
    )
}

#[cfg(test)]
mod activity_progress_tests {
    use super::*;

    #[test]
    fn blank_progress_falls_back_to_the_ordered_active_role() {
        let settings = json!({"schema":{
            "statuses":[
                {"id":"zeta","order":10,"roles":["active"]},
                {"id":"alpha","order":10,"roles":["active"]},
                {"id":"closed","order":1,"roles":["terminal"]},
                {"id":"later","order":20,"roles":["active"]}
            ],
            "workflow":{"in_progress_status":" ", "open_status":"absent"}
        }});
        assert_eq!(
            Lifecycle::from_values(&settings, &Value::Null, &Value::Null)
                .activity_progress_status(),
            "alpha"
        );
    }
}

/// Returns the canonical built-in roles when no statuses are configured.
pub(crate) fn defaults() -> Vec<Value> {
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
pub(crate) fn token(value: &str) -> String {
    value
        .trim()
        .to_lowercase()
        .split(|c: char| c == '-' || c.is_whitespace())
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("_")
}

/// Supplies the published preview scaffold defaults without changing observational reads.
pub(crate) fn default_schema(name: &str) -> Value {
    if name == "statuses" {
        json!({"statuses":defaults()})
    } else {
        json!({"workflow":{"draft_status":"draft","open_status":"open","in_progress_status":"in_progress","blocked_status":"blocked","close_status":"closed","canceled_status":"canceled"}})
    }
}

/// Discovers explicit schema locations or the published default sidecar path.
pub(crate) fn schema_path(root: &Path, schema: &Value, name: &str) -> std::path::PathBuf {
    root.join(
        schema["files"][name]
            .as_str()
            .unwrap_or(&format!("schema/{name}.json")),
    )
}

/// Loads optional runtime schema data without requiring the scaffold to exist.
pub(crate) fn optional_schema(
    root: &Path,
    schema: &Value,
    name: &str,
) -> Result<Value, PmRustError> {
    let path = schema_path(root, schema, name);
    match fs::read(&path) {
        Ok(bytes) => Ok(crate::strict_json::from_slice(&bytes).unwrap_or(Value::Null)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Value::Null),
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
