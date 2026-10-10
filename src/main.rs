//! `pm-rust` command-line interface.

use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand, value_parser};
use pm_rust::{
    CloseItem, CommentItem, CreateItem, CreateResult, ItemFilter, ListOptions, MutationResult,
    OwnershipItem, UpdateItem, Workspace,
};
use serde::Serialize;

#[derive(Debug, Parser)]
#[command(name = "pm-rust", version, about = "Rust-native pm workspace reader")]
struct Cli {
    /// Workspace, nested path, or `.agents/pm` tracker root.
    #[arg(long, default_value = ".")]
    workspace: PathBuf,
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// List stable item projections.
    List {
        /// Emit the published list JSON envelope.
        #[arg(long)]
        json: bool,
        /// Maximum estimated output tokens, or unbounded.
        #[arg(long, requires = "json")]
        output_budget: Option<String>,
        /// Maximum delivered rows, or unbounded.
        #[arg(long, requires = "json")]
        output_limit: Option<String>,
        /// Include terminal items and return full metadata.
        #[arg(long, requires = "json", conflicts_with = "status")]
        all: bool,
        /// Return complete metadata without changing the selected statuses.
        #[arg(long, requires = "json")]
        full: bool,
        /// Explicit identity projection.
        #[arg(long, requires = "json", conflicts_with = "full")]
        brief: bool,
        /// Maximum rows in the producer page.
        #[arg(long, requires = "json")]
        limit: Option<String>,
        /// Skip matching rows before producing a page.
        #[arg(long, requires = "json", conflicts_with = "after")]
        offset: Option<String>,
        /// Continue after a producer cursor.
        #[arg(long, requires = "json")]
        after: Option<String>,
        /// Return every matching row after the cursor or offset.
        #[arg(long, requires = "json")]
        no_truncate: bool,
        /// Resume a snapshot-bound output budget continuation.
        #[arg(long, requires = "json")]
        output_cursor: Option<String>,
        /// Apply the built-in triage projection.
        #[arg(long = "for", requires = "json")]
        intent: Option<String>,
        /// Override the intent's token ceiling (at least 256).
        #[arg(long, requires = "json")]
        token_budget: Option<String>,
        /// Fixed read timestamp for reproducible compatibility fixtures.
        #[arg(long, requires = "json")]
        timestamp: Option<String>,
        /// Exact lifecycle status.
        #[arg(long)]
        status: Option<String>,
        /// Exact item type, compared case-insensitively.
        #[arg(long = "type")]
        item_type: Option<String>,
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
    },
    /// Release the ownership of an explicit item.
    Release(OwnershipArgs),
    /// Close one open canonical item with an immutable closing summary.
    Close {
        /// Exact stable identifier of the item to close.
        id: String,
        /// Required non-empty immutable closing summary.
        #[arg(long)]
        reason: String,
        /// Asserted mutation author.
        #[arg(long)]
        author: String,
        /// Deterministic UTC RFC 3339 timestamp; current time is used when absent.
        #[arg(long)]
        timestamp: Option<String>,
        /// Recover an expired lock before closing.
        #[arg(long)]
        force_stale_lock: bool,
    },
}

/// Arguments shared by explicit ownership commands.
#[derive(Debug, clap::Args)]
struct OwnershipArgs {
    /// Stable item identifier.
    id: String,
    /// Asserted author and claim principal.
    #[arg(long)]
    author: String,
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

#[cfg(test)]
#[path = "../tests/support/main_unit.rs"]
mod tests;

/// Dispatches one parsed command against its discovered workspace.
#[allow(clippy::too_many_lines)]
fn run(cli: Cli) -> Result<(), Box<dyn std::error::Error>> {
    let workspace = Workspace::discover(&cli.workspace)?;
    match cli.command {
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
        } => {
            let filters = ItemFilter {
                status,
                item_type,
                id,
            };
            if json {
                let now = timestamp.unwrap_or_else(pm_rust::current_timestamp);
                pm_rust::validate_timestamp(&now)?;
                let result = workspace.list_page(
                    &filters,
                    &ListOptions {
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
                write_json(&result)?;
                if result.get("output_budget_exceeded").is_some() {
                    return Err(Box::new(pm_rust::PmRustError::OutputBudgetExceeded));
                }
            } else {
                write_json(&workspace.list(filters)?)?;
            }
        }
        Command::Get { id } => write_json(&workspace.get(&id)?)?,
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
        Command::Claim { args, if_available } => {
            return write_json(&workspace.claim(&ownership_request(args, if_available))?);
        }
        Command::Release(args) => {
            return write_json(&workspace.release(&ownership_request(args, false))?);
        }
        Command::Close {
            id,
            reason,
            author,
            timestamp,
            force_stale_lock,
        } => {
            write_json(&close_payload(
                &workspace,
                CloseItem {
                    id,
                    reason,
                    author,
                    timestamp,
                    provenance_role: None,
                    force_stale_lock,
                },
            )?)?;
        }
    }
    Ok(())
}

/// Stamps explicit ownership requests with the published implementer role.
fn ownership_request(args: OwnershipArgs, if_available: bool) -> OwnershipItem {
    OwnershipItem {
        id: args.id,
        author: args.author,
        timestamp: args.timestamp,
        message: args.message,
        force: args.force,
        if_available,
        provenance_role: Some("implementer".to_owned()),
    }
}

/// Creates one item, stamping the argv-derived implementer role.
fn create_payload(
    workspace: &Workspace,
    mut request: CreateItem,
) -> Result<CreateResult, Box<dyn std::error::Error>> {
    request.provenance_role = Some("implementer".to_owned());
    workspace.create(request).map_err(Into::into)
}

/// Applies one field update, stamping the argv-derived implementer role.
fn update_payload(
    workspace: &Workspace,
    mut request: UpdateItem,
) -> Result<MutationResult, Box<dyn std::error::Error>> {
    request.provenance_role = Some("implementer".to_owned());
    workspace.update(request).map_err(Into::into)
}

/// Appends one comment without an argv-derived role.
fn comment_payload(
    workspace: &Workspace,
    request: &CommentItem,
) -> Result<MutationResult, Box<dyn std::error::Error>> {
    workspace.comment(request).map_err(Into::into)
}

/// Closes one item, stamping the argv-derived implementer role.
fn close_payload(
    workspace: &Workspace,
    mut request: CloseItem,
) -> Result<MutationResult, Box<dyn std::error::Error>> {
    request.provenance_role = Some("implementer".to_owned());
    workspace.close(request).map_err(Into::into)
}

/// Parses the command line and maps success or failure to the process exit code.
fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            if let Some(pm_rust::PmRustError::OwnershipRefusal { detail, context }) =
                error.downcast_ref::<pm_rust::PmRustError>()
            {
                let args = published_read_arguments(std::env::args_os().skip(1));
                let payload = ownership_error_json(detail, context, &args);
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
                // Cursor flags require JSON; a closed diagnostic pipe still exits 2.
                let _ = write_json_to(&mut std::io::stderr().lock(), &payload);
            } else {
                eprintln!("pm-rust: {error}");
            }
            ExitCode::from(2)
        }
    }
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
        if arg == "--timestamp" || arg == "--workspace" {
            args.next();
        } else if !arg.starts_with("--timestamp=") && !arg.starts_with("--workspace=") {
            arguments.push(arg);
        }
    }
    arguments
}

/// Formats the published machine-readable refusal for a rejected list cursor.
fn cursor_error_json(code: &str, detail: &str, args: &[String]) -> serde_json::Value {
    let mut provided = Vec::new();
    for arg in args {
        if arg.starts_with("--") && !provided.contains(arg) {
            provided.push(arg.clone());
        }
    }
    let mut payload = serde_json::json!({"code":code,"required":"Adjust command input or tracker state and retry.","recovery":{"attempted_command":format!("pm {}",args.join(" ")),"normalized_args":args,"provided_fields":provided}});
    if code == "invalid_query_cursor" {
        payload["next_steps"] = serde_json::json!([
            "Repeat the original query without --after to obtain a fresh cursor."
        ]);
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
            serde_json::json!({"surface":"list","exit_code":2}),
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
    let mut payload = cursor_error_json(
        context["code"].as_str().unwrap_or("command_failed"),
        detail,
        args,
    );
    match context["code"].as_str() {
        Some("ownership_conflict") => {
            payload["title"] = serde_json::json!("Ownership conflict");
            payload["why"] = serde_json::json!(
                "Ownership checks prevent accidental concurrent mutations on claimed items and protect against conflicting writes."
            );
        }
        Some("lock_conflict") => {
            payload["title"] = serde_json::json!("Lock conflict");
            payload["required"] = serde_json::json!(
                "Wait for lock release, or use --force where supported if lock is stale and safe to override."
            );
            payload["why"] =
                serde_json::json!("Locking protects item files from concurrent write races.");
            payload["examples"] =
                serde_json::json!(["pm update pm-a1b2 --status in_progress --force"]);
        }
        None => {
            payload["title"] = serde_json::json!("Command failed");
            // The generic published refusal has no explanatory why field.
            let mut fields = payload.as_object().cloned().unwrap_or_default();
            fields.remove("why");
            payload = serde_json::Value::Object(fields);
        }
        _ => {}
    }
    for key in ["required", "why", "examples", "next_steps"] {
        if let Some(value) = context.get(key) {
            payload[key] = value.clone();
        }
    }
    payload["exit_code"] = serde_json::json!(4);
    payload["refusal"] = serde_json::json!({"surface":args[0],"exit_code":4});
    payload
}
