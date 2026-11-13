//! Argument-vector construction for executable recovery advice.

/// Renders a complete pm invocation, quoting every supplied argument.
#[must_use]
pub fn render_pm_command<I, S>(args: I) -> String
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    format!(
        "pm {}",
        args.into_iter()
            .map(|arg| crate::quote_command_arg(arg.as_ref()))
            .collect::<Vec<_>>()
            .join(" ")
    )
}

/// Decodes argument quoting without evaluating shell operators or substitutions.
///
/// This is the inverse of the platform renderer, not a shell interpreter. It
/// permits existing string-only advice to acquire bootstrap scope as an argv.
/// Unclosed quotes or escapes return `None` rather than executable advice.
/// On Windows, a backslash run before `"` emits half as many backslashes:
/// an even run toggles quoting, while an odd run emits a literal quote. Other
/// backslash runs stay literal; unquoted whitespace separates arguments.
#[must_use]
pub fn parse_command_args(text: &str) -> Option<Vec<String>> {
    let mut chars = text.chars().peekable();
    let mut args = Vec::new();
    let mut arg = String::new();
    let mut quote = None;
    let mut started = false;
    while let Some(ch) = chars.next() {
        if quote.is_none() && matches!(ch, ' ' | '\t' | '\n') {
            if started {
                args.push(std::mem::take(&mut arg));
                started = false;
            }
            continue;
        }
        #[cfg(not(windows))]
        if quote != Some('\'') && ch == '\\' && chars.peek() == Some(&'\n') {
            chars.next();
            continue;
        }
        #[cfg(not(windows))]
        if (quote.is_none() && matches!(ch, ';' | '&' | '|' | '<' | '>' | '(' | ')'))
            || (quote != Some('\'') && matches!(ch, '$' | '`'))
        {
            return None;
        }
        started = true;
        #[cfg(not(windows))]
        match (quote, ch) {
            (None, '\'' | '"') => quote = Some(ch),
            (Some(delimiter), ch) if ch == delimiter => quote = None,
            (None | Some('"'), '\\') => {
                let escaped = chars.next()?;
                if quote.is_some() && !matches!(escaped, '$' | '`' | '"' | '\\') {
                    arg.push('\\');
                }
                arg.push(escaped);
            }
            _ => arg.push(ch),
        }
        #[cfg(windows)]
        match ch {
            '"' => quote = if quote.is_some() { None } else { Some('"') },
            '\\' => {
                let mut count = 1;
                while chars.peek() == Some(&'\\') {
                    chars.next();
                    count += 1;
                }
                if chars.peek() == Some(&'"') {
                    chars.next();
                    arg.push_str(&"\\".repeat(count / 2));
                    if count % 2 == 1 {
                        arg.push('"');
                    } else {
                        quote = if quote.is_some() { None } else { Some('"') };
                    }
                } else {
                    arg.push_str(&"\\".repeat(count));
                }
            }
            _ => arg.push(ch),
        }
    }
    if quote.is_some() {
        return None;
    }
    if started {
        args.push(arg);
    }
    Some(args)
}

#[cfg(test)]
#[path = "../tests/support/command_line_unit.rs"]
mod tests;
