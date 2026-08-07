//! `pm-rust` command-line interface.

use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand, value_parser};
use pm_rust::{CreateItem, ItemFilter, Workspace};
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
        #[arg(long, default_value = "open")]
        status: String,
        /// Priority from zero through four.
        #[arg(long, default_value_t = 2, value_parser = value_parser!(u8).range(0..=4))]
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
}

fn write_json(value: &impl Serialize) -> Result<(), Box<dyn std::error::Error>> {
    let mut stdout = std::io::stdout().lock();
    write_json_to(&mut stdout, value)?;
    Ok(())
}

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
        } => write_json(&workspace.create(CreateItem {
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
        })?)?,
    }
    Ok(())
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("pm-rust: {error}");
            ExitCode::from(2)
        }
    }
}
