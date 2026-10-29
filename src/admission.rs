//! Bounded, non-writing admission of the native canonical mutation slice.
use crate::{CreateItem, ItemDocument, PmRustError, UpdateItem, Workspace};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Read,
    path::{Component, Path, PathBuf},
};

/// Hard ceilings, reported on successful previews and typed refusals.
#[derive(Clone, Copy, Debug, Serialize)]
pub struct AdmissionLimits {
    /// Maximum complete UTF-8 JSON request bytes.
    pub input_bytes: usize,
    /// Maximum UTF-8 bytes for either an ID or the explicit author.
    pub identity_bytes: usize,
    /// One extra byte may be read solely to detect an over-limit source or stdin.
    pub over_limit_probe_bytes: u8,
    /// Maximum intents in one request.
    pub intents: usize,
    /// Maximum aggregate source bytes per observation, including configuration.
    pub source_bytes: u64,
    /// Maximum decoded source items.
    pub source_rows: usize,
    /// Maximum visited directory entries per observation.
    pub source_entries: usize,
    /// Maximum nested item directory depth.
    pub source_depth: usize,
    /// Maximum complete response bytes in either JSON or TOON, excluding one newline.
    pub output_bytes: usize,
}

/// Fixed protocol-v1 limits; callers split batches or select a smaller tracker.
pub const ADMISSION_LIMITS: AdmissionLimits = AdmissionLimits {
    input_bytes: 262_144,
    identity_bytes: 128,
    over_limit_probe_bytes: 1,
    intents: 100,
    source_bytes: 16_777_216,
    source_rows: 10_000,
    source_entries: 100_000,
    source_depth: 32,
    output_bytes: 65_536,
};

/// Canonical operation discriminator. Unknown kinds refuse the entire request.
#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AdmissionKind {
    /// Explicit-ID creation, adopting an existing target like the core coordinator.
    Create,
    /// Scalar replacement of an existing item.
    Update,
    /// Other core or extension operations are outside this native slice.
    #[serde(other)]
    Unsupported,
}

/// Strict canonical SDK options for this native slice; no importer mappings.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AdmissionOptions {
    /// Human-readable title; blank values receive a required-field diagnostic.
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub title: Option<String>,
    /// Description replacement; empty is meaningful.
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub description: Option<String>,
    /// Built-in canonical type, required for create and unsupported on update.
    #[serde(rename = "type")]
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub item_type: Option<String>,
    /// Canonical nonterminal, nonblocked runtime status.
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub status: Option<String>,
    /// Integer priority in 0..=4; other u8 values receive a row diagnostic.
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub priority: Option<u8>,
    /// Canonical comma-separated tag replacement, using native normalization.
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub tags: Option<String>,
    /// Markdown body replacement; empty is meaningful.
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub body: Option<String>,
    /// Creates must explicitly select progressive option readiness.
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub create_mode: Option<String>,
    /// Creates explicitly assert no parent with the SDK sentinel `none`.
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub parent: Option<String>,
    /// Creates explicitly assert no assignee with the SDK sentinel `none`.
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub assignee: Option<String>,
    /// Creates explicitly bypass similarity advisory policy; must be true.
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub allow_duplicate: Option<bool>,
}

/// Distinguishes an omitted optional option from an explicitly invalid JSON null.
fn present<'de, D, T>(reader: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(reader).map(Some)
}

/// One canonical mutation intent, evaluated in request order.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AdmissionIntent {
    /// Canonical core operation.
    pub op: AdmissionKind,
    /// Full canonical identifier; no implicit prefix expansion.
    pub id: String,
    /// Only the declared scalar slice is accepted.
    pub options: AdmissionOptions,
}

/// Typed SDK batch. Every call is a fresh evaluation, without replay journals.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AdmissionBatch {
    /// Explicit mutation author; no ambient identity or focus is inherited.
    pub author: String,
    /// Dense ordered batch with 1..=100 intents.
    pub mutations: Vec<AdmissionIntent>,
}

/// A bounded refusal with a stable code and actionable remedy, without source data.
#[derive(Clone, Debug, Serialize, thiserror::Error)]
#[error("{code}; {required}")]
pub struct AdmissionRefusal {
    /// Stable refusal identity.
    pub code: &'static str,
    /// What the caller must do before retrying.
    pub required: &'static str,
    /// All protocol byte and row ceilings, including on failure.
    pub limits: AdmissionLimits,
}

/// Builds only constant diagnostics, never echoing arbitrary source content.
fn refuse(code: &'static str, required: &'static str) -> AdmissionRefusal {
    AdmissionRefusal {
        code,
        required,
        limits: ADMISSION_LIMITS,
    }
}

/// Malformed, ambiguous or unsupported JSON shape.
fn invalid() -> AdmissionRefusal {
    refuse(
        "admission_invalid_request",
        "Use the documented strict JSON batch, full IDs of at most 128 bytes, an explicit author, and 1..100 create/update intents within input_bytes.",
    )
}

impl AdmissionBatch {
    /// Parses one strict bounded JSON document, refusing duplicate keys and unknown fields.
    ///
    /// # Errors
    /// Returns a typed refusal before workspace discovery or any source access.
    pub fn from_json(bytes: &[u8]) -> Result<Self, AdmissionRefusal> {
        if bytes.len() > ADMISSION_LIMITS.input_bytes {
            return Err(invalid());
        }
        let batch: Self = crate::strict_json::from_slice(bytes).map_err(|_| invalid())?;
        batch.validate()?;
        Ok(batch)
    }

    /// Validates the same boundary for typed SDK callers.
    fn validate(&self) -> Result<(), AdmissionRefusal> {
        if self.author.trim().is_empty()
            || self.author.len() > ADMISSION_LIMITS.identity_bytes
            || self.mutations.is_empty()
            || self.mutations.len() > ADMISSION_LIMITS.intents
        {
            return Err(invalid());
        }
        for intent in &self.mutations {
            if matches!(intent.op, AdmissionKind::Unsupported) {
                return Err(refuse(
                    "admission_unsupported_intent",
                    "Use create/update from the declared native slice; use the published pm SDK for other operation kinds.",
                ));
            }
            if intent.id.is_empty()
                || intent.id.len() > ADMISSION_LIMITS.identity_bytes
                || intent.id.starts_with('-')
                || intent.id.ends_with('-')
                || !intent
                    .id
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
            {
                return Err(invalid());
            }
        }
        Ok(())
    }
}

/// Counts serialized SDK bytes without allocating a second unbounded request.
#[derive(Default)]
struct InputCounter(usize);

impl std::io::Write for InputCounter {
    /// Stops at the request ceiling even when one supplied string is enormous.
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() > ADMISSION_LIMITS.input_bytes - self.0 {
            return Err(std::io::Error::other("admission input byte limit"));
        }
        self.0 += bytes.len();
        Ok(bytes.len())
    }

    /// Counting has no pending buffered output.
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Per-intent result. Refused targets suppress subsequent intents on that target.
#[derive(Clone, Debug, Serialize)]
pub struct AdmissionDiagnostic {
    /// Zero-based position in the request.
    pub index: usize,
    /// Canonical operation.
    pub op: AdmissionKind,
    /// Canonical target identity.
    pub id: String,
    /// `admitted`, `unchanged`, `refused`, or `skipped`.
    pub state: &'static str,
    /// Stable admission or refusal code.
    pub code: &'static str,
    /// Missing or unsupported option, when one is identifiable.
    pub field: Option<&'static str>,
    /// Remedy; absent on admission.
    pub required: Option<&'static str>,
}

/// Compact evidence. No proposed bodies or arbitrary error text are emitted.
#[derive(Clone, Debug, Serialize)]
pub struct AdmissionPreview {
    /// Versioned native protocol.
    pub protocol: &'static str,
    /// True only when every row is admitted or currently unchanged.
    pub ok: bool,
    /// All hard byte and row limits.
    pub limits: AdmissionLimits,
    /// Canonical request size for SDK callers, original bytes for the JSON boundary.
    pub input_bytes: usize,
    /// Observed decoded source items.
    pub source_rows: usize,
    /// Observed source bytes per observation; the final check reads these again.
    pub source_bytes: u64,
    /// Observed visited directory entries per observation.
    pub source_entries: usize,
    /// Ordered per-intent verdicts, without early row-failure truncation.
    pub diagnostics: Vec<AdmissionDiagnostic>,
    /// Limits on semantic and concurrency claims.
    pub unresolved_commit_constraints: [&'static str; 3],
}

/// Native reader refusing unsafe sources and exceeded ceilings without partial results.
#[derive(Default)]
struct Source {
    /// Test-only location for deterministic competing-writer injection.
    #[cfg(test)]
    root: PathBuf,
    /// Every admitted file, keyed by tracker-relative path.
    files: BTreeMap<PathBuf, Vec<u8>>,
    /// Decoded native documents, retaining every foreign field.
    items: BTreeMap<String, ItemDocument>,
    /// Captured registry, shared with native read and apply classification.
    lifecycle: Option<crate::list_registry::Lifecycle>,
    /// Total source bytes admitted before decode.
    bytes: u64,
    /// Directory entries visited, including ignored entries.
    entries: usize,
}

/// Uniform non-sensitive source refusal.
fn source_error() -> AdmissionRefusal {
    refuse(
        "admission_source",
        "Use readable regular UTF-8 native items and tracker-relative regular JSON schema files; repair duplicate IDs or source integrity before retrying.",
    )
}

/// Uniform source budget refusal.
fn source_limit() -> AdmissionRefusal {
    refuse(
        "admission_source_limit",
        "Select a smaller tracker within source_bytes, source_rows, source_entries and source_depth; no partial admission is returned.",
    )
}

impl Source {
    /// Opens only regular sources and bounds reads even if a file grows after stat.
    fn read(&mut self, root: &Path, relative: &Path) -> Result<Vec<u8>, AdmissionRefusal> {
        guard_relative(relative)?;
        let mut path = root.to_path_buf();
        for component in relative.components() {
            path.push(component);
            if path.is_symlink() {
                return Err(source_error());
            }
        }
        #[cfg(test)]
        tests::checkpoint(root);
        let (mut file, size) =
            crate::changes::open_regular_history(&path).map_err(|_| source_error())?;
        if size > ADMISSION_LIMITS.source_bytes - self.bytes {
            return Err(source_limit());
        }
        self.read_bytes(&mut file).inspect(|bytes| {
            self.files.insert(relative.to_path_buf(), bytes.clone());
            #[cfg(test)]
            tests::checkpoint(root);
        })
    }

    /// Detects growth beyond stat admission and preserves read failures.
    fn read_bytes(&mut self, reader: &mut dyn Read) -> Result<Vec<u8>, AdmissionRefusal> {
        let mut bytes = Vec::new();
        reader
            .take(
                ADMISSION_LIMITS.source_bytes - self.bytes
                    + u64::from(ADMISSION_LIMITS.over_limit_probe_bytes),
            )
            .read_to_end(&mut bytes)
            .map_err(|_| source_error())?;
        self.bytes += bytes.len() as u64;
        if self.bytes > ADMISSION_LIMITS.source_bytes {
            return Err(source_limit());
        }
        Ok(bytes)
    }

    /// Reads configuration through the same guarded, budgeted byte boundary as items.
    fn json(&mut self, root: &Path, relative: &Path) -> Result<Value, AdmissionRefusal> {
        crate::strict_json::from_slice(&self.read(root, relative)?).map_err(|_| source_error())
    }

    /// Visits native item directories without Git, recovery, locks or scratch writes.
    fn walk(&mut self, root: &Path, relative: &Path, depth: usize) -> Result<(), AdmissionRefusal> {
        if depth > ADMISSION_LIMITS.source_depth {
            return Err(source_limit());
        }
        let mut entries = fs::read_dir(root.join(relative)).map_err(|_| source_error())?;
        self.walk_entries(root, relative, depth, &mut entries)
    }

    /// Streams a bounded directory iterator while preserving enumeration failures.
    fn walk_entries(
        &mut self,
        root: &Path,
        relative: &Path,
        depth: usize,
        entries: &mut dyn Iterator<Item = std::io::Result<fs::DirEntry>>,
    ) -> Result<(), AdmissionRefusal> {
        for entry in entries {
            self.entries += 1;
            if self.entries > ADMISSION_LIMITS.source_entries {
                return Err(source_limit());
            }
            let entry = entry.map_err(|_| source_error())?;
            let kind = entry.file_type();
            self.visit_entry(root, relative, depth, &entry, kind)?;
        }
        Ok(())
    }

    /// Uses the actual directory entry and admits only canonical item source files.
    fn visit_entry(
        &mut self,
        root: &Path,
        relative: &Path,
        depth: usize,
        entry: &fs::DirEntry,
        kind: std::io::Result<fs::FileType>,
    ) -> Result<(), AdmissionRefusal> {
        let name = entry.file_name();
        if depth == 0
            && crate::workspace::NON_ITEM_DIRECTORIES.contains(&name.to_string_lossy().as_ref())
        {
            return Ok(());
        }
        let kind = kind.map_err(|_| source_error())?;
        let path = relative.join(name);
        if kind.is_symlink() {
            return Err(source_error());
        }
        if kind.is_dir() {
            self.walk(root, &path, depth + 1)?;
        } else if depth > 0 && path.extension().is_some_and(|ext| ext == "toon") {
            if self.items.len() >= ADMISSION_LIMITS.source_rows {
                return Err(source_limit());
            }
            let bytes = self.read(root, &path)?;
            let text = std::str::from_utf8(&bytes).map_err(|_| source_error())?;
            let item = crate::item::decode_item(&path, text).map_err(|_| source_error())?;
            if self.items.insert(item.metadata.id.clone(), item).is_some() {
                return Err(source_error());
            }
        }
        Ok(())
    }

    /// Captures fields/types/workflows too, so unsupported contracts never silently admit.
    fn load(root: &Path) -> Result<(Self, bool), AdmissionRefusal> {
        let mut source = Self::default();
        #[cfg(test)]
        {
            source.root = root.to_path_buf();
        }
        let settings = source.json(root, Path::new("settings.json"))?;
        crate::mutation::validate_preview_settings(&source.files[Path::new("settings.json")])
            .map_err(|_| source_error())?;
        let mut unsupported = [
            &settings["schema"]["fields"],
            &settings["schema"]["type_workflows"],
            &settings["item_types"]["definitions"],
            &settings["validation"]["metadata_required_fields"],
        ]
        .iter()
        .any(|v| declares_contract(v));
        let mut statuses = Value::Null;
        let mut workflows = Value::Null;
        for (name, key) in [
            ("types", "definitions"),
            ("fields", "fields"),
            ("statuses", "statuses"),
            ("workflows", "type_workflows"),
        ] {
            let fallback = format!("schema/{name}.json");
            let relative = Path::new(
                settings["schema"]["files"][name]
                    .as_str()
                    .unwrap_or(&fallback),
            );
            // Never probe outside the tracker, including missing configured paths.
            guard_relative(relative)?;
            let metadata = fs::symlink_metadata(root.join(relative));
            #[cfg(test)]
            let metadata = tests::schema_metadata(metadata);
            if optional_source(metadata)? {
                let schema = source.json(root, relative)?;
                if name == "statuses" {
                    statuses = schema.clone();
                }
                if name == "workflows" {
                    workflows = schema.clone();
                }
                if !schema.is_object() && !schema.is_array() {
                    return Err(source_error());
                }
                if name != "statuses"
                    && declares_contract(schema.get(key).unwrap_or(if schema.is_array() {
                        &schema
                    } else {
                        &Value::Null
                    }))
                {
                    unsupported = true;
                }
            }
        }
        source.lifecycle = Some(crate::list_registry::Lifecycle::from_values(
            &settings, &statuses, &workflows,
        ));
        source.walk(root, Path::new(""), 0)?;
        Ok((source, unsupported))
    }
}

/// Admits only lexical tracker-relative paths before any filesystem probe.
fn guard_relative(relative: &Path) -> Result<(), AdmissionRefusal> {
    if relative
        .components()
        .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err(source_error());
    }
    Ok(())
}

/// Missing optional schema files use defaults; every other metadata error refuses.
fn optional_source(metadata: std::io::Result<fs::Metadata>) -> Result<bool, AdmissionRefusal> {
    match metadata {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(_) => Err(source_error()),
    }
}

/// Refuses malformed as well as nonempty unsupported declarations, preserving reads.
fn declares_contract(value: &Value) -> bool {
    !value.is_null() && value.as_array().is_none_or(|entries| !entries.is_empty())
}

/// Required-field row refusal, with a concrete repair rather than parsing error prose.
fn required(field: &'static str) -> (&'static str, Option<&'static str>, &'static str) {
    (
        "required_field",
        Some(field),
        "Supply missing fields. Preserve any supplied strict readiness, relationship, assignee or similarity constraint by using the published SDK; the native slice needs explicit progressive mode, empty scope and similarity bypass.",
    )
}

/// Internal bounded row error: code, option and remedy.
type RowError = (&'static str, Option<&'static str>, &'static str);

/// Runs apply's preparation functions; only the persistence phase is absent.
fn prepare(
    source: &Source,
    intent: &AdmissionIntent,
    author: &str,
    before: Option<&ItemDocument>,
) -> Result<ItemDocument, RowError> {
    let options = &intent.options;
    let lifecycle = source.lifecycle.as_ref().ok_or((
        "invalid_schema",
        None,
        "Repair the runtime status registry and retry.",
    ))?;
    if let Some(status) = &options.status
        && (!lifecycle.known(status)
            || lifecycle.canonical(status) != *status
            || lifecycle.terminal(status)
            || lifecycle.role(status, "blocked")
            || !(lifecycle.role(status, "active") || lifecycle.role(status, "draft")))
    {
        return Err((
            "unsupported_status",
            Some("status"),
            "Use a canonical active/draft runtime status; use the published pm SDK for aliases, terminal or blocked transitions.",
        ));
    }
    match intent.op {
        AdmissionKind::Create => prepare_create(source, intent, author),
        AdmissionKind::Update => prepare_update(lifecycle, intent, author, before),
        AdmissionKind::Unsupported => Err(("unsupported_intent", None, "Use create/update.")),
    }
}

/// Applies the native scalar-create contract after shared registry admission.
fn prepare_create(
    source: &Source,
    intent: &AdmissionIntent,
    author: &str,
) -> Result<ItemDocument, RowError> {
    let options = &intent.options;
    let timestamp = Some(crate::current_timestamp());
    let tags = options
        .tags
        .as_ref()
        .map(|csv| csv.split(',').map(str::to_owned).collect());
    let title = options
        .title
        .as_ref()
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| required("title"))?;
    let item_type = options
        .item_type
        .as_ref()
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| required("type"))?;
    let status = options.status.clone().ok_or_else(|| required("status"))?;
    let priority = options.priority.ok_or_else(|| required("priority"))?;
    for (field, value, expected) in [
        ("createMode", options.create_mode.as_deref(), "progressive"),
        ("parent", options.parent.as_deref(), "none"),
        ("assignee", options.assignee.as_deref(), "none"),
    ] {
        if value != Some(expected) {
            return Err(required(field));
        }
    }
    if options.allow_duplicate != Some(true) {
        return Err(required("allowDuplicate"));
    }
    crate::mutation::prepare_create_preview(
        source.files.get(Path::new("settings.json")).ok_or((
            "invalid_schema",
            None,
            "Restore settings.json and retry.",
        ))?,
        CreateItem {
            id: intent.id.clone(),
            title: title.clone(),
            description: options.description.clone().unwrap_or_default(),
            item_type: item_type.clone(),
            status,
            priority,
            tags: tags.unwrap_or_default(),
            body: options.body.clone().unwrap_or_default(),
            author: author.to_owned(),
            timestamp,
            message: None,
            provenance_role: None,
            force_stale_lock: false,
        },
    )
    .map_err(|error| native_error(&error))
}

/// Applies native scalar-update preparation to the current candidate document.
fn prepare_update(
    lifecycle: &crate::list_registry::Lifecycle,
    intent: &AdmissionIntent,
    author: &str,
    before: Option<&ItemDocument>,
) -> Result<ItemDocument, RowError> {
    let options = &intent.options;
    let timestamp = Some(crate::current_timestamp());
    let tags = options
        .tags
        .as_ref()
        .map(|csv| csv.split(',').map(str::to_owned).collect());
    if options.item_type.is_some()
        || options.create_mode.is_some()
        || options.parent.is_some()
        || options.assignee.is_some()
        || options.allow_duplicate.is_some()
    {
        return Err((
            "unsupported_options",
            None,
            "Update only title, description, status, priority, tags or body; use the published pm SDK for scope and type changes.",
        ));
    }
    let before = before.ok_or((
        "item_not_found",
        None,
        "Read the current target ID through the SDK, or create it earlier in this batch.",
    ))?;
    if options.title.as_ref().is_some_and(|s| s.trim().is_empty()) {
        return Err(required("title"));
    }
    if options.status.is_some()
        && (lifecycle.terminal(&before.metadata.status)
            || lifecycle.role(&before.metadata.status, "blocked")
            || before.metadata.extra.contains_key("dependencies")
            || before.metadata.extra.contains_key("blocked_by"))
    {
        return Err((
            "unsupported_transition",
            Some("status"),
            "Use the published pm SDK for reopening, unblocking or dependency-sensitive status transitions.",
        ));
    }
    crate::mutation::prepare_update_preview(
        before,
        UpdateItem {
            id: intent.id.clone(),
            title: options.title.clone(),
            description: options.description.clone(),
            status: options.status.clone(),
            priority: options.priority,
            tags,
            body: options.body.clone(),
            author: author.to_owned(),
            timestamp,
            message: None,
            provenance_role: None,
            force_stale_lock: false,
        },
    )
    .map_err(|error| native_error(&error))
}

/// Converts native field/encoding failures to bounded stable row diagnostics.
fn native_error(error: &PmRustError) -> RowError {
    match error {
        PmRustError::InvalidCreateRequest { .. } | PmRustError::InvalidMutationRequest { .. } => (
            "invalid_fields",
            None,
            "Use canonical built-in types, the configured ID prefix, priority 0..4 and at least one update field.",
        ),
        _ => (
            "invalid_document",
            None,
            "Use values supported by the native document encoder and retry.",
        ),
    }
}

/// Evaluates independent targets in order, suppressing dependents of refused targets.
fn evaluate(
    source: &Source,
    batch: &AdmissionBatch,
    unsupported: bool,
) -> (Vec<AdmissionDiagnostic>, bool) {
    let mut current = source.items.clone();
    let mut refused = BTreeSet::new();
    let mut diagnostics = Vec::new();
    for (index, intent) in batch.mutations.iter().enumerate() {
        #[cfg(test)]
        tests::checkpoint(&source.root);
        let before = current.get(&intent.id);
        let mut diagnostic = AdmissionDiagnostic {
            index,
            op: intent.op,
            id: intent.id.clone(),
            state: "admitted",
            code: "admitted",
            field: None,
            required: None,
        };
        if refused.contains(&intent.id) {
            diagnostic.state = "skipped";
            diagnostic.code = "prior_target_refused";
            diagnostic.required = Some(
                "Repair the earlier refused intent on this target, then preview the complete batch again.",
            );
        } else if matches!(intent.op, AdmissionKind::Create)
            && before.is_some_and(|item| {
                item.metadata
                    .extra
                    .get("close_reason")
                    .and_then(Value::as_str)
                    .is_some_and(|reason| {
                        reason.starts_with("Compensated: interrupted bulk transaction ")
                    })
            })
        {
            refused.insert(intent.id.clone());
            diagnostic.state = "refused";
            diagnostic.code = "compensated_create";
            diagnostic.required = Some(
                "Review the current target and its native history through the published SDK; explicitly reconcile compensation before retrying.",
            );
        } else if matches!(intent.op, AdmissionKind::Create) && before.is_some() {
            diagnostic.state = "unchanged";
            diagnostic.code = "existing_create_adopted";
        } else {
            let result = if unsupported {
                Err((
                    "unsupported_schema",
                    None,
                    "Use the published pm SDK for declared custom fields, type definitions or type workflows; native admission supports the built-in scalar contract.",
                ))
            } else {
                prepare(source, intent, &batch.author, before)
            };
            match result {
                Ok(document) => {
                    if let Some(before) = before {
                        let mut comparison = document.clone();
                        comparison
                            .metadata
                            .updated_at
                            .clone_from(&before.metadata.updated_at);
                        if comparison == *before {
                            diagnostic.state = "unchanged";
                            diagnostic.code = "current_values_match";
                        }
                    }
                    current.insert(intent.id.clone(), document);
                }
                Err((code, field, remedy)) => {
                    refused.insert(intent.id.clone());
                    diagnostic.state = "refused";
                    diagnostic.code = code;
                    diagnostic.field = field;
                    diagnostic.required = Some(remedy);
                }
            }
        }
        diagnostics.push(diagnostic);
    }
    (diagnostics, refused.is_empty())
}

/// Checks both generated encodings before either can be delivered.
fn check_output_lengths(json: usize, toon: usize) -> Result<(), AdmissionRefusal> {
    if json > ADMISSION_LIMITS.output_bytes || toon > ADMISSION_LIMITS.output_bytes {
        return Err(refuse(
            "admission_output_limit",
            "Split the batch to fit output_bytes in both JSON and TOON; no diagnostic rows are silently omitted.",
        ));
    }
    Ok(())
}

impl Workspace {
    /// Previews ordered canonical intents with the same preparation as native apply.
    ///
    /// No source or scratch files are written, no recovery runs, and no locks,
    /// Node, Git, hooks or network are used. Two bounded source observations
    /// detect visible interleaving; this is neither atomic isolation nor a future
    /// commit reservation. Apply must re-read targets and growing history revisions
    /// immediately before each write; ordinary SDK writers do not share a workspace
    /// transaction lock. The remaining check/write window is unresolved.
    ///
    /// # Errors
    /// Returns typed boundary, source, resource or observed-change refusals.
    pub fn preview_item_mutations(
        &self,
        batch: &AdmissionBatch,
    ) -> Result<AdmissionPreview, AdmissionRefusal> {
        batch.validate()?;
        let mut counter = InputCounter::default();
        serde_json::to_writer(&mut counter, batch).map_err(|_| invalid())?;
        let input_bytes = counter.0;
        let (source, unsupported) = Source::load(self.pm_root())?;
        let (diagnostics, ok) = evaluate(&source, batch, unsupported);
        #[cfg(test)]
        tests::checkpoint(self.pm_root());
        let (after, _) = Source::load(self.pm_root())?;
        if source.files != after.files {
            return Err(refuse(
                "admission_source_changed",
                "Tracker changed during preview; retry against current SDK state. A preview never reserves a later commit.",
            ));
        }
        let preview = AdmissionPreview {
            protocol: "pm-rust/item-admission/v1",
            ok,
            limits: ADMISSION_LIMITS,
            input_bytes,
            source_rows: source.items.len(),
            source_bytes: source.bytes,
            source_entries: source.entries,
            diagnostics,
            unresolved_commit_constraints: [
                "concurrent_tracker_changes_and_check_write_window",
                "extension_guards_hooks_and_pending_recovery",
                "native_slice_only_no_atomic_batch_commit_or_replay",
            ],
        };
        let value = crate::ListOutput::from(serde_json::json!(&preview));
        check_output_lengths(
            crate::stringify_json(&preview, false).len(),
            crate::toon_output::render_output(&value).len(),
        )
        .map(|()| preview)
    }
}

#[cfg(test)]
#[path = "../tests/support/admission_unit.rs"]
mod tests;
