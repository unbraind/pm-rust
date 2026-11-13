//! Argument-vector rendering and decoding boundaries.
use super::*;
use crate::test_advice_corpus as corpus;

/// Every argument spelling survives the platform renderer and its nonexecuting decoder.
#[test]
fn round15_command_argument_round_trip() {
    for value in corpus::VALUES {
        let args = ["--pm-path", value, "comment", value, "--message", value];
        let command = render_pm_command(args);
        let expected = std::iter::once("pm")
            .chain(args)
            .map(str::to_owned)
            .collect::<Vec<_>>();
        assert_eq!(parse_command_args(&command), Some(expected));
    }
    assert_eq!(
        render_pm_command(["get", "sample-ordinary"]),
        "pm get sample-ordinary"
    );
    assert_eq!(parse_command_args(" \t\n"), Some(vec![]));
    assert_eq!(parse_command_args("pm \"unterminated"), None);
}

/// POSIX escapes preserve literals without ever evaluating shell syntax.
#[cfg(not(windows))]
#[test]
fn round15_posix_decoder_edges() {
    for (text, args) in [
        ("pm 'single quoted'", vec!["pm", "single quoted"]),
        ("pm \"a\\qb\"", vec!["pm", "a\\qb"]),
        ("pm a\\ b", vec!["pm", "a b"]),
        ("pm a\\\nb", vec!["pm", "ab"]),
        ("pm \"a\\\nb\"", vec!["pm", "ab"]),
        ("pm 'a\\b'", vec!["pm", "a\\b"]),
    ] {
        assert_eq!(
            parse_command_args(text),
            Some(args.into_iter().map(str::to_owned).collect())
        );
    }
    for input in [
        "pm a;b",
        "pm a&&b",
        "pm a|b",
        "pm <input",
        "pm >output",
        "pm (x)",
        "pm $(x)",
        "pm `x`",
        "pm \"$(x)\"",
    ] {
        assert_eq!(parse_command_args(input), None);
    }
    assert_eq!(parse_command_args("\\\n"), Some(vec![]));
    assert_eq!(parse_command_args("pm trailing\\"), None);
    assert_eq!(parse_command_args("pm 'unclosed"), None);
}

/// Concrete commands preserve every argument when parsed by the platform's POSIX shell.
#[cfg(unix)]
#[test]
fn round15_posix_shell_argument_round_trip() -> Result<(), Box<dyn std::error::Error>> {
    for value in corpus::VALUES {
        let args = ["--pm-path", value, "comment", value, "--message", value];
        corpus::assert_shell_args(
            &render_pm_command(args),
            &std::iter::once("pm").chain(args).collect::<Vec<_>>(),
        )?;
    }
    Ok(())
}
