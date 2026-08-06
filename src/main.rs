//! `pm-rust` read-only command-line interface.

use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use pm_rust::{ItemFilter, Workspace};
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
