//! Lossless option boundaries shared by bootstrap, dispatch and refusal projection.
use crate::cli::Cli;
use clap::CommandFactory;
use std::sync::OnceLock;

/// One real option, excluding positionals and values that resemble flags.
pub struct OptionToken<'a> {
    /// Canonical long spelling, including aliases resolved through the CLI definition.
    pub name: &'a str,
    /// First physical argv index occupied by the option.
    pub start: usize,
    /// First physical argv index after this option and its values.
    pub end: usize,
    /// Explicit value, including an attached equals value.
    pub value: Option<&'a str>,
    /// Whether this option precedes the command.
    pub bootstrap: bool,
    /// Whether this spelling is fixed by the native or published CLI.
    pub(crate) declared: bool,
}

/// Parsed boundaries retain original argv positions without interpreting positional data.
pub struct Invocation<'a> {
    /// The first command token, after consuming bootstrap option values.
    pub command: Option<usize>,
    /// Indices of the recognized command name, including its optional item namespace.
    pub(crate) command_tokens: std::ops::Range<usize>,
    /// Position of the delimiter when it ends option parsing.
    pub(crate) delimiter: Option<usize>,
    /// Actual option occurrences before the argument delimiter.
    pub options: Vec<OptionToken<'a>>,
}

/// One immutable parser definition supplies arity and aliases for every consumer.
fn definition() -> &'static clap::Command {
    static DEFINITION: OnceLock<clap::Command> = OnceLock::new();
    DEFINITION.get_or_init(|| {
        let mut command = Cli::command();
        command.build();
        command
    })
}

/// Resolves a long name, alias or exact short spelling in one parser scope.
fn declared_option<'a>(command: &'a clap::Command, spelling: &str) -> Option<&'a clap::Arg> {
    command.get_arguments().find(|argument| {
        argument
            .get_long()
            .is_some_and(|name| spelling.strip_prefix("--") == Some(name))
            || argument.get_all_aliases().is_some_and(|aliases| {
                aliases
                    .iter()
                    .any(|name| spelling.strip_prefix("--") == Some(*name))
            })
            || argument
                .get_short()
                .is_some_and(|name| spelling == format!("-{name}"))
    })
}

impl<'a> Invocation<'a> {
    /// Parses declared boundaries; a standalone `--` ends options unless consumed as a value.
    pub fn parse(args: &'a [String]) -> Self {
        let mut result = Self {
            command: None,
            command_tokens: 0..0,
            delimiter: None,
            options: Vec::new(),
        };
        let mut command = definition();
        let mut index = 0;
        while let Some(token) = args.get(index) {
            if token == "--" {
                result.delimiter = Some(index);
                break;
            }
            if token.starts_with('-') && token != "-" {
                let start = index;
                let (spelling, attached) = token
                    .split_once('=')
                    .map_or((token.as_str(), None), |(name, value)| (name, Some(value)));
                let argument = declared_option(command, spelling);
                let name = argument
                    .and_then(clap::Arg::get_long)
                    .unwrap_or(spelling.trim_start_matches('-'));
                // Published shared controls also classify diagnostics for unsupported native options.
                let shared_value = matches!(
                    name,
                    "workspace" | "pm-path" | "path" | "author" | "output-format" | "timestamp"
                );
                let takes_value =
                    argument.map_or(shared_value, |arg| arg.get_action().takes_values());
                let range = argument.and_then(clap::Arg::get_num_args);
                let maximum = range.map_or(usize::from(takes_value), |range| range.max_values());
                let allow_hyphen = argument.is_some_and(clap::Arg::is_allow_hyphen_values_set);
                let mut value = attached;
                index += 1;
                if attached.is_none() {
                    for _ in 0..maximum {
                        let Some(next) = args.get(index) else {
                            break;
                        };
                        if next.starts_with('-') && next != "-" && !allow_hyphen {
                            let spelling =
                                next.split_once('=').map_or(next.as_str(), |(name, _)| name);
                            // Notes accept free text beginning with a hyphen, up to declared options.
                            if name != "note"
                                || spelling == "--"
                                || spelling == "--output-format"
                                || declared_option(command, spelling).is_some()
                                || declared_option(definition(), spelling).is_some()
                            {
                                break;
                            }
                        }
                        value.get_or_insert(next.as_str());
                        index += 1;
                    }
                }
                result.options.push(OptionToken {
                    name: if matches!(name, "pm-path" | "path") {
                        "workspace"
                    } else {
                        name
                    },
                    start,
                    end: index,
                    value,
                    bootstrap: result.command.is_none(),
                    declared: argument.is_some() || shared_value,
                });
                continue;
            }
            if result.command.is_none() {
                result.command = Some(index);
                // Native item aliases are normalized after bootstrap recognition.
                let name = if token == "item" {
                    args.get(index + 1).map_or("", String::as_str)
                } else {
                    token.as_str()
                };
                if let Some(subcommand) =
                    definition().find_subcommand(if name == "comments" { "comment" } else { name })
                {
                    command = subcommand;
                    result.command_tokens = index..index + 1 + usize::from(token == "item");
                }
            }
            index += 1;
        }
        result
    }

    /// Explicit tracker scope is meaningful only before the command.
    #[must_use]
    pub fn scope(&self) -> Option<&'a str> {
        self.options
            .iter()
            .find(|option| option.bootstrap && option.name == "workspace")
            .and_then(|option| option.value)
    }

    /// Whether this physical argv index is a real author/output control.
    #[must_use]
    pub fn is_identity_or_format(&self, index: usize) -> bool {
        self.options.iter().any(|option| {
            option.start == index && matches!(option.name, "author" | "json" | "output-format")
        })
    }
}
