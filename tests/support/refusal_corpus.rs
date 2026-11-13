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
    (
        "https://intranet pass@host.invalid",
        "https://[redacted]@host.invalid",
    ),
    (
        "https://u:p@host.invalid",
        "https://[redacted]@host.invalid",
    ),
    (
        "https://127.0.0.1:8080 then ops@example.invalid",
        "https://127.0.0.1:8080 then ops@example.invalid",
    ),
    (
        "https://[::1] and admin@example.invalid",
        "https://[::1] and admin@example.invalid",
    ),
    (
        "https://[::1]:8080 then ops@example.invalid",
        "https://[::1]:8080 then ops@example.invalid",
    ),
    (
        "https://localhost or ops@example.invalid",
        "https://localhost or ops@example.invalid",
    ),
    (
        "https://localhost:8080 then ops@example.invalid",
        "https://localhost:8080 then ops@example.invalid",
    ),
    (
        "https://stored.name user@host.invalid",
        "https://stored.name user@host.invalid",
    ),
    (
        "https://stored.name user name@host.invalid",
        "https://stored.name user name@host.invalid",
    ),
    (
        "https:// host@host.invalid",
        "https://[redacted]@host.invalid",
    ),
    (
        "https://bad..label pass@host.invalid",
        "https://[redacted]@host.invalid",
    ),
    (
        "https://localhost:bad pass@host.invalid",
        "https://[redacted]@host.invalid",
    ),
    (
        "https://[bad] pass@host.invalid",
        "https://[redacted]@host.invalid",
    ),
    (
        "https://[::1]junk pass@host.invalid",
        "https://[redacted]@host.invalid",
    ),
    (
        "https://localhost: pass@host.invalid",
        "https://[redacted]@host.invalid",
    ),
    (
        "https://localhost:65536 pass@host.invalid",
        "https://[redacted]@host.invalid",
    ),
    (
        "https://[bad pass@host.invalid",
        "https://[redacted]@host.invalid",
    ),
    (
        "https://LOCALHOST:80 then ops@example.invalid",
        "https://LOCALHOST:80 then ops@example.invalid",
    ),
    (
        "https://127.0.0.1:8080/path then ops@example.invalid",
        "https://127.0.0.1:8080/path then ops@example.invalid",
    ),
    (
        "https://host.invalid:8080?ref=x then ops@example.invalid",
        "https://host.invalid:8080?ref=x then ops@example.invalid",
    ),
    (
        "https://localhost@host.invalid then text",
        "https://[redacted]@host.invalid then text",
    ),
    (
        "https://bad.name:secret:2 pass@host.invalid",
        "https://[redacted]@host.invalid",
    ),
    (
        "https://bad!.name pass@host.invalid",
        "https://[redacted]@host.invalid",
    ),
    (
        "https://-bad.name pass@host.invalid",
        "https://[redacted]@host.invalid",
    ),
    (
        "https://bad-.name pass@host.invalid",
        "https://[redacted]@host.invalid",
    ),
    (
        "https://host.invalid. then ops@example.invalid",
        "https://host.invalid. then ops@example.invalid",
    ),
    (
        "https://host-name.invalid then ops@example.invalid",
        "https://host-name.invalid then ops@example.invalid",
    ),
    (
        "https://bad!x.name pass@host.invalid",
        "https://[redacted]@host.invalid",
    ),
    ("user@example.invalid", "user@example.invalid"),
];

/// Identical URL boundaries apply regardless of surrounding producer decoration.
pub const DECORATIONS: &[&str] = &["", "Gate: ", "Contact "];
