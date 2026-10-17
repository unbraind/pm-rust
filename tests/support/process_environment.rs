//! Runtime-only environments shared by native and published CLI fixtures.

/// Gives both implementations the same runtime environment without inherited agent context.
pub fn isolate_environment(command: &mut std::process::Command) {
    command.env_clear();
    // The clear is deliberate - the point is a reproducible environment - but a
    // few variables are load-bearing for the interpreter itself rather than for
    // the program under test. On Windows, node.exe fails to start without
    // SystemRoot, so clearing it breaks the launch before the published CLI
    // ever runs. Restore those from the parent, then apply the deterministic
    // overrides on top so they still win.
    for name in [
        "LLVM_PROFILE_FILE",
        "PATH",
        "HOME",
        "SystemRoot",
        "SystemDrive",
        "windir",
        "TEMP",
        "TMP",
        "USERPROFILE",
        "COMSPEC",
        "PATHEXT",
        "NUMBER_OF_PROCESSORS",
        "PROCESSOR_ARCHITECTURE",
    ] {
        if let Ok(value) = std::env::var(name) {
            command.env(name, value);
        }
    }
}
