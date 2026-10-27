//! Deterministic prefix, schema and concurrent-admission property contracts.
use super::*;
use serde_json::{Value, json};
use std::{cell::Cell, io::Write, rc::Rc};

/// Fallible assertion result.
type TestResult = Result<(), Box<dyn std::error::Error>>;

/// Real native tracker; no SDK mocking or external executables.
fn fixture() -> Result<(tempfile::TempDir, Workspace), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let root = dir.path().join(".agents/pm");
    fs::create_dir_all(root.join("history"))?;
    fs::write(
        root.join("settings.json"),
        r#"{"id_prefix":"demo-","item_format":"toon"}"#,
    )?;
    Ok((dir, Workspace::discover(&root)?))
}

/// Appends a canonical compact-projectable record without imposing timestamp order.
fn append(workspace: &Workspace, id: &str, value: &Value, crlf: bool) -> io::Result<()> {
    let mut file = fs::OpenOptions::new().append(true).create(true).open(
        workspace
            .pm_root()
            .join("history")
            .join(format!("{id}.jsonl")),
    )?;
    write!(
        file,
        " {} {}",
        crate::stringify_json(value, false),
        if crlf { "\r\n" } else { "\n" }
    )
}

/// Previous-release event fields, including free-form strings and relationship patches.
fn record(index: u64) -> Value {
    let mut entry = json!({"ts":format!("2026-01-{:02}T00:00:00Z", index % 28 + 1),
        "author":format!("  foreign-author-{}  ", index % 4),
        "op":(["create","update","comment","custom"][usize::try_from(index % 4).unwrap_or(0)]),
        "patch":(0..index%5).map(|n| json!({"op":"replace","path":format!("/metadata/field-{n}"),"value":{"parent":"demo-peer","dependencies":[{"id":"demo-peer","kind":"custom"}],"status":format!("custom-active-{index}"),"body":format!("  text {index}\r\n雪  ")}})).collect::<Vec<_>>(),
        "foreign_field":{"constructor":true}});
    if index % 3 != 1 {
        entry["message"] = json!(format!("  text {index}\r\n雪  "));
    }
    entry
}

/// A fixed seeded model generates 500 operation sequences, including restart/replay.
#[test]
fn seeded_500_sequences_preserve_physical_events_and_foreign_data() -> TestResult {
    let (_dir, workspace) = fixture()?;
    let adapter = LocalChangeAdapter::new(workspace.clone(), SessionCancellation::default());
    let mut seed = 0x7cba_9342_u64;
    let mut expected = BTreeMap::<String, Vec<MutationEvent>>::new();
    let mut observed = BTreeMap::<String, Vec<MutationEvent>>::new();
    let mut request = ChangeRequest {
        limit: 3,
        ..ChangeRequest::default()
    };
    let mut saved = Vec::new();
    for sequence in 0..500 {
        for _ in 0..=((seed >> 8) % 4) {
            seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
            let id = format!("demo-{}", seed % 4);
            let entry = record(seed % 7); // Replay older input as a new physical event.
            append(&workspace, &id, &entry, sequence % 2 == 0)?;
            let events = expected.entry(id.clone()).or_default();
            // Reference projection is read BACK through the SDK, never normalized by the test.
            let output = workspace.history(
                &id,
                &crate::HistoryOptions {
                    full: true,
                    limit: Some("1".into()),
                    ..crate::HistoryOptions::default()
                },
            )?;
            let history = output["history"].as_array().ok_or("history")?;
            let authoritative = history.last().ok_or("entry")?;
            assert_eq!(authoritative, &entry);
            let version = events.len() as u64 + 1;
            events.push(MutationEvent {
                item_id: id,
                version,
                ts: authoritative["ts"].as_str().ok_or("ts")?.into(),
                author: authoritative["author"].as_str().ok_or("author")?.into(),
                operation: authoritative["op"].as_str().ok_or("op")?.into(),
                message: authoritative["message"].as_str().map(str::to_owned),
                patch_count: authoritative["patch"].as_array().ok_or("patch")?.len() as u64,
            });
        }
        if sequence % 5 == 0 {
            let page = adapter.pull(&request)?;
            page.validate(&request)?;
            let roundtrip: ChangePage =
                crate::strict_json::from_str(&crate::stringify_json(&page, false))?;
            assert_eq!(roundtrip.events, page.events);
            assert!(page.stringify_toon().contains(PROTOCOL));
            let toon: Value = toon_format::decode_strict(&page.stringify_toon())?;
            let decoded: ChangePage = serde_json::from_value(toon)?;
            assert_eq!(decoded.events, page.events);
            assert_eq!(decoded.next_cursor, page.next_cursor);
            saved.push((request.clone(), page.events.clone()));
            for event in page.events {
                observed
                    .entry(event.item_id.clone())
                    .or_default()
                    .push(event);
            }
            request.cursor = Some(page.next_cursor);
        }
    }
    loop {
        let page = adapter.pull(&request)?;
        for event in page.events {
            observed
                .entry(event.item_id.clone())
                .or_default()
                .push(event);
        }
        request.cursor = Some(page.next_cursor);
        if !page.has_more {
            break;
        }
    }
    assert_eq!(observed, expected);
    assert!(adapter.pull(&request)?.events.is_empty());
    // Old cursors re-evaluate from their admitted per-stream positions, including new streams.
    for (old, _) in saved {
        let positions: BTreeMap<String, Prefix> = match &old.cursor {
            Some(raw) => {
                crate::strict_json::from_slice::<Cursor>(&URL_SAFE_NO_PAD.decode(raw)?)?.prefixes
            }
            None => BTreeMap::new(),
        };
        let reference: Vec<_> = expected
            .iter()
            .flat_map(|(id, rows)| {
                rows.iter().skip(
                    positions
                        .get(id)
                        .map_or(0, |p| usize::try_from(p.version).unwrap_or(usize::MAX)),
                )
            })
            .take(usize::from(old.limit))
            .cloned()
            .collect();
        assert_eq!(adapter.pull(&old)?.events, reference);
    }
    Ok(())
}

/// Every transport boundary can race with a peer append without skipping/duplicating.
#[test]
#[allow(clippy::items_after_statements)]
fn interleavings_at_every_pull_boundary() -> TestResult {
    for boundary in 0..5 {
        let (_dir, workspace) = fixture()?;
        append(&workspace, "demo-1", &record(0), false)?;
        let token = SessionCancellation::default();
        /// Real native adapter with one competing SDK event injected at an exchange edge.
        struct Peer {
            inner: LocalChangeAdapter,
            boundary: u8,
            calls: Rc<Cell<u8>>,
        }
        impl NativeAdapter for Peer {
            fn manifest(&self) -> AdapterManifest {
                self.inner.manifest()
            }
            fn exchange(&mut self, input: &[u8], limit: usize) -> io::Result<Vec<u8>> {
                self.calls.set(self.calls.get() + 1);
                if self.boundary == 1 && self.calls.get() == 1 {
                    append(&self.inner.workspace, "demo-1", &record(1), true)?;
                }
                let output = self.inner.exchange(input, limit)?;
                if self.boundary == 2 && self.calls.get() == 1 {
                    append(&self.inner.workspace, "demo-1", &record(1), false)?;
                }
                Ok(output)
            }
        }
        if boundary == 0 {
            append(&workspace, "demo-1", &record(1), false)?;
        }
        let calls = Rc::new(Cell::new(0));
        let adapter = Peer {
            inner: LocalChangeAdapter::new(workspace.clone(), token.clone()),
            boundary,
            calls: calls.clone(),
        };
        let mut reader = ChangeReader::new(
            adapter,
            ChangeRequest {
                limit: 1,
                ..ChangeRequest::default()
            },
            token,
        )?;
        let first = reader.pull()?;
        assert_eq!(first.events[0].version, 1);
        if boundary >= 3 {
            append(&workspace, "demo-1", &record(1), false)?;
        }
        // Disable the once-per-path injection after its first exchange.

        let second = reader.pull()?;
        assert_eq!(second.events[0].version, 2);
        assert!(reader.pull()?.events.is_empty());
        assert_eq!(calls.get(), 3);
    }
    Ok(())
}

/// Strict input/output corpora exercise extreme values before any source dispatch.
#[test]
fn strict_boundaries_fuzz_and_refusal_guidance() -> TestResult {
    let (_dir, workspace) = fixture()?;
    let adapter = LocalChangeAdapter::new(workspace, SessionCancellation::default());
    for (limit, output_bytes, cursor) in [
        (0, 1024, None),
        (101, 1024, None),
        (1, 0, None),
        (1, 1_048_577, None),
        (u16::MAX, u32::MAX, None),
        (1, 1024, Some(String::new())),
        (1, 1024, Some("x".repeat(524_289))),
    ] {
        let error = adapter
            .pull(&ChangeRequest {
                cursor,
                limit,
                output_bytes,
            })
            .err()
            .ok_or("must refuse")?;
        assert_eq!(error.code(), "changes_invalid_request");
    }
    for raw in [
        r#"{"cursor":null,"limit":1,"output_bytes":1024,"other":true}"#,
        r#"{"limit":1,"limit":2,"output_bytes":1024}"#,
        r#"{"limit":1e309,"output_bytes":1024}"#,
        r#"{"limit":-1,"output_bytes":1024}"#,
        r#"{"limit":1.5,"output_bytes":1024}"#,
        r#"{"limit":[],"output_bytes":1024}"#,
        r#"{"limit":null,"output_bytes":1024}"#,
    ] {
        assert!(crate::strict_json::from_str::<ChangeRequest>(raw).is_err());
    }
    let valid = ChangeRequest::default();
    let page = adapter.pull(&valid)?;
    for field in ["protocol", "next_cursor", "events", "skipped"] {
        let mut value = json!(&page);
        value[field] = Value::Null;
        assert!(serde_json::from_value::<ChangePage>(value).is_err());
    }
    for error in [
        ChangeRefusal::InvalidRequest,
        ChangeRefusal::StaleCursor,
        ChangeRefusal::Cancelled,
        ChangeRefusal::ScanLimit,
        ChangeRefusal::OutputLimit,
        ChangeRefusal::InvalidPage,
        ChangeRefusal::Source,
        ChangeRefusal::Backend {
            code: "backend_stale".into(),
            message: "stale".into(),
            remedy: "reorient".into(),
            details: Box::new(json!({"receipt":true})),
        },
    ] {
        assert!(!error.code().is_empty());
        assert!(error.to_string().contains(';'));
    }
    for bytes in [1024, 1_048_576] {
        let request = ChangeRequest {
            limit: 100,
            output_bytes: bytes,
            ..valid.clone()
        };
        assert!(adapter.pull(&request).is_ok());
    }
    Ok(())
}

/// Prefix retention detects truncation, pruning, rebinding and same-length ABA.
#[test]
fn stale_prefixes_and_partial_appends() -> TestResult {
    let (_dir, workspace) = fixture()?;
    append(&workspace, "demo-1", &record(0), false)?;
    let path = workspace.pm_root().join("history/demo-1.jsonl");
    let adapter = LocalChangeAdapter::new(workspace.clone(), SessionCancellation::default());
    let first = adapter.pull(&ChangeRequest::default())?;
    let request = ChangeRequest {
        cursor: Some(first.next_cursor),
        ..ChangeRequest::default()
    };
    let original = fs::read(&path)?;
    let partial = crate::stringify_json(&record(1), false);
    fs::OpenOptions::new()
        .append(true)
        .open(&path)?
        .write_all(partial.as_bytes())?;
    assert!(adapter.pull(&request)?.events.is_empty());
    fs::OpenOptions::new()
        .append(true)
        .open(&path)?
        .write_all(b"\n")?;
    assert_eq!(adapter.pull(&request)?.events[0].version, 2);
    for raw in [
        b"".as_slice(),
        b"x\n",
        original[..original.len() - 1].as_ref(),
    ] {
        fs::write(&path, raw)?;
        assert_eq!(
            adapter.pull(&request).err().ok_or("stale")?.code(),
            "changes_stale_cursor"
        );
    }
    fs::write(&path, &original)?;
    let mut changed = original.clone();
    changed[2] = b'X';
    fs::write(&path, changed)?;
    assert!(matches!(
        adapter.pull(&request),
        Err(ChangeRefusal::StaleCursor)
    ));
    fs::remove_file(&path)?;
    assert!(matches!(
        adapter.pull(&request),
        Err(ChangeRefusal::StaleCursor)
    ));
    for cursor in [
        "!".into(),
        URL_SAFE_NO_PAD.encode("bad json"),
        URL_SAFE_NO_PAD.encode(r#"{"protocol":"old","tracker":"x","prefixes":{}}"#),
    ] {
        assert!(matches!(
            adapter.pull(&ChangeRequest {
                cursor: Some(cursor),
                ..ChangeRequest::default()
            }),
            Err(ChangeRefusal::StaleCursor)
        ));
    }
    fs::write(&path, &original)?;
    let decoded = URL_SAFE_NO_PAD.decode(request.cursor.as_ref().ok_or("cursor")?)?;
    for mutation in ["protocol", "tracker", "bytes", "version", "hash"] {
        let mut value: Value = serde_json::from_slice(&decoded)?;
        match mutation {
            "protocol" | "tracker" => value[mutation] = json!("other"),
            "bytes" | "version" => value["prefixes"]["demo-1"][mutation] = json!(0),
            _ => value["prefixes"]["demo-1"][mutation] = json!("other"),
        }
        assert!(matches!(
            adapter.pull(&ChangeRequest {
                cursor: Some(URL_SAFE_NO_PAD.encode(crate::stringify_json(&value, false))),
                ..ChangeRequest::default()
            }),
            Err(ChangeRefusal::StaleCursor)
        ));
    }
    Ok(())
}

/// Foreign records fail alone; omissions consume the same bounded page budget.
#[test]
fn foreign_records_and_producer_pages_share_admission() -> TestResult {
    let (_dir, workspace) = fixture()?;
    let path = workspace.pm_root().join("history/demo-1.jsonl");
    let mut raw = "\n \r\n\u{feff}\u{00a0}\n{broken}\n{}\n"
        .as_bytes()
        .to_vec();
    for record in [
        json!({"ts":0}),
        json!({"ts":"time","author":0}),
        json!({"ts":"time","author":"a","op":0}),
        json!({"ts":"time","author":"a","op":"update","patch":null}),
        json!({"ts":"time","author":"a","op":"update","patch":[],"message":null}),
    ] {
        raw.extend_from_slice(format!("{record}\n").as_bytes());
    }
    fs::write(path, raw)?;
    append(
        &workspace,
        "demo-1",
        &json!({"ts":"historic free-form time", "author":"", "op":"custom", "patch":[]}),
        true,
    )?;
    append(&workspace, "demo-1", &record(2), false)?;
    let adapter = LocalChangeAdapter::new(workspace, SessionCancellation::default());
    let first = adapter.pull(&ChangeRequest {
        limit: 2,
        ..ChangeRequest::default()
    })?;
    assert_eq!(first.skipped.len(), 2);
    assert!(first.has_more);
    assert_eq!(first.skipped[0].reason, "foreign_or_invalid_history_record");
    let request = ChangeRequest {
        cursor: Some(first.next_cursor),
        ..ChangeRequest::default()
    };
    let second = adapter.pull(&request)?;
    assert_eq!(second.skipped.len(), 5);
    assert_eq!(second.events.len(), 2);
    assert!(second.events[0].message.is_none());
    second.validate(&request)?;
    let mut invalid = second.clone();
    invalid.events[0].version = 0;
    assert!(matches!(
        invalid.validate(&request),
        Err(ChangeRefusal::InvalidPage)
    ));
    invalid.events[0].version = u64::MAX;
    assert!(invalid.validate(&request).is_err());
    invalid.events[0].version = 1;
    invalid.skipped[0].version = 0;
    assert!(invalid.validate(&request).is_err());
    invalid.skipped[0].version = u64::MAX;
    assert!(invalid.validate(&request).is_err());
    for field in ["protocol", "next_cursor"] {
        let mut value = json!(&second);
        value[field] = json!("");
        let invalid: ChangePage = serde_json::from_value(value)?;
        assert!(invalid.validate(&request).is_err());
    }
    let mut invalid = second.clone();
    invalid.next_cursor = "x".repeat(524_289);
    assert!(invalid.validate(&request).is_err());
    invalid
        .next_cursor
        .clone_from(request.cursor.as_ref().ok_or("cursor")?);
    assert!(invalid.validate(&request).is_err());
    assert!(
        second
            .validate(&ChangeRequest {
                limit: 1,
                ..request.clone()
            })
            .is_err()
    );
    let mut oversized = second.clone();
    oversized.events[0].message = Some("x".repeat(70_000));
    assert!(matches!(
        oversized.validate(&request),
        Err(ChangeRefusal::OutputLimit)
    ));
    Ok(())
}

/// Wire-only peers exercise protocol corruption, not mocked SDK operations.
struct WirePeer {
    /// Explicit wire ABI, including incompatible declarations.
    abi: String,
    /// Declared capabilities, optionally unsupported.
    capabilities: Vec<AdapterCapability>,
    /// Encoded authoritative outcome to return.
    outcome: AdapterOutcome,
    /// Optional cancellation injected after a complete exchange.
    cancel: Option<SessionCancellation>,
}
impl NativeAdapter for WirePeer {
    fn manifest(&self) -> AdapterManifest {
        AdapterManifest {
            abi: self.abi.clone(),
            capabilities: self.capabilities.clone(),
            max_frame_bytes: NATIVE_ADAPTER_MAX_BYTES,
        }
    }
    fn exchange(&mut self, bytes: &[u8], _: usize) -> io::Result<Vec<u8>> {
        let request: AdapterRequest = crate::strict_json::from_slice(bytes)?;
        if let Some(token) = &self.cancel {
            token.cancel();
        }
        Ok(crate::stringify_json(
            &AdapterReply {
                abi: NATIVE_ADAPTER_ABI.into(),
                request_id: request.request_id,
                outcome: self.outcome.clone(),
            },
            false,
        )
        .into_bytes())
    }
}

/// Unsupported adapters refuse before dispatch; failed pages/cancellation never admit cursors.
#[test]
fn backend_refusals_and_cancelled_exchange_retain_checkpoint() -> TestResult {
    let (_dir, workspace) = fixture()?;
    let cancellation = SessionCancellation::default();
    let adapter = LocalChangeAdapter::new(workspace, cancellation.clone());
    let page = adapter.pull(&ChangeRequest::default())?;
    let success = AdapterOutcome::Success {
        output: json!(&page),
    };
    let peer = |outcome, capabilities, cancel| WirePeer {
        abi: NATIVE_ADAPTER_ABI.into(),
        capabilities,
        outcome,
        cancel,
    };
    let error = ChangeReader::new(
        peer(success.clone(), vec![], None),
        ChangeRequest::default(),
        cancellation.clone(),
    )
    .err()
    .ok_or("unsupported")?;
    assert_eq!(error.code(), "changes_adapter_refused");
    assert!(error.to_string().contains("authorized adapter"));
    let mut reader = ChangeReader::new(
        peer(
            success.clone(),
            vec![AdapterCapability::Changes],
            Some(cancellation.clone()),
        ),
        ChangeRequest::default(),
        cancellation.clone(),
    )?;
    assert!(matches!(reader.pull(), Err(ChangeRefusal::Cancelled)));
    assert!(reader.cursor().is_none());
    assert!(matches!(reader.pull(), Err(ChangeRefusal::Cancelled)));
    assert!(matches!(
        adapter.pull(&ChangeRequest::default()),
        Err(ChangeRefusal::Cancelled)
    ));
    let cancel = SessionCancellation::default();
    for outcome in [
        AdapterOutcome::Success {
            output: json!({"events":[]}),
        },
        AdapterOutcome::Refused {
            code: "stale_backend_cursor".into(),
            message: "stale".into(),
            remedy: "reorient using SDK".into(),
            details: json!({"retained":true}),
        },
    ] {
        let mut reader = ChangeReader::new(
            peer(outcome, vec![AdapterCapability::Changes], None),
            ChangeRequest::default(),
            cancel.clone(),
        )?;
        let error = reader.pull().err().ok_or("refusal")?;
        assert!(reader.cursor().is_none());
        if let ChangeRefusal::Backend {
            code,
            message,
            remedy,
            details,
        } = error
        {
            assert_eq!(code, "stale_backend_cursor");
            assert_eq!(message, "stale");
            assert_eq!(remedy, "reorient using SDK");
            assert_eq!(*details, json!({"retained":true}));
        } else {
            assert!(matches!(error, ChangeRefusal::InvalidPage));
        }
    }
    let mut invalid = page;
    invalid.protocol = "old".into();
    let mut reader = ChangeReader::new(
        peer(
            AdapterOutcome::Success {
                output: json!(invalid),
            },
            vec![AdapterCapability::Changes],
            None,
        ),
        ChangeRequest::default(),
        cancel,
    )?;
    assert!(matches!(reader.pull(), Err(ChangeRefusal::InvalidPage)));
    assert!(reader.cursor().is_none());
    Ok(())
}

/// Source work limits are hard bounds and never alter authoritative history.
#[test]
fn scan_limits_source_failures_and_empty_polls() -> TestResult {
    let (_dir, workspace) = fixture()?;
    let adapter = LocalChangeAdapter::new(workspace.clone(), SessionCancellation::default());
    let first = adapter.pull(&ChangeRequest::default())?;
    assert!(first.events.is_empty());
    let second = adapter.pull(&ChangeRequest {
        cursor: Some(first.next_cursor.clone()),
        ..ChangeRequest::default()
    })?;
    assert_eq!(first.next_cursor, second.next_cursor);
    let history = workspace.pm_root().join("history");
    assert!(matches!(
        regular_history_length(&fs::metadata(&history)?),
        Err(ChangeRefusal::Source)
    ));
    fs::write(history.join("ignored.txt"), "foreign")?;
    fs::create_dir(history.join("ignored.jsonl"))?;
    assert!(adapter.pull(&ChangeRequest::default()).is_ok());
    let large = history.join("large.jsonl");
    fs::File::create(&large)?.set_len(SCAN_BYTES + 1)?;
    assert!(matches!(
        adapter.pull(&ChangeRequest::default()),
        Err(ChangeRefusal::ScanLimit)
    ));
    fs::remove_file(&large)?;
    for index in 0..STREAMS {
        fs::write(history.join(format!("{index}.txt")), "")?;
    }
    assert!(matches!(
        adapter.pull(&ChangeRequest::default()),
        Err(ChangeRefusal::ScanLimit)
    ));
    fs::remove_dir_all(&history)?;
    assert!(matches!(
        adapter.pull(&ChangeRequest::default()),
        Err(ChangeRefusal::Source)
    ));
    Ok(())
}

/// Symlinks and nonregular handles cannot redirect native history reads.
#[cfg(unix)]
#[test]
fn source_file_kinds_are_admitted_on_the_opened_handle() -> TestResult {
    use std::os::unix::{ffi::OsStringExt as _, fs::symlink};
    let (_dir, workspace) = fixture()?;
    let root = workspace.pm_root();
    let history = root.join("history");
    assert!(matches!(
        open_regular_history(&history),
        Err(ChangeRefusal::Source)
    ));
    assert!(matches!(
        open_regular_history(&history.join("missing")),
        Err(ChangeRefusal::Source)
    ));
    let outside = root.join("outside");
    fs::create_dir(&outside)?;
    let link = history.join("link.jsonl");
    symlink(&outside, &link)?;
    assert!(open_history(&link).is_err());
    let adapter = LocalChangeAdapter::new(workspace.clone(), SessionCancellation::default());
    assert!(adapter.pull(&ChangeRequest::default()).is_ok());
    fs::remove_file(&link)?;
    fs::remove_dir(&history)?;
    symlink(&outside, &history)?;
    assert!(matches!(
        adapter.pull(&ChangeRequest::default()),
        Err(ChangeRefusal::Source)
    ));
    fs::remove_file(&history)?;
    fs::create_dir(&history)?;
    let invalid = std::ffi::OsString::from_vec(vec![255, b'.', b'j', b's', b'o', b'n', b'l']);
    fs::write(history.join(invalid), "\n")?;
    assert!(matches!(
        adapter.pull(&ChangeRequest::default()),
        Err(ChangeRefusal::Source)
    ));
    Ok(())
}

thread_local! {
    /// Fault injection affects only a test's owned real-file operation.
    static IO_FAILURE: Cell<u8> = const { Cell::new(0) };
    /// One cancellation or filesystem transition at a native step boundary.
    static BOUNDARY: Cell<(u8, u8)> = const { Cell::new((0, 0)) };
}

/// Preserves real I/O by default; tests can inject an otherwise rare OS failure.
pub(super) fn inject_io<T>(result: io::Result<T>, stage: u8) -> io::Result<T> {
    if IO_FAILURE.with(|setting| setting.get() == stage) {
        Err(io::Error::other("injected OS failure"))
    } else {
        result
    }
}

/// Injects a competing transition at an exact file/cancellation admission edge.
pub(super) fn boundary(stage: u8, adapter: &LocalChangeAdapter) {
    BOUNDARY.with(|setting| {
        let (selected, action) = setting.get();
        if selected == stage {
            setting.set((0, 0));
            if action == 1 {
                adapter.cancellation.cancel();
            }
            if action == 2 {
                let _ = fs::remove_file(adapter.workspace.pm_root().join("history/demo-1.jsonl"));
            }
            if action == 3 {
                let _ = append(&adapter.workspace, "demo-1", &record(1), true);
            }
        }
    });
}

/// Every native step revalidates cancellation; OS errors preserve the old position.
#[test]
fn local_step_interleavings_and_io_failure_propagation() -> TestResult {
    for stage in 1..=4 {
        let (_dir, workspace) = fixture()?;
        append(&workspace, "demo-1", &record(0), false)?;
        let adapter = LocalChangeAdapter::new(workspace, SessionCancellation::default());
        BOUNDARY.with(|setting| setting.set((stage, 1)));
        assert!(matches!(
            adapter.pull(&ChangeRequest::default()),
            Err(ChangeRefusal::Cancelled)
        ));
        BOUNDARY.with(|setting| setting.set((0, 0)));
    }
    for stage in 1..=4 {
        let (_dir, workspace) = fixture()?;
        append(&workspace, "demo-1", &record(0), false)?;
        let adapter = LocalChangeAdapter::new(workspace, SessionCancellation::default());
        IO_FAILURE.with(|setting| setting.set(stage));
        let actual = adapter.pull(&ChangeRequest::default());
        IO_FAILURE.with(|setting| setting.set(0));
        assert!(matches!(actual, Err(ChangeRefusal::Source)));
    }
    for stage in 1..=4 {
        let (_dir, workspace) = fixture()?;
        append(&workspace, "demo-1", &record(0), false)?;
        let adapter = LocalChangeAdapter::new(workspace, SessionCancellation::default());
        BOUNDARY.with(|setting| setting.set((stage, 3)));
        let first = adapter.pull(&ChangeRequest {
            limit: 1,
            ..ChangeRequest::default()
        })?;
        let request = ChangeRequest {
            cursor: Some(first.next_cursor),
            ..ChangeRequest::default()
        };
        assert_eq!(first.events[0].version, 1);
        assert_eq!(adapter.pull(&request)?.events[0].version, 2);
    }
    let (_dir, workspace) = fixture()?;
    append(&workspace, "demo-1", &record(0), false)?;
    let mut adapter = LocalChangeAdapter::new(workspace, SessionCancellation::default());
    BOUNDARY.with(|setting| setting.set((2, 2)));
    assert!(matches!(
        adapter.pull(&ChangeRequest::default()),
        Err(ChangeRefusal::Source)
    ));
    assert!(adapter.exchange(b"{}", 1024).is_err());
    let get = AdapterRequest {
        abi: NATIVE_ADAPTER_ABI.into(),
        request_id: "changes".into(),
        operation: AdapterOperation::Get {
            id: "demo-1".into(),
        },
    };
    assert!(
        adapter
            .exchange(crate::stringify_json(&get, false).as_bytes(), 1024)
            .is_err()
    );
    let changes = AdapterRequest {
        operation: AdapterOperation::Changes {
            request: ChangeRequest {
                limit: 0,
                ..ChangeRequest::default()
            },
        },
        ..get
    };
    let reply: AdapterReply = crate::strict_json::from_slice(
        &adapter.exchange(crate::stringify_json(&changes, false).as_bytes(), 1024)?,
    )?;
    assert!(matches!(reply.outcome, AdapterOutcome::Refused { .. }));
    Ok(())
}

/// Every fallible projection field fails alone; output guards cover either encoding.
#[test]
fn projection_and_page_extremes() -> TestResult {
    for input in [
        b"\xff".as_slice(),
        br#"{"ts":"t"}"#,
        br#"{"ts":"t","author":"a"}"#,
        br#"{"ts":"t","author":"a","op":"u"}"#,
    ] {
        assert!(project_event("demo-1", 1, input).is_none());
    }
    assert!(!blank_line(b"\x85\n"));
    assert!(!blank_line("\u{0085}\n".as_bytes()));
    let (_dir, workspace) = fixture()?;
    append(&workspace, "demo-1", &record(0), false)?;
    let adapter = LocalChangeAdapter::new(workspace.clone(), SessionCancellation::default());
    let mut page = adapter.pull(&ChangeRequest::default())?;
    page.events[0].patch_count = u64::MAX;
    assert!(matches!(
        page.validate(&ChangeRequest::default()),
        Err(ChangeRefusal::InvalidPage)
    ));
    page.events[0].patch_count = 0;
    page.events.push(MutationEvent {
        version: 2,
        message: None,
        ..page.events[0].clone()
    });
    page.next_cursor = "x".repeat(1100);
    let json = crate::stringify_json(&page, false).len() + 1;
    let toon = page.stringify_toon().len() + 1;
    let request = ChangeRequest {
        output_bytes: u32::try_from(json.max(toon) - 1)?,
        ..ChangeRequest::default()
    };
    assert!(matches!(
        page.validate(&request),
        Err(ChangeRefusal::OutputLimit)
    ));
    page.validate(&ChangeRequest {
        output_bytes: u32::try_from(json.max(toon))?,
        ..ChangeRequest::default()
    })?;
    let mut entry = record(2);
    entry["message"] = json!("x".repeat(70_000));
    append(&workspace, "demo-1", &entry, false)?;
    assert!(matches!(
        adapter.pull(&ChangeRequest::default()),
        Err(ChangeRefusal::OutputLimit)
    ));
    let cursor = adapter
        .pull(&ChangeRequest {
            limit: 1,
            ..ChangeRequest::default()
        })?
        .next_cursor;
    let mut value: Value = serde_json::from_slice(&URL_SAFE_NO_PAD.decode(&cursor)?)?;
    let prefix = value["prefixes"]["demo-1"].clone();
    for index in 0..=STREAMS {
        value["prefixes"][format!("foreign-{index}")] = prefix.clone();
    }
    assert!(matches!(
        adapter.pull(&ChangeRequest {
            cursor: Some(URL_SAFE_NO_PAD.encode(crate::stringify_json(&value, false))),
            ..ChangeRequest::default()
        }),
        Err(ChangeRefusal::StaleCursor)
    ));
    // A hash-valid cursor boundary still must be a complete newline and accurate ordinal.
    let mut value: Value = serde_json::from_slice(&URL_SAFE_NO_PAD.decode(cursor)?)?;
    let raw = fs::read(workspace.pm_root().join("history/demo-1.jsonl"))?;
    value["prefixes"]["demo-1"]["bytes"] = json!(1);
    value["prefixes"]["demo-1"]["hash"] = json!(crate::history::sha256_digest(&raw[..1]));
    assert!(matches!(
        adapter.pull(&ChangeRequest {
            cursor: Some(URL_SAFE_NO_PAD.encode(crate::stringify_json(&value, false))),
            ..ChangeRequest::default()
        }),
        Err(ChangeRefusal::StaleCursor)
    ));
    Ok(())
}

/// All client admission/transport failures leave the exact last admitted cursor untouched.
#[test]
#[allow(clippy::items_after_statements)]
fn reader_admission_and_wire_failure_boundaries() -> TestResult {
    let peer = |outcome| WirePeer {
        abi: NATIVE_ADAPTER_ABI.into(),
        capabilities: vec![AdapterCapability::Changes],
        outcome,
        cancel: None,
    };
    let success = AdapterOutcome::Success {
        output: json!({"protocol":PROTOCOL,"events":[],"skipped":[],"next_cursor":"opaque","has_more":false}),
    };
    let token = SessionCancellation::default();
    assert!(
        ChangeReader::new(
            peer(success.clone()),
            ChangeRequest {
                limit: 0,
                ..ChangeRequest::default()
            },
            token.clone()
        )
        .is_err()
    );
    let mut incompatible = peer(success.clone());
    incompatible.abi = "old".into();
    assert!(ChangeReader::new(incompatible, ChangeRequest::default(), token.clone()).is_err());
    let mut reader = ChangeReader::new(
        peer(success.clone()),
        ChangeRequest::default(),
        token.clone(),
    )?;
    reader.pull()?;
    assert_eq!(reader.cursor(), Some("opaque"));
    let too_large = AdapterOutcome::Success {
        output: json!("x".repeat(NATIVE_ADAPTER_MAX_BYTES)),
    };
    let mut reader = ChangeReader::new(
        peer(too_large),
        ChangeRequest {
            cursor: Some("retained".into()),
            ..ChangeRequest::default()
        },
        token.clone(),
    )?;
    assert!(matches!(reader.pull(), Err(ChangeRefusal::Adapter(_))));
    assert_eq!(reader.cursor(), Some("retained"));
    /// Incompatible manifest refuses construction without exchange.
    struct Old;
    impl NativeAdapter for Old {
        fn manifest(&self) -> AdapterManifest {
            AdapterManifest {
                abi: "old".into(),
                capabilities: vec![],
                max_frame_bytes: 1,
            }
        }
        fn exchange(&mut self, _: &[u8], _: usize) -> io::Result<Vec<u8>> {
            Err(io::Error::other("must not dispatch"))
        }
    }
    assert!(ChangeReader::new(Old, ChangeRequest::default(), token).is_err());
    Ok(())
}

/// The adapter honors the caller's encoded response ceiling, including refusal frames.
#[test]
fn local_adapter_frame_ceiling_includes_its_envelope() -> TestResult {
    let (_dir, workspace) = fixture()?;
    let mut entry = record(0);
    entry["message"] = json!("x".repeat(2000));
    append(&workspace, "demo-1", &entry, false)?;
    let mut adapter = LocalChangeAdapter::new(workspace, SessionCancellation::default());
    let request = AdapterRequest {
        abi: NATIVE_ADAPTER_ABI.into(),
        request_id: "changes".into(),
        operation: AdapterOperation::Changes {
            request: ChangeRequest::default(),
        },
    };
    let raw = crate::stringify_json(&request, false);
    let bytes = adapter.exchange(raw.as_bytes(), 1024)?;
    assert!(bytes.len() <= 1024);
    let reply: AdapterReply = crate::strict_json::from_slice(&bytes)?;
    assert!(
        matches!(reply.outcome,AdapterOutcome::Refused { code,.. } if code=="changes_output_limit")
    );
    assert!(adapter.exchange(raw.as_bytes(), 1).is_err());
    Ok(())
}

/// Windows preserves non-UTF-8 filename refusal without attempting a redirected read.
#[cfg(windows)]
#[test]
fn windows_history_filename_admission() -> TestResult {
    use std::os::windows::ffi::OsStringExt as _;
    let (_dir, workspace) = fixture()?;
    let mut name = vec![0xd800];
    name.extend(".jsonl".encode_utf16());
    fs::write(
        workspace
            .pm_root()
            .join("history")
            .join(std::ffi::OsString::from_wide(&name)),
        "\n",
    )?;
    let adapter = LocalChangeAdapter::new(workspace, SessionCancellation::default());
    assert!(matches!(
        adapter.pull(&ChangeRequest::default()),
        Err(ChangeRefusal::Source)
    ));
    Ok(())
}

/// A real local adapter whose third exchange delivers a delayed copy of its first reply.
struct DelayedReply {
    /// Genuine backend containing retained canonical history.
    inner: LocalChangeAdapter,
    /// One previously successful encoded reply.
    first: Option<Vec<u8>>,
    /// Ordered exchange boundary, independent of cursor/event timestamps.
    calls: u8,
}
impl NativeAdapter for DelayedReply {
    fn manifest(&self) -> AdapterManifest {
        self.inner.manifest()
    }
    fn exchange(&mut self, request: &[u8], limit: usize) -> io::Result<Vec<u8>> {
        self.calls += 1;
        if self.calls == 3 {
            return self
                .first
                .clone()
                .ok_or_else(|| io::Error::other("missing first reply"));
        }
        let response = self.inner.exchange(request, limit)?;
        if self.calls == 1 {
            self.first = Some(response.clone());
        }
        Ok(response)
    }
}

/// Correlation rejects a delayed older page after two successes, retaining the last cursor.
#[test]
fn delayed_older_pages_cannot_rewind_a_reader() -> TestResult {
    let (_dir, workspace) = fixture()?;
    for index in 0..3 {
        append(&workspace, "demo-1", &record(index), false)?;
    }
    let cancellation = SessionCancellation::default();
    let adapter = DelayedReply {
        inner: LocalChangeAdapter::new(workspace, cancellation.clone()),
        first: None,
        calls: 0,
    };
    let mut reader = ChangeReader::new(
        adapter,
        ChangeRequest {
            limit: 1,
            ..ChangeRequest::default()
        },
        cancellation,
    )?;
    assert_eq!(reader.pull()?.events[0].version, 1);
    assert_eq!(reader.pull()?.events[0].version, 2);
    let retained = reader.cursor().ok_or("cursor")?.to_owned();
    assert!(matches!(
        reader.pull(),
        Err(ChangeRefusal::Adapter(
            crate::native_adapter::AdapterRefusal {
                code: crate::native_adapter::AdapterRefusalCode::InvalidFrame,
                ..
            }
        ))
    ));
    assert_eq!(reader.cursor(), Some(retained.as_str()));
    assert_eq!(reader.pull()?.events[0].version, 3);
    Ok(())
}

/// Duplicate physical identities in a backend page never become admitted work.
#[test]
fn duplicate_backend_positions_refuse_before_cursor_admission() -> TestResult {
    let (_dir, workspace) = fixture()?;
    append(&workspace, "demo-1", &record(0), false)?;
    let token = SessionCancellation::default();
    let mut page =
        LocalChangeAdapter::new(workspace, token.clone()).pull(&ChangeRequest::default())?;
    page.events.push(page.events[0].clone());
    assert!(matches!(
        page.validate(&ChangeRequest::default()),
        Err(ChangeRefusal::InvalidPage)
    ));
    page.events.pop();
    page.skipped.push(SkippedChange {
        item_id: "demo-1".into(),
        version: 1,
        reason: "foreign".into(),
    });
    let peer = WirePeer {
        abi: NATIVE_ADAPTER_ABI.into(),
        capabilities: vec![AdapterCapability::Changes],
        outcome: AdapterOutcome::Success {
            output: json!(page),
        },
        cancel: None,
    };
    let mut reader = ChangeReader::new(peer, ChangeRequest::default(), token)?;
    assert!(matches!(reader.pull(), Err(ChangeRefusal::InvalidPage)));
    assert!(reader.cursor().is_none());
    Ok(())
}
