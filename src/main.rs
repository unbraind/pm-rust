//! `pm-rust` command-line interface.

use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand, value_parser};
use pm_rust::{
    CloseItem, CommentItem, CreateItem, CreateResult, ItemFilter, MutationResult, UpdateItem,
    Workspace,
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
        /// Exact lifecycle status.
        #[arg(long)]
        status: Option<String>,
        /// Exact item type, compared case-insensitively.
        #[arg(long = "type")]
        item_type: Option<String>,
        /// Exact stable item identifier.
        #[arg(long)]
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
    serde_json::to_writer_pretty(&mut *writer, value)?;
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
            status,
            item_type,
            id,
        } => write_json(&workspace.list(ItemFilter {
            status,
            item_type,
            id,
        })?)?,
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
            eprintln!("pm-rust: {error}");
            ExitCode::from(2)
        }
    }
}
