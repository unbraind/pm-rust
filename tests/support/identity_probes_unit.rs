//! Real local session fixtures for collection attribution probes.
use super::*;
use serde_json::Map;
use std::io::Write as _;
type TestResult = Result<(), Box<dyn std::error::Error>>;

/// Creates a provider home and deterministic native identity environment.
fn fixture(
    harness: &str,
) -> Result<(tempfile::TempDir, BTreeMap<String, String>), Box<dyn std::error::Error>> {
    let root = tempfile::tempdir()?;
    let mut env = BTreeMap::from([
        (
            "HOME".to_owned(),
            root.path().to_string_lossy().into_owned(),
        ),
        (
            "USERPROFILE".to_owned(),
            root.path().to_string_lossy().into_owned(),
        ),
    ]);
    if harness == "codex" {
        env.insert("CODEX_THREAD_ID".to_owned(), "fixture-session".to_owned());
    } else {
        env.insert("CLAUDE_CODE".to_owned(), "1".to_owned());
        env.insert(
            "CLAUDE_CODE_SESSION_ID".to_owned(),
            "fixture-session".to_owned(),
        );
    }
    Ok((root, env))
}

/// Local Codex records select newest turn metadata and emit hashed-record resolver evidence.
#[test]
fn codex_turns_and_probe_controls() -> TestResult {
    let (root, mut env) = fixture("codex")?;
    let path = root.path().join(".codex/sessions/year/month");
    fs::create_dir_all(&path)?;
    fs::write(
        path.join("rollout-fixture-session.jsonl"),
        "invalid\n[]\nnull\n{\"type\":\"other\"}\n{\"type\":\"turn_context\",\"payload\":{\"model\":\"old\",\"effort\":\"low\"}}\n{\"type\":\"turn_context\",\"payload\":{\"model\":\"new\"}}\n",
    )?;
    env.insert("AI_AGENT".to_owned(), "codex/1.2.3".to_owned());
    let identity = AuthorIdentity::resolve(Some("fixture"), None, &env).for_collections(&env);
    assert_eq!(identity.provenance["model"]["value"], "new");
    assert_eq!(identity.provenance["effort"]["value"], "low");
    assert_eq!(identity.provenance["version"]["value"], "1.2.3");
    assert_eq!(identity.resolver_outcomes["model"]["status"], "resolved");
    env.insert("PM_AGENT_PROBES".to_owned(), "off".to_owned());
    let identity = AuthorIdentity::resolve(Some("fixture"), None, &env).for_collections(&env);
    assert_eq!(
        identity.resolver_outcomes["model"]["reason"],
        "probes_disabled"
    );
    env.insert("PM_AGENT_MODEL".to_owned(), "override".to_owned());
    let identity = AuthorIdentity::resolve(Some("fixture"), None, &env).for_collections(&env);
    assert!(!identity.resolver_outcomes.contains_key("model"));
    env.insert("PM_AGENT_EFFORT".to_owned(), "high".to_owned());
    let identity = AuthorIdentity::resolve(Some("fixture"), None, &env).for_collections(&env);
    assert!(!identity.resolver_outcomes.contains_key("effort"));
    env.remove("PM_AGENT_EFFORT");
    env.remove("PM_AGENT_PROBES");
    env.remove("CODEX_THREAD_ID");
    env.insert("CODEX_HOME".to_owned(), "fixture".to_owned());
    let identity = AuthorIdentity::resolve(None, None, &env).for_collections(&env);
    assert_eq!(
        identity.resolver_outcomes["effort"]["reason"],
        "harness_unavailable"
    );
    env.insert("CODEX_THREAD_ID".to_owned(), "invalid/id".to_owned());
    let identity = AuthorIdentity::resolve(None, None, &env).for_collections(&env);
    assert_eq!(
        identity.resolver_outcomes["effort"]["reason"],
        "resolver_failed"
    );
    env.insert("CODEX_THREAD_ID".to_owned(), "valid-id".to_owned());
    env.remove("HOME");
    env.remove("USERPROFILE");
    assert_eq!(local_probe("codex", &env, root.path()).status, "available");
    Ok(())
}

/// Directory search budgets, regular-file checks and fallback ambiguity are deterministic.
#[test]
fn session_file_selection_boundaries() -> TestResult {
    let root = tempfile::tempdir()?;
    assert!(codex_file(root.path(), "-id.jsonl", 5, &mut 0).is_none());
    assert!(codex_file(root.path(), "-id.jsonl", 0, &mut 512).is_none());
    assert!(codex_file(&root.path().join("absent"), "-id.jsonl", 0, &mut 0).is_none());
    let nested = root.path().join("nested");
    fs::create_dir(&nested)?;
    fs::write(nested.join("new-id.jsonl"), "")?;
    assert!(codex_file(root.path(), "-id.jsonl", 0, &mut 0).is_some());
    fs::write(root.path().join("a.txt"), "")?;
    assert!(codex_file(root.path(), "-absent.jsonl", 0, &mut 0).is_none());
    assert!(codex_file(root.path(), "-absent.jsonl", 0, &mut 511).is_none());
    assert_eq!(file_status(root.path()), "missing");
    assert_eq!(file_status(&root.path().join("absent")), "missing");
    let invalid = root.path().join("invalid\0name");
    assert_eq!(file_status(&invalid), "failed");
    assert_eq!(claude_file(&invalid, "id", root.path()).1, "failed");
    assert_eq!(claude_fallback(&invalid, "id").1, "failed");
    assert_eq!(
        claude_fallback(&root.path().join("absent"), "id").1,
        "missing"
    );
    assert_eq!(claude_candidates(vec![invalid]).1, "failed");
    assert_eq!(claude_file(root.path(), "id", root.path()).1, "missing");
    fs::write(nested.join("id.jsonl"), "")?;
    assert_eq!(claude_file(root.path(), "id", root.path()).1, "available");
    let second = root.path().join("second");
    fs::create_dir(&second)?;
    fs::write(second.join("id.jsonl"), "")?;
    assert_eq!(claude_file(root.path(), "id", root.path()).1, "ambiguous");
    let cwd = root.path().join("project");
    let slug: String = cwd
        .to_string_lossy()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '-'
            }
        })
        .collect();
    let preferred = root.path().join(slug);
    fs::create_dir(&preferred)?;
    fs::write(preferred.join("id.jsonl"), "")?;
    assert_eq!(
        claude_file(root.path(), "id", &cwd).0,
        Some(preferred.join("id.jsonl"))
    );
    Ok(())
}

/// Claude JSON objects preserve first values, breadth/depth ceilings and provider aliases.
#[test]
fn claude_metadata_and_unavailable_records() -> TestResult {
    let (root, mut env) = fixture("claude-code")?;
    let path = root.path().join(".claude/projects/project");
    fs::create_dir_all(&path)?;
    fs::write(
        path.join("fixture-session.jsonl"),
        "{\"message\":{\"model\":\"first\",\"version\":\"1.2.3\"}}\n{\"model\":\"last\"}\n",
    )?;
    let identity = AuthorIdentity::resolve(None, None, &env).for_collections(&env);
    assert_eq!(identity.provenance["model"]["value"], "first");
    assert_eq!(identity.provenance["version"]["value"], "1.2.3");
    env.insert("CLAUDE_MODEL".to_owned(), "alias".to_owned());
    let identity = AuthorIdentity::resolve(None, None, &env).for_collections(&env);
    assert_eq!(identity.provenance["model"]["source"], "environment");
    env.insert("CLAUDE_CODE_SESSION_ID".to_owned(), "absent".to_owned());
    let identity = AuthorIdentity::resolve(None, None, &env).for_collections(&env);
    assert_eq!(
        identity.resolver_outcomes["version"]["reason"],
        "resolver_input_missing"
    );
    env.insert("CLAUDE_CODE_SESSION_ID".to_owned(), "bad/id".to_owned());
    assert_eq!(
        local_probe("claude-code", &env, root.path()).status,
        "unavailable"
    );
    env.insert("CLAUDE_CODE_SESSION_ID".to_owned(), "valid-id".to_owned());
    env.remove("HOME");
    env.remove("USERPROFILE");
    assert_eq!(
        local_probe("claude-code", &env, root.path()).status,
        "missing"
    );
    let mut found = BTreeMap::new();
    claude_row(&json!(["ignored"]), &mut found);
    assert!(found.is_empty());
    claude_row(
        &json!({"a":{"b":{"c":{"d":{"e":{"model":"too-deep"}}}}},"wrong":{},"model":7}),
        &mut found,
    );
    assert!(found.is_empty());
    let mut object = Map::new();
    for index in 0..70 {
        object.insert(index.to_string(), json!({"ignored":true}));
    }
    claude_row(&Value::Object(object), &mut found);
    assert!(found.is_empty());
    Ok(())
}

/// Provider chunks obey their byte and line ceilings, including malformed rows.
#[test]
fn bounded_reader_order_and_limits() -> TestResult {
    use std::io::Cursor;
    let mut empty = Cursor::new(Vec::<u8>::new());
    assert!(read_probe(&mut empty, true)?.is_empty());
    let mut invalid = Cursor::new(vec![0xff]);
    assert!(read_probe(&mut invalid, false)?.is_empty());
    let mut rows = BTreeMap::new();
    row(&"x".repeat(262_145), true, &mut rows);
    assert!(rows.is_empty());
    let root = tempfile::tempdir()?;
    for codex in [true, false] {
        let path = root
            .path()
            .join(if codex { "codex.jsonl" } else { "claude.jsonl" });
        let mut file = fs::File::create(&path)?;
        let cap = if codex { 4_194_304 } else { 1_048_576 };
        file.set_len(cap + 1024)?;
        file.write_all(if codex {
            b"{\"type\":\"turn_context\",\"payload\":{\"effort\":\"head\"}}\n"
        } else {
            b"{\"model\":\"head\"}\n"
        })?;
        file.seek(SeekFrom::End(-100))?;
        file.write_all(if codex {
            b"\n{\"type\":\"turn_context\",\"payload\":{\"model\":\"tail\"}}\n"
        } else {
            b"\n{\"version\":\"tail\"}\n"
        })?;
        drop(file);
        let found = read_probe(&mut fs::File::open(path)?, codex)?;
        assert_eq!(found[if codex { "model" } else { "version" }], "tail");
        assert_eq!(found[if codex { "effort" } else { "model" }], "head");
    }
    let raw = format!(
        "{{\"type\":\"turn_context\",\"payload\":{{\"model\":\"old\"}}}}\n{}",
        "{}\n".repeat(4096)
    );
    assert!(read_probe(&mut Cursor::new(raw.as_bytes()), true)?.is_empty());
    Ok(())
}

/// Version suffixes and explicit environment aliases retain their source classification.
#[test]
fn version_and_alias_controls() {
    for (raw, expected, status) in [
        ("", None, "unavailable"),
        ("other/1.2.3", None, "unavailable"),
        ("codex", None, "available"),
        ("codex/1.2", Some("1.2"), "available"),
        ("codex v1.2.3-beta.1", Some("1.2.3-beta.1"), "available"),
        ("codex/1", None, "available"),
        ("codex/1..2", None, "available"),
        ("codex/1.2-", None, "available"),
        ("codex/1.2+bad+suffix", None, "available"),
        ("codex/1.2.3.4", None, "available"),
    ] {
        let found = agent_version(raw, "codex");
        assert_eq!((found.0.as_deref(), found.1), (expected, status), "{raw}");
    }
    for (raw, value) in [
        ("codex\\v1.2.3", None),
        ("codex:1.2.3", None),
        ("codex=1.2.3", None),
        ("codex @v1.2.3", Some("1.2.3")),
    ] {
        assert_eq!(agent_version(raw, "codex").0.as_deref(), value);
    }
    assert_eq!(text("\u{0085}"), Some("\u{0085}".to_owned()));
    assert!(!session(""));
    assert!(!session(&"x".repeat(129)));
    assert!(session("valid_id-1"));
    assert_eq!(text("\u{feff} value "), Some("value".to_owned()));
    assert_eq!(text("  "), None);
    let mut env = BTreeMap::from([("PI_AGENT".to_owned(), "1".to_owned())]);
    assert!(
        AuthorIdentity::resolve(None, None, &env)
            .for_collections(&env)
            .resolver_outcomes
            .is_empty()
    );
    env = BTreeMap::from([
        ("CODEX_HOME".to_owned(), "fixture".to_owned()),
        ("CODEX_EFFORT".to_owned(), "high".to_owned()),
    ]);
    let identity = AuthorIdentity::resolve(None, None, &env).for_collections(&env);
    assert_eq!(identity.provenance["effort"]["value"], "high");
}

/// Models a storage device losing seek capability at either bounded-read boundary.
struct SeekFailure {
    /// Real in-memory byte stream for successful operations.
    source: std::io::Cursor<Vec<u8>>,
    /// Operation to refuse.
    fail_at: usize,
    /// Number of seeks attempted.
    calls: usize,
}
impl Read for SeekFailure {
    fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
        self.source.read(bytes)
    }
}
impl Seek for SeekFailure {
    fn seek(&mut self, position: SeekFrom) -> std::io::Result<u64> {
        self.calls += 1;
        if self.calls == self.fail_at {
            Err(std::io::Error::other("fixture seek unavailable"))
        } else {
            self.source.seek(position)
        }
    }
}

/// Optional metadata I/O errors remain failures instead of successful empty byte snapshots.
#[test]
fn probe_reader_io_failures() -> TestResult {
    for fail_at in [1, 2] {
        let mut reader = SeekFailure {
            source: std::io::Cursor::new(b"{}".to_vec()),
            fail_at,
            calls: 0,
        };
        assert!(read_probe(&mut reader, true).is_err());
        let mut bytes = [0; 1];
        let _ = reader.read(&mut bytes)?;
    }
    let root = tempfile::tempdir()?;
    let mut write_only = fs::File::create(root.path().join("write-only.jsonl"))?;
    write_only.set_len(1)?;
    assert!(read_probe(&mut write_only, true).is_err());
    Ok(())
}
