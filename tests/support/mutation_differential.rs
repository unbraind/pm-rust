//! Deterministic live CLI driver and fixture utilities.
use super::published_cli::PublishedCli;
use std::fs;
use std::path::Path;
#[path = "process_environment.rs"]
mod process_environment;
pub use process_environment::isolate_environment;
const CLOCK: &str = "2026-08-22T10:00:00.000Z";
#[path = "oracle_driver.rs"]
mod oracle_driver;
pub use oracle_driver::write_driver;

/// Runs one command with a minimal deterministic environment.
pub fn run_minimal(
    program: &Path,
    arguments: &[String],
    working_directory: &Path,
) -> Result<std::process::Output, Box<dyn std::error::Error>> {
    let mut command = super::published_cli::oracle_command(program);
    command.current_dir(working_directory);
    isolate_environment(&mut command);
    // Synthetic oracle invocations must not enqueue or deliver host telemetry.
    // This is the published CLI's process opt-out, not a replaced service.
    command.env("DO_NOT_TRACK", "1");
    command.env("FIXED_CLOCK", CLOCK);
    command.args(arguments).output().map_err(Into::into)
}

/// Recursively copies one directory's regular files and subdirectories.
pub fn copy_directory(source: &Path, destination: &Path) -> Result<(), Box<dyn std::error::Error>> {
    fs::create_dir_all(destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let target = destination.join(entry.file_name());
        if entry.path().is_dir() {
            copy_directory(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}
