//! ABI conformance, pre-write admission and real non-git SDK interleavings.
use pm_rust::native_adapter::{
    AdapterCapability as Capability, AdapterManifest, AdapterOperation as Operation,
    AdapterOutcome, AdapterRefusalCode as Code, AdapterReply, AdapterRequest,
    NATIVE_ADAPTER_ABI as ABI, NATIVE_ADAPTER_MAX_BYTES as MAX, NativeAdapter, NativeAdapterClient,
    check_adapter_manifest, check_adapter_reply, check_adapter_transcript, prepare_adapter_request,
};
use pm_rust::{OwnershipItem, PmRustError, Workspace};
use serde_json::{Value, json};
use std::{cell::Cell, fs, io, path::Path, process::Command, rc::Rc};

/// Fallible assertion result.
type TestResult = Result<(), Box<dyn std::error::Error>>;

/// Declares the synthetic authorized instance's narrow operations.
fn manifest() -> AdapterManifest {
    AdapterManifest {
        abi: ABI.into(),
        capabilities: vec![
            Capability::Get,
            Capability::List,
            Capability::Claim,
            Capability::Release,
        ],
        max_frame_bytes: MAX,
    }
}

/// Creates an independently correlated request.
fn request(operation: Operation) -> AdapterRequest {
    AdapterRequest {
        abi: ABI.into(),
        request_id: "call-1".into(),
        operation,
    }
}

/// Creates a canonical response without modifying its backend payload.
fn reply(outcome: AdapterOutcome) -> AdapterReply {
    AdapterReply {
        abi: ABI.into(),
        request_id: "call-1".into(),
        outcome,
    }
}

/// Encodes known serializable protocol values.
fn bytes(value: &impl serde::Serialize) -> Vec<u8> {
    pm_rust::stringify_json(value, false).into_bytes()
}

/// Protocol peer for malformed-frame tests; it does not mock any SDK operation.
struct Peer {
    /// Exact response or a disconnected transport.
    response: Option<Vec<u8>>,
    /// Number of dispatches observed across client ownership.
    calls: Rc<Cell<usize>>,
}
impl NativeAdapter for Peer {
    fn manifest(&self) -> AdapterManifest {
        manifest()
    }
    fn exchange(&mut self, _: &[u8], limit: usize) -> io::Result<Vec<u8>> {
        assert_eq!(limit, MAX);
        self.calls.set(self.calls.get() + 1);
        self.response
            .clone()
            .ok_or_else(|| io::Error::other("disconnected after commit"))
    }
}

#[test]
fn canonical_outcomes_survive_the_byte_boundary() -> TestResult {
    let output = json!({"claimed_by":"peer", "skipped":true,"warnings":["claim_skipped_held_by:peer"],"mutation_receipt":{"changed_count":0},"omission_receipt":{"has_omissions":true},"next_cursor":"opaque","large":9_007_199_254_740_991_u64,"unicode":"ä\n雪"});
    let outcomes = [
        AdapterOutcome::Success {
            output: output.clone(),
        },
        AdapterOutcome::Refused {
            code: "ownership_conflict".into(),
            message: "foreign owner".into(),
            remedy: "select another item".into(),
            details: output,
        },
    ];
    for outcome in outcomes {
        let response = reply(outcome);
        let calls = Rc::new(Cell::new(0));
        let mut client = NativeAdapterClient::new(Peer {
            response: Some(bytes(&response)),
            calls: calls.clone(),
        })?;
        let actual = client.call(&request(Operation::Get {
            id: "demo-1".into(),
        }))?;
        assert_eq!(
            serde_json::to_value(actual)?,
            serde_json::to_value(response)?
        );
        assert_eq!(calls.get(), 1);
    }
    Ok(())
}

#[test]
fn admission_refuses_before_transport_or_writes() -> TestResult {
    /// A read-only adapter cannot receive a mutating operation.
    struct ReadOnly(Peer);
    impl NativeAdapter for ReadOnly {
        fn manifest(&self) -> AdapterManifest {
            AdapterManifest {
                capabilities: vec![Capability::Get],
                ..manifest()
            }
        }
        fn exchange(&mut self, input: &[u8], limit: usize) -> io::Result<Vec<u8>> {
            self.0.exchange(input, limit)
        }
    }
    let calls = Rc::new(Cell::new(0));
    let mut client = NativeAdapterClient::new(ReadOnly(Peer {
        response: None,
        calls: calls.clone(),
    }))?;
    for operation in [
        Operation::Claim {
            id: "demo-1".into(),
            author: "caller".into(),
        },
        Operation::Release {
            id: "demo-1".into(),
            author: "caller".into(),
        },
    ] {
        assert_eq!(
            client
                .call(&request(operation))
                .err()
                .ok_or("must refuse")?
                .code,
            Code::UnsupportedCapability
        );
    }
    let mut invalid = request(Operation::Get {
        id: "demo-1".into(),
    });
    invalid.abi = "pm-native-adapter/2".into();
    assert_eq!(
        client.call(&invalid).err().ok_or("must refuse")?.code,
        Code::IncompatibleAbi
    );
    invalid.abi = ABI.into();
    invalid.request_id.clear();
    assert_eq!(
        client.call(&invalid).err().ok_or("must refuse")?.code,
        Code::InvalidFrame
    );
    assert_eq!(calls.get(), 0);
    Ok(())
}

#[test]
fn versions_fields_and_limits_are_fail_closed() -> TestResult {
    for abi in [
        "",
        "pm-native-adapter/0",
        "pm-native-adapter/1.1",
        "pm-native-adapter/2",
    ] {
        assert_eq!(
            check_adapter_manifest(&AdapterManifest {
                abi: abi.into(),
                ..manifest()
            })
            .err()
            .ok_or("version")?
            .code,
            Code::IncompatibleAbi
        );
    }
    for size in [0, MAX + 1] {
        assert_eq!(
            check_adapter_manifest(&AdapterManifest {
                max_frame_bytes: size,
                ..manifest()
            })
            .err()
            .ok_or("size")?
            .code,
            Code::FrameLimit
        );
    }
    assert!(
        check_adapter_manifest(&AdapterManifest {
            max_frame_bytes: 1,
            ..manifest()
        })
        .is_ok()
    );
    Ok(())
}

#[test]
fn operation_fields_and_lengths_fail_closed() -> TestResult {
    for operation in [
        Operation::Get { id: " ".into() },
        Operation::Get {
            id: "x".repeat(257),
        },
        Operation::List {
            limit: 0,
            after: None,
            output_cursor: None,
        },
        Operation::List {
            limit: 101,
            after: None,
            output_cursor: None,
        },
        Operation::Claim {
            id: String::new(),
            author: "caller".into(),
        },
        Operation::Claim {
            id: "demo-1".into(),
            author: String::new(),
        },
        Operation::Release {
            id: String::new(),
            author: "caller".into(),
        },
        Operation::Release {
            id: "demo-1".into(),
            author: String::new(),
        },
    ] {
        assert_eq!(
            prepare_adapter_request(&manifest(), &request(operation))
                .err()
                .ok_or("invalid operation")?
                .code,
            Code::InvalidFrame
        );
    }
    for operation in [
        Operation::List {
            limit: 1,
            after: None,
            output_cursor: None,
        },
        Operation::List {
            limit: 100,
            after: None,
            output_cursor: None,
        },
        Operation::Claim {
            id: "demo-1".into(),
            author: "caller".into(),
        },
        Operation::Release {
            id: "demo-1".into(),
            author: "caller".into(),
        },
    ] {
        prepare_adapter_request(&manifest(), &request(operation))?;
    }
    let mut call = request(Operation::Get {
        id: "demo-1".into(),
    });
    call.request_id = "x".repeat(257);
    assert_eq!(
        prepare_adapter_request(&manifest(), &call)
            .err()
            .ok_or("id limit")?
            .code,
        Code::InvalidFrame
    );
    call.request_id = "x".repeat(256);
    let encoded = prepare_adapter_request(&manifest(), &call)?;
    prepare_adapter_request(
        &AdapterManifest {
            max_frame_bytes: encoded.len(),
            ..manifest()
        },
        &call,
    )?;
    assert_eq!(
        prepare_adapter_request(
            &AdapterManifest {
                max_frame_bytes: encoded.len() - 1,
                ..manifest()
            },
            &call
        )
        .err()
        .ok_or("request size")?
        .code,
        Code::FrameLimit
    );
    Ok(())
}

#[test]
fn dispatched_failures_are_never_retried_or_reported_as_no_write() -> TestResult {
    let call = request(Operation::Claim {
        id: "demo-1".into(),
        author: "caller".into(),
    });
    let mut wrong_version = reply(AdapterOutcome::Success { output: json!({}) });
    wrong_version.abi = "other".into();
    let mut wrong_id = reply(AdapterOutcome::Success { output: json!({}) });
    wrong_id.request_id = "peer-call".into();
    for (response, code) in [
        (None, Code::OutcomeUnknown),
        (Some(b"{".to_vec()), Code::InvalidFrame),
        (Some(bytes(&wrong_version)), Code::IncompatibleAbi),
        (Some(bytes(&wrong_id)), Code::InvalidFrame),
        (Some(vec![b' '; MAX + 1]), Code::FrameLimit),
    ] {
        let calls = Rc::new(Cell::new(0));
        let mut client = NativeAdapterClient::new(Peer {
            response,
            calls: calls.clone(),
        })?;
        let error = client.call(&call).err().ok_or("must refuse")?;
        assert_eq!(error.code, code);
        assert!(error.dispatched);
        assert!(!error.remedy.is_empty());
        assert!(!error.to_string().contains("disconnected after commit"));
        assert_eq!(calls.get(), 1);
    }
    Ok(())
}

#[test]
fn transcript_checks_do_not_confuse_validation_with_execution() -> TestResult {
    let call = request(Operation::Get {
        id: "demo-1".into(),
    });
    for outcome in [
        None,
        Some(AdapterOutcome::Success { output: json!({}) }),
        Some(AdapterOutcome::Refused {
            code: "denied".into(),
            message: "not authorized".into(),
            remedy: "request access".into(),
            details: json!({"exit_code":3}),
        }),
    ] {
        let recorded = outcome.map(reply);
        let receipt = check_adapter_transcript(&bytes(
            &json!({"manifest":manifest(),"request":call,"reply":recorded}),
        ))?;
        assert_eq!(receipt["dispatched"], false);
        assert_eq!(receipt["receipt_scope"], "abi_compatibility_only");
        assert!(bytes(&receipt).len() < 400);
    }
    for invalid in [
        json!({}),
        json!({"manifest":manifest(),"request":call,"reply":null,"foreign":true}),
        json!({"manifest":manifest(),"request":{"abi":ABI,"request_id":"call-1","operation":{"action":"get","id":"demo-1","force":true}}}),
    ] {
        assert_eq!(
            check_adapter_transcript(&bytes(&invalid))
                .err()
                .ok_or("invalid transcript")?
                .code,
            Code::InvalidFrame
        );
    }
    assert_eq!(
        check_adapter_transcript(&vec![b' '; MAX + 1])
            .err()
            .ok_or("size")?
            .code,
        Code::FrameLimit
    );
    let mut invalid = call.clone();
    invalid.abi = "future".into();
    assert!(check_adapter_reply(&manifest(), &invalid, b"{}").is_err());
    let transcript = json!({"manifest":manifest(),"request":invalid});
    assert!(check_adapter_transcript(&bytes(&transcript)).is_err());
    let mut recorded = reply(AdapterOutcome::Success { output: json!({}) });
    recorded.request_id = "different".into();
    assert!(
        check_adapter_transcript(&bytes(
            &json!({"manifest":manifest(),"request":call,"reply":recorded})
        ))
        .is_err()
    );
    Ok(())
}

/// Initializes a genuine scratch tracker without an enclosing repository.
fn tracker() -> Result<tempfile::TempDir, Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let initialized = Command::new("node")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/oracle/launch.mts"))
        .current_dir(directory.path())
        .arg("--path")
        .arg(directory.path().join(".agents/pm"))
        .args(["init", "--defaults", "--agent-guidance", "skip"])
        .env(
            "GIT_CEILING_DIRECTORIES",
            directory.path().parent().ok_or("parent")?,
        )
        .output()?;
    assert!(
        initialized.status.success(),
        "{}",
        String::from_utf8_lossy(&initialized.stderr)
    );
    // The authoritative SDK policy must reject non-owner releases under its lock.
    let root = directory.path().join(".agents/pm");
    let mut settings: Value = serde_json::from_slice(&fs::read(root.join("settings.json"))?)?;
    settings["governance"]["preset"] = json!("strict");
    settings["agent_identity"]["probes_enabled"] = json!(false);
    fs::write(root.join("settings.json"), bytes(&settings))?;
    Ok(directory)
}

/// Explicit author request; claim uses the SDK's atomic skip-on-peer path.
fn ownership(id: &str, author: &str) -> OwnershipItem {
    OwnershipItem {
        id: id.into(),
        author: author.into(),
        timestamp: None,
        message: None,
        force: false,
        if_available: true,
        provenance_role: None,
    }
}

/// Runs the native SDK's real ownership transition inside the synthetic remote peer.
fn transition(workspace: &Workspace, operation: &Operation) -> AdapterOutcome {
    let result = match operation {
        Operation::Claim { id, author } => workspace.claim(&ownership(id, author)),
        Operation::Release { id, author } => workspace.release(&ownership(id, author)),
        _ => {
            return AdapterOutcome::Success {
                output: json!({"read_only":true}),
            };
        }
    };
    match result {
        Ok(value) => AdapterOutcome::Success {
            output: json!(value),
        },
        Err(PmRustError::OwnershipRefusal { detail, context }) => AdapterOutcome::Refused {
            code: "ownership_conflict".into(),
            message: detail,
            remedy: "skip foreign ownership and select another item".into(),
            details: context,
        },
        Err(error) => AdapterOutcome::Refused {
            code: "sdk_refused".into(),
            message: error.to_string(),
            remedy: "inspect authoritative tracker".into(),
            details: Value::Null,
        },
    }
}

/// Remote adapter that injects a real SDK peer at each boundary it can touch.
struct RacingPeer<'a> {
    /// One shared non-git tracker for both agents.
    workspace: &'a Workspace,
    /// Boundary index for the competing transition.
    boundary: usize,
    /// Independent peer operation.
    competitor: Operation,
}
impl RacingPeer<'_> {
    /// Injects the competing SDK transition at one selected boundary.
    fn step(&self, index: usize) {
        if self.boundary == index {
            let _ = transition(self.workspace, &self.competitor);
            if let Operation::Release { id, .. } = &self.competitor {
                let _ = transition(
                    self.workspace,
                    &Operation::Claim {
                        id: id.clone(),
                        author: "peer".into(),
                    },
                );
            }
        }
    }
}
impl NativeAdapter for RacingPeer<'_> {
    fn manifest(&self) -> AdapterManifest {
        self.step(0);
        let result = manifest();
        self.step(1);
        result
    }
    fn exchange(&mut self, input: &[u8], _: usize) -> io::Result<Vec<u8>> {
        self.step(2);
        let call: AdapterRequest = serde_json::from_slice(input)?;
        self.step(3);
        let outcome = transition(self.workspace, &call.operation);
        self.step(4);
        let encoded = bytes(&AdapterReply {
            abi: ABI.into(),
            request_id: call.request_id,
            outcome,
        });
        self.step(5);
        Ok(encoded)
    }
}

#[test]
fn every_adapter_boundary_preserves_authoritative_claim_and_release() -> TestResult {
    let directory = tracker()?;
    let workspace = Workspace::discover(directory.path())?;
    // Two operation paths, all six observable boundaries, the SAME tracker.
    for release in [false, true] {
        for boundary in 0..6 {
            let id = format!("race-{}-{boundary}", u8::from(release));
            let root = directory.path().join(".agents/pm/tasks");
            fs::create_dir_all(&root)?;
            fs::write(
                root.join(format!("{id}.toon")),
                format!(
                    "id: {id}\ntitle: Race fixture\ntype: Task\nstatus: open\npriority: 1\ncreated_at: \"2026-10-21T12:00:00.000Z\"\nupdated_at: \"2026-10-21T12:00:00.000Z\"\n"
                ),
            )?;
            let operation = if release {
                workspace.claim(&ownership(&id, "caller"))?;
                Operation::Release {
                    id: id.clone(),
                    author: "caller".into(),
                }
            } else {
                Operation::Claim {
                    id: id.clone(),
                    author: "caller".into(),
                }
            };
            let competitor = if release {
                Operation::Release {
                    id: id.clone(),
                    author: "caller".into(),
                }
            } else {
                Operation::Claim {
                    id: id.clone(),
                    author: "peer".into(),
                }
            };
            let mut client = NativeAdapterClient::new(RacingPeer {
                workspace: &workspace,
                boundary,
                competitor,
            })?;
            let response = client.call(&request(operation))?;
            let stored = workspace.get(&id)?;
            if release {
                assert_eq!(stored.metadata.extra["assignee"], "peer");
                match response.outcome {
                    AdapterOutcome::Success { output } => {
                        assert!(boundary >= 4);
                        assert_eq!(output["released_by"], "caller");
                    }
                    AdapterOutcome::Refused { code, remedy, .. } => {
                        assert!(boundary < 4);
                        assert_eq!(code, "ownership_conflict");
                        assert!(remedy.contains("skip foreign"));
                    }
                }
            } else {
                let expected = if boundary < 4 { "peer" } else { "caller" };
                assert_eq!(stored.metadata.extra["assignee"], expected);
                match response.outcome {
                    AdapterOutcome::Success { output } => {
                        assert_eq!(output["claimed_by"], expected);
                        if boundary < 4 {
                            assert_eq!(output["skipped"], true);
                            assert_eq!(output["changed_field_count"], 0);
                        }
                    }
                    AdapterOutcome::Refused { .. } => {
                        return Err("atomic claim should skip peer".into());
                    }
                }
            }
        }
    }
    // A package must not delete foreign claims, even through a transport.
    let mut client = NativeAdapterClient::new(RacingPeer {
        workspace: &workspace,
        boundary: 99,
        competitor: Operation::List {
            limit: 1,
            after: None,
            output_cursor: None,
        },
    })?;
    let response = client.call(&request(Operation::Release {
        id: "race-0-0".into(),
        author: "caller".into(),
    }))?;
    assert!(
        matches!(response.outcome,AdapterOutcome::Refused {code,..} if code=="ownership_conflict")
    );
    assert_eq!(
        workspace.get("race-0-0")?.metadata.extra["assignee"],
        "peer"
    );
    Ok(())
}

#[test]
fn cli_validates_without_tracker_git_node_or_backend_execution() -> TestResult {
    let directory = tempfile::tempdir()?;
    let input = bytes(
        &json!({"manifest":manifest(),"request":request(Operation::Get {id:"demo-1".into()})}),
    );
    for json_output in [false, true] {
        for valid in [false, true] {
            let mut command = assert_cmd::Command::new(env!("CARGO_BIN_EXE_pm-rust"));
            command
                .current_dir(directory.path())
                .env("PATH", directory.path())
                .env(
                    "GIT_CEILING_DIRECTORIES",
                    directory.path().parent().ok_or("parent")?,
                )
                .arg("adapter-check");
            if json_output {
                command.arg("--json");
            }
            let output = command
                .write_stdin(if valid { input.clone() } else { b"{}".to_vec() })
                .output()?;
            assert_eq!(output.status.success(), valid);
            let stdout = String::from_utf8(output.stdout)?;
            assert!(stdout.len() < 600);
            if json_output {
                let value: Value = serde_json::from_str(&stdout)?;
                assert_eq!(value["ok"], valid);
            } else {
                assert!(stdout.contains(if valid {
                    "dispatched: false"
                } else {
                    "invalid_frame"
                }));
            }
        }
    }
    assert!(!directory.path().join(".agents").exists());
    Ok(())
}
