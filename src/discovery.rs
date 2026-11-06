//! Bounded discovery of the compiled native product, independent of tracker state.
//!
//! Parser metadata is shared with execution. Evidence describes tested slices,
//! never compatibility inferred from a version or permission to execute a write.

use crate::native_adapter::{AdapterCapability, AdapterManifest, check_adapter_manifest};
use clap::CommandFactory;
use serde::{Deserialize, Serialize};

/// Exact, independently versioned discovery wire identity.
pub const DISCOVERY_PROTOCOL: &str = "pm-rust/capabilities/v1";
/// Hard encoded response ceiling, including its trailing newline.
pub const DISCOVERY_MAX_BYTES: u32 = 131_072;
/// Hard untrusted request admission ceiling, before JSON decoding.
pub const DISCOVERY_INPUT_BYTES: usize = 8192;

/// One strict discovery input; no filesystem or process discovery is performed.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DiscoveryRequest {
    /// Must equal [`DISCOVERY_PROTOCOL`].
    pub protocol: String,
    /// Exact compiled command name, or all operations when absent.
    pub operation: Option<String>,
    /// Complete JSON and TOON response ceiling; 1024 through 131072 bytes.
    pub output_bytes: u32,
    /// Optional caller-supplied authorization declaration, not a live probe.
    pub adapter: Option<AdapterManifest>,
}

impl Default for DiscoveryRequest {
    fn default() -> Self {
        Self {
            protocol: DISCOVERY_PROTOCOL.into(),
            operation: None,
            output_bytes: DISCOVERY_MAX_BYTES,
            adapter: None,
        }
    }
}

/// Stable discovery refusals; none dispatch a command or touch a tracker.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DiscoveryRefusalCode {
    /// Invalid shape, duplicate key, size, or numeric domain.
    InvalidRequest,
    /// Unknown wire revision.
    IncompatibleProtocol,
    /// Operation is not in the evidenced compiled inventory.
    UnsupportedOperation,
    /// Caller-supplied adapter declaration fails the existing ABI admission.
    InvalidAdapter,
    /// A complete response cannot fit; no partial availability is returned.
    OutputLimit,
    /// Compiled parser and evidenced inventory disagree; fail closed.
    ContractDrift,
}

/// Typed recovery bundle, without echoing untrusted strings or private paths.
#[derive(Clone, Debug, Deserialize, Serialize, thiserror::Error)]
#[serde(deny_unknown_fields)]
#[error("{code:?}; {required}")]
pub struct DiscoveryRefusal {
    /// Stable machine-readable classification.
    pub code: DiscoveryRefusalCode,
    /// Concrete remedy, safe to display to an agent.
    pub required: String,
    /// Argument vector for a supported, read-only recovery invocation.
    pub retry_args: Vec<String>,
}

/// Constructs recovery through the same typed refusal for CLI and SDK callers.
fn refuse(code: DiscoveryRefusalCode) -> DiscoveryRefusal {
    let required = match code {
        DiscoveryRefusalCode::InvalidRequest => {
            "Use the strict DiscoveryRequest fields, output_bytes 1024..131072, and at most 8192 input bytes."
        }
        DiscoveryRefusalCode::IncompatibleProtocol => {
            "Use protocol pm-rust/capabilities/v1; upgrade caller and product together for another revision."
        }
        DiscoveryRefusalCode::UnsupportedOperation => {
            "Choose an exact operation from capabilities; unlisted commands and flags are unsupported. Use the published pm CLI for the wider host surface."
        }
        DiscoveryRefusalCode::InvalidAdapter => {
            "Supply a valid pm-native-adapter/1 manifest with unique granted capabilities and an admitted frame limit; no transport was opened."
        }
        DiscoveryRefusalCode::OutputLimit => {
            "Raise output_bytes to 131072 or select one operation; incomplete availability is never returned."
        }
        DiscoveryRefusalCode::ContractDrift => {
            "Use a build whose native parser and invocation evidence agree; report this contract drift before executing operations."
        }
    };
    DiscoveryRefusal {
        code,
        required: required.into(),
        retry_args: vec!["capabilities".into(), "--json".into()],
    }
}

impl DiscoveryRequest {
    /// Decodes exactly one bounded strict JSON request, rejecting duplicate keys.
    ///
    /// # Errors
    /// Returns a typed refusal before source access for malformed or unsupported inputs.
    pub fn from_json(bytes: &[u8]) -> Result<Self, DiscoveryRefusal> {
        if bytes.len() > DISCOVERY_INPUT_BYTES {
            return Err(refuse(DiscoveryRefusalCode::InvalidRequest));
        }
        let value: serde_json::Value = crate::strict_json::from_slice(bytes)
            .map_err(|_| refuse(DiscoveryRefusalCode::InvalidRequest))?;
        if !value.is_object() {
            return Err(refuse(DiscoveryRefusalCode::InvalidRequest));
        }
        if value
            .get("adapter")
            .is_some_and(|adapter| !adapter.is_null() && !adapter.is_object())
        {
            return Err(refuse(DiscoveryRefusalCode::InvalidAdapter));
        }
        if let Some(adapter) = value.get("adapter").filter(|adapter| !adapter.is_null()) {
            serde_json::from_value::<AdapterManifest>(adapter.clone())
                .map_err(|_| refuse(DiscoveryRefusalCode::InvalidAdapter))?;
        }
        let request: Self = serde_json::from_value(value)
            .map_err(|_| refuse(DiscoveryRefusalCode::InvalidRequest))?;
        request.validate()?;
        Ok(request)
    }

    /// Converts parsed CLI values through the same strict request admission schema.
    ///
    /// # Errors
    /// Returns the same typed refusal as [`Self::from_json`].
    pub fn from_cli(
        protocol: String,
        operation: Option<String>,
        output_bytes: u32,
        adapter: Option<&str>,
    ) -> Result<Self, DiscoveryRefusal> {
        let adapter = adapter
            .map(|text| {
                if text.len() > DISCOVERY_INPUT_BYTES {
                    return Err(refuse(DiscoveryRefusalCode::InvalidRequest));
                }
                let value: serde_json::Value = crate::strict_json::from_str(text)
                    .map_err(|_| refuse(DiscoveryRefusalCode::InvalidAdapter))?;
                if !value.is_object() {
                    return Err(refuse(DiscoveryRefusalCode::InvalidAdapter));
                }
                serde_json::from_value::<AdapterManifest>(value)
                    .map_err(|_| refuse(DiscoveryRefusalCode::InvalidAdapter))
            })
            .transpose()?;
        Self::from_json(
            &crate::stringify_json(
                &Self {
                    protocol,
                    operation,
                    output_bytes,
                    adapter,
                },
                false,
            )
            .into_bytes(),
        )
    }

    /// Applies semantic domains also when an embedding constructs the typed request.
    fn validate(&self) -> Result<(), DiscoveryRefusal> {
        if self.protocol.len() > 64
            || self
                .operation
                .as_ref()
                .is_some_and(|name| name.is_empty() || name.len() > 64)
            || !(1024..=DISCOVERY_MAX_BYTES).contains(&self.output_bytes)
        {
            return Err(refuse(DiscoveryRefusalCode::InvalidRequest));
        }
        if self.protocol != DISCOVERY_PROTOCOL {
            return Err(refuse(DiscoveryRefusalCode::IncompatibleProtocol));
        }
        if let Some(adapter) = &self.adapter {
            check_adapter_manifest(adapter)
                .map_err(|_| refuse(DiscoveryRefusalCode::InvalidAdapter))?;
            if adapter.capabilities.len() > 5
                || adapter
                    .capabilities
                    .iter()
                    .enumerate()
                    .any(|(i, value)| adapter.capabilities[..i].contains(value))
            {
                return Err(refuse(DiscoveryRefusalCode::InvalidAdapter));
            }
        }
        Ok(())
    }
}

/// Omits the documented false default without changing typed round trips.
#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_false(value: &bool) -> bool {
    !value
}

/// Omits the documented zero lower arity bound for value-free flags.
#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_zero(value: &usize) -> bool {
    *value == 0
}

/// Parser-derived positional or flag contract; aliases retain their exact spelling.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ArgumentContract {
    /// Internal stable parser identity.
    pub id: String,
    /// Positional index, or null for an option.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position: Option<usize>,
    /// Accepted long, short and alias spellings, including hidden aliases.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub spellings: Vec<String>,
    /// Minimum values per occurrence.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub min_values: usize,
    /// Maximum values per occurrence; null denotes an unbounded variadic argument.
    pub max_values: Option<usize>,
    /// Separator for a delimited value list, when the parser supplies one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value_delimiter: Option<char>,
    /// Whether a value may begin with a hyphen rather than introduce another flag.
    #[serde(default, skip_serializing_if = "is_false")]
    pub allow_hyphen_values: bool,
    /// Whether the parser requires the argument.
    #[serde(default, skip_serializing_if = "is_false")]
    pub required: bool,
    /// Parser action: `Set`, `Append`, `SetTrue`, `Help` or `Version`.
    pub action: String,
    /// Values used when absent.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub defaults: Vec<String>,
    /// Values used for a bare optional-value flag.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub missing_defaults: Vec<String>,
    /// Enumerated parser choices; empty means runtime validation applies.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub choices: Vec<String>,
    /// Conflicting parser argument identities.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub conflicts: Vec<String>,
    /// Hidden fixture or diagnostic control, still accepted by this build.
    #[serde(default, skip_serializing_if = "is_false")]
    pub hidden: bool,
    /// Runtime value domain and purpose from the executable's documentation.
    pub description: String,
}

/// Evidence classification, never a full-oracle compatibility claim.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Provenance {
    /// Only the declared native slice is compared with the pinned published oracle.
    OracleSlice,
    /// Independent native product addition with invocation evidence.
    NativeOnly,
}

/// One executable command with real invocation and optional oracle receipts.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OperationContract {
    /// Exact canonical command.
    pub name: String,
    /// Additional accepted command paths, including canonical recovery namespaces.
    pub aliases: Vec<String>,
    /// Existing public SDK operations used by this command.
    pub sdk: Vec<String>,
    /// Oracle slice or native-only addition.
    pub provenance: Provenance,
    /// Executed invocation receipt covering this operation.
    pub invocation_receipt: String,
    /// Differential receipt for the declared slice, absent for native additions.
    pub parity_receipt: Option<String>,
    /// All accepted parser arguments, including help and hidden controls.
    pub arguments: Vec<ArgumentContract>,
    /// Repeating a scalar option replaces its previous value when true.
    pub repeated_scalar_last_wins: bool,
    /// Actual supported wire encodings.
    pub encodings: Vec<String>,
    /// One document, newline-delimited JSON pages, or help text.
    pub framing: String,
    /// Supported output selection, budget, and continuation dimensions.
    pub dimensions: Vec<String>,
    /// Whether some accepted invocations may write tracker state.
    pub may_mutate: bool,
    /// Explicit native subset and refusal boundary.
    pub scope: String,
    /// Recovery schema IDs; plain text/OS errors can also occur.
    pub recovery: Vec<String>,
}

/// Operation visibility at the backend boundary, independent of connectivity.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BackendContract {
    /// Native directory or caller-injected authorized byte adapter.
    pub kind: String,
    /// `compiled`, `requires_adapter`, or `declared_only`; never connectivity or permission.
    pub availability: String,
    /// Only local or explicitly granted operations, never speculative methods.
    pub operations: Vec<String>,
    /// Preconditions to execute; discovery itself performs no backend read.
    pub required: String,
}

/// Machine-readable field shape of an existing typed refusal envelope.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RecoverySchema {
    /// Stable discovery-local schema ID referenced by operations.
    pub id: String,
    /// Stream carrying the structured envelope.
    pub channel: String,
    /// Required top-level field names and their wire types.
    pub required: Vec<[String; 2]>,
    /// Optional fields, retained as opaque canonical producer output.
    pub optional: Vec<[String; 2]>,
    /// Standard JSON Schema for structured envelopes; absent for text-only parser errors.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wire_schema: Option<serde_json::Value>,
    /// Path to human/actionable recovery guidance.
    pub remedy: String,
    /// Existing public Rust error type, including nested structures.
    pub sdk_type: String,
}

/// Complete bounded native inventory; it contains no tracker data or credentials.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DiscoveryResponse {
    /// Exact versioned discovery identity.
    pub protocol: String,
    /// Crate release, not an inferred compatibility guarantee.
    pub product_version: String,
    /// Baseline published oracle; an operation's scope names any newer independent pin.
    pub oracle_version: String,
    /// Unlisted methods/options are unavailable; never fall back by version.
    pub unknown_is_unsupported: bool,
    /// Discovery does not require Git, Node or an initialized tracker.
    pub runtime_requirements: Vec<String>,
    /// Scope of root options: `before_command` (not inherited global flags).
    pub root_option_scope: String,
    /// Shared root flags, distinct from per-operation arguments.
    pub root_arguments: Vec<ArgumentContract>,
    /// All selected evidenced compiled commands, without pagination or omissions.
    pub operations: Vec<OperationContract>,
    /// Local implementation and optional caller-granted adapter surface.
    pub backends: Vec<BackendContract>,
    /// Typed recovery field schemas, preserving existing producer ownership.
    pub recovery_schemas: Vec<RecoverySchema>,
    /// True: some source, parser and operating-system errors remain plain text.
    pub unstructured_errors_possible: bool,
    /// Admitted complete JSON and TOON bytes, including newline.
    pub output_bytes: u32,
}

impl DiscoveryResponse {
    /// Uses the shared native codecs; output never truncates an operation or flag.
    ///
    /// # Errors
    /// Returns a typed refusal if encoding fails or the whole response exceeds its bound.
    pub fn render(&self, json: bool) -> Result<String, DiscoveryRefusal> {
        if !(1024..=DISCOVERY_MAX_BYTES).contains(&self.output_bytes) {
            return Err(refuse(DiscoveryRefusalCode::OutputLimit));
        }
        self.check_encoded(if json {
            Ok(crate::stringify_json(self, false))
        } else {
            toon_format::encode_default(self)
        })
    }

    /// Keeps codec failures and byte admission on one tested delivery boundary.
    fn check_encoded(
        &self,
        encoded: Result<String, toon_format::ToonError>,
    ) -> Result<String, DiscoveryRefusal> {
        let text = encoded?;
        if text.len() + 1 > self.output_bytes as usize {
            return Err(refuse(DiscoveryRefusalCode::OutputLimit));
        }
        Ok(text)
    }
}

impl From<toon_format::ToonError> for DiscoveryRefusal {
    fn from(_: toon_format::ToonError) -> Self {
        refuse(DiscoveryRefusalCode::ContractDrift)
    }
}

/// Extracts every accepted spelling and parser rule from the executable's tree.
fn arguments(command: &clap::Command) -> Vec<ArgumentContract> {
    command
        .get_arguments()
        .map(|arg| {
            let mut spellings = Vec::new();
            if let Some(long) = arg.get_long() {
                spellings.push(format!("--{long}"));
            }
            spellings.extend(
                arg.get_all_aliases()
                    .unwrap_or_default()
                    .into_iter()
                    .map(|name| format!("--{name}")),
            );
            if let Some(short) = arg.get_short() {
                spellings.push(format!("-{short}"));
            }
            spellings.extend(
                arg.get_all_short_aliases()
                    .unwrap_or_default()
                    .into_iter()
                    .map(|name| format!("-{name}")),
            );
            let range = arg.get_num_args().unwrap_or_default();
            ArgumentContract {
                id: arg.get_id().as_str().into(),
                position: arg.get_index(),
                spellings,
                min_values: range.min_values(),
                max_values: (range.max_values() != usize::MAX).then_some(range.max_values()),
                value_delimiter: arg.get_value_delimiter(),
                allow_hyphen_values: arg.is_allow_hyphen_values_set(),
                required: arg.is_required_set(),
                action: format!("{:?}", arg.get_action()),
                defaults: arg
                    .get_default_values()
                    .iter()
                    .map(|value| value.to_string_lossy().into_owned())
                    .collect(),
                missing_defaults: if arg.get_id() == "validate_close" {
                    strings(&[crate::cli::VALIDATE_CLOSE_DEFAULT])
                } else {
                    Vec::new()
                },
                choices: arg
                    .get_value_parser()
                    .possible_values()
                    .into_iter()
                    .flatten()
                    .map(|value| value.get_name().into())
                    .collect(),
                conflicts: command
                    .get_arguments()
                    .filter(|other| {
                        command
                            .get_arg_conflicts_with(arg)
                            .iter()
                            .any(|value| value.get_id() == other.get_id())
                            || command
                                .get_arg_conflicts_with(other)
                                .iter()
                                .any(|value| value.get_id() == arg.get_id())
                    })
                    .map(|value| value.get_id().as_str().into())
                    .collect(),
                hidden: arg.is_hide_set(),
                description: arg.get_help().map_or_else(String::new, ToString::to_string),
            }
        })
        .collect()
}

/// Converts compact static metadata into owned SDK wire strings.
fn strings(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).into()).collect()
}

/// Evidenced metadata for each actual executable operation, failing closed on drift.
#[allow(clippy::too_many_lines)]
fn operation(command: &clap::Command) -> Result<OperationContract, DiscoveryRefusal> {
    let name = command.get_name();
    let (sdk, parity, scope): (&[&str], Option<&str>, &str) = match name {
        "capabilities" => (
            &["discovery::discover_capabilities"],
            None,
            "Compiled surface only; never probes tracker, transport or installed extensions.",
        ),
        "preview-item-mutations" => (
            &[
                "AdmissionBatch::from_json",
                "Workspace::preview_item_mutations",
            ],
            None,
            "Bounded explicit-scope progressive creates and scalar updates; custom schemas/complex lifecycle refuse; no commit guarantee.",
        ),
        "changes" => (
            &["LocalChangeAdapter::pull"],
            None,
            "One canonical event page; retained-prefix cursors; foreign records skipped with reasons.",
        ),
        "explain" => (
            &["Workspace::explain_list"],
            None,
            "Only list brief/full; measured scan bounds, unknown heap/output/time; does not execute query.",
        ),
        "adapter-check" => (
            &["native_adapter::check_adapter_transcript"],
            None,
            "Offline transcript admission only; CLI never dispatches backend calls. SDK call requires an authorized adapter.",
        ),
        "read-session" => (
            &["Workspace::read_session", "ReadSession::next_page"],
            None,
            "Immutable disk-backed full-source snapshot; bounded pages and accounted memory; no scan savings or atomic multi-writer snapshot claim.",
        ),
        "assurance" => (
            &["Workspace::assurance_read"],
            Some(
                "tests/parity_1021_contract.rs::bounded_assurance_discovery_resumes_and_refuses_stale_snapshots",
            ),
            "Read configured declaration IDs/show only; no evaluation or registry mutation.",
        ),
        "context" => (
            &["Workspace::context"],
            Some(
                "tests/orientation_differential.rs::context_intents_and_sections_match_current_oracle",
            ),
            "Oracle 2026.11.5: core orient/handoff intents, explicit sections and activity limits; refuses sources with 10000 or more items and runtime intent declarations; no dynamic extension execution.",
        ),
        "history" => (
            &["Workspace::history"],
            Some("tests/orientation_differential.rs::history_matches_oracle"),
            "Read/verify canonical history only; no redact, repair, compact or retention mutation.",
        ),
        "deps" => (
            &["Workspace::deps"],
            Some("tests/orientation_differential.rs::relationship_packets_match_oracle"),
            "Oracle 2026.11.5: read dependency tree/graph/packet only; supported direction/kind/bounds are runtime validated.",
        ),
        "search" => (
            &["Workspace::search"],
            Some("tests/search_collections_differential.rs::search_matches_oracle_small_json"),
            "Weighted native keyword search; no vector/provider search.",
        ),
        "files" | "docs" | "test" => (
            &["Workspace::collection"],
            Some("tests/search_collections_differential.rs::collection_commands_match_oracle"),
            "Linked collection read/add/remove; test execution is unsupported; no Git required.",
        ),
        "list" => (
            &["Workspace::list_page"],
            Some("tests/list_differential.rs::unbounded_list_matches_published_cli_and_golden"),
            "Only parser-listed filters; triage intent only. --cursor is a native alias; no host-wide schema/extension options.",
        ),
        "get" => (
            &["Workspace::get"],
            None,
            "Native-only flat complete ItemDocument projection. Published get entity envelopes, linked/claim/child facets and field projections are unsupported.",
        ),
        "create" => (
            &["Workspace::create"],
            Some("tests/create_contract.rs::sdk_create_matches_the_published_sdk_fixture_exactly"),
            "Explicit ID, built-in type and declared scalar fields only; no import/export/sync or relationship mutation.",
        ),
        "update" => (
            &["Workspace::update"],
            Some("tests/mutation_contract.rs::native_update_matches_the_published_bytes_exactly"),
            "Declared scalar fields only; preserved foreign metadata; no generic field/relationship mutation.",
        ),
        "comment" => (
            &["Workspace::comment"],
            Some("tests/mutation_contract.rs::native_comment_matches_the_published_bytes_exactly"),
            "Append one canonical comment; no comments editing, redaction or repair.",
        ),
        "claim" => (
            &[
                "Workspace::claim_with_options",
                "Workspace::claim_next",
                "Workspace::start_task",
                "Workspace::claim",
                "Workspace::update",
            ],
            Some(
                "tests/claim_release_differential.rs::claim_release_sequences_match_published_bytes_and_envelopes",
            ),
            "Atomic explicit/next claim; --start is composed and may retain a successful claim if starting fails.",
        ),
        "release" => (
            &[
                "Workspace::release_with_options",
                "Workspace::pause_task",
                "Workspace::release",
                "Workspace::update",
            ],
            Some(
                "tests/claim_release_differential.rs::claim_release_sequences_match_published_bytes_and_envelopes",
            ),
            "Canonical release policy; --pause composes a status update before release; partial failures retain receipts.",
        ),
        "start-task" => (
            &[
                "Workspace::start_task",
                "Workspace::claim_with_options",
                "Workspace::claim",
                "Workspace::update",
            ],
            Some("tests/claim_release_differential.rs::composed_lifecycle_matches_published_bytes"),
            "Composed explicit-ID lifecycle only, not an atomic multi-step transaction; partial-success receipts remain authoritative.",
        ),
        "pause-task" => (
            &[
                "Workspace::pause_task",
                "Workspace::release_with_options",
                "Workspace::update",
                "Workspace::release",
            ],
            Some("tests/claim_release_differential.rs::composed_lifecycle_matches_published_bytes"),
            "Composed explicit-ID lifecycle only, not an atomic multi-step transaction; partial-success receipts remain authoritative.",
        ),
        "close" => (
            &["Workspace::close", "Workspace::release"],
            Some("tests/mutation_contract.rs::native_close_matches_the_published_bytes_exactly"),
            "Built-in close fields; optional composed release; existing governance and lock recovery apply.",
        ),
        "help" => (
            &[],
            None,
            "Parser help only; text output, no SDK operation or tracker access.",
        ),
        _ => return Err(refuse(DiscoveryRefusalCode::ContractDrift)),
    };
    let args = arguments(command);
    let encodings = if name == "help" {
        strings(&["text"])
    } else if [
        "get",
        "create",
        "update",
        "comment",
        "claim",
        "release",
        "start-task",
        "pause-task",
    ]
    .contains(&name)
    {
        strings(&["json"])
    } else {
        strings(&["toon", "json"])
    };
    let dimensions = args
        .iter()
        .filter(|arg| {
            [
                "json",
                "full",
                "brief",
                "summary",
                "limit",
                "offset",
                "after",
                "output_cursor",
                "output_budget",
                "output_limit",
                "intent",
                "token_budget",
                "depth",
                "format",
                "direction",
                "kind",
                "collapse",
                "node_limit",
                "edge_limit",
                "output_format",
                "memory_bytes",
                "page_items",
                "output_tokens",
                "output_bytes",
                "operation",
                "compact",
                "verify",
            ]
            .contains(&arg.id.as_str())
        })
        .map(|arg| arg.id.clone())
        .collect();
    let recovery = match name {
        "capabilities" => "discovery",
        "changes" | "explain" => "native_required",
        "read-session" => "session",
        "adapter-check" => "adapter",
        "preview-item-mutations" => "admission",
        "help" => "parser",
        _ => "pm_command",
    };
    let mut aliases = command
        .get_all_aliases()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    if name == "deps" {
        aliases.push("item deps".into());
    }
    if name == "comment" {
        aliases.push("item comments".into());
    }
    Ok(OperationContract {
        name: name.into(),
        aliases,
        sdk: strings(sdk),
        provenance: if parity.is_some() {
            Provenance::OracleSlice
        } else {
            Provenance::NativeOnly
        },
        invocation_receipt: "tests/capability_contract.rs::every_advertised_operation_is_invoked"
            .into(),
        parity_receipt: parity.map(str::to_owned),
        arguments: args,
        repeated_scalar_last_wins: command.is_args_override_self(),
        encodings,
        framing: if name == "read-session" {
            "pages: ndjson or successive TOON documents"
        } else {
            "one document"
        }
        .into(),
        dimensions,
        may_mutate: [
            "files",
            "docs",
            "test",
            "create",
            "update",
            "comment",
            "claim",
            "release",
            "start-task",
            "pause-task",
            "close",
        ]
        .contains(&name),
        scope: scope.into(),
        recovery: strings(&[recovery]),
    })
}

/// Compact source description of one producer-owned refusal envelope.
type RecoveryDefinition<'a> = (
    &'a str,
    &'a str,
    &'a [(&'a str, &'a str)],
    &'a [(&'a str, &'a str)],
    &'a str,
    &'a str,
);

/// Reuses producer-owned typed refusal layouts rather than translating their payloads.
#[allow(clippy::too_many_lines)]
fn recovery_schemas() -> Vec<RecoverySchema> {
    let definitions: &[RecoveryDefinition<'_>] = &[
        (
            "discovery",
            "stderr",
            &[
                ("ok", "false"),
                ("refusal", "DiscoveryRefusal"),
                ("exit_code", "2"),
            ],
            &[],
            "refusal.required; refusal.retry_args",
            "discovery::DiscoveryRefusal {code:DiscoveryRefusalCode,required:string,retry_args:string[]}",
        ),
        (
            "native_required",
            "stderr",
            &[
                ("ok", "false"),
                ("code", "string"),
                ("required", "string"),
                ("exit_code", "2"),
            ],
            &[],
            "required",
            "QueryCostRefusal | changes::ChangeRefusal",
        ),
        (
            "session",
            "stderr",
            &[
                ("code", "string"),
                ("detail", "string"),
                ("required", "string"),
                ("exit_code", "2"),
            ],
            &[],
            "required",
            "SessionRefusal",
        ),
        (
            "adapter",
            "stdout",
            &[("ok", "false"), ("refusal", "AdapterRefusal")],
            &[],
            "refusal.remedy; reconcile dispatched outcomes before retry",
            "native_adapter::AdapterRefusal {code:AdapterRefusalCode,dispatched:bool,message:string,remedy:string}",
        ),
        (
            "admission",
            "stderr",
            &[
                ("ok", "false"),
                ("code", "string"),
                ("required", "string"),
                ("limits", "AdmissionLimits"),
                ("exit_code", "2"),
            ],
            &[],
            "required",
            "admission::AdmissionRefusal {code:string,required:string,limits:AdmissionLimits}",
        ),
        (
            "pm_command",
            "stderr when JSON requested",
            &[("code", "string")],
            &[
                ("required", "string"),
                ("recovery", "object"),
                ("exit_code", "integer"),
                ("mutation_receipt", "object"),
                ("refusal", "object"),
            ],
            "required; recovery.suggested_retry_args when present; preserve original canonical receipt",
            "PmRustError; command-specific canonical JSON recovery; not every error has this envelope",
        ),
        (
            "parser",
            "stderr",
            &[],
            &[],
            "Correct argv using advertised arguments; help/version succeed on stdout",
            "clap::Error (text)",
        ),
    ];
    definitions
        .iter()
        .map(
            |(id, channel, required, optional, remedy, sdk_type)| RecoverySchema {
                id: (*id).into(),
                channel: (*channel).into(),
                required: required
                    .iter()
                    .map(|(name, kind)| [(*name).into(), (*kind).into()])
                    .collect(),
                optional: optional
                    .iter()
                    .map(|(name, kind)| [(*name).into(), (*kind).into()])
                    .collect(),
                wire_schema: (*id != "parser").then(|| recovery_wire_schema(required, optional)),
                remedy: (*remedy).into(),
                sdk_type: (*sdk_type).into(),
            },
        )
        .collect()
}

/// Builds a standard schema from the same recovery field descriptors exposed to callers.
fn recovery_wire_schema(required: &[(&str, &str)], optional: &[(&str, &str)]) -> serde_json::Value {
    let properties: serde_json::Map<_, _> = required
        .iter()
        .chain(optional)
        .map(|(name, kind)| ((*name).into(), recovery_field_schema(kind)))
        .collect();
    serde_json::json!({"type":"object","required":required.iter().map(|(name, _)| *name).collect::<Vec<_>>(),"properties":properties,"additionalProperties":true})
}

/// Describes producer-owned nested types without translating any actual refusal payload.
fn recovery_field_schema(kind: &str) -> serde_json::Value {
    use serde_json::json;
    match kind {
        "false" => json!({"const":false}),
        "2" => json!({"const":2}),
        "integer" => json!({"type":"integer","minimum":0,"maximum":255}),
        "string" => json!({"type":"string"}),
        "DiscoveryRefusal" => {
            json!({"type":"object","additionalProperties":false,"required":["code","required","retry_args"],"properties":{
            "code":{"enum":[DiscoveryRefusalCode::InvalidRequest,DiscoveryRefusalCode::IncompatibleProtocol,DiscoveryRefusalCode::UnsupportedOperation,DiscoveryRefusalCode::InvalidAdapter,DiscoveryRefusalCode::OutputLimit,DiscoveryRefusalCode::ContractDrift]},
            "required":{"type":"string"},"retry_args":{"type":"array","items":{"type":"string"}}}})
        }
        "AdapterRefusal" => {
            use crate::native_adapter::AdapterRefusalCode;
            json!({"type":"object","additionalProperties":false,"required":["code","dispatched","message","remedy"],"properties":{
                "code":{"enum":[AdapterRefusalCode::IncompatibleAbi,AdapterRefusalCode::UnsupportedCapability,AdapterRefusalCode::InvalidFrame,AdapterRefusalCode::FrameLimit,AdapterRefusalCode::OutcomeUnknown]},
                "dispatched":{"type":"boolean"},"message":{"type":"string"},"remedy":{"type":"string"}}})
        }
        "AdmissionLimits" => json!({"const":crate::admission::ADMISSION_LIMITS}),
        _ => json!({"type":"object"}),
    }
}

/// Returns a complete native product inventory, or refuses without partial output.
///
/// No Git, Node, tracker, extension loader, transport, mutation or journal is read.
/// Adapter metadata is an explicit declaration, never a live availability probe.
///
/// # Errors
/// Refuses unknown protocols/operations, invalid input or manifests, parser drift,
/// and output bounds too small to retain every advertised flag and recovery schema.
pub fn discover_capabilities(
    request: &DiscoveryRequest,
) -> Result<DiscoveryResponse, DiscoveryRefusal> {
    discover_from_parser(request, &mut crate::cli::Cli::command())
}

/// Binds the complete parser to evidence before advertising any selected operation.
fn discover_from_parser(
    request: &DiscoveryRequest,
    parser: &mut clap::Command,
) -> Result<DiscoveryResponse, DiscoveryRefusal> {
    request.validate()?;
    parser.build();
    let all = parser
        .get_subcommands()
        .map(operation)
        .collect::<Result<Vec<_>, _>>()?;
    let operations: Vec<_> = all
        .into_iter()
        .filter(|op| {
            request
                .operation
                .as_ref()
                .is_none_or(|name| *name == op.name)
        })
        .collect();
    if operations.is_empty() {
        return Err(refuse(DiscoveryRefusalCode::UnsupportedOperation));
    }
    let local = operations
        .iter()
        .filter(|op| !["capabilities", "adapter-check", "help"].contains(&op.name.as_str()))
        .map(|op| op.name.clone())
        .collect();
    let adapter = request.adapter.as_ref();
    let remote = adapter.map_or_else(Vec::new, |manifest| {
        manifest
            .capabilities
            .iter()
            .map(|capability| {
                match capability {
                    AdapterCapability::Changes => "changes",
                    AdapterCapability::Get => "get",
                    AdapterCapability::List => "list",
                    AdapterCapability::Claim => "claim",
                    AdapterCapability::Release => "release",
                }
                .into()
            })
            .filter(|name: &String| {
                request
                    .operation
                    .as_ref()
                    .is_none_or(|selected| selected == name)
            })
            .collect()
    });
    let response = DiscoveryResponse {
        protocol: DISCOVERY_PROTOCOL.into(), product_version: env!("CARGO_PKG_VERSION").into(), oracle_version: crate::COMPATIBLE_PM_VERSION.into(),
        unknown_is_unsupported: true, runtime_requirements: Vec::new(), root_option_scope: "before_command".into(), root_arguments: arguments(parser), operations,
        backends: vec![
            BackendContract { kind: "native_directory".into(), availability: "compiled".into(), operations: local, required: "Readable canonical tracker; writes additionally require canonical policy, ownership and storage admission. No connectivity was probed.".into() },
            BackendContract { kind: "injected_adapter".into(), availability: if adapter.is_some() { "declared_only" } else { "requires_adapter" }.into(), operations: remote, required: "Explicit authorized NativeAdapter implementing pm-native-adapter/1. Manifest grants are not transport, permission or parity receipts. CLI does not provide a remote dispatcher.".into() },
        ], recovery_schemas: recovery_schemas(), unstructured_errors_possible: true, output_bytes: request.output_bytes,
    };
    response.render(true)?;
    response.render(false)?;
    Ok(response)
}

#[cfg(test)]
#[path = "../tests/support/discovery_unit.rs"]
mod tests;
