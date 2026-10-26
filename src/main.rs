//! `pm-rust` command-line interface.

use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand, value_parser};
use pm_rust::{
    AuthorIdentity, CloseItem, CommentItem, CreateItem, CreateResult, ItemFilter, ListOptions,
    MutationResult, OwnershipItem, OwnershipOptions, SelectionOptions, UpdateItem, Workspace,
};
use serde::Serialize;

#[derive(Debug, Parser)]
#[command(name = "pm-rust", version, about = "Rust-native pm workspace reader")]
struct Cli {
    /// Workspace, nested path, or `.agents/pm` tracker root.
    #[arg(long, aliases = ["pm-path", "path"], default_value = ".")]
    workspace: PathBuf,
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
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
        #[arg(long, default_value_t = pm_rust::default_status())]
        status: String,
        /// Priority from zero through four.
        #[arg(long, default_value_t = pm_rust::default_priority(), value_parser = value_parser!(u8).range(0..=4))]
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
        author: String,
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
        author: String,
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
        author: String,
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
struct CollectionArgs {
    /// Existing item identity.
    id: String,
    /// Repeatable structured or bare additions.
    #[arg(long, allow_hyphen_values = true)]
    add: Vec<String>,
    /// Repeatable exact removal selectors.
    #[arg(long, allow_hyphen_values = true)]
    remove: Vec<String>,
    /// One-based linked test removal index.
    #[arg(long, allow_hyphen_values = true)]
    remove_index: Vec<String>,
    /// Explicit listing mode.
    #[arg(long)]
    list: bool,
    /// Asserted author, otherwise resolved from the environment and settings.
    #[arg(long)]
    author: Option<String>,
    /// Reproducible mutation clock.
    #[arg(long)]
    timestamp: Option<String>,
    /// History explanation.
    #[arg(long, short = 'm')]
    message: Option<String>,
    /// Approved ownership and stale lock override.
    #[arg(long)]
    force: bool,
    /// Keep file insertion order.
    #[arg(long)]
    append_stable: bool,
    /// Annotate added artifacts.
    #[arg(long)]
    note: Option<String>,
    /// Emit JSON instead of TOON.
    #[arg(long)]
    json: bool,
}

/// Published closure field arguments, including short outcome aliases.
#[derive(Debug, Default, clap::Args)]
struct CloseArgs {
    /// Resolution summary and fallback closing reason.
    #[arg(long)]
    resolution: Option<String>,
    /// Expected closure outcome.
    #[arg(long, visible_alias = "expected")]
    expected_result: Option<String>,
    /// Observed closure outcome.
    #[arg(long, visible_alias = "actual")]
    actual_result: Option<String>,
    /// Alternate reason option; a nonblank canonical reason takes precedence.
    #[arg(long)]
    close_reason: Option<String>,
    /// Completion timestamp, date or relative token.
    #[arg(long, allow_hyphen_values = true)]
    completed_at: Option<String>,
    /// Existing canonical duplicate target.
    #[arg(long, short = 'd')]
    duplicate_of: Option<String>,
    /// History message and fallback closing reason.
    #[arg(long, short = 'm')]
    message: Option<String>,
    /// Closure validation mode; a bare flag selects warn.
    #[arg(long, num_args = 0..=1, default_missing_value = "warn")]
    validate_close: Option<String>,
    /// Approved ownership and repeat-close override.
    #[arg(long)]
    force: bool,
    /// Release ownership after recording the closure.
    #[arg(long)]
    release_assignment: bool,
    /// Emit a compact JSON receipt; otherwise emit TOON.
    #[arg(long)]
    json: bool,
}

/// Arguments shared by explicit ownership commands.
#[derive(Debug, clap::Args)]
struct OwnershipArgs {
    /// Stable item identifier.
    id: Option<String>,
    /// Asserted author and claim principal.
    #[arg(long)]
    author: Option<String>,
    /// Fixed UTC RFC 3339 mutation clock, no later than the current UTC instant.
    #[arg(long)]
    timestamp: Option<String>,
    /// Optional history message.
    #[arg(long)]
    message: Option<String>,
    /// Override ownership and terminal-claim checks; recover stale locks.
    #[arg(long)]
    force: bool,
    /// Emit the published compact JSON receipt.
    #[arg(long)]
    json: bool,
}

/// Filters shared by native claim-next and its SDK equivalent.
#[derive(Debug, Default, clap::Args)]
struct SelectionArgs {
    /// Exact runtime item type.
    #[arg(long = "type")]
    item_type: Option<String>,
    /// Exact tag.
    #[arg(long)]
    tag: Option<String>,
    /// Exact priority.
    #[arg(long)]
    priority: Option<u8>,
    /// Descendants of this parent.
    #[arg(long)]
    parent: Option<String>,
    /// Exact sprint.
    #[arg(long)]
    sprint: Option<String>,
    /// Exact release.
    #[arg(long)]
    release: Option<String>,
    /// Ownership selection policy.
    #[arg(long)]
    assignee_filter: Option<String>,
    /// Include human decisions.
    #[arg(long)]
    include_decisions: bool,
    /// Include gates.
    #[arg(long)]
    include_gates: bool,
    /// Include active containers.
    #[arg(long)]
    include_containers: bool,
    /// Maximum ranked candidates to attempt.
    #[arg(long)]
    max_attempts: Option<usize>,
}

/// Writes one pretty JSON response to the process standard output stream.
fn write_json(value: &impl Serialize) -> Result<(), Box<dyn std::error::Error>> {
    let mut stdout = std::io::stdout().lock();
    write_json_to(&mut stdout, value)?;
    Ok(())
}

/// Serializes one response to a caller-supplied writer and flushes it.
fn write_json_to(
    writer: &mut dyn Write,
    value: &impl Serialize,
) -> Result<(), Box<dyn std::error::Error>> {
    pm_rust::write_pretty_json(&mut *writer, value)?;
    writer.write_all(b"\n")?;
    writer
        .flush()
        .map_err(|error| -> Box<dyn std::error::Error> { Box::new(error) })
}

/// Writes a complete session frame and propagates codec, pipe and flush failures.
fn write_session_page_to(
    writer: &mut dyn std::io::Write,
    page: &pm_rust::SessionPage,
    json: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let bytes = if json {
        let mut bytes = pm_rust::stringify_json(page, false);
        bytes.push('\n');
        bytes
    } else {
        page.stringify_toon()?
    };
    writer.write_all(bytes.as_bytes())?;
    writer.flush()?;
    Ok(())
}

#[cfg(test)]
#[path = "../tests/support/main_unit.rs"]
mod tests;

/// Checks stdin with bounded admission and renders a compact receipt or typed refusal.
fn run_adapter_check(
    reader: &mut dyn std::io::Read,
    writer: &mut dyn Write,
    json: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut bytes = Vec::new();
    reader
        .take((pm_rust::native_adapter::NATIVE_ADAPTER_MAX_TRANSCRIPT_BYTES + 1) as u64)
        .read_to_end(&mut bytes)?;
    let result = pm_rust::native_adapter::check_adapter_transcript(&bytes);
    let value = match &result {
        Ok(value) => value.clone(),
        Err(error) => serde_json::json!({"ok": false, "refusal": error}),
    };
    let output = pm_rust::ListOutput::from(value);
    write_adapter_receipt_to(writer, &output, json)?;
    result?;
    Ok(())
}

/// Writes a compact ABI receipt, propagating codec, pipe and flush failures.
fn write_adapter_receipt_to(
    writer: &mut dyn Write,
    output: &pm_rust::ListOutput,
    json: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let rendered = if json {
        pm_rust::stringify_json(output, false)
    } else {
        pm_rust::stringify_toon(output)?
    };
    writer.write_all(rendered.as_bytes())?;
    writer.write_all(b"\n")?;
    writer.flush()?;
    Ok(())
}

/// Dispatches one parsed command using process-owned standard streams.
fn run(cli: Cli) -> Result<(), Box<dyn std::error::Error>> {
    run_with_io(
        cli,
        &mut std::io::stdin().lock(),
        &mut std::io::stdout().lock(),
    )
}

/// Dispatches workspace commands or an adapter check with caller-owned streams.
#[allow(clippy::too_many_lines)]
fn run_with_io(
    cli: Cli,
    reader: &mut dyn Read,
    writer: &mut dyn Write,
) -> Result<(), Box<dyn std::error::Error>> {
    let discover = || Workspace::discover(&cli.workspace);
    match cli.command {
        Command::Explain {
            read: _,
            full,
            max_entries,
            max_bytes,
            json,
        } => {
            let workspace = discover().map_err(|_| pm_rust::QueryCostRefusal::Source)?;
            let plan = workspace.explain_list(&pm_rust::QueryCostRequest {
                projection: if full {
                    pm_rust::QueryProjection::Full
                } else {
                    pm_rust::QueryProjection::Brief
                },
                max_entries,
                max_bytes,
            })?;
            let output = pm_rust::ListOutput::from(serde_json::json!(plan));
            write_adapter_receipt_to(writer, &output, json)?;
        }
        Command::AdapterCheck { json } => run_adapter_check(reader, writer, json)?,
        Command::ReadSession {
            timestamp,
            memory_bytes,
            page_items,
            output_tokens,
            cancel_file,
            json,
        } => {
            let workspace = discover()?;
            let cancellation = pm_rust::SessionCancellation::default();
            if cancel_file.as_ref().is_some_and(|path| path.exists()) {
                cancellation.cancel();
            }
            let limits = pm_rust::ReadSessionLimits {
                memory_bytes,
                page_items,
                output_tokens,
                toon: !json,
            };
            let mut session = workspace.read_session(
                &limits,
                cancellation.clone(),
                &timestamp.unwrap_or_else(pm_rust::current_timestamp),
            )?;
            let mut stdout = std::io::stdout().lock();
            loop {
                if cancel_file.as_ref().is_some_and(|path| path.exists()) {
                    cancellation.cancel();
                }
                let Some(page) = session.next_page()? else {
                    break;
                };
                write_session_page_to(&mut stdout, &page, json)?;
            }
            return Ok(());
        }
        Command::Assurance {
            action,
            kind,
            id,
            output_budget,
            output_limit,
            output_cursor,
            json,
        } => {
            let workspace = discover()?;
            let id = if action == "show" {
                Some(id.ok_or_else(|| pm_rust::PmRustError::InvalidReadRequest {
                    reason: "assurance show requires an id".to_owned(),
                })?)
            } else {
                None
            };
            let result = workspace.assurance_read(
                &kind,
                id.as_deref(),
                &ListOptions {
                    output_budget,
                    output_limit,
                    output_cursor,
                    toon: !json,
                    ..ListOptions::default()
                },
            )?;
            return write_search_to(&mut std::io::stdout().lock(), &result, json);
        }
        Command::Context {
            limit,
            token_budget,
            depth,
            after,
            no_tags,
            no_extension_health,
            json,
            timestamp,
        } => {
            let workspace = discover()?;
            let now = timestamp.unwrap_or_else(pm_rust::current_timestamp);
            let result = workspace.context(
                &pm_rust::ContextOptions {
                    limit,
                    token_budget,
                    depth,
                    after,
                    no_tags,
                    no_extension_health,
                    toon: !json,
                },
                &now,
            )?;
            write_search_to(&mut std::io::stdout().lock(), &result, json)?;
        }
        Command::History {
            id,
            limit,
            full,
            compact,
            verify,
            strict_exit,
            json,
        } => {
            let workspace = discover()?;
            if full && compact {
                return Err(Box::new(pm_rust::PmRustError::CommandRefusal {detail:"History projection options are mutually exclusive. Use --compact, --provenance, or --full.".to_owned(),exit_code:2,context:serde_json::json!({})}));
            }
            if strict_exit && !verify {
                return Err(Box::new(pm_rust::PmRustError::CommandRefusal {
                    detail:
                        "--strict-exit requires --verify (it gates on the verification result)."
                            .to_owned(),
                    exit_code: 2,
                    context: serde_json::json!({}),
                }));
            }
            let result = workspace.history(
                &id,
                &pm_rust::HistoryOptions {
                    limit,
                    verify,
                    full,
                    compact,
                    toon: !json,
                },
            )?;
            write_search_to(&mut std::io::stdout().lock(), &result, json)?;
            if strict_exit && result["verification"]["ok"] == false {
                return Err(Box::new(pm_rust::PmRustError::HistoryVerificationFailed));
            }
        }
        Command::Deps {
            id,
            format,
            max_depth,
            direction,
            kind,
            cursor,
            collapse,
            summary,
            full,
            node_limit,
            edge_limit,
            token_budget,
            output_format,
            json,
        } => {
            let workspace = discover()?;
            let json = output_format
                .as_deref()
                .map_or(json, |format| format == "json");
            let result = workspace.deps(
                &id,
                &pm_rust::DepsOptions {
                    format,
                    max_depth,
                    direction,
                    kind,
                    cursor,
                    collapse,
                    summary,
                    full,
                    node_limit,
                    edge_limit,
                    token_budget,
                    toon: !json,
                },
            )?;
            return write_search_to(&mut std::io::stdout().lock(), &result, json);
        }
        Command::Search {
            query,
            limit,
            status,
            item_type,
            tag,
            after,
            output_cursor,
            output_budget,
            output_limit,
            json,
        } => {
            let workspace = discover()?;
            let result = workspace.search(
                &query.join(" "),
                &pm_rust::SearchOptions {
                    status,
                    item_type,
                    tag,
                    output: ListOptions {
                        limit,
                        after,
                        output_cursor,
                        output_budget,
                        output_limit,
                        toon: !json,
                        ..ListOptions::default()
                    },
                },
            )?;
            return write_search_to(&mut std::io::stdout().lock(), &result, json);
        }
        Command::Files(args) => run_collection(&discover()?, "files", args)?,
        Command::Docs(args) => run_collection(&discover()?, "docs", args)?,
        Command::Test(args) => run_collection(&discover()?, "tests", args)?,
        Command::List {
            json,
            output_budget,
            output_limit,
            brief,
            limit,
            offset,
            after,
            no_truncate,
            output_cursor,
            intent,
            token_budget,
            all,
            full,
            timestamp,
            status,
            item_type,
            id,
            parent,
            tag,
            sort,
            order,
        } => {
            let workspace = discover()?;
            let filters = ItemFilter {
                status,
                item_type,
                id,
            };
            {
                let now = timestamp.unwrap_or_else(pm_rust::current_timestamp);
                pm_rust::validate_timestamp(&now)?;
                let result = workspace.list_page(
                    &filters,
                    &ListOptions {
                        parent,
                        tag,
                        sort,
                        order,
                        toon: !json,
                        all,
                        full,
                        brief,
                        limit,
                        offset,
                        after,
                        no_truncate,
                        output_limit,
                        output_budget,
                        output_cursor,
                        intent,
                        token_budget,
                    },
                    &now,
                )?;
                write_list_to(&mut std::io::stdout().lock(), &result, json)?;
                if result.get("output_budget_exceeded").is_some() {
                    return Err(Box::new(pm_rust::PmRustError::OutputBudgetExceeded));
                }
            }
        }
        Command::Get { id } => write_json(&discover()?.get(&id)?)?,
        Command::Create {
            id,
            title,
            description,
            item_type,
            status,
            priority,
            tags,
            body,
            author,
            timestamp,
            message,
            force_stale_lock,
        } => {
            let workspace = discover()?;
            write_json(&create_payload(
                &workspace,
                CreateItem {
                    id,
                    title,
                    description,
                    item_type,
                    status,
                    priority,
                    tags,
                    body,
                    author,
                    timestamp,
                    message,
                    provenance_role: None,
                    force_stale_lock,
                },
            )?)?;
        }
        Command::Update {
            id,
            title,
            description,
            status,
            priority,
            tags_csv,
            body,
            author,
            timestamp,
            message,
            force_stale_lock,
        } => {
            let workspace = discover()?;
            write_json(&update_payload(
                &workspace,
                UpdateItem {
                    id,
                    title,
                    description,
                    status,
                    priority,
                    // A provided CSV value expresses replacement intent even
                    // when it normalizes to an empty tag list. Each segment is
                    // trimmed before the empty filter so `--tags "alpha, beta"`
                    // stores `beta`, not `" beta"`, and `--tags "alpha, "`
                    // stores only `alpha` rather than keeping a whitespace-only
                    // segment.
                    tags: tags_csv.map(|csv| {
                        csv.split(',')
                            .map(str::trim)
                            .map(str::to_owned)
                            .filter(|tag| !tag.is_empty())
                            .collect()
                    }),
                    body,
                    author,
                    timestamp,
                    message,
                    provenance_role: None,
                    force_stale_lock,
                },
            )?)?;
        }
        Command::Comment {
            id,
            text,
            author,
            timestamp,
            message,
            force_stale_lock,
        } => {
            let workspace = discover()?;
            write_json(&comment_payload(
                &workspace,
                &CommentItem {
                    id,
                    text,
                    author,
                    timestamp,
                    message,
                    provenance_role: None,
                    force_stale_lock,
                },
            )?)?;
        }
        Command::Claim {
            args,
            if_available,
            next,
            start,
            selection,
        } => {
            let workspace = discover()?;
            if start && args.id.is_none() {
                return Err(Box::new(pm_rust::PmRustError::CommandRefusal {
                    detail: "Specify an item id or pass --next".to_owned(),
                    exit_code: 2,
                    context: serde_json::json!({}),
                }));
            }
            if start && (next || if_available) {
                return Err(Box::new(pm_rust::PmRustError::CommandRefusal {
                    detail: "--start cannot be combined with --next or --if-available for an explicit item".to_owned(), exit_code: 2,
                    context: serde_json::json!({"code":"invalid_argument_value","required":"Remove --next and --if-available when starting an explicit item.","next_steps":["Remove --next and --if-available, keep the explicit item ID and other supplied arguments, then retry."]}),
                }));
            }
            if next == args.id.is_some() {
                return Err(Box::new(pm_rust::PmRustError::CommandRefusal {
                    detail: if next {
                        "Specify either an item id or --next, not both"
                    } else {
                        "Specify an item id or pass --next"
                    }
                    .to_owned(),
                    exit_code: 2,
                    context: serde_json::json!({}),
                }));
            }
            let (request, options) = ownership_request(&workspace, args, if_available)?;
            if next {
                return write_json(&workspace.claim_next(
                    &request,
                    &options,
                    &SelectionOptions {
                        item_type: selection.item_type,
                        tag: selection.tag,
                        priority: selection.priority,
                        parent: selection.parent,
                        sprint: selection.sprint,
                        release: selection.release,
                        assignee_filter: selection.assignee_filter,
                        include_decisions: selection.include_decisions,
                        include_gates: selection.include_gates,
                        include_containers: selection.include_containers,
                        max_attempts: selection.max_attempts,
                    },
                )?);
            }
            if start {
                return write_json(&workspace.start_task(&request, &options)?);
            }
            return write_json(&workspace.claim_with_options(&request, &options)?);
        }
        Command::Release { args, pause } => {
            let workspace = discover()?;
            let (request, options) = ownership_request(&workspace, args, false)?;
            if pause {
                return write_json(&workspace.pause_task(&request, &options)?);
            }
            return write_json(&workspace.release_with_options(&request, &options)?);
        }
        Command::StartTask(args) => {
            let workspace = discover()?;
            let (mut request, options) = ownership_request(&workspace, args, false)?;
            request.provenance_role = None;
            return write_json(&workspace.start_task(&request, &options)?);
        }
        Command::PauseTask(args) => {
            let workspace = discover()?;
            let (mut request, options) = ownership_request(&workspace, args, false)?;
            request.provenance_role = None;
            return write_json(&workspace.pause_task(&request, &options)?);
        }
        Command::Close {
            id,
            text,
            reason,
            author,
            timestamp,
            force_stale_lock,
            fields,
        } => {
            let workspace = discover()?;
            let close_author = author.clone();
            let result = close_payload(
                &workspace,
                CloseItem {
                    id,
                    reason: text
                        .filter(|text| !text.is_empty())
                        .or_else(|| {
                            reason
                                .filter(|reason| !reason.trim().is_empty())
                                .or(fields.close_reason)
                        })
                        .unwrap_or_default(),
                    author,
                    timestamp,
                    provenance_role: None,
                    fields: pm_rust::CloseFields {
                        resolution: fields.resolution,
                        expected_result: fields.expected_result,
                        actual_result: fields.actual_result,
                        completed_at: fields.completed_at,
                        duplicate_of: fields.duplicate_of,
                        message: fields.message.clone(),
                        validate_close: fields.validate_close,
                        force: fields.force,
                    },
                    force_stale_lock,
                },
            )?;
            let mut receipt = result.close_receipt.unwrap_or_default();
            if fields.release_assignment {
                let release = mutation_identity(Some(&close_author), None)
                    .with(|| {
                        workspace.release(&OwnershipItem {
                            id: result.item.metadata.id.clone(),
                            author: close_author,
                            timestamp: Some(result.item.metadata.updated_at.clone()),
                            message: fields.message,
                            force: fields.force,
                            if_available: false,
                            provenance_role: Some("implementer".to_owned()),
                        })
                    })
                    .map_err(|error| {
                        pm_rust::completed_step_failure(error, &result.item.metadata.id, "close")
                    })?;
                let mut changed: std::collections::BTreeSet<String> =
                    result.close_changed_fields.into_iter().collect();
                if release.changed_field_count > 0 {
                    changed.extend(["assignee", "claim_principal"].map(str::to_owned));
                }
                receipt = serde_json::json!({"id":result.item.metadata.id,"action":"close_task","close":receipt,"release":release,"status":result.item.metadata.status,"changed_field_count":changed.len()});
            }
            write_close_receipt(&receipt, fields.json)?;
        }
    }
    Ok(())
}

/// Resolves the collection author and renders the published receipt.
fn run_collection(
    workspace: &Workspace,
    key: &str,
    args: CollectionArgs,
) -> Result<(), Box<dyn std::error::Error>> {
    run_collection_with_cwd(workspace, key, args, std::env::current_dir())
}

/// Propagates an unavailable invocation directory before a collection can mutate storage.
fn run_collection_with_cwd(
    workspace: &Workspace,
    key: &str,
    args: CollectionArgs,
    cwd: std::io::Result<PathBuf>,
) -> Result<(), Box<dyn std::error::Error>> {
    let settings: serde_json::Value = pm_rust::strict_json::from_str(&std::fs::read_to_string(
        workspace.pm_root().join("settings.json"),
    )?)?;
    let identity = mutation_identity(args.author.as_deref(), settings["author_default"].as_str());
    let result = workspace.collection(
        &args.id,
        key,
        &pm_rust::CollectionOptions {
            add: args.add,
            remove: args.remove,
            remove_index: args.remove_index,
            author: identity.author.clone(),
            identity: Some(identity),
            timestamp: args.timestamp,
            message: args.message,
            force: args.force,
            append_stable: args.append_stable,
            note: args.note,
        },
        &cwd?,
    )?;
    write_list_to(
        &mut std::io::stdout().lock(),
        &pm_rust::ListOutput::from(result),
        args.json,
    )
}

/// Writes search receipts before reporting an infeasible requested output budget.
fn write_search_to(
    mut writer: &mut dyn Write,
    result: &pm_rust::ListOutput,
    json: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    write_list_to(&mut writer, result, json)?;
    if result.get("output_budget_exceeded").is_some() {
        return Err(Box::new(pm_rust::PmRustError::OutputBudgetExceeded));
    }
    Ok(())
}

/// Writes published compact closure output in the selected wire format.
fn write_close_receipt(
    value: &serde_json::Value,
    json: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    write_close_receipt_to(&mut std::io::stdout().lock(), value, json)
}

/// Writes closure receipts to a caller-provided stream and propagates flush failures.
fn write_close_receipt_to(
    writer: &mut dyn Write,
    value: &serde_json::Value,
    json: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    if json {
        return write_json_to(writer, value);
    }
    write_close_toon(writer, value, "")?;
    writer.flush()?;
    Ok(())
}

/// Renders compact mutation TOON scalars with the oracle's quoted string style.
fn write_close_toon(
    writer: &mut dyn Write,
    value: &serde_json::Value,
    indent: &str,
) -> std::io::Result<()> {
    if let Some(fields) = value.as_object() {
        for (key, value) in fields {
            if value.is_null() {
                continue;
            }
            if value.is_object() {
                writeln!(writer, "{indent}{key}:")?;
                write_close_toon(writer, value, &format!("{indent}  "))?;
            } else if value.is_array() {
                let mut field = serde_json::Map::new();
                field.insert(key.clone(), value.clone());
                // JSON values use infallible built-in serde serializers.
                let encoded = toon_format::encode_default(&serde_json::Value::Object(field))
                    .unwrap_or_default();
                for line in encoded.lines() {
                    writeln!(writer, "{indent}{line}")?;
                }
            } else {
                writeln!(
                    writer,
                    "{indent}{key}: {}",
                    pm_rust::stringify_json(value, false)
                )?;
            }
        }
    }
    Ok(())
}

/// Resolves bounded provenance once at the CLI mutation boundary.
fn mutation_identity(author: Option<&str>, configured: Option<&str>) -> AuthorIdentity {
    let environment = std::env::vars_os()
        .map(|(key, value)| {
            (
                key.to_string_lossy().into_owned(),
                value.to_string_lossy().into_owned(),
            )
        })
        .collect();
    AuthorIdentity::resolve(author, configured, &environment).for_collections(&environment)
}

/// Stamps explicit ownership requests with the published implementer role.
fn ownership_request(
    workspace: &Workspace,
    args: OwnershipArgs,
    if_available: bool,
) -> Result<(OwnershipItem, OwnershipOptions), Box<dyn std::error::Error>> {
    if let Some(author) = &args.author
        && author.trim().is_empty()
    {
        return Err(Box::new(pm_rust::PmRustError::InvalidMutationRequest {
            reason: "author must not be empty".to_owned(),
        }));
    }
    let settings: serde_json::Value = pm_rust::strict_json::from_slice(&std::fs::read(
        workspace.pm_root().join("settings.json"),
    )?)?;
    let identity = mutation_identity(args.author.as_deref(), settings["author_default"].as_str());
    let request = OwnershipItem {
        id: args.id.unwrap_or_default(),
        author: identity.author.clone(),
        timestamp: args.timestamp,
        message: args.message,
        force: args.force,
        if_available,
        provenance_role: Some("implementer".to_owned()),
    };
    Ok((
        request,
        OwnershipOptions {
            identity: Some(identity),
            release_bypass: false,
        },
    ))
}

/// Creates one item, stamping the argv-derived implementer role.
fn create_payload(
    workspace: &Workspace,
    mut request: CreateItem,
) -> Result<CreateResult, Box<dyn std::error::Error>> {
    request.provenance_role = Some("implementer".to_owned());
    mutation_identity(Some(&request.author), None)
        .with(|| workspace.create(request))
        .map_err(Into::into)
}

/// Applies one field update, stamping the argv-derived implementer role.
fn update_payload(
    workspace: &Workspace,
    mut request: UpdateItem,
) -> Result<MutationResult, Box<dyn std::error::Error>> {
    request.provenance_role = Some("implementer".to_owned());
    mutation_identity(Some(&request.author), None)
        .with(|| workspace.update(request))
        .map_err(Into::into)
}

/// Appends one comment without an argv-derived role.
fn comment_payload(
    workspace: &Workspace,
    request: &CommentItem,
) -> Result<MutationResult, Box<dyn std::error::Error>> {
    mutation_identity(Some(&request.author), None)
        .with(|| workspace.comment(request))
        .map_err(Into::into)
}

/// Closes one item, stamping the argv-derived implementer role.
fn close_payload(
    workspace: &Workspace,
    mut request: CloseItem,
) -> Result<MutationResult, Box<dyn std::error::Error>> {
    request.provenance_role = Some("implementer".to_owned());
    mutation_identity(Some(&request.author), None)
        .with(|| workspace.close(request))
        .map_err(Into::into)
}

/// Accepts canonical item namespaces emitted by verified retry commands.
fn native_arguments(mut args: Vec<std::ffi::OsString>) -> Vec<std::ffi::OsString> {
    let mut index = 1;
    while let Some(arg) = args.get(index) {
        let arg = arg.to_string_lossy();
        if matches!(arg.as_ref(), "--workspace" | "--pm-path" | "--path") {
            index += 2;
        } else if ["--workspace=", "--pm-path=", "--path="]
            .iter()
            .any(|flag| arg.starts_with(flag))
        {
            index += 1;
        } else {
            if arg == "item" {
                match args.get(index + 1).and_then(|arg| arg.to_str()) {
                    Some("deps") => {
                        args.remove(index);
                    }
                    Some("comments") => {
                        args.remove(index);
                        args[index] = "comment".into();
                    }
                    _ => {}
                }
            }
            break;
        }
    }
    args
}

/// Resolves explicit wire encoding before the legacy JSON switch for error output.
fn json_requested(args: &[String]) -> bool {
    let explicit = args
        .windows(2)
        .find(|pair| pair[0] == "--output-format")
        .map(|pair| pair[1].as_str())
        .or_else(|| {
            args.iter()
                .find_map(|arg| arg.strip_prefix("--output-format="))
        });
    explicit.map_or_else(
        || args.iter().any(|arg| arg == "--json"),
        |format| format == "json",
    )
}

/// Parses the command line and maps success or failure to the process exit code.
#[allow(clippy::too_many_lines)]
fn main() -> ExitCode {
    let cli = match Cli::try_parse_from(native_arguments(std::env::args_os().collect())) {
        Ok(cli) => cli,
        Err(error) => {
            let mut args = published_read_arguments(std::env::args_os().skip(1));
            let orientation = args
                .iter()
                .position(|arg| !arg.starts_with('-'))
                .filter(|&index| matches!(args[index].as_str(), "context" | "history" | "deps"));
            if error.kind() == clap::error::ErrorKind::UnknownArgument
                && let Some(index) = orientation
            {
                let command = args.remove(index);
                args.insert(0, command);
                let allowed = match args[0].as_str() {
                    "context" => vec![
                        "--limit",
                        "--max-items",
                        "--token-budget",
                        "--token_budget",
                        "--depth",
                        "--after",
                        "--cursor",
                        "--no-tags",
                        "--no-extension-health",
                        "--json",
                    ],
                    "history" => vec![
                        "--limit",
                        "--full",
                        "--compact",
                        "--verify",
                        "--strict-exit",
                        "--fail-on-warn",
                        "--json",
                    ],
                    _ => vec![
                        "--format",
                        "--max-depth",
                        "--direction",
                        "--kind",
                        "--cursor",
                        "--collapse",
                        "--summary",
                        "--full",
                        "--node-limit",
                        "--edge-limit",
                        "--token-budget",
                        "--output-format",
                        "--json",
                    ],
                };
                let flag = rejected_orientation_argument(&error, &args, &allowed);
                let detail = format!(
                    "Native {} does not support {flag}; use the published pm CLI for this option.",
                    args[0]
                );
                let payload = orientation_error_json(
                    &detail,
                    &serde_json::json!({"code":"unsupported_native_option","required":"Use the published pm CLI for options outside the documented native slice.","title":"Unsupported native option","flag":flag,"value":flag}),
                    &args,
                    2,
                );
                let mut stderr = std::io::stderr().lock();
                let _ = if json_requested(&args) {
                    write_json_to(&mut stderr, &payload)
                } else {
                    write_cursor_refusal_to(&mut stderr, &payload)
                };
                return ExitCode::from(2);
            }
            // Keep Clap's bytes and statuses while allowing normal process teardown.
            // Immediate exit bypasses profile finalization on Windows.
            let _ = error.print();
            return if error.use_stderr() {
                ExitCode::from(2)
            } else {
                ExitCode::SUCCESS
            };
        }
    };
    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            if let Some(refusal) = error.downcast_ref::<pm_rust::QueryCostRefusal>() {
                let payload = serde_json::json!({"ok":false,"code":refusal.code(),"required":refusal.to_string(),"exit_code":2});
                let _ = write_json_to(&mut std::io::stderr().lock(), &payload);
                return ExitCode::from(2);
            }
            if let Some(refusal) = error.downcast_ref::<pm_rust::SessionRefusal>() {
                let payload = serde_json::json!({"code":refusal.code(),"detail":refusal.to_string(),"required":refusal.to_string(),"exit_code":2});
                let _ = write_json_to(&mut std::io::stderr().lock(), &payload);
                return ExitCode::from(2);
            }
            if matches!(
                error.downcast_ref::<pm_rust::PmRustError>(),
                Some(pm_rust::PmRustError::HistoryVerificationFailed)
            ) {
                return ExitCode::from(1);
            }
            if let Some(pm_rust::PmRustError::CommandRefusal {
                detail,
                exit_code,
                context,
            }) = error.downcast_ref::<pm_rust::PmRustError>()
            {
                let args = published_read_arguments(std::env::args_os().skip(1));
                let payload = command_error_json(detail, context, &args, *exit_code);
                let mut stderr = std::io::stderr().lock();
                let _ = write_command_refusal_to(&mut stderr, detail, context, &args, &payload);
                return ExitCode::from(*exit_code);
            }
            if let Some(pm_rust::PmRustError::OwnershipRefusal { detail, context }) =
                error.downcast_ref::<pm_rust::PmRustError>()
            {
                let args = published_read_arguments(std::env::args_os().skip(1));
                let mut payload = ownership_error_json(detail, context, &args);
                if context["code"] == "lock_conflict" {
                    preserve_lock_scope(
                        &mut payload,
                        &std::env::args_os()
                            .skip(1)
                            .map(|arg| arg.to_string_lossy().into_owned())
                            .collect::<Vec<_>>(),
                    );
                }
                let mut stderr = std::io::stderr().lock();
                let _ = if args.first().is_some_and(|s| s == "item") && !json_requested(&args) {
                    write_cursor_refusal_to(&mut stderr, &payload)
                } else {
                    write_json_to(&mut stderr, &payload)
                };
                return ExitCode::from(4);
            }
            if let Some(pm_rust::PmRustError::LockConflict { .. }) =
                error.downcast_ref::<pm_rust::PmRustError>()
            {
                let args = published_read_arguments(std::env::args_os().skip(1));
                let mut payload = ownership_error_json(
                    &error.to_string(),
                    &serde_json::json!({"code":"lock_conflict"}),
                    &args,
                );
                preserve_lock_scope(
                    &mut payload,
                    &std::env::args_os()
                        .skip(1)
                        .map(|arg| arg.to_string_lossy().into_owned())
                        .collect::<Vec<_>>(),
                );
                let _ = write_json_to(&mut std::io::stderr().lock(), &payload);
                return ExitCode::from(4);
            }
            if matches!(
                error.downcast_ref::<pm_rust::PmRustError>(),
                Some(pm_rust::PmRustError::OutputBudgetExceeded)
            ) {
                return ExitCode::from(2);
            }
            if let Some(pm_rust::PmRustError::ReadCursor { code, detail }) =
                error.downcast_ref::<pm_rust::PmRustError>()
            {
                let args = published_read_arguments(std::env::args_os().skip(1));
                let payload = cursor_error_json(code, detail, &args);
                let mut stderr = std::io::stderr().lock();
                let _ = if json_requested(&args) {
                    write_json_to(&mut stderr, &payload)
                } else {
                    write_cursor_refusal_to(&mut stderr, &payload)
                };
            } else {
                eprintln!("pm-rust: {error}");
            }
            ExitCode::from(2)
        }
    }
}

/// Retains Clap's rejected token, falling back only to unsupported option names.
fn rejected_orientation_argument<'a>(
    error: &'a clap::Error,
    args: &'a [String],
    allowed: &[&str],
) -> &'a str {
    let rejected = match error.get(clap::error::ContextKind::InvalidArg) {
        Some(clap::error::ContextValue::String(arg)) => Some(arg.as_str()),
        _ => None,
    };
    rejected
        .or_else(|| {
            args.iter()
                .map(String::as_str)
                .filter(|arg| arg.starts_with('-'))
                .find(|arg| !allowed.contains(&arg.split('=').next().unwrap_or_default()))
        })
        .map_or("option", |arg| arg.split('=').next().unwrap_or_default())
}

/// Writes a command refusal using its requested encoding and published human fallback.
fn write_command_refusal_to(
    writer: &mut dyn Write,
    detail: &str,
    context: &serde_json::Value,
    args: &[String],
    payload: &serde_json::Value,
) -> Result<(), Box<dyn std::error::Error>> {
    if context["code"] == "unknown_error" && !json_requested(args) {
        writeln!(writer, "{detail}")?;
        return Ok(());
    }
    if args.first().is_some_and(|command| {
        matches!(
            command.as_str(),
            "item" | "search" | "context" | "history" | "deps"
        )
    }) && !json_requested(args)
    {
        write_cursor_refusal_to(writer, payload)
    } else {
        write_json_to(writer, payload)
    }
}

/// Preserves ordinary command refusals and the published unhandled-error envelope.
fn command_error_json(
    detail: &str,
    context: &serde_json::Value,
    args: &[String],
    exit_code: u8,
) -> serde_json::Value {
    if context["recovery"]["token_budget"].is_object() {
        return context.clone();
    }
    if context["code"] == "unknown_error" {
        return unhandled_error_json(detail, exit_code);
    }
    if let Some(recovery) = context["recovery"]
        .as_object()
        .filter(|_| context["code"] == "invalid_query_cursor")
    {
        let mut payload = cursor_error_json("invalid_query_cursor", detail, args);
        let retry = context["recovery"]["suggested_retry"].clone();
        let steps = payload["next_steps"]
            .as_array()
            .into_iter()
            .flatten()
            .cloned()
            .chain([retry])
            .collect::<Vec<_>>();
        payload["next_steps"] = serde_json::json!(steps);
        for (key, value) in recovery {
            payload["recovery"][key] = value.clone();
        }
        return payload;
    }
    if context["reason"] == "assurance_evaluation_refused" {
        let mut payload = ownership_error_json(detail, context, args);
        payload["exit_code"] = serde_json::json!(exit_code);
        payload["refusal"] = serde_json::json!({"surface":"assurance","rejected_value":context["value"],"exit_code":exit_code});
        payload["changed"] = context["changed"].clone();
        payload["value"] = context["value"].clone();
        for (key, value) in context["recovery"].as_object().into_iter().flatten() {
            payload["recovery"][key] = value.clone();
        }
        return payload;
    }
    if args
        .first()
        .is_some_and(|arg| matches!(arg.as_str(), "context" | "history" | "deps"))
    {
        return orientation_error_json(detail, context, args, exit_code);
    }
    if args.first().is_some_and(|arg| arg == "search") {
        return search_error_json(detail, context, args, exit_code);
    }
    if args.first().is_some_and(|arg| arg == "item") {
        return collection_error_json(detail, context, args, exit_code);
    }
    let mut payload = ownership_error_json(detail, context, args);
    payload["exit_code"] = serde_json::json!(exit_code);
    payload["refusal"] = if args.first().is_some_and(|command| command == "close") {
        close_error_refusal(detail, &payload, exit_code)
    } else {
        command_refusal_detail(detail, args, exit_code)
    };
    payload
}

/// Renders the published `TypeError` envelope without adding command-recovery metadata.
fn unhandled_error_json(detail: &str, exit_code: u8) -> serde_json::Value {
    serde_json::json!({"code":"unknown_error","required":"Inspect command input and runtime state, then retry.","exit_code":exit_code,"type":"urn:pm-cli:error:unknown_error","title":"Unhandled error","detail":detail,"why":"Unexpected runtime failures can occur from environment or extension-level issues.","examples":["pm --help","pm health --json"],"refusal":{"surface":"command","exit_code":exit_code}})
}

/// Preserves the published canonical dependency spelling in recovery arguments.
fn orientation_arguments(args: &[String]) -> Vec<String> {
    let mut args = args.to_vec();
    if args.first().is_some_and(|arg| arg == "deps") {
        args.insert(0, "item".to_owned());
    }
    args
}

/// Matches orientation command failures to the published problem envelope.
fn orientation_error_json(
    detail: &str,
    context: &serde_json::Value,
    args: &[String],
    exit_code: u8,
) -> serde_json::Value {
    let args = orientation_arguments(args);
    let command = &args[0];
    let mut context = context.clone();
    if detail.starts_with("--strict-exit requires --verify") {
        context["code"] = serde_json::json!("invalid_argument_value");
    }

    if context["code"] == "invalid_argument_value" {
        context["required"] =
            serde_json::json!("Use values that match documented command constraints.");
        context["title"] = serde_json::json!("Invalid argument value");
        context["next_steps"] = serde_json::json!([
            "Check allowed values in command help, then rerun with corrected input."
        ]);
        context["examples"] = serde_json::json!([
            format!("pm {command} --help"),
            "pm contracts --command <command> --flags-only --json".to_owned()
        ]);
    }
    let mut result = ownership_error_json(detail, &context, &args);
    if context["code"] == "item_not_found" {
        result["required"] =
            serde_json::json!("Use an existing item ID from current tracker data.");
        result["title"] = serde_json::json!("Item ID not found");
        result["why"] = serde_json::json!("Mutation and read commands operate only on known IDs.");
        result["examples"] = serde_json::json!([
            "pm list --status open --limit 20",
            "pm search \"<keyword>\" --limit 10"
        ]);
        result["next_steps"] = serde_json::json!([
            "Confirm the active --path/PM_PATH scope, then retry with a valid id."
        ]);
        result["refusal"] = serde_json::json!({"surface":"id","rejected_value":context["id"],"exit_code":exit_code});
        result["flag"] = serde_json::json!("id");
        result["value"] = context["id"].clone();
    } else {
        result["refusal"] = command_refusal_detail(detail, &args, exit_code);
    }
    if detail.starts_with("--strict-exit requires --verify") {
        result["recovery"]["missing"] = serde_json::json!(["--verify"]);
        result["recovery"]["suggested_retry"] = serde_json::json!(format!(
            "pm {} --verify",
            args.iter()
                .map(|arg| pm_rust::quote_command_arg(arg))
                .collect::<Vec<_>>()
                .join(" ")
        ));
    }
    result["exit_code"] = serde_json::json!(exit_code);
    // Insertion order is part of the wire contract.
    let mut ordered = serde_json::Map::new();
    for key in [
        "code",
        "required",
        "recovery",
        "next_steps",
        "exit_code",
        "type",
        "title",
        "detail",
        "why",
        "examples",
        "refusal",
        "flag",
        "value",
    ] {
        if let Some(value) = result.get(key) {
            ordered.insert(key.to_owned(), value.clone());
        }
    }
    serde_json::Value::Object(ordered)
}

/// Preserves keyword-search usage classification and closed-domain recovery.
fn search_error_json(
    detail: &str,
    context: &serde_json::Value,
    args: &[String],
    exit_code: u8,
) -> serde_json::Value {
    let mut context = context.clone();
    if context.get("code").is_none()
        && (detail.starts_with("Invalid ")
            || detail.contains(" must be ")
            || detail.contains(" requires "))
    {
        context = serde_json::json!({"code":"invalid_argument_value"});
    }
    if context["code"] == "invalid_argument_value" {
        context["required"] =
            serde_json::json!("Use values that match documented command constraints.");
        context["title"] = serde_json::json!("Invalid argument value");
        if context.get("next_steps").is_none() {
            context["next_steps"] = serde_json::json!([
                "Check allowed values in command help, then rerun with corrected input."
            ]);
            context["examples"] = serde_json::json!([
                "pm search --help",
                "pm contracts --command <command> --flags-only --json"
            ]);
        }
    }
    let mut result = ownership_error_json(detail, &context, args);
    result["recovery"]["normalized_args"] = serde_json::json!(args);
    result["exit_code"] = serde_json::json!(exit_code);
    result["refusal"] = command_refusal_detail(detail, args, exit_code);
    if detail.starts_with("--output-budget ") || detail.starts_with("--output-limit ") {
        result["refusal"] = serde_json::json!({"surface":"search","exit_code":exit_code});
    }
    if let Some(recovery) = context["recovery"].as_object() {
        for (key, value) in recovery {
            result["recovery"][key] = value.clone();
        }
    }
    if let Some(domain) = context["recovery"].get("allowed_values") {
        let rejected = args.iter().enumerate().find_map(|(index, arg)| {
            arg.strip_prefix("--type=").or_else(|| {
                (arg == "--type")
                    .then(|| args.get(index + 1).map(String::as_str))
                    .flatten()
            })
        });
        result["refusal"] = serde_json::json!({"surface":"--type","rejected_value":rejected,"legal_domain":domain,"exit_code":exit_code});
        // The pinned oracle extracts equals-form values, but emits a corrected
        // retry only for the separate-token spelling. Preserve its exact bytes.
        if let Some(index) = args.iter().position(|arg| arg == "--type") {
            let mut retry = args.to_vec();
            if let Some(value) = retry.get_mut(index + 1) {
                domain[0].as_str().unwrap_or_default().clone_into(value);
            }
            result["examples"] = serde_json::json!([
                format!(
                    "pm {}",
                    retry
                        .iter()
                        .map(|arg| pm_rust::quote_command_arg(arg))
                        .collect::<Vec<_>>()
                        .join(" ")
                ),
                "pm search --help"
            ]);
        } else {
            result["examples"] = serde_json::json!([
                "pm search --help",
                "pm contracts --command <command> --flags-only --json"
            ]);
        }
        let provided = result["recovery"]["provided_fields"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(serde_json::Value::as_str)
            .map(|flag| flag.split('=').next().unwrap_or_default())
            .collect::<Vec<_>>();
        result["recovery"]["provided_fields"] = serde_json::json!(provided);
    }
    if let Ok(Cli {
        command: Command::Search { query, .. },
        ..
    }) = Cli::try_parse_from(std::iter::once("pm-rust".to_owned()).chain(args.iter().cloned()))
        && let Some(examples) = result["examples"].as_array_mut()
    {
        for example in examples {
            if let Some(text) = example.as_str() {
                *example = serde_json::json!(
                    text.replace("<query>", &pm_rust::quote_command_arg(&query.join(" ")))
                );
            }
        }
    }
    result
}

/// Removes native clock and discovery controls from published error recovery arguments.
///
/// Callers pass `args_os` so a non-UTF-8 argument cannot panic this refusal
/// path. Each argument is lossily owned, then published in the normal JSON
/// envelope with exit code 2.
fn published_read_arguments<I, S>(args: I) -> Vec<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    let mut arguments = Vec::new();
    let mut args = args
        .into_iter()
        .map(|arg| arg.as_ref().to_string_lossy().into_owned());
    while let Some(arg) = args.next() {
        if matches!(
            arg.as_str(),
            "--timestamp" | "--workspace" | "--pm-path" | "--path"
        ) {
            args.next();
        } else if !["--timestamp=", "--workspace=", "--pm-path=", "--path="]
            .iter()
            .any(|prefix| arg.starts_with(prefix))
        {
            arguments.push(arg);
        }
    }
    if arguments.starts_with(&["item".to_owned(), "deps".to_owned()]) {
        arguments.remove(0);
    }
    if arguments.first().is_some_and(|command| command == "close") {
        for arg in &mut arguments {
            let alias = match arg.as_str() {
                "--expected" => Some("--expected-result"),
                "--actual" => Some("--actual-result"),
                _ => None,
            };
            if let Some(alias) = alias {
                alias.clone_into(arg);
            }
        }
    }
    if arguments
        .first()
        .is_some_and(|command| matches!(command.as_str(), "files" | "docs" | "test"))
    {
        arguments.insert(0, "item".to_owned());
    }
    arguments
}

/// Applies the oracle's item namespace and error classification to collection refusals.
fn collection_error_json(
    detail: &str,
    context: &serde_json::Value,
    args: &[String],
    exit_code: u8,
) -> serde_json::Value {
    let mut context = context.clone();
    if context.get("code").is_none()
        && exit_code == 2
        && (detail.starts_with("Invalid ")
            || detail.contains(" must be ")
            || detail.contains(" requires "))
    {
        context = serde_json::json!({"code":"invalid_argument_value","required":"Use values that match documented command constraints.","next_steps":["Check allowed values in command help, then rerun with corrected input."],"title":"Invalid argument value","examples":["pm item --help","pm contracts --command <command> --flags-only --json"]});
    }
    let missing = detail.starts_with("Item ") && detail.ends_with(" not found");
    if missing {
        let steps = context.get("next_steps").cloned().unwrap_or_else(|| {
            serde_json::json!([
                "Confirm the active --path/PM_PATH scope, then retry with a valid id."
            ])
        });
        context = serde_json::json!({"code":"item_not_found","required":"Use an existing item ID from current tracker data.","next_steps":steps,"title":"Item ID not found","why":"Mutation and read commands operate only on known IDs.","examples":["pm list --status open --limit 20","pm search \"<keyword>\" --limit 10"]});
    }
    let mut result = ownership_error_json(detail, &context, args);
    if let Some(recovery) = context["recovery"].as_object() {
        for (key, value) in recovery {
            result["recovery"][key] = value.clone();
        }
    }
    result["exit_code"] = serde_json::json!(exit_code);
    result["refusal"] = command_refusal_detail(detail, &args[1..], exit_code);
    if missing {
        let id = &detail["Item ".len()..detail.len() - " not found".len()];
        result["refusal"] =
            serde_json::json!({"surface":"id","rejected_value":id,"exit_code":exit_code});
        result["flag"] = serde_json::json!("id");
        result["value"] = serde_json::json!(id);
    }
    if context["code"] == "unknown_option" {
        result.as_object_mut().map(|map| map.shift_remove("why"));
        result["refusal"] = serde_json::json!({"surface":"--remove-index","rejected_value":"--remove-index","exit_code":exit_code});
    }
    result
}

/// Maps an explicit native workspace into the published retry command's tracker scope.
fn preserve_lock_scope(payload: &mut serde_json::Value, raw: &[String]) {
    let workspace = raw.iter().enumerate().find_map(|(index, arg)| {
        if matches!(arg.as_str(), "--workspace" | "--pm-path" | "--path") {
            raw.get(index + 1).cloned()
        } else {
            ["--workspace=", "--pm-path=", "--path="]
                .iter()
                .find_map(|prefix| arg.strip_prefix(prefix).map(str::to_owned))
        }
    });
    if let Some(workspace) = workspace {
        let mut args = vec!["--pm-path".to_owned(), workspace];
        args.extend(
            payload["recovery"]["normalized_args"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(serde_json::Value::as_str)
                .map(str::to_owned),
        );
        let command = format!(
            "pm {}",
            args.iter()
                .map(|arg| pm_rust::quote_command_arg(arg))
                .collect::<Vec<_>>()
                .join(" ")
        );
        payload["recovery"]["attempted_command"] = serde_json::json!(command);
        payload["recovery"]["normalized_args"] = serde_json::json!(args);
        payload["recovery"]["suggested_retry"] = serde_json::json!(command);
        payload["recovery"]["suggested_retry_args"] = serde_json::json!(args);
        payload["examples"] = serde_json::json!([command]);
    }
}

/// Formats the published machine-readable refusal for a rejected list cursor.
fn cursor_error_json(code: &str, detail: &str, args: &[String]) -> serde_json::Value {
    let mut provided = Vec::new();
    for arg in args {
        if arg.starts_with("--") && !provided.contains(arg) {
            provided.push(arg.clone());
        }
    }
    let recovery_args = orientation_arguments(args);
    let mut payload = serde_json::json!({"code":code,"required":"Adjust command input or tracker state and retry.","recovery":{"attempted_command":format!("pm {}",recovery_args.iter().map(|arg| pm_rust::quote_command_arg(arg)).collect::<Vec<_>>().join(" ")),"normalized_args":recovery_args,"provided_fields":provided}});
    if code == "invalid_query_cursor" {
        let flag = if args
            .iter()
            .any(|arg| arg == "--cursor" || arg.starts_with("--cursor="))
        {
            "--cursor"
        } else {
            "--after"
        };
        payload["next_steps"] = serde_json::json!([format!(
            "Repeat the original query without {flag} to obtain a fresh cursor."
        )]);
    }
    for (key, value) in [
        ("exit_code", serde_json::json!(2)),
        (
            "type",
            serde_json::json!(format!("urn:pm-cli:error:{code}")),
        ),
        ("title", serde_json::json!(detail)),
        ("detail", serde_json::json!(detail)),
        (
            "why",
            serde_json::json!(
                "pm enforces explicit, deterministic contracts for data and command semantics."
            ),
        ),
        (
            "examples",
            serde_json::json!(["pm --help", "pm <command> --help"]),
        ),
        (
            "refusal",
            serde_json::json!({"surface":args.first().filter(|command| matches!(command.as_str(),"context"|"deps"|"search"|"assurance")).map_or("list",String::as_str),"exit_code":2}),
        ),
    ] {
        payload[key] = value;
    }
    payload
}

/// Formats a published ownership conflict and command recovery receipt.
fn ownership_error_json(
    detail: &str,
    context: &serde_json::Value,
    args: &[String],
) -> serde_json::Value {
    let mut recovery_args = args.to_vec();
    if context["code"] == "lock_conflict" && args.first().is_some_and(|arg| arg == "comment") {
        "comments".clone_into(&mut recovery_args[0]);
        recovery_args.insert(0, "item".to_owned());
    }
    let mut payload = cursor_error_json(
        context["code"].as_str().unwrap_or("command_failed"),
        detail,
        &recovery_args,
    );
    match context["code"].as_str() {
        Some("ownership_conflict") => {
            payload["title"] = serde_json::json!("Ownership conflict");
            payload["required"] = serde_json::json!(
                "Run as the assigned owner, claim the item when appropriate, or use --force only for an approved override."
            );
            payload["next_steps"] = serde_json::json!([
                "Use --force for approved systematic metadata updates performed by leads or maintainers.",
                "Use --force when correcting known stale metadata after coordinating ownership changes.",
                "For non-terminal reassignment, prefer \"pm claim <ID>\" before running other mutations."
            ]);
            payload["examples"] = serde_json::json!([
                "pm claim pm-a1b2",
                "pm update pm-a1b2 --status in_progress --force"
            ]);
            payload["why"] = serde_json::json!(
                "Ownership checks prevent accidental concurrent mutations on claimed items and protect against conflicting writes."
            );
        }
        Some("lock_conflict") => {
            payload["title"] = serde_json::json!("Lock conflict");
            payload["required"] = serde_json::json!(
                "Wait for lock release, then retry the same operation. Set PM_LOCK_WAIT_MS to a larger bounded millisecond budget for expected contention."
            );
            payload["why"] =
                serde_json::json!("Locking protects item files from concurrent write races.");
            let retry = payload["recovery"]["attempted_command"].clone();
            payload["examples"] = serde_json::json!([retry]);
            payload["recovery"]["suggested_retry"] = retry;
            payload["recovery"]["suggested_retry_args"] =
                payload["recovery"]["normalized_args"].clone();
        }
        None => {
            payload["title"] = serde_json::json!("Command failed");
        }
        _ => {}
    }
    apply_close_error_recovery(&mut payload, context, args);
    for key in ["required", "examples", "next_steps", "title"] {
        if let Some(value) = context.get(key) {
            payload[key] = value.clone();
        }
    }
    let include_why = context["code"]
        .as_str()
        .is_some_and(|code| code != "invalid_argument_value");
    if include_why && let Some(why) = context.get("why") {
        payload["why"] = why.clone();
    }
    let mut ordered = serde_json::Map::new();
    for key in [
        "code",
        "required",
        "recovery",
        "next_steps",
        "exit_code",
        "type",
        "title",
        "detail",
        "why",
        "examples",
        "refusal",
    ] {
        if key == "why" && !include_why {
            continue;
        }
        if let Some(value) = payload.get(key) {
            ordered.insert(key.to_owned(), value.clone());
        }
    }
    payload = serde_json::Value::Object(ordered);
    payload["exit_code"] = serde_json::json!(4);
    payload["refusal"] = command_refusal_detail(
        detail,
        if args.first().is_some_and(|s| s == "item") {
            &args[1..]
        } else {
            args
        },
        4,
    );
    for key in ["flag", "value"] {
        if let Some(value) = context.get(key) {
            payload[key] = value.clone();
        }
    }
    payload
}

/// Adds actionable close recovery fields and the published compact title limit.
fn apply_close_error_recovery(
    payload: &mut serde_json::Value,
    context: &serde_json::Value,
    args: &[String],
) {
    if args.first().is_some_and(|command| command == "close")
        && let Some(recovery) = context["recovery"].as_object()
    {
        for (key, value) in recovery {
            payload["recovery"][key] = value.clone();
        }
    }
    if payload["title"]
        .as_str()
        .is_some_and(|title| title.chars().count() > 120)
    {
        payload["title"] = serde_json::json!(format!(
            "{}...",
            payload["title"]
                .as_str()
                .unwrap_or_default()
                .chars()
                .take(117)
                .collect::<String>()
        ));
    }
}

/// Formats command-specific rejected flag values in usage refusals.
fn command_refusal_detail(detail: &str, args: &[String], exit_code: u8) -> serde_json::Value {
    let surface = detail
        .split_whitespace()
        .filter(|word| word.starts_with("--"))
        .map(|word| word.trim_end_matches([',', '.']))
        .find(|word| args.iter().any(|arg| arg == word));
    if let Some(surface) = surface {
        if detail.contains("not both") {
            return serde_json::json!({"surface":surface,"exit_code":exit_code});
        }
        let rejected = args
            .iter()
            .position(|arg| arg == surface)
            .and_then(|index| args.get(index + 1))
            // The published extractor omits these shared author/output controls.
            .filter(|arg| !matches!(arg.as_str(), "--author" | "--json"));
        if let Some(rejected) = rejected {
            return serde_json::json!({"surface":surface,"rejected_value":rejected,"exit_code":exit_code});
        }
        return serde_json::json!({"surface":surface,"exit_code":exit_code});
    }
    serde_json::json!({"surface":args[0],"exit_code":exit_code})
}

/// Extracts closure refusal flags from prose and recovery evidence in oracle order.
fn close_error_refusal(
    detail: &str,
    payload: &serde_json::Value,
    exit_code: u8,
) -> serde_json::Value {
    let normalized = payload["recovery"]["normalized_args"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let args: Vec<&str> = normalized
        .iter()
        .filter_map(serde_json::Value::as_str)
        .collect();
    let flag = payload["flag"].as_str().or_else(|| {
        detail
            .split(|ch: char| !ch.is_ascii_alphanumeric() && ch != '-' && ch != '_')
            .filter(|word| word.starts_with("--"))
            .find(|word| args.iter().any(|arg| arg == word))
    });
    let surface = flag
        .or_else(|| payload["recovery"]["missing"][0].as_str())
        .unwrap_or("close");
    let mut receipt = serde_json::json!({"surface":surface});
    if let Some(flag) = flag {
        let rejected = args
            .iter()
            .position(|arg| *arg == flag)
            .and_then(|index| args.get(index + 1))
            .filter(|arg| !matches!(**arg, "--json" | "--author"));
        if let Some(rejected) = rejected {
            receipt["rejected_value"] = serde_json::json!(rejected);
        }
    }
    receipt["exit_code"] = serde_json::json!(exit_code);
    receipt
}

/// Writes list output in the same encoding used for its budget measurement.
fn write_list_to(
    writer: &mut dyn Write,
    result: &pm_rust::ListOutput,
    json: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    if json {
        result.write_read_json(writer)?;
        writer.write_all(b"\n")?;
        writer.flush()?;
        return Ok(());
    }
    writer.write_all(pm_rust::stringify_toon(result)?.as_bytes())?;
    writer.flush()?;
    Ok(())
}

/// Writes the published human-readable cursor diagnostic when JSON was not requested.
fn write_cursor_refusal_to(
    writer: &mut dyn Write,
    payload: &serde_json::Value,
) -> Result<(), Box<dyn std::error::Error>> {
    let attempted = payload["recovery"]["normalized_args"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(serde_json::Value::as_str)
        .map(pm_rust::quote_command_arg)
        .collect::<Vec<_>>()
        .join(" ");
    let attempted = payload["recovery"]["attempted_command"]
        .as_str()
        .map_or_else(|| format!("pm {attempted}"), str::to_owned);
    let mut parts = vec![format!(
        "Error: {}\n\nWhat is required:\n  {}",
        payload["title"].as_str().unwrap_or_default(),
        payload["required"].as_str().unwrap_or_default()
    )];
    if let Some(steps) = payload["next_steps"].as_array() {
        parts.push("\nNext steps:".to_owned());
        parts.extend(
            steps
                .iter()
                .map(|step| format!("  - {}", step.as_str().unwrap_or_default())),
        );
    }
    parts.push(format!(
        "\nWhat happened:\n  {}",
        payload["detail"].as_str().unwrap_or_default()
    ));
    if let Some(why) = payload["why"].as_str() {
        parts.push(format!("\nWhy:\n  {why}"));
    } else if payload["recovery"]["token_budget"].is_object() {
        parts.push("\nWhy:\n  Budget advice is verified against the current query. A minimum is reported only after every smaller positive budget is checked.".to_owned());
    } else if payload["code"] == "command_failed" {
        parts.push("\nWhy:\n  pm enforces explicit, deterministic contracts for data and command semantics.".to_owned());
    } else if payload["code"] == "invalid_argument_value" {
        parts.push("\nWhy:\n  Validation protects data consistency and deterministic behavior across commands.".to_owned());
    } else if payload["code"] == "unknown_option" {
        parts.push(
            "\nWhy:\n  Option contracts are command-specific and intentionally validated."
                .to_owned(),
        );
    }
    if let Some(examples) = payload["examples"].as_array() {
        parts.push("\nExamples:".to_owned());
        parts.extend(
            examples
                .iter()
                .map(|example| format!("  - {}", example.as_str().unwrap_or_default())),
        );
    }
    if payload["recovery"]["token_budget"].is_object() {
        append_budget_recovery(&mut parts, &payload["recovery"]);
    } else {
        parts.push("\nRecovery bundle:".to_owned());
        if let Some(retry) = payload["recovery"]["suggested_retry"].as_str() {
            parts.push(format!("  suggested_retry: {retry}"));
        }
        parts.push(format!("  attempted_command: {attempted}"));
    }
    if let Some(fields) = payload["recovery"]["provided_fields"]
        .as_array()
        .filter(|fields| !fields.is_empty())
    {
        parts.push(format!(
            "  provided_fields: {}",
            fields
                .iter()
                .filter_map(serde_json::Value::as_str)
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    for key in [
        "allowed_values",
        "candidate_commands",
        "candidate_commands_total",
        "option_scope",
    ] {
        if let Some(value) = payload["recovery"].get(key) {
            let value = value.as_array().map_or_else(
                || {
                    value
                        .as_str()
                        .map_or_else(|| value.to_string(), str::to_owned)
                },
                |values| {
                    values
                        .iter()
                        .filter_map(serde_json::Value::as_str)
                        .collect::<Vec<_>>()
                        .join(", ")
                },
            );
            parts.push(format!("  {key}: {value}"));
        }
    }
    writer.write_all(format!("{}\n", parts.join("\n")).as_bytes())?;
    writer.flush()?;
    Ok(())
}

/// Renders compact, verified packet recovery without inventing attempted-command fields.
fn append_budget_recovery(parts: &mut Vec<String>, recovery: &serde_json::Value) {
    let bounds = &recovery["token_budget"];
    let feasible = bounds["feasible_tokens"]
        .as_u64()
        .map_or_else(|| "unavailable".to_owned(), |value| value.to_string());
    let minimum = bounds["minimum_feasible_tokens"]
        .as_u64()
        .map_or_else(|| "not established".to_owned(), |value| value.to_string());
    parts.push(format!("\nRecovery bundle:\n  requested_token_budget: {}\n  feasible_token_budget: {feasible}\n  minimum_token_budget: {minimum}", bounds["requested_tokens"]));
    parts.push(format!(
        "  suggested_retry: {}",
        recovery["suggested_retry"].as_str().unwrap_or_default()
    ));
    parts.push("  suggested_flags: --token-budget".to_owned());
    parts.push(format!(
        "  next_best_command: {}",
        recovery["next_best_command"].as_str().unwrap_or_default()
    ));
    parts.push("  fallback_candidates:".to_owned());
    for candidate in recovery["fallback_candidates"]
        .as_array()
        .into_iter()
        .flatten()
    {
        parts.push(format!(
            "    - {}: {} ({})",
            candidate["source"].as_str().unwrap_or_default(),
            candidate["command"].as_str().unwrap_or_default(),
            candidate["reason"].as_str().unwrap_or_default()
        ));
    }
}
