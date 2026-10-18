//! Deterministic live CLI driver and fixture utilities.
use super::published_cli::PublishedCli;
use std::fs;
use std::path::{Path, PathBuf};
#[path = "process_environment.rs"]
mod process_environment;
pub use process_environment::isolate_environment;
const CLOCK: &str = "2026-08-22T10:00:00.000Z";
/// Encodes one path as a JSON string literal.
///
/// A Windows path contains backslashes, which are escape introducers inside a
/// JavaScript double-quoted string. JSON-encoding the path keeps the driver
/// script syntactically valid on every platform.
fn json_string(value: &str) -> String {
    use std::fmt::Write as _;
    let mut out = String::from("\"");
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Renders the deterministic recipe driver used to execute the published CLI.
///
/// The driver pins the recipe clock at [`CLOCK`] with zero tick and replaces
/// the global `Date` constructor with one returning the same fixed instant, so
/// even code paths that bypass the recipe clock write reproducible values.
///
/// `sdk` is the SDK entry the driver imports and `entry` the published CLI
/// entry script; both are JSON-encoded before interpolation.
fn driver_script(sdk: &Path, entry: &Path) -> String {
    let sdk_json = json_string(&sdk.to_string_lossy());
    let entry_json = json_string(&entry.to_string_lossy());
    let template = r#"
// Differential-conformance driver: runs one real published-pm CLI invocation
// under a reproducible workspace recipe (fixed clock, zero tick) with the
// wall-clock Date pinned to the same instant. Both interpolated paths are
// JSON-encoded so Windows backslashes do not break the string literals, and
// both imports go through `pathToFileURL` so the specifiers are valid on every
// platform.
import { pathToFileURL } from "node:url";
const { runWithWorkspaceRecipe, release, claim, startTask } = await import(pathToFileURL(__SDK_JSON__));

const fixed = Date.parse(process.env.FIXED_CLOCK);
class pinnedDate extends Date {
  constructor(...args) {
    args.length === 0 ? super(fixed) : super(...args);
  }
  static now() {
    return fixed;
  }
}
globalThis.Date = pinnedDate;
process.argv = [process.argv[0], "pm", ...process.argv.slice(2)];
const recipe = {
  schema: "https://schema.unbrained.dev/pm/workspace-recipe/v1",
  clock: process.env.FIXED_CLOCK,
  tickMs: 0,
  seed: "conformance-seed",
  operations: [],
};
try {
  await runWithWorkspaceRecipe(recipe, async () => {
    if (process.argv[2] === "__release_bypass") {
      const result = await release(process.argv[3], { author: process.argv[4], ownershipReleaseBypass: true }, { path: process.cwd() });
      console.log(JSON.stringify(result));
    } else if (process.argv[2] === "__ownership_error") {
      try {
        await (process.argv[3] === "claim" ? claim : process.argv[3] === "start" ? startTask : release)(process.argv[4], { author: "alpha" }, { path: process.cwd() });
        console.log(JSON.stringify({ ok: true }));
      } catch (error) {
        console.log(JSON.stringify({ detail: error.message, exit_code: error.exitCode, context: error.context }));
      }
    } else if (process.argv[2] === "__start_if_available") {
      try {
        await startTask(process.argv[3], { author: process.argv[4], ifAvailable: true }, { path: process.cwd() });
        console.log(JSON.stringify({ ok: true }));
      } catch (error) {
        console.log(JSON.stringify({ detail: error.message, exit_code: error.exitCode, context: error.context }));
      }
    } else {
      await import(pathToFileURL(__ENTRY_JSON__));
    }
  });
} catch (error) {
  if (error && error.name !== "CommanderError") {
    console.error(error);
    process.exitCode = 1;
  }
}
"#;
    template
        .replace("__SDK_JSON__", &sdk_json)
        .replace("__ENTRY_JSON__", &entry_json)
}

/// Writes the driver script into the scratch directory and returns its path.
pub fn write_driver(
    directory: &Path,
    published: &PublishedCli,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let sdk = published.package_root.join("dist/cli-bundle/sdk.js");
    if !sdk.is_file() {
        return Err(format!("published SDK bundle not found at {}", sdk.display()).into());
    }
    let path = directory.join("conformance-driver.mjs");
    fs::write(&path, driver_script(&sdk, &published.entry))?;
    Ok(path)
}

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
