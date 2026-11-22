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
    /// Refuses URL userinfo in an identity before a mutation can persist it.
    ///
    /// # Errors
    /// Returns [`PmRustError::IdentityCredentials`] for a credential-bearing identity.
    pub fn validate_credentials(
        identity: &str,
        identity_source: &'static str,
    ) -> Result<(), PmRustError> {
        let identity = identity.trim();
        if let Some((start, end)) = identity_userinfo(identity) {
            crate::refusal::enroll_rejected_identity(identity, &identity[start..end]);
            return Err(PmRustError::IdentityCredentials { identity_source });
        }
        Ok(())
    }

    /// Redacts possible URL userinfo in a single asserted or historic identity.
    #[must_use]
    pub fn redact_credentials(identity: &str) -> std::borrow::Cow<'_, str> {
        match identity_userinfo(identity) {
            Some((start, end)) => {
                format!("{}[redacted]{}", &identity[..start], &identity[end..]).into()
            }
            None => identity.into(),
        }
    }

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
            .and_then(|text| crate::strict_json::from_str::<Value>(&text).ok());
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

/// Locates possible userinfo in an identity using the published whole-value rule.
pub(crate) fn identity_userinfo(identity: &str) -> Option<(usize, usize)> {
    let colon = identity.find(':')?;
    let at = identity
        .rfind('@')
        .into_iter()
        .chain(identity.rfind("%40"))
        .max()?;
    if colon >= at {
        return None;
    }
    let tail = &identity[colon + 1..at];
    let slashes = tail
        .bytes()
        .take_while(|byte| matches!(byte, b'/' | b'\\'))
        .count();
    let start = colon + 1 + slashes.min(2);
    if start == at || public_identity_url(identity, colon) {
        return None;
    }
    Some((start, at))
}

/// Recognizes public scoped-package and blob URLs whose path contains an at sign.
fn public_identity_url(identity: &str, colon: usize) -> bool {
    let scheme = identity[..colon]
        .rsplit(|ch: char| !ch.is_ascii_alphanumeric() && !matches!(ch, '+' | '.' | '-'))
        .next()
        .unwrap_or_default();
    if !["http", "https", "ws", "wss", "ftp"]
        .iter()
        .any(|known| scheme.eq_ignore_ascii_case(known))
        || identity.contains(['\t', '\n', '\r'])
    {
        return false;
    }
    let Some((authority, path)) = identity[colon + 1..]
        .trim_start_matches(['/', '\\'])
        .split_once('/')
    else {
        return false;
    };
    !authority.contains([':', '@', '%'])
        && !path.starts_with(['/', '\\'])
        && !path.contains([':', '%'])
        && !path
            .as_bytes()
            .windows(2)
            .any(|pair| pair.iter().all(|byte| matches!(byte, b'/' | b'\\')))
}

/// Returns a trimmed nonempty identity signal.
fn nonempty(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

/// Loads the configured status and workflow schema files over embedded defaults.
pub(crate) fn runtime_settings(pm_root: &Path) -> Result<Value, PmRustError> {
    let mut settings = read_json(&pm_root.join("settings.json"))?;
    crate::mutation::normalize_mutation_settings(&mut settings);
    let mut schema = settings["schema"].as_object().cloned().unwrap_or_default();
    for (name, keys) in [
        ("statuses", &["statuses"][..]),
        ("workflows", &["workflow", "type_workflows"][..]),
    ] {
        {
            let explicit = schema
                .get("files")
                .and_then(|files| files[name].as_str())
                .is_some();
            let path =
                crate::list_registry::schema_path(pm_root, &Value::Object(schema.clone()), name);
            let external = if explicit {
                match read_json(&path) {
                    Err(PmRustError::Io { source, .. })
                        if name == "workflows" && source.kind() == std::io::ErrorKind::NotFound =>
                    {
                        continue;
                    }
                    result => result?,
                }
            } else {
                crate::list_registry::optional_schema(
                    pm_root,
                    &Value::Object(schema.clone()),
                    name,
                )?
            };
            for key in keys {
                let value = external.get(*key).or_else(|| {
                    if explicit {
                        None
                    } else if *key == "statuses" {
                        external
                            .as_array()
                            .map(|_| &external)
                            .or_else(|| external.get("definitions"))
                    } else if *key == "workflow" && external.is_object() {
                        Some(&external)
                    } else {
                        None
                    }
                });
                if let Some(value) = value {
                    let value = if *key == "statuses" {
                        let mut entries = schema
                            .get(*key)
                            .and_then(Value::as_array)
                            .filter(|entries| !entries.is_empty())
                            .cloned()
                            .unwrap_or_else(crate::list_registry::defaults);
                        for entry in value.as_array().into_iter().flatten() {
                            let existing = entry["id"].as_str().and_then(|id| {
                                entries.iter_mut().rev().find(|before| {
                                    before["id"].as_str().is_some_and(|before| {
                                        crate::list_registry::token(before)
                                            == crate::list_registry::token(id)
                                    })
                                })
                            });
                            if let Some(existing) = existing {
                                *existing = entry.clone();
                            } else {
                                entries.push(entry.clone());
                            }
                        }
                        Value::Array(entries)
                    } else if *key == "workflow" && !explicit {
                        let mut merged = schema
                            .get(*key)
                            .and_then(Value::as_object)
                            .cloned()
                            .unwrap_or_default();
                        merged.extend(
                            value
                                .as_object()
                                .into_iter()
                                .flatten()
                                .map(|(key, value)| (key.clone(), value.clone())),
                        );
                        Value::Object(merged)
                    } else {
                        value.clone()
                    };
                    schema.insert((*key).to_owned(), value);
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
    match crate::strict_json::from_slice::<Value>(&bytes) {
        Ok(value) if value.is_object() => Ok(value),
        Ok(_) => Err(PmRustError::InvalidReadRequest {
            reason: "Expected a settings or schema object".to_owned(),
        }),
        Err(error) => Err(PmRustError::InvalidReadRequest {
            reason: error.to_string(),
        }),
    }
}

/// Uses the same scoped identity as claim, with an explicit-author SDK fallback.
pub(crate) fn caller_principal(author: &str) -> String {
    ATTRIBUTION.with(|slot| {
        slot.borrow().as_ref().map_or_else(
            || author.trim().to_owned(),
            |identity| identity.principal.clone(),
        )
    })
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
    /// Move a selected claim into progress in the same atomic write.
    pub start: bool,
}

/// Filters and atomic retry budget for native claim-next selection.
// Independent selection/start flags mirror the public CLI and SDK option contract.
#[allow(clippy::struct_excessive_bools)]
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
    /// Start the winning ranked claim atomically.
    pub start: bool,
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
        self.mutate_ownership(request, options, true)
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
        self.mutate_ownership(request, options, false)
    }

    /// Validates lineage before commit and publishes attribution while the item is fenced.
    fn mutate_ownership(
        &self,
        request: &OwnershipItem,
        options: &OwnershipOptions,
        claim: bool,
    ) -> Result<OwnershipResult, PmRustError> {
        let parents = std::cell::RefCell::new(BTreeMap::new());
        with_identity(self.pm_root(), options, || {
            crate::mutation::ownership_item_with_options(
                self.pm_root(),
                request,
                claim,
                options,
                &mut || {
                    let admitted_start =
                        claim && (options.start || crate::lifecycle_admission::explicit_claim());
                    *parents.borrow_mut() = ownership_parents(self, request, admitted_start)?;
                    Ok(())
                },
                &mut |receipt, session_gate_held| {
                    if let Err(error) = record_semantics(
                        self,
                        request,
                        options,
                        claim,
                        session_gate_held,
                        &parents.borrow(),
                    ) {
                        let action = if claim { "claim" } else { "release" };
                        append_attribution_warning(receipt, action, &error);
                    }
                },
            )
        })
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
            return Err(crate::collections::refusal(
                "--start cannot be combined with --next or --if-available for an explicit item",
                2,
                json!({"code":"invalid_argument_value","required":"Remove --next and --if-available when starting an explicit item.","next_steps":["Remove --next and --if-available, keep the explicit item ID and other supplied arguments, then retry."]}),
            ));
        }
        crate::lifecycle_admission::with_explicit_start(|| {
            let mut claim_options = options.clone();
            claim_options.start = false;
            let claim = self.claim_with_options(request, &claim_options)?;
            let update = with_identity(self.pm_root(), options, || {
                crate::mutation::lifecycle_status(self.pm_root(), request, options, true)
            })
            .map_err(|error| partial_failure(error, &request.id, "claim"))?;
            Ok(
                json!({"id":claim.id,"action":"start_task","claim":claim,"update":update,"status":update["status"],"changed_field_count":u64::try_from(claim.changed_field_count).unwrap_or_default() + update["changed_field_count"].as_u64().unwrap_or_default()}),
            )
        })
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

/// Validates lineage under the item lock, after recovery and ownership refusals.
fn ownership_parents(
    workspace: &Workspace,
    request: &OwnershipItem,
    admitted_start: bool,
) -> Result<BTreeMap<String, Option<String>>, PmRustError> {
    let parents = workspace.read_parent_index(admitted_start)?;
    if !parents.contains_key(&request.id) {
        return Err(PmRustError::ItemNotFound {
            id: request.id.clone(),
        });
    }
    Ok(parents)
}

/// Reports a derived-state failure after the ownership mutation has already committed.
pub(crate) fn append_attribution_warning(
    receipt: &mut OwnershipResult,
    action: &str,
    error: &PmRustError,
) {
    let cause = match error {
        PmRustError::CommandRefusal { context, .. } => {
            context["code"].as_str().unwrap_or("unexpected_error")
        }
        PmRustError::LockConflict { .. } => "lock_conflict",
        _ => "unexpected_error",
    };
    let (status, detail) = match cause {
        crate::mutation::SESSION_PUBLICATION_CONFLICT => (
            "unrecorded",
            format!(
                "was not updated ({cause}). To recover, reconcile only the session's active-item attribution for this item against current ownership."
            ),
        ),
        "lock_conflict" => (
            "unrecorded",
            "was not updated (lock_conflict). Once the lock clears, check the session's active-item attribution against current ownership and reconcile it if needed. If pm health reports a stale lock, remove it with pm gc --scope locks.".to_owned(),
        ),
        crate::mutation::SESSION_DURABILITY_UNKNOWN => (
            "durability_unknown",
            format!(
                "may already be visible, and its durability is unknown ({cause}). Check the session's active-item attribution for this item against current ownership and reconcile it only if needed."
            ),
        ),
        _ => (
            "unrecorded",
            format!(
                "was not updated ({cause}). After fixing the session update failure, check and reconcile the session's active-item attribution against current ownership."
            ),
        ),
    };
    receipt
        .warnings
        .push(format!("{action}_semantic_attribution_{status}"));
    receipt.suggestions.push(format!(
        "The {action} of {} is saved, but the session attribution that names this agent's active work {detail}",
        receipt.id,
    ));
}

/// Stores ownership-derived role and topic for later attribution resolution.
fn record_semantics(
    workspace: &Workspace,
    request: &OwnershipItem,
    options: &OwnershipOptions,
    claim: bool,
    session_gate_held: bool,
    parents: &BTreeMap<String, Option<String>>,
) -> Result<(), PmRustError> {
    let principal = options
        .identity
        .as_ref()
        .map_or(request.author.as_str(), |identity| {
            identity.principal.as_str()
        });
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
    crate::mutation::mutate_session_state(workspace.pm_root(), session_gate_held, &mut |state| {
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
        Ok(())
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
        PmRustError::LockConflict { .. } => (
            4,
            json!({"code":"lock_conflict","recovery":{"retry_after_ms":250}}),
        ),
        PmRustError::CommandRefusal {
            exit_code, context, ..
        } => (exit_code, context),
        _ => (1, json!({})),
    };
    let original_why = context["why"]
        .as_str()
        .map_or(String::new(), |why| format!(" {why}"));
    let get = crate::render_pm_command(["get", id]);
    let history = crate::render_pm_command(["history", id]);
    let mut steps = if completed == "close" {
        vec![json!(get), json!(history)]
    } else {
        vec![json!(format!(
            "Inspect the persisted state with {get} and {history} before retrying the failed step."
        ))]
    };
    if let Some(original) = context["next_steps"].as_array() {
        steps.extend(original.clone());
    }
    context["item_id"] = json!(id);
    if matches!(completed, "close" | "update") {
        context["completed_step"] = json!(completed);
        if context["code"] == "stale_lock_requires_override" {
            let retry_release = json!(
                "Remove the expired lease, then retry only release with the original identity and applicable release options."
            );
            for step in &mut steps {
                if *step == crate::mutation::RETRY_STALE_LOCK {
                    *step = retry_release.clone();
                }
            }
            context["required"] = retry_release;
        } else if context["code"] == "lock_conflict" {
            context["required"] = if completed == "update" {
                json!(
                    "Wait for the lock to clear and verify current ownership before retrying only release with the applicable identity and options."
                )
            } else {
                json!(
                    "Wait for the lock to clear, then retry the release step with the original identity and applicable release options."
                )
            };
        }
    }
    context["why"] = json!(format!(
        "{completed} completed for {id} before the following step failed. No rollback was performed.{original_why}"
    ));
    context["next_steps"] = json!(steps);
    crate::collections::refusal(detail, exit_code, context)
}

#[cfg(test)]
#[path = "../tests/support/ownership_unit.rs"]
pub(crate) mod tests;
