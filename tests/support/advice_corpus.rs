//! Shared adversarial argv and a POSIX shell parser receipt.

/// Shell syntax remains data, including whitespace, substitutions and quote boundaries.
pub const VALUES: &[&str] = &[
    "sample-ordinary",
    "sample-missing;printf injected",
    "sample-missing&&printf injected",
    "sample-missing|printf injected",
    "sample-$(printf injected)",
    "sample-`printf injected`",
    "sample-line\nprintf injected",
    "sample-\"double'quote",
    "sample-\\tail",
    "sample-☃",
    "path with space\\",
    "",
];

/// Parses advice with a real shell without invoking pm or any mutation command.
#[cfg(unix)]
pub fn assert_shell_args(
    command: &str,
    expected: &[&str],
) -> Result<(), Box<dyn std::error::Error>> {
    let script = format!("set -- {command}; printf '%s\\000' \"$@\"");
    let output = std::process::Command::new("sh")
        .args(["-c", &script])
        .output()?;
    assert!(output.status.success(), "shell parse must succeed");
    let actual = output.stdout.split(|byte| *byte == 0).collect::<Vec<_>>();
    let mut bytes = expected
        .iter()
        .map(|arg| arg.as_bytes())
        .collect::<Vec<_>>();
    bytes.push(b"");
    assert_eq!(
        actual, bytes,
        "advice must preserve exactly its intended argv"
    );
    Ok(())
}
