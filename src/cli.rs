//! Shared native parser definitions used by execution and discovery.

use clap::{Parser, Subcommand, value_parser};
use std::path::PathBuf;

/// Bare close-validation flag value, shared with discovery metadata.
pub const VALIDATE_CLOSE_DEFAULT: &str = "warn";

/// Native command line, shared with the executable.
#[derive(Debug, Parser)]
#[command(name = "pm-rust", version, about = "Rust-native pm workspace reader")]
pub struct Cli {
    /// Workspace, nested path, or `.agents/pm` tracker root.
    #[arg(long, aliases = ["pm-path", "path"], default_value = ".")]
    pub workspace: PathBuf,
    /// Requested native operation.
    #[command(subcommand)]
    pub command: Command,
}

/// Executable native operations; discovery never invents additional commands.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Discover the compiled native product surface without opening a tracker.
    Capabilities {
        /// Exact discovery protocol; unknown versions refuse before any source access.
        #[arg(long, default_value = crate::discovery::DISCOVERY_PROTOCOL)]
        protocol: String,
        /// Restrict the complete response to one exact operation name.
        #[arg(long)]
        operation: Option<String>,
        /// Encoded response ceiling including newline (1024..131072).
        #[arg(long, default_value_t = crate::discovery::DISCOVERY_MAX_BYTES)]
        output_bytes: u32,
        /// Explicit adapter declaration as strict JSON; never opens a transport.
        #[arg(long)]
        adapter: Option<String>,
        /// Emit compact JSON instead of TOON.
        #[arg(long)]
        json: bool,
    },
    /// Preview bounded canonical item intents from strict JSON on stdin, without writes.
    PreviewItemMutations {
        /// Emit compact JSON instead of TOON.
        #[arg(long)]
        json: bool,
    },
    /// Pull one bounded page of canonical mutation facts without Node or Git.
    Changes {
        /// Opaque cursor returned by the same adapter; absent starts retained history.
        #[arg(long)]
        cursor: Option<String>,
        /// Maximum admitted events plus skipped records (1..100).
        #[arg(long, default_value_t = 20)]
        limit: u16,
        /// Complete JSON and TOON page ceiling (1024..1048576).
        #[arg(long, default_value_t = 65536)]
        output_bytes: u32,
        /// Cancel before a pull when this caller-owned marker exists.
        #[arg(long)]
        cancel_file: Option<PathBuf>,
        /// Emit compact JSON instead of TOON.
        #[arg(long)]
        json: bool,
    },
    /// Explain native list scan work without executing a query or changing state.
    Explain {
        /// Supported read family (filters and cursors are not admitted).
        #[arg(value_parser = ["list"])]
        read: String,
        /// Plan full item decoding, including bodies, instead of brief scalars.
        #[arg(long)]
        full: bool,
        /// Maximum visited filesystem entries (1..=1000000).
        #[arg(long, default_value_t = 100_000)]
        max_entries: u32,
        /// Maximum inspected source bytes (1..=1073741824).
        #[arg(long, default_value_t = 67_108_864)]
        max_bytes: u64,
        /// Emit compact JSON instead of TOON.
        #[arg(long)]
        json: bool,
    },
    /// Validate a native adapter transcript from stdin without executing backend operations.
    AdapterCheck {
        /// Emit compact JSON instead of TOON.
        #[arg(long)]
        json: bool,
    },
    /// Stream a strictly validated immutable tracker snapshot under resource admission limits.
    ReadSession {
        /// Reproducible read clock for conformance fixtures.
        #[arg(long, hide = true)]
        timestamp: Option<String>,
        /// Accounted working-set ceiling in bytes (allocator RSS is measured separately).
        #[arg(long, default_value_t = 536_870_912)]
        memory_bytes: u64,
        /// Maximum canonical rows per delivered page.
        #[arg(long, default_value_t = 20)]
        page_items: usize,
        /// Existing per-page renderer token ceiling.
        #[arg(long, default_value_t = 4000)]
        output_tokens: usize,
        /// Cancel at a page boundary when this caller-owned marker exists.
        #[arg(long)]
        cancel_file: Option<PathBuf>,
        /// Emit one compact JSON frame per line instead of TOON documents.
        #[arg(long)]
        json: bool,
    },
    /// Read configured assurance declarations without changing the registry.
    Assurance {
        /// Read one declaration or discover configured IDs.
        #[arg(value_parser = ["show", "list"])]
        action: String,
        /// Canonical declaration kind.
        kind: String,
        /// Exact declaration ID for show.
        id: Option<String>,
        /// Discovery output ceiling, accepted for bounded recovery commands.
        #[arg(long)]
        output_budget: Option<String>,
        /// Maximum delivered declaration rows.
        #[arg(long, allow_hyphen_values = true)]
        output_limit: Option<String>,
        /// Continue a snapshot-bound declaration page.
        #[arg(long)]
        output_cursor: Option<String>,
        /// Emit JSON.
        #[arg(long)]
        json: bool,
    },
    /// Render the default brief agent orientation snapshot.
    #[command(args_override_self = true)]
    Context {
        /// Core context intent: orient or handoff.
        #[arg(long = "for")]
        intent: Option<String>,
        /// Include an optional section (repeatable).
        #[arg(long = "section", action = clap::ArgAction::Append)]
        sections: Vec<String>,
        /// Maximum retained activity entries.
        #[arg(long, allow_hyphen_values = true)]
        activity_limit: Option<String>,
        /// Maximum ranked focus rows.
        #[arg(long, alias = "max-items", allow_hyphen_values = true)]
        limit: Option<String>,
        /// Focus packing token ceiling.
        #[arg(long, alias = "token_budget", allow_hyphen_values = true)]
        token_budget: Option<String>,
        /// Orientation depth: brief, standard, deep or full.
        #[arg(long)]
        depth: Option<String>,
        /// Continue after a ranked focus page.
        #[arg(long, alias = "cursor")]
        after: Option<String>,
        /// Omit tags from focus rows.
        #[arg(long)]
        no_tags: bool,
        /// Omit static installed extension diagnostics.
        #[arg(long)]
        no_extension_health: bool,
        /// Emit JSON.
        #[arg(long)]
        json: bool,
        /// Reproducible read clock for conformance fixtures.
        #[arg(long, hide = true)]
        timestamp: Option<String>,
    },
    /// Read immutable history and optionally verify integrity.
    #[command(args_override_self = true)]
    History {
        /// Item identity.
        id: String,
        /// Latest record count.
        #[arg(long, allow_hyphen_values = true)]
        limit: Option<String>,
        /// Full entries instead of compact timeline rows.
        #[arg(long)]
        full: bool,
        /// Explicit default compact rows.
        #[arg(long)]
        compact: bool,
        /// Verify record hashes and replay.
        #[arg(long)]
        verify: bool,
        /// Fail when verification reports an error.
        #[arg(long, alias = "fail-on-warn")]
        strict_exit: bool,
        /// Emit JSON.
        #[arg(long)]
        json: bool,
    },
    /// List outgoing dependency trees and graphs.
    #[command(args_override_self = true)]
    Deps {
        /// Item identity.
        id: String,
        /// Tree or graph projection.
        #[arg(long)]
        format: Option<String>,
        /// Maximum traversal depth.
        #[arg(long, allow_hyphen_values = true)]
        max_depth: Option<String>,
        /// Packet traversal direction: outgoing, incoming or both.
        #[arg(long)]
        direction: Option<String>,
        /// Relationship kind filter; repeat or separate with commas.
        #[arg(long)]
        kind: Vec<String>,
        /// Continue a relationship packet.
        #[arg(long)]
        cursor: Option<String>,
        /// Repeated-node collapse policy.
        #[arg(long)]
        collapse: Option<String>,
        /// Counts without payload rows.
        #[arg(long)]
        summary: bool,
        /// Explicit complete projection.
        #[arg(long)]
        full: bool,
        /// Delivered node ceiling.
        #[arg(long, allow_hyphen_values = true)]
        node_limit: Option<String>,
        /// Delivered edge ceiling.
        #[arg(long, allow_hyphen_values = true)]
        edge_limit: Option<String>,
        /// Complete output ceiling.
        #[arg(long, allow_hyphen_values = true)]
        token_budget: Option<String>,
        /// Explicit wire format for replayable recovery commands.
        #[arg(long, value_parser = ["json", "toon"])]
        output_format: Option<String>,
        /// Emit JSON.
        #[arg(long)]
        json: bool,
    },
    /// Search weighted keyword occurrences across canonical item text.
    #[command(args_override_self = true)]
    Search {
        /// Query tokens, joined by a single space.
        #[arg(required=true,num_args=1..)]
        query: Vec<String>,
        /// Maximum producer hits.
        #[arg(long, allow_hyphen_values = true)]
        limit: Option<String>,
        /// Status CSV.
        #[arg(long)]
        status: Option<String>,
        /// Type CSV.
        #[arg(long = "type")]
        item_type: Option<String>,
        /// Tag CSV.
        #[arg(long, alias = "tags")]
        tag: Option<String>,
        /// Producer continuation.
        #[arg(long)]
        after: Option<String>,
        /// Snapshot continuation.
        #[arg(long)]
        output_cursor: Option<String>,
        /// Estimated output token ceiling.
        #[arg(long, allow_hyphen_values = true)]
        output_budget: Option<String>,
        /// Delivered hit ceiling.
        #[arg(long, allow_hyphen_values = true)]
        output_limit: Option<String>,
        /// Emit JSON instead of TOON.
        #[arg(long)]
        json: bool,
    },
    /// List or edit linked source files.
    Files(CollectionArgs),
    /// List or edit linked documentation.
    Docs(CollectionArgs),
    /// List or edit linked test commands.
    Test(CollectionArgs),
    /// List stable item projections.
    List {
        /// Emit the published list JSON envelope.
        #[arg(long)]
        json: bool,
        /// Maximum estimated output tokens, or unbounded.
        #[arg(long)]
        output_budget: Option<String>,
        /// Maximum delivered rows, or unbounded.
        #[arg(long)]
        output_limit: Option<String>,
        /// Include terminal items and return full metadata.
        #[arg(long, conflicts_with = "status")]
        all: bool,
        /// Return complete metadata without changing the selected statuses.
        #[arg(long)]
        full: bool,
        /// Explicit identity projection.
        #[arg(long, conflicts_with = "full")]
        brief: bool,
        /// Maximum rows in the producer page.
        #[arg(long)]
        limit: Option<String>,
        /// Skip matching rows before producing a page.
        #[arg(long, conflicts_with = "after")]
        offset: Option<String>,
        /// Continue after a producer cursor (`--cursor` is a native alias).
        #[arg(long, alias = "cursor")]
        after: Option<String>,
        /// Return every matching row after the cursor or offset.
        #[arg(long)]
        no_truncate: bool,
        /// Resume a snapshot-bound output budget continuation.
        #[arg(long)]
        output_cursor: Option<String>,
        /// Apply the built-in triage projection.
        #[arg(long = "for")]
        intent: Option<String>,
        /// Override the intent's token ceiling (at least 256).
        #[arg(long)]
        token_budget: Option<String>,
        /// Fixed read timestamp for reproducible compatibility fixtures.
        #[arg(long)]
        timestamp: Option<String>,
        /// Exact lifecycle status.
        #[arg(long)]
        status: Option<String>,
        /// Exact item type, compared case-insensitively.
        #[arg(long = "type")]
        item_type: Option<String>,
        /// Exact parent identity.
        #[arg(long)]
        parent: Option<String>,
        /// Comma-separated tag selection.
        #[arg(long)]
        tag: Option<String>,
        /// Sort field, including published timestamp aliases.
        #[arg(long)]
        sort: Option<String>,
        /// Sort direction.
        #[arg(long)]
        order: Option<String>,
        /// Exact stable item identifier.
        #[arg(long, alias = "ids")]
        id: Option<String>,
    },
    /// Read one complete item document by identifier.
    Get {
        /// Exact stable item identifier.
        id: String,
    },
    /// Create one canonical item with an explicit stable identifier.
    Create {
        /// Explicit identifier including the configured project prefix.
        #[arg(long)]
        id: String,
        /// Human-readable title.
        #[arg(long)]
        title: String,
        /// Human-readable description.
        #[arg(long, default_value = "")]
        description: String,
        /// Canonical built-in item type.
        #[arg(long = "type")]
        item_type: String,
        /// Runtime lifecycle state.
        #[arg(long, default_value_t = crate::default_status())]
        status: String,
        /// Priority from zero through four.
        #[arg(long, default_value_t = crate::default_priority(), value_parser = value_parser!(u8).range(0..=4))]
        priority: u8,
        /// Comma-separated tags.
        #[arg(long, value_delimiter = ',')]
        tags: Vec<String>,
        /// Long-form Markdown body.
        #[arg(long, default_value = "")]
        body: String,
        /// Asserted mutation author.
        #[arg(long)]
        author: String,
        /// Deterministic UTC RFC 3339 timestamp; current time is used when absent.
        #[arg(long)]
        timestamp: Option<String>,
        /// Optional create-history message.
        #[arg(long)]
        message: Option<String>,
        /// Recover an expired lock before creating.
        #[arg(long)]
        force_stale_lock: bool,
    },
    /// Update fields on one existing canonical item.
    Update {
        /// Exact stable identifier of the item to mutate.
        id: String,
        /// Replacement human-readable title.
        #[arg(long)]
        title: Option<String>,
        /// Replacement human-readable description.
        #[arg(long)]
        description: Option<String>,
        /// Replacement runtime lifecycle state.
        #[arg(long)]
        status: Option<String>,
        /// Replacement priority from zero through four.
        #[arg(long, value_parser = value_parser!(u8).range(0..=4))]
        priority: Option<u8>,
        /// Comma-separated replacement tags.
        #[arg(long = "tags", alias = "tags-csv")]
        tags_csv: Option<String>,
        /// Replacement long-form Markdown body.
        #[arg(long)]
        body: Option<String>,
        /// Asserted mutation author.
        #[arg(long)]
        author: Option<String>,
        /// Deterministic UTC RFC 3339 timestamp; current time is used when absent.
        #[arg(long)]
        timestamp: Option<String>,
        /// Optional update-history message.
        #[arg(long)]
        message: Option<String>,
        /// Recover an expired lock before updating.
        #[arg(long)]
        force_stale_lock: bool,
    },
    /// Append one comment row to an existing canonical item.
    Comment {
        /// Exact stable identifier of the item to mutate.
        id: String,
        /// Non-empty comment text appended as the newest row.
        text: String,
        /// Asserted mutation author.
        #[arg(long)]
        author: Option<String>,
        /// Deterministic UTC RFC 3339 timestamp; current time is used when absent.
        #[arg(long)]
        timestamp: Option<String>,
        /// Optional comment-history message.
        #[arg(long)]
        message: Option<String>,
        /// Recover an expired lock before commenting.
        #[arg(long)]
        force_stale_lock: bool,
    },
    /// Atomically claim an explicit item for an asserted author.
    Claim {
        /// Common explicit ownership inputs.
        #[command(flatten)]
        args: OwnershipArgs,
        /// Skip an item already claimed by another author.
        #[arg(long)]
        if_available: bool,
        /// Select and atomically claim the next ranked actionable item.
        #[arg(long)]
        next: bool,
        /// Claim first and then move the explicit item to in progress.
        #[arg(long)]
        start: bool,
        /// Selection filters and retry budget.
        #[command(flatten)]
        selection: SelectionArgs,
    },
    /// Release the ownership of an explicit item.
    #[command(mut_arg("id", |arg| arg.required(true)))]
    Release {
        /// Common ownership inputs.
        #[command(flatten)]
        args: OwnershipArgs,
        /// Move to open before releasing ownership.
        #[arg(long)]
        pause: bool,
    },
    /// Claim and start an explicit item.
    #[command(mut_arg("id", |arg| arg.required(true)))]
    StartTask(OwnershipArgs),
    /// Open and release an explicit item.
    #[command(mut_arg("id", |arg| arg.required(true)))]
    PauseTask(OwnershipArgs),
    /// Close one open canonical item with an immutable closing summary.
    #[command(args_override_self = true)]
    Close {
        /// Exact stable identifier of the item to close.
        id: String,
        /// Positional closing summary, taking precedence over reason options.
        text: Option<String>,
        /// Immutable closing summary.
        #[arg(long, short = 'r')]
        reason: Option<String>,
        /// Asserted mutation author.
        #[arg(long)]
        author: Option<String>,
        /// Deterministic UTC RFC 3339 timestamp; current time is used when absent.
        #[arg(long)]
        timestamp: Option<String>,
        /// Recover an expired lock before closing.
        #[arg(long)]
        force_stale_lock: bool,
        /// Published optional close metadata and controls.
        #[command(flatten)]
        fields: CloseArgs,
    },
}

/// Common linked collection flags.
#[derive(Debug, Default, clap::Args)]
#[allow(clippy::struct_excessive_bools)]
pub struct CollectionArgs {
    /// Existing item identity.
    pub id: String,
    /// Repeatable structured or bare additions.
    #[arg(long, allow_hyphen_values = true)]
    pub add: Vec<String>,
    /// Repeatable exact removal selectors.
    #[arg(long, allow_hyphen_values = true)]
    pub remove: Vec<String>,
    /// One-based linked test removal index.
    #[arg(long, allow_hyphen_values = true)]
    pub remove_index: Vec<String>,
    /// Explicit listing mode.
    #[arg(long)]
    pub list: bool,
    /// Asserted author, otherwise resolved from the environment and settings.
    #[arg(long)]
    pub author: Option<String>,
    /// Reproducible mutation clock.
    #[arg(long)]
    pub timestamp: Option<String>,
    /// History explanation.
    #[arg(long, short = 'm')]
    pub message: Option<String>,
    /// Approved ownership and stale lock override.
    #[arg(long)]
    pub force: bool,
    /// Keep file insertion order.
    #[arg(long)]
    pub append_stable: bool,
    /// Annotate added artifacts.
    #[arg(long)]
    pub note: Option<String>,
    /// Emit JSON instead of TOON.
    #[arg(long)]
    pub json: bool,
}

/// Published closure field arguments, including short outcome aliases.
#[derive(Debug, Default, clap::Args)]
pub struct CloseArgs {
    /// Resolution summary and fallback closing reason.
    #[arg(long)]
    pub resolution: Option<String>,
    /// Expected closure outcome.
    #[arg(long, visible_alias = "expected")]
    pub expected_result: Option<String>,
    /// Observed closure outcome.
    #[arg(long, visible_alias = "actual")]
    pub actual_result: Option<String>,
    /// Alternate reason option; a nonblank canonical reason takes precedence.
    #[arg(long)]
    pub close_reason: Option<String>,
    /// Completion timestamp, date or relative token.
    #[arg(long, allow_hyphen_values = true)]
    pub completed_at: Option<String>,
    /// Existing canonical duplicate target.
    #[arg(long, short = 'd')]
    pub duplicate_of: Option<String>,
    /// History message and fallback closing reason.
    #[arg(long, short = 'm')]
    pub message: Option<String>,
    /// Closure validation mode; a bare flag selects warn.
    #[arg(long, num_args = 0..=1, default_missing_value = VALIDATE_CLOSE_DEFAULT)]
    pub validate_close: Option<String>,
    /// Approved ownership and repeat-close override.
    #[arg(long)]
    pub force: bool,
    /// Release ownership after recording the closure.
    #[arg(long)]
    pub release_assignment: bool,
    /// Emit a compact JSON receipt; otherwise emit TOON.
    #[arg(long)]
    pub json: bool,
}

/// Arguments shared by explicit ownership commands.
#[derive(Debug, clap::Args)]
pub struct OwnershipArgs {
    /// Stable item identifier.
    pub id: Option<String>,
    /// Asserted author and claim principal.
    #[arg(long)]
    pub author: Option<String>,
    /// Fixed UTC RFC 3339 mutation clock, no later than the current UTC instant.
    #[arg(long)]
    pub timestamp: Option<String>,
    /// Optional history message.
    #[arg(long)]
    pub message: Option<String>,
    /// Override ownership and terminal-claim checks; recover stale locks.
    #[arg(long)]
    pub force: bool,
    /// Emit the published compact JSON receipt.
    #[arg(long)]
    pub json: bool,
}

/// Filters shared by native claim-next and its SDK equivalent.
#[derive(Debug, Default, clap::Args)]
pub struct SelectionArgs {
    /// Exact runtime item type.
    #[arg(long = "type")]
    pub item_type: Option<String>,
    /// Exact tag.
    #[arg(long)]
    pub tag: Option<String>,
    /// Exact priority.
    #[arg(long)]
    pub priority: Option<u8>,
    /// Descendants of this parent.
    #[arg(long)]
    pub parent: Option<String>,
    /// Exact sprint.
    #[arg(long)]
    pub sprint: Option<String>,
    /// Exact release.
    #[arg(long)]
    pub release: Option<String>,
    /// Ownership selection policy.
    #[arg(long)]
    pub assignee_filter: Option<String>,
    /// Include human decisions.
    #[arg(long)]
    pub include_decisions: bool,
    /// Include gates.
    #[arg(long)]
    pub include_gates: bool,
    /// Include active containers.
    #[arg(long)]
    pub include_containers: bool,
    /// Maximum ranked candidates to attempt.
    #[arg(long)]
    pub max_attempts: Option<usize>,
}
