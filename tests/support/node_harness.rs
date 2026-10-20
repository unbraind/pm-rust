//! Shared runtime capability boundary for typed behavioral harnesses.
use std::{ffi::OsString, process::Command};

/// A capable runtime and its native test-runner spelling.
pub struct Interpreter {
    /// Selected executable, preserving the caller's runtime override.
    executable: OsString,
    /// Node and Bun expose their test runners with different arguments.
    test_flag: &'static str,
}

impl Interpreter {
    /// Starts a typed script using the capability-checked runtime.
    pub fn command(&self) -> Command {
        Command::new(&self.executable)
    }

    /// Starts the runtime's test runner without changing script semantics.
    #[allow(dead_code)]
    pub fn test_command(&self) -> Command {
        let mut command = self.command();
        command.arg(self.test_flag);
        command
    }
}

/// Finds a typed-module interpreter, skipping unavailable optional tooling only.
pub fn interpreter_or_skip() -> Result<Option<Interpreter>, Box<dyn std::error::Error>> {
    let required = std::env::var_os("PM_RUST_REQUIRE_PUBLISHED_CLI").is_some();
    let interpreter = std::env::var_os("PM_NODE_INTERPRETER").unwrap_or_else(|| "node".into());
    let capability = match Command::new(&interpreter)
        .args([
            "--input-type=module",
            "-e",
            "await import('./scripts/context-real-helpers.mts'); console.log(process.versions.bun ? 'test' : '--test')",
        ])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
    {
        Ok(output) => output,
        Err(error) if !required && error.kind() == std::io::ErrorKind::NotFound => {
            println!("skip: Node harness needs an interpreter with type stripping");
            return Ok(None);
        }
        Err(error) => return Err(error.into()),
    };
    if !capability.status.success() && !required {
        println!("skip: Node harness needs an interpreter with type stripping");
        return Ok(None);
    }
    assert!(
        capability.status.success(),
        "Node harness requires type stripping when PM_RUST_REQUIRE_PUBLISHED_CLI is set: {}",
        String::from_utf8_lossy(&capability.stderr)
    );
    let test_flag = if String::from_utf8_lossy(&capability.stdout).trim() == "test" {
        "test"
    } else {
        "--test"
    };
    Ok(Some(Interpreter {
        executable: interpreter,
        test_flag,
    }))
}
