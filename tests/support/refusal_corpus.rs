//! Shared bare/decorated authority cases for registration, redaction and process refusals.

/// Input and emitted URL text; equality denotes a public value with safe retry advice.
pub const AUTHORITIES: &[(&str, &str)] = &[
    (
        "https://stored and private-token@host.invalid",
        "https://[redacted]@host.invalid",
    ),
    (
        "https://stored user@host.invalid",
        "https://[redacted]@host.invalid",
    ),
    (
        "https://user:private-token@host.invalid",
        "https://[redacted]@host.invalid",
    ),
    (
        "https://host.invalid or user@example.invalid",
        "https://host.invalid or user@example.invalid",
    ),
    (
        "https://host.invalid and user@example.invalid",
        "https://host.invalid and user@example.invalid",
    ),
    (
        "https://host.invalid/path user@example.invalid",
        "https://host.invalid/path user@example.invalid",
    ),
    (
        "https://host.invalid?ref=user@example.invalid",
        "https://host.invalid?ref=user@example.invalid",
    ),
    (
        "https://host.invalid#user@example.invalid",
        "https://host.invalid#user@example.invalid",
    ),
    (
        "https://host.invalid/path@ref",
        "https://host.invalid/path@ref",
    ),
    ("user@example.invalid", "user@example.invalid"),
];

/// Identical URL boundaries apply regardless of surrounding producer decoration.
pub const DECORATIONS: &[&str] = &["", "Gate: ", "Contact "];
