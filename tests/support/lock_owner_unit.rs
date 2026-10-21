//! Kernel identity receipts remain conservative for missing or foreign coordinates.
use super::*;

#[test]
fn malformed_receipts_are_unknown() {
    for value in [
        serde_json::json!(null),
        serde_json::json!({"pid":0}),
        serde_json::json!({"pid":-1}),
        serde_json::json!({"pid":1}),
        serde_json::json!({"pid":1,"host_identity":"foreign","process_start_identity":"0"}),
        serde_json::json!({"pid":1,"host_identity":"foreign","process_start_identity":"bad"}),
    ] {
        assert_eq!(inspect(&value), "unknown");
    }
}

#[cfg(target_os = "linux")]
#[test]
fn kernel_receipts_distinguish_live_dead_and_reused_processes()
-> Result<(), Box<dyn std::error::Error>> {
    let mut identity = capture();
    identity["pid"] = serde_json::json!(std::process::id());
    assert_eq!(inspect(&identity), "alive");
    identity["process_start_identity"] = serde_json::json!("0");
    assert_eq!(inspect(&identity), "dead");
    let mut child = std::process::Command::new("sh")
        .args(["-c", "exit 0"])
        .spawn()?;
    identity["pid"] = serde_json::json!(child.id());
    child.wait()?;
    assert_eq!(inspect(&identity), "dead");
    Ok(())
}

#[cfg(target_os = "linux")]
#[test]
fn malformed_kernel_coordinates_are_refused() {
    for raw in ["", "(name)", "(name) x", "(broken"] {
        assert_eq!(process_start(raw), None);
    }
    assert_eq!(
        process_start(&format!(
            "123 (parenthesized (name)) {} 456",
            vec!["0"; 19].join(" ")
        )),
        Some("456".to_owned())
    );
    assert_eq!(
        process_start(&format!("123 (name) {} bad", vec!["0"; 19].join(" "))),
        None
    );
    for (boot, ns) in [
        ("", "bad"),
        ("", "pid:[1]"),
        ("00000000-0000-0000-0000-00000000000A", "pid:[1]"),
        ("00000000-0000-0000-0000-000000000000", "pid:[]"),
        ("00000000-0000-0000-0000-000000000000", "pid:[bad]"),
        ("00000000-0000-0000-0000-00000000000Z", "pid:[1]"),
    ] {
        assert_eq!(host_identity(boot, ns), None);
    }
}

#[cfg(target_os = "linux")]
#[test]
fn inaccessible_kernel_coordinates_never_authorize_recovery()
-> Result<(), Box<dyn std::error::Error>> {
    use std::os::unix::fs::symlink;
    let scratch = tempfile::tempdir()?;
    assert_eq!(capture_at(scratch.path()), serde_json::json!({}));
    let boot = "00000000-0000-0000-0000-000000000000";
    std::fs::create_dir_all(scratch.path().join("sys/kernel/random"))?;
    std::fs::write(scratch.path().join("sys/kernel/random/boot_id"), boot)?;
    assert_eq!(current_host(scratch.path()), None);
    std::fs::create_dir_all(scratch.path().join("self/ns"))?;
    symlink("pid:[1]", scratch.path().join("self/ns/pid"))?;
    std::fs::write(scratch.path().join("self/stat"), "bad")?;
    assert_eq!(capture_at(scratch.path()), serde_json::json!({}));
    let value = serde_json::json!({"pid":1,"host_identity":host_identity(boot,"pid:[1]"),"process_start_identity":"1"});
    assert_eq!(
        inspect_at(&value, scratch.path(), |_| Err(rustix::io::Errno::PERM)),
        "unknown"
    );
    assert_eq!(inspect_at(&value, scratch.path(), |_| Ok(())), "unknown");
    std::fs::create_dir(scratch.path().join("1"))?;
    std::fs::write(scratch.path().join("1/stat"), "broken")?;
    assert_eq!(inspect_at(&value, scratch.path(), |_| Ok(())), "unknown");
    assert_eq!(host_identity(boot, "pid:[1"), None);
    assert_eq!(
        capture()
            .get("host_identity")
            .and_then(Value::as_str)
            .map(str::len),
        Some(64)
    );
    for value in [
        serde_json::json!({"pid":1,"host_identity":"h"}),
        serde_json::json!({"pid":1,"host_identity":"h","process_start_identity":""}),
        serde_json::json!({"pid":9_223_372_036_854_775_807_i64}),
    ] {
        assert_eq!(inspect(&value), "unknown");
    }
    Ok(())
}
