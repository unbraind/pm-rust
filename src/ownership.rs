//! Native selection, session identity and composed ownership lifecycle operations.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::path::Path;

use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::{OwnershipItem, OwnershipResult, PmRustError, Workspace};

thread_local! {
    /// Scoped attribution; isolated between simultaneous SDK callers.
    static ATTRIBUTION: RefCell<Option<AuthorIdentity>> = const { RefCell::new(None) };
}

/// Resolved display identity, ownership principal and history attribution.
#[derive(Clone, Debug, PartialEq)]
pub struct AuthorIdentity {
    /// Display author used in item and history metadata.
    pub author: String,
    /// Session-specific principal for detected identities; display author otherwise.
    pub principal: String,
    /// Identity origin: asserted, configured, detected or unknown.
    pub source: &'static str,
    /// Detected agent harness.
    pub harness: Option<String>,
    /// Hashed session identifier, never the raw session value.
    pub instance: Option<String>,
    /// Available provenance dimensions and their detection sources.
    pub provenance: serde_json::Map<String, Value>,
    /// Bounded local resolver outcomes retained in mutation history.
    pub resolver_outcomes: serde_json::Map<String, Value>,
}

impl AuthorIdentity {
    /// Resolves explicit author, environment author, configured author, then harness.
    #[must_use]
    #[allow(clippy::too_many_lines)]
    pub fn resolve(
        asserted: Option<&str>,
        configured: Option<&str>,
        env: &BTreeMap<String, String>,
    ) -> Self {
        let descriptors = [
            (
                "claude-code",
                &["CLAUDE_CODE", "CLAUDECODE"][..],
                "CLAUDE_CODE_SESSION_ID",
                "ANTHROPIC_MODEL",
            ),
            (
                "codex",
                &["CODEX_HOME", "CODEX_CI", "CODEX_THREAD_ID"][..],
                "CODEX_THREAD_ID",
                "CODEX_MODEL",
            ),
            (
                "pi",
                &["PI_AGENT", "PI_CODING_AGENT"][..],
                "PI_SESSION_ID",
                "PI_MODEL",
            ),
            (
                "opencode",
                &["OPENCODE", "OPENCODE_SESSION_ID"][..],
                "OPENCODE_SESSION_ID",
                "OPENCODE_MODEL",
            ),
            (
                "cursor",
                &["CURSOR_AGENT", "CURSOR_TRACE_ID"][..],
                "CURSOR_TRACE_ID",
                "CURSOR_MODEL",
            ),
            (
                "aider",
                &["AIDER", "AIDER_MODEL"][..],
                "AIDER_SESSION_ID",
                "AIDER_MODEL",
            ),
            (
                "gemini-cli",
                &["GEMINI_CLI", "GEMINI_CLI_HOME"][..],
                "GEMINI_CLI_SESSION_ID",
                "GEMINI_MODEL",
            ),
            (
                "ci",
                &["CI", "GITHUB_ACTIONS", "BUILDKITE", "GITLAB_CI"][..],
                "",
                "",
            ),
        ];
        let detected = descriptors.iter().find(|(_, keys, _, _)| {
            keys.iter()
                .any(|key| nonempty(env.get(*key).map(String::as_str)).is_some())
        });
        let harness = detected.map(|descriptor| descriptor.0.to_owned());
        let instance = detected.and_then(|(name, _, session, _)| {
            nonempty(env.get(*session).map(String::as_str)).map(|session| {
                hex_prefix(
                    format!(
                        "pm-agent-instance:v1\0{name}\0{}",
                        session.chars().take(256).collect::<String>()
                    )
                    .as_bytes(),
                    24,
                )
            })
        });
        let (author, source) =
            if let Some(value) = asserted.or_else(|| env.get("PM_AUTHOR").map(String::as_str)) {
                nonempty(Some(value)).map_or_else(
                    || ("unknown".to_owned(), "unknown"),
                    |author| (author.to_owned(), "asserted"),
                )
            } else if let Some(author) = nonempty(configured) {
                (author.to_owned(), "configured")
            } else if let Some(harness) = &harness {
                (format!("harness:{harness}"), "detected")
            } else {
                ("unknown".to_owned(), "unknown")
            };
        let principal = if source == "detected" {
            instance
                .as_ref()
                .map_or_else(|| author.clone(), |instance| format!("{author}#{instance}"))
        } else {
            author.clone()
        };
        let mut provenance = serde_json::Map::new();
        for dimension in ["model", "effort", "role", "topic", "version"] {
            let override_key = format!("PM_AGENT_{}", dimension.to_uppercase());
            let environment_key = match dimension {
                "model" => detected.map_or("", |descriptor| descriptor.3),
                "effort" if harness.as_deref() == Some("codex") => "CODEX_REASONING_EFFORT",
                "effort" if harness.as_deref() == Some("claude-code") => "CLAUDE_EFFORT",
                "role" if harness.as_deref() == Some("codex") => "CODEX_SESSION_ROLE",
                _ => "",
            };
            let override_value = nonempty(env.get(&override_key).map(String::as_str));
            let value =
                override_value.or_else(|| nonempty(env.get(environment_key).map(String::as_str)));
            if let Some(value) = value {
                provenance.insert(dimension.to_owned(), json!({"value":value.chars().take(256).collect::<String>(),"source":if override_value.is_some(){"override"}else{"environment"}}));
            } else if detected.is_some() && dimension != "version" {
                provenance.insert(dimension.to_owned(), Value::Null);
            }
        }
        Self {
            author,
            principal,
            source,
            harness,
            instance,
            provenance,
            resolver_outcomes: serde_json::Map::new(),
        }
    }

    /// Resolves bounded local model/version probes for mutation history attribution.
    #[must_use]
    pub fn for_collections(mut self, env: &BTreeMap<String, String>) -> Self {
        crate::identity_probes::enrich(&mut self, env);
        self
    }

    /// Executes a synchronous mutation with scoped attribution, restoring on unwind.
    pub fn with<T>(&self, operation: impl FnOnce() -> T) -> T {
        let previous = ATTRIBUTION.with(|slot| slot.replace(Some(self.clone())));
        let _restore = RestoreAttribution(previous);
        operation()
    }

    /// Adds inferred role and topic from this session's native or published state.
    fn with_semantics(&self, pm_root: &Path) -> Self {
        let mut identity = self.clone();
        let state = fs::read_to_string(pm_root.join("runtime/session.json"))
            .ok()
            .and_then(|text| serde_json::from_str::<Value>(&text).ok());
        if let Some(instance) = &identity.instance
            && let Some(semantic) = state
                .as_ref()
                .and_then(|state| state["semantic_attribution"].get(instance))
        {
            for key in ["role", "topic"] {
                if identity.provenance.get(key).is_none_or(Value::is_null)
                    && let Some(value) = semantic[key].as_str()
                {
                    identity.provenance.insert(key.to_owned(), json!({"value":value,"source":"inferred","confidence":semantic["confidence"],"rule_version":semantic["rule_version"],"evidence":semantic["evidence"]}));
                }
            }
        }
        identity
    }
}

/// Restores a previous SDK caller's attribution on success, error or unwind.
struct RestoreAttribution(Option<AuthorIdentity>);
impl Drop for RestoreAttribution {
    fn drop(&mut self) {
        ATTRIBUTION.with(|slot| {
            slot.replace(self.0.take());
        });
    }
}

/// Returns a trimmed nonempty identity signal.
fn nonempty(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

/// Loads the configured status and workflow schema files over embedded defaults.
pub(crate) fn runtime_settings(pm_root: &Path) -> Result<Value, PmRustError> {
    let mut settings = read_json(&pm_root.join("settings.json"))?;
    let mut schema = settings["schema"].as_object().cloned().unwrap_or_default();
    for (name, keys) in [
        ("statuses", &["statuses"][..]),
        ("workflows", &["workflow", "type_workflows"][..]),
    ] {
        if let Some(relative) = schema.get("files").and_then(|files| files[name].as_str()) {
            let external = read_json(&pm_root.join(relative))?;
            for key in keys {
                if let Some(value) = external.get(*key) {
                    schema.insert((*key).to_owned(), value.clone());
                }
            }
        }
    }
    settings["schema"] = Value::Object(schema);
    Ok(settings)
}

/// Reads one schema object with errors attributed to its actual file.
pub(crate) fn read_json(path: &Path) -> Result<Value, PmRustError> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(source) => {
            return Err(PmRustError::Io {
                path: path.to_path_buf(),
                source,
            });
        }
    };
    match serde_json::from_slice::<Value>(&bytes) {
        Ok(value) if value.is_object() => Ok(value),
        Ok(_) => Err(PmRustError::InvalidReadRequest {
            reason: "Expected a settings or schema object".to_owned(),
        }),
        Err(error) => Err(PmRustError::InvalidReadRequest {
            reason: error.to_string(),
        }),
    }
}

/// Stamps identity and provenance before the immutable history record is hashed.
pub(crate) fn stamp_attribution(entry: &mut crate::history::HistoryEntry<'_>, role: Option<&str>) {
    ATTRIBUTION.with(|slot| {
        if let Some(identity) = slot.borrow().as_ref() {
            entry.author_source = identity.source;
            entry.agent_harness.clone_from(&identity.harness);
            entry.agent_instance.clone_from(&identity.instance);
            entry.agent_model = identity
                .provenance
                .get("model")
                .and_then(|value| value["value"].as_str())
                .map(str::to_owned);
            entry.agent_model_source = identity
                .provenance
                .get("model")
                .and_then(|value| value["source"].as_str())
                .map(str::to_owned);
            let mut provenance = identity.provenance.clone();
            if let Some(role) = role
                && provenance.get("role").is_none_or(Value::is_null)
            {
                provenance.insert("role".to_owned(), json!({"value":role,"source":"argv"}));
            }
            entry.agent_provenance = if provenance.is_empty() {
                None
            } else {
                Some(Value::Object(provenance))
            };
            if !identity.resolver_outcomes.is_empty() {
                entry.context =
                    Some(json!({"agent_provenance_outcomes":identity.resolver_outcomes}));
            }
        }
    });
}

/// Additional ownership controls exposed to native SDK callers.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct OwnershipOptions {
    /// Resolved session identity and provenance, when supplied.
    pub identity: Option<AuthorIdentity>,
    /// Approved package-level handoff that only clears ownership metadata.
    pub release_bypass: bool,
}

/// Filters and atomic retry budget for native claim-next selection.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SelectionOptions {
    /// Exact type filter, case insensitive.
    pub item_type: Option<String>,
    /// Exact tag filter.
    pub tag: Option<String>,
    /// Exact priority filter.
    pub priority: Option<u8>,
    /// Select descendants of this parent.
    pub parent: Option<String>,
    /// Exact sprint filter.
    pub sprint: Option<String>,
    /// Exact release filter.
    pub release: Option<String>,
    /// Ownership filter: assigned or unassigned; omitted includes both.
    pub assignee_filter: Option<String>,
    /// Include human decisions among claimable candidates.
    pub include_decisions: bool,
    /// Include gates among claimable candidates.
    pub include_gates: bool,
    /// Include active containers.
    pub include_containers: bool,
    /// Attempt at most this many ranked candidates (default ten).
    pub max_attempts: Option<usize>,
}

impl Workspace {
    /// Claims an explicit item with detected session identity or SDK handoff controls.
    ///
    /// # Errors
    /// Returns validation, lock, ownership or durable storage failures.
    pub fn claim_with_options(
        &self,
        request: &OwnershipItem,
        options: &OwnershipOptions,
    ) -> Result<OwnershipResult, PmRustError> {
        let parents = self.read_parent_index()?;
        let receipt = with_identity(self.pm_root(), options, || {
            crate::mutation::ownership_item_with_options(self.pm_root(), request, true, options)
        })?;
        if receipt.skipped.is_none() {
            record_semantics(self, request, options, true, &parents)?;
        }
        Ok(receipt)
    }

    /// Releases an explicit item with session identity and an optional approved bypass.
    ///
    /// # Errors
    /// Returns validation, lock, ownership or durable storage failures.
    pub fn release_with_options(
        &self,
        request: &OwnershipItem,
        options: &OwnershipOptions,
    ) -> Result<OwnershipResult, PmRustError> {
        let parents = self.read_parent_index()?;
        let receipt = with_identity(self.pm_root(), options, || {
            crate::mutation::ownership_item_with_options(self.pm_root(), request, false, options)
        })?;
        record_semantics(self, request, options, false, &parents)?;
        Ok(receipt)
    }

    /// Claims first, then starts the item; a failed update preserves the claim.
    ///
    /// # Errors
    /// Returns a refusal carrying persisted-state guidance after a partial failure.
    pub fn start_task(
        &self,
        request: &OwnershipItem,
        options: &OwnershipOptions,
    ) -> Result<Value, PmRustError> {
        // The published SDK refuses this option before the claim step.
        if request.if_available {
            return Err(PmRustError::CommandRefusal {
                detail:
                    "--start cannot be combined with --next or --if-available for an explicit item"
                        .to_owned(),
                exit_code: 2,
                context: json!({"code":"invalid_argument_value","required":"Remove --next and --if-available when starting an explicit item.","next_steps":["Remove --next and --if-available, keep the explicit item ID and other supplied arguments, then retry."]}),
            });
        }
        let claim = self.claim_with_options(request, options)?;
        let update = with_identity(self.pm_root(), options, || {
            crate::mutation::lifecycle_status(self.pm_root(), request, options, true)
        })
        .map_err(|error| partial_failure(error, &request.id, "claim"))?;
        Ok(
            json!({"id":claim.id,"action":"start_task","claim":claim,"update":update,"status":update["status"],"changed_field_count":u64::try_from(claim.changed_field_count).unwrap_or_default() + update["changed_field_count"].as_u64().unwrap_or_default()}),
        )
    }

    /// Opens the item first, then releases it; a failed release preserves the update.
    ///
    /// # Errors
    /// Returns a refusal carrying persisted-state guidance after a partial failure.
    pub fn pause_task(
        &self,
        request: &OwnershipItem,
        options: &OwnershipOptions,
    ) -> Result<Value, PmRustError> {
        let update = with_identity(self.pm_root(), options, || {
            crate::mutation::lifecycle_status(self.pm_root(), request, options, false)
        })?;
        let release = self
            .release_with_options(request, options)
            .map_err(|error| partial_failure(error, &request.id, "update"))?;
        Ok(
            json!({"id":release.id,"action":"pause_task","update":update,"release":release,"status":release.status,"changed_field_count":update["changed_field_count"].as_u64().unwrap_or_default()+u64::try_from(release.changed_field_count).unwrap_or_default()}),
        )
    }
}

/// Applies the supplied attribution without consulting ambient SDK process state.
pub(crate) fn with_identity<T>(
    pm_root: &Path,
    options: &OwnershipOptions,
    operation: impl FnOnce() -> T,
) -> T {
    if let Some(identity) = &options.identity {
        identity.with_semantics(pm_root).with(operation)
    } else {
        operation()
    }
}

/// Stores ownership-derived role and topic for later attribution resolution.
fn record_semantics(
    workspace: &Workspace,
    request: &OwnershipItem,
    options: &OwnershipOptions,
    claim: bool,
    parents: &BTreeMap<String, Option<String>>,
) -> Result<(), PmRustError> {
    let principal = options
        .identity
        .as_ref()
        .map_or(request.author.as_str(), |identity| {
            identity.principal.as_str()
        });
    let key = if let Some(instance) = principal
        .rsplit_once('#')
        .map(|(_, instance)| instance)
        .filter(|instance| {
            instance.len() == 24 && instance.bytes().all(|byte| byte.is_ascii_hexdigit())
        }) {
        instance.to_owned()
    } else {
        format!("author-{}", hex_prefix(principal.as_bytes(), 24))
    };
    let Some(mut current) = parents.get(&request.id).cloned() else {
        return Err(PmRustError::ItemNotFound {
            id: request.id.clone(),
        });
    };
    let mut lineage = vec![request.id.clone()];
    while let Some(parent) = current {
        if lineage.len() >= 16 || lineage.contains(&parent) || !parents.contains_key(&parent) {
            break;
        }
        lineage.push(parent.clone());
        current = parents.get(&parent).cloned().flatten();
    }
    crate::mutation::mutate_session_state(workspace.pm_root(), |state| {
        let mut attribution = state
            .get("semantic_attribution")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        let before = attribution.get(&key).cloned().unwrap_or(Value::Null);
        let mut ids: Vec<String> =
            before["active_item_ids"]
                .as_array()
                .map_or_else(Vec::new, |ids| {
                    ids.iter()
                        .filter_map(|id| id.as_str().map(str::to_owned))
                        .collect()
                });
        if claim {
            if !ids.contains(&request.id) {
                ids.push(request.id.clone());
            }
        } else {
            ids.retain(|id| id != &request.id);
        }
        if ids.len() > 64 {
            ids.drain(..ids.len() - 64);
        }
        ids.sort_by(|left, right| crate::collation::compare(left, right));
        let focus = before["focused_item_id"].as_str();
        if !claim && ids.is_empty() && focus.is_none() {
            attribution.remove(&key);
        } else {
            let topic = focus.map_or_else(|| workset_topic(&ids), str::to_owned);
            let mut evidence: Vec<String> = ids.iter().map(|id| format!("claim:{id}")).collect();
            if let Some(focus) = focus {
                evidence.push(format!("focus:{focus}"));
            }
            if claim {
                evidence.extend(lineage.iter().map(|id| format!("lineage:{id}")));
            } else {
                evidence.push(format!("release:{}", request.id));
            }
            evidence.truncate(32);
            let mut semantic = json!({"role":if claim{"implementer"}else{"release-operator"},"topic":topic,"confidence":if focus.is_some(){"high"}else{"medium"},"rule_version":"v2","evidence":evidence,"active_item_ids":ids});
            if let Some(focus) = focus {
                semantic["focused_item_id"] = json!(focus);
            }
            attribution.insert(key, semantic);
        }
        if attribution.is_empty() {
            state.remove("semantic_attribution");
        } else {
            state.insert(
                "semantic_attribution".to_owned(),
                Value::Object(attribution),
            );
        }
    })
}

/// Produces a fixed-length hexadecimal digest without retaining raw session signals.
fn hex_prefix(bytes: &[u8], length: usize) -> String {
    let mut hex = String::new();
    for byte in Sha256::digest(bytes) {
        let _ = write!(hex, "{byte:02x}");
    }
    hex[..length].to_owned()
}

/// Builds the published bounded topic for a sorted active workset.
fn workset_topic(ids: &[String]) -> String {
    if ids.len() == 1 {
        return ids[0].clone();
    }
    let joined = ids.join("+");
    let topic = format!("workset:{joined}");
    if topic.len() <= 256 {
        topic
    } else {
        format!(
            "workset:{}+{}",
            joined.chars().take(230).collect::<String>(),
            hex_prefix(ids.join("\0").as_bytes(), 12)
        )
    }
}

/// Wraps a later failure with evidence that an earlier mutation remains durable.
#[must_use]
pub fn partial_failure(error: PmRustError, id: &str, completed: &str) -> PmRustError {
    let detail = error.to_string();
    let (exit_code, mut context) = match error {
        PmRustError::OwnershipRefusal { context, .. } => (4, context),
        PmRustError::CommandRefusal {
            exit_code, context, ..
        } => (exit_code, context),
        _ => (1, json!({})),
    };
    let original_why = context["why"]
        .as_str()
        .map_or(String::new(), |why| format!(" {why}"));
    let mut steps = vec![json!(format!(
        "Inspect the persisted state with pm get {id} and pm history {id} before retrying the failed step."
    ))];
    if let Some(original) = context["next_steps"].as_array() {
        steps.extend(original.clone());
    }
    context["item_id"] = json!(id);
    context["why"] = json!(format!(
        "{completed} completed for {id} before the following step failed. No rollback was performed.{original_why}"
    ));
    context["next_steps"] = json!(steps);
    PmRustError::CommandRefusal {
        detail,
        exit_code,
        context,
    }
}

#[cfg(test)]
#[path = "../tests/support/ownership_unit.rs"]
pub(crate) mod tests;
