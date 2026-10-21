//! Conservative boot, PID namespace and process-start receipts for item locks.
use serde_json::Value;

/// Extracts Linux field 22 after the potentially parenthesized process name.
#[cfg(target_os = "linux")]
fn process_start(raw: &str) -> Option<String> {
    let (_, fields) = raw.rsplit_once(')')?;
    let start = fields.split_whitespace().nth(19)?;
    start
        .bytes()
        .all(|byte| byte.is_ascii_digit())
        .then(|| start.to_owned())
}

/// Hashes validated kernel coordinates without exposing host names.
#[cfg(target_os = "linux")]
fn host_identity(boot: &str, namespace: &str) -> Option<String> {
    let boot = boot.trim();
    let pid = namespace.strip_prefix("pid:[")?.strip_suffix(']')?;
    if boot.len() != 36
        || !boot
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase() || byte == b'-')
        || pid.is_empty()
        || !pid.bytes().all(|byte| byte.is_ascii_digit())
    {
        return None;
    }
    Some(crate::history::sha256_digest(
        format!("{boot}:{namespace}").as_bytes(),
    ))
}

/// Reads only coordinates the running kernel can verify in this namespace.
#[cfg(target_os = "linux")]
fn current_host(root: &std::path::Path) -> Option<String> {
    let boot = std::fs::read_to_string(root.join("sys/kernel/random/boot_id")).ok()?;
    let namespace = std::fs::read_link(root.join("self/ns/pid")).ok()?;
    host_identity(&boot, &namespace.to_string_lossy())
}

/// Captures both coordinates together, or publishes neither on unsupported hosts.
pub(crate) fn capture() -> Value {
    #[cfg(target_os = "linux")]
    {
        capture_at(std::path::Path::new("/proc"))
    }
    #[cfg(not(target_os = "linux"))]
    {
        serde_json::json!({})
    }
}

/// Captures a receipt from one procfs mount, leaving inaccessible coordinates unknown.
#[cfg(target_os = "linux")]
fn capture_at(root: &std::path::Path) -> Value {
    let receipt = current_host(root).zip(
        std::fs::read_to_string(root.join("self/stat"))
            .ok()
            .and_then(|raw| process_start(&raw)),
    );
    if let Some((host, start)) = receipt {
        return serde_json::json!({"host_identity":host,"process_start_identity":start});
    }
    serde_json::json!({})
}

/// Classifies only a matching boot/namespace and a valid original-process receipt.
pub(crate) fn inspect(value: &Value) -> &'static str {
    #[cfg(target_os = "linux")]
    {
        inspect_at(
            value,
            std::path::Path::new("/proc"),
            rustix::process::test_kill_process,
        )
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = value;
        "unknown"
    }
}

/// Separates process observation from classification for inaccessible-kernel fault probes.
#[cfg(target_os = "linux")]
fn inspect_at(
    value: &Value,
    root: &std::path::Path,
    probe: fn(rustix::process::Pid) -> rustix::io::Result<()>,
) -> &'static str {
    let Some(pid) = value["pid"]
        .as_i64()
        .and_then(|pid| i32::try_from(pid).ok())
        .filter(|pid| *pid > 0)
        .and_then(rustix::process::Pid::from_raw)
    else {
        return "unknown";
    };
    let Some(host) = value["host_identity"].as_str() else {
        return "unknown";
    };
    let Some(start) = value["process_start_identity"]
        .as_str()
        .filter(|start| !start.is_empty() && start.bytes().all(|byte| byte.is_ascii_digit()))
    else {
        return "unknown";
    };
    if current_host(root).as_deref() != Some(host) {
        return "unknown";
    }
    match probe(pid) {
        Err(rustix::io::Errno::SRCH) => return "dead",
        Err(_) => return "unknown",
        Ok(()) => {}
    }
    let current = std::fs::read_to_string(root.join(format!("{}/stat", pid.as_raw_nonzero())))
        .ok()
        .and_then(|raw| process_start(&raw));
    match current.as_deref() {
        None => "unknown",
        Some(current) if current == start => "alive",
        Some(_) => "dead",
    }
}

#[cfg(test)]
#[path = "../tests/support/lock_owner_unit.rs"]
mod tests;
