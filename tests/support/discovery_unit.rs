//! Discovery schema, parser agreement, denial, bounds and producer round trips.
use super::*;
use clap::Parser;

/// Fallible contract result.
type TestResult = Result<(), Box<dyn std::error::Error>>;

/// Every compiled command and exact spelling comes from the executable parser.
#[test]
#[allow(clippy::too_many_lines)]
fn parser_inventory_and_wire_producers_agree() -> TestResult {
    let request = DiscoveryRequest::default();
    let response = discover_capabilities(&request)?;
    assert_eq!(response.protocol, DISCOVERY_PROTOCOL);
    assert_eq!(response.oracle_version, crate::COMPATIBLE_PM_VERSION);
    assert!(response.unknown_is_unsupported);
    assert!(response.runtime_requirements.is_empty());
    let mut parser = crate::cli::Cli::command();
    parser.build();
    assert_eq!(response.operations.len(), parser.get_subcommands().count());
    for operation in &response.operations {
        let command = parser.find_subcommand(&operation.name).ok_or("command")?;
        assert_eq!(
            serde_json::to_value(&operation.arguments)?,
            serde_json::to_value(arguments(command))?
        );
        assert_eq!(
            operation.provenance == Provenance::OracleSlice,
            operation.parity_receipt.is_some()
        );
        assert!(!operation.invocation_receipt.is_empty());
        for recovery in &operation.recovery {
            assert!(
                response
                    .recovery_schemas
                    .iter()
                    .any(|schema| schema.id == *recovery)
            );
        }
        let selected = discover_capabilities(&DiscoveryRequest {
            operation: Some(operation.name.clone()),
            ..request.clone()
        })?;
        assert_eq!(selected.operations.len(), 1);
        assert_eq!(selected.operations[0].name, operation.name);
    }
    let list = response
        .operations
        .iter()
        .find(|op| op.name == "list")
        .ok_or("list")?;
    assert!(
        list.arguments
            .iter()
            .any(|arg| arg.spellings.contains(&"--cursor".into()))
    );
    let close = response
        .operations
        .iter()
        .find(|op| op.name == "close")
        .ok_or("close")?;
    assert!(
        close
            .arguments
            .iter()
            .any(|arg| arg.spellings.contains(&"-r".into()))
    );
    assert!(
        close
            .arguments
            .iter()
            .any(|arg| arg.missing_defaults == ["warn"])
    );
    assert!(list.arguments.iter().any(|arg| !arg.conflicts.is_empty()));
    let context = response
        .operations
        .iter()
        .find(|op| op.name == "context")
        .ok_or("context")?;
    assert!(context.arguments.iter().any(|arg| arg.hidden));
    let search = response
        .operations
        .iter()
        .find(|op| op.name == "search")
        .ok_or("search")?;
    assert!(search.arguments.iter().any(|arg| arg.max_values.is_none()));
    for json in [false, true] {
        let rendered = response.render(json)?;
        assert!(rendered.len() < DISCOVERY_MAX_BYTES as usize);
        let decoded = if json {
            crate::strict_json::from_str(&rendered)?
        } else {
            toon_format::decode_default(&rendered)?
        };
        let roundtrip: DiscoveryResponse = serde_json::from_value(decoded)?;
        assert_eq!(
            serde_json::to_value(&roundtrip)?,
            serde_json::to_value(&response)?
        );
        let small = DiscoveryResponse {
            output_bytes: 1,
            ..response.clone()
        };
        assert_eq!(
            small.render(json).err().ok_or("limit")?.code,
            DiscoveryRefusalCode::OutputLimit
        );
    }
    assert_eq!(
        operation(&clap::Command::new("not-evidenced"))
            .err()
            .ok_or("drift")?
            .code,
        DiscoveryRefusalCode::ContractDrift
    );
    // Root spelling and canonical recovery namespaces are independently accepted.
    for args in [
        ["pm-rust", "--path", ".", "capabilities"],
        ["pm-rust", "--pm-path", ".", "capabilities"],
    ] {
        crate::cli::Cli::try_parse_from(args)?;
    }
    Ok(())
}

/// Strict admission rejects unsafe numeric, sparse, duplicate, oversized and unknown inputs.
#[test]
#[allow(clippy::too_many_lines)]
fn discovery_must_refuse_and_must_allow_corpora() -> TestResult {
    for input in [
        "null",
        "[]",
        "{}",
        "{\"protocol\":1}",
        r#"{"protocol":"pm-rust/capabilities/v1","output_bytes":131072,"extra":true}"#,
        r#"{"protocol":"pm-rust/capabilities/v1","output_bytes":131072,"output_bytes":1024}"#,
        r#"{"protocol":"pm-rust/capabilities/v1","output_bytes":1e309}"#,
        r#"{"protocol":"pm-rust/capabilities/v1","output_bytes":NaN}"#,
        r#"{"protocol":"pm-rust/capabilities/v1","output_bytes":Infinity}"#,
        r#"{"protocol":"pm-rust/capabilities/v1","output_bytes":-1}"#,
        r#"{"protocol":"pm-rust/capabilities/v1","output_bytes":1.5}"#,
        r#"{"protocol":"pm-rust/capabilities/v1","output_bytes":4294967296}"#,
        r#"{"protocol":"pm-rust/capabilities/v1","output_bytes":131072,"operation":[]}"#,
        r#"{"protocol":"pm-rust/capabilities/v1","output_bytes":131072,"adapter":{"abi":"pm-native-adapter/1","capabilities":[,"get"],"max_frame_bytes":10}}"#,
    ] {
        assert_eq!(
            DiscoveryRequest::from_json(input.as_bytes())
                .err()
                .ok_or("request refusal")?
                .code,
            DiscoveryRefusalCode::InvalidRequest,
            "{input}"
        );
    }
    assert!(DiscoveryRequest::from_json(&vec![b' '; DISCOVERY_INPUT_BYTES + 1]).is_err());
    let base = DiscoveryRequest::default();
    for request in [
        DiscoveryRequest {
            protocol: "x".repeat(65),
            ..base.clone()
        },
        DiscoveryRequest {
            operation: Some(String::new()),
            ..base.clone()
        },
        DiscoveryRequest {
            operation: Some("x".repeat(65)),
            ..base.clone()
        },
        DiscoveryRequest {
            output_bytes: 0,
            ..base.clone()
        },
        DiscoveryRequest {
            output_bytes: 1023,
            ..base.clone()
        },
        DiscoveryRequest {
            output_bytes: DISCOVERY_MAX_BYTES + 1,
            ..base.clone()
        },
        DiscoveryRequest {
            output_bytes: u32::MAX,
            ..base.clone()
        },
    ] {
        assert_eq!(
            discover_capabilities(&request).err().ok_or("refusal")?.code,
            DiscoveryRefusalCode::InvalidRequest
        );
    }
    assert_eq!(
        discover_capabilities(&DiscoveryRequest {
            protocol: "v999".into(),
            ..base.clone()
        })
        .err()
        .ok_or("protocol")?
        .code,
        DiscoveryRefusalCode::IncompatibleProtocol
    );
    for name in [
        "import",
        "export",
        "sync",
        "merge",
        "delete",
        "next",
        "constructor",
        "__proto__",
        "list ",
        "List",
    ] {
        assert_eq!(
            discover_capabilities(&DiscoveryRequest {
                operation: Some(name.into()),
                ..base.clone()
            })
            .err()
            .ok_or("unsupported")?
            .code,
            DiscoveryRefusalCode::UnsupportedOperation
        );
    }
    assert_eq!(
        discover_capabilities(&DiscoveryRequest {
            output_bytes: 1024,
            ..base.clone()
        })
        .err()
        .ok_or("limit")?
        .code,
        DiscoveryRefusalCode::OutputLimit
    );
    for bytes in [
        serde_json::to_vec(&base)?,
        serde_json::to_string_pretty(&base)?
            .replace('\n', "\r\n")
            .into_bytes(),
    ] {
        let admitted = DiscoveryRequest::from_json(&bytes)?;
        assert_eq!(
            discover_capabilities(&admitted)?.protocol,
            DISCOVERY_PROTOCOL
        );
    }
    for code in [
        DiscoveryRefusalCode::InvalidRequest,
        DiscoveryRefusalCode::IncompatibleProtocol,
        DiscoveryRefusalCode::UnsupportedOperation,
        DiscoveryRefusalCode::InvalidAdapter,
        DiscoveryRefusalCode::OutputLimit,
        DiscoveryRefusalCode::ContractDrift,
    ] {
        let refusal = refuse(code);
        assert!(!refusal.required.is_empty());
        assert!(!refusal.to_string().is_empty());
        assert_eq!(refusal.retry_args, ["capabilities", "--json"]);
        let decoded: DiscoveryRefusal =
            crate::strict_json::from_str(&crate::stringify_json(&refusal, false))?;
        assert_eq!(decoded.code, code);
    }
    Ok(())
}

/// Objects are required at both discovery and adapter wire boundaries.
#[test]
fn discovery_rejects_positional_struct_arrays() -> TestResult {
    for input in [
        r#"["pm-rust/capabilities/v1",null,131072,null]"#,
        r#"["pm-rust/capabilities/v1","get",131072,null]"#,
        "null",
        "true",
        "0",
        "\"request\"",
    ] {
        // proof-behavior
        assert_eq!(
            DiscoveryRequest::from_json(input.as_bytes())
                .err()
                .ok_or("shape")?
                .code,
            DiscoveryRefusalCode::InvalidRequest,
            "{input}"
        );
    }
    let adapter = r#"["pm-native-adapter/1",["get"],1024]"#;
    let input = format!(
        r#"{{"protocol":"pm-rust/capabilities/v1","output_bytes":131072,"adapter":{adapter}}}"#
    );
    // proof-behavior
    assert_eq!(
        DiscoveryRequest::from_json(input.as_bytes())
            .err()
            .ok_or("adapter shape")?
            .code,
        DiscoveryRefusalCode::InvalidAdapter
    );
    let base = DiscoveryRequest::default();
    // proof-behavior
    assert_eq!(
        DiscoveryRequest::from_cli(
            base.protocol.clone(),
            None,
            base.output_bytes,
            Some(adapter)
        )
        .err()
        .ok_or("CLI adapter shape")?
        .code,
        DiscoveryRefusalCode::InvalidAdapter
    );
    for input in [serde_json::to_string(&base)?,
        r#"{"protocol":"pm-rust/capabilities/v1","output_bytes":131072}"#.into(),
        r#"{"protocol":"pm-rust/capabilities/v1","operation":"get","output_bytes":131072,"adapter":{"abi":"pm-native-adapter/1","capabilities":["get"],"max_frame_bytes":1024}}"#.into()] {
        discover_capabilities(&DiscoveryRequest::from_json(input.as_bytes())?)?;
    }
    Ok(())
}

/// Both public admission paths classify malformed adapter content identically.
#[test]
fn nested_adapter_refusal_codes_match_cli() -> TestResult {
    for adapter in [
        r#"{"abi":"pm-native-adapter/1","capabilities":["destroy"],"max_frame_bytes":1}"#,
        r#"{"abi":"pm-native-adapter/1","capabilities":["get"],"max_frame_bytes":1,"extra":true}"#,
        r#"{"abi":"pm-native-adapter/1","capabilities":"get","max_frame_bytes":1}"#,
        r#"{"abi":"pm-native-adapter/1","capabilities":["get"],"max_frame_bytes":"1"}"#,
        "{}",
    ] {
        let input = format!(
            r#"{{"protocol":"pm-rust/capabilities/v1","output_bytes":131072,"adapter":{adapter}}}"#
        );
        let sdk = DiscoveryRequest::from_json(input.as_bytes())
            .err()
            .ok_or("SDK refusal")?;
        let cli = DiscoveryRequest::from_cli(
            DISCOVERY_PROTOCOL.into(),
            None,
            DISCOVERY_MAX_BYTES,
            Some(adapter),
        )
        .err()
        .ok_or("CLI refusal")?;
        // proof-behavior
        assert_eq!(sdk.code, DiscoveryRefusalCode::InvalidAdapter, "{adapter}");
        // proof-behavior
        assert_eq!(serde_json::to_value(sdk)?, serde_json::to_value(cli)?);
    }
    Ok(())
}

/// Every directed parser declaration is advertised and enforced in both directions.
#[test]
fn parser_conflicts_are_reciprocal() -> TestResult {
    let mut parser = crate::cli::Cli::command();
    parser.build();
    for command in parser.get_subcommands() {
        let inventory = arguments(command);
        for arg in command.get_arguments() {
            for conflict in command.get_arg_conflicts_with(arg) {
                for (left, right) in [(arg, conflict), (conflict, arg)] {
                    let contract = inventory
                        .iter()
                        .find(|value| value.id == left.get_id().as_str())
                        .ok_or("argument")?;
                    // proof-behavior
                    assert!(
                        contract.conflicts.contains(&right.get_id().to_string()),
                        "{}: {} conflicts with {}",
                        command.get_name(),
                        left.get_id(),
                        right.get_id()
                    );
                }
            }
        }
    }
    for (left, right, value) in [
        ("status", "all", "open"),
        ("full", "brief", ""),
        ("after", "offset", "fixture"),
    ] {
        let mut args = vec!["pm-rust", "list"];
        let left_flag = format!("--{left}");
        let right_flag = format!("--{right}");
        args.push(&left_flag);
        if !value.is_empty() {
            args.push(value);
        }
        args.push(&right_flag);
        if right == "offset" {
            args.push("1");
        }
        // proof-behavior
        assert_eq!(
            crate::cli::Cli::try_parse_from(args)
                .err()
                .ok_or("conflict")?
                .kind(),
            clap::error::ErrorKind::ArgumentConflict
        );
    }
    Ok(())
}

/// Independently execute every bare optional-value flag to verify missing defaults.
#[test]
fn missing_defaults_match_bare_parser_execution() -> TestResult {
    let mut parser = crate::cli::Cli::command();
    parser.build();
    for command in parser.get_subcommands() {
        for contract in arguments(command) {
            if contract.min_values != 0 || contract.max_values == Some(0) {
                continue;
            }
            let flag = contract.spellings.first().ok_or("optional flag")?;
            // Ignore unrelated required positional arguments; no product dispatch occurs.
            let matches = command
                .clone()
                .ignore_errors(true)
                .try_get_matches_from([command.get_name(), flag])?;
            let values: Vec<String> = matches
                .get_raw(&contract.id)
                .into_iter()
                .flatten()
                .map(|value| value.to_string_lossy().into_owned())
                .collect();
            // proof-behavior
            assert_eq!(
                contract.missing_defaults,
                values,
                "{}: {}",
                command.get_name(),
                contract.id
            );
        }
    }
    Ok(())
}

/// Lifecycle commands include their public wrappers and option-dependent entry points.
#[test]
fn ownership_sdk_mappings_include_public_entry_points() -> TestResult {
    let response = discover_capabilities(&DiscoveryRequest::default())?;
    for (name, entry_points) in [
        (
            "claim",
            vec![
                "Workspace::claim_with_options",
                "Workspace::claim_next",
                "Workspace::start_task",
            ],
        ),
        (
            "release",
            vec!["Workspace::release_with_options", "Workspace::pause_task"],
        ),
        (
            "start-task",
            vec!["Workspace::start_task", "Workspace::claim_with_options"],
        ),
        (
            "pause-task",
            vec!["Workspace::pause_task", "Workspace::release_with_options"],
        ),
    ] {
        let operation = response
            .operations
            .iter()
            .find(|op| op.name == name)
            .ok_or("operation")?;
        // proof-behavior
        assert_eq!(&operation.sdk[..entry_points.len()], entry_points);
    }
    Ok(())
}

/// Every backend describes only selected operations, including all adapter grant subsets.
#[test]
fn selected_operation_filters_every_backend() -> TestResult {
    let all = discover_capabilities(&DiscoveryRequest::default())?;
    for op in all.operations {
        for subset in 0..32 {
            let adapter = AdapterManifest {
                abi: crate::native_adapter::NATIVE_ADAPTER_ABI.into(),
                capabilities: [
                    AdapterCapability::Changes,
                    AdapterCapability::Get,
                    AdapterCapability::List,
                    AdapterCapability::Claim,
                    AdapterCapability::Release,
                ]
                .into_iter()
                .enumerate()
                .filter(|(i, _)| subset & (1 << i) != 0)
                .map(|(_, cap)| cap)
                .collect(),
                max_frame_bytes: 1024,
            };
            let granted = adapter
                .capabilities
                .iter()
                .any(|cap| crate::stringify_json(cap, false) == format!("\"{}\"", op.name));
            let selected = discover_capabilities(&DiscoveryRequest {
                operation: Some(op.name.clone()),
                adapter: Some(adapter),
                ..DiscoveryRequest::default()
            })?;
            let local = if ["capabilities", "adapter-check", "help"].contains(&op.name.as_str()) {
                vec![]
            } else {
                vec![op.name.clone()]
            };
            // proof-behavior
            assert_eq!(selected.backends[0].operations, local);
            // proof-behavior
            assert_eq!(
                selected.backends[1].operations,
                if granted {
                    vec![op.name.clone()]
                } else {
                    vec![]
                }
            );
        }
    }
    Ok(())
}

/// Authorization manifests are explicitly supplied and use existing ABI admission.
#[test]
fn backend_inventory_never_invents_availability() -> TestResult {
    let base = DiscoveryRequest::default();
    let empty = discover_capabilities(&base)?;
    assert_eq!(empty.backends[1].availability, "requires_adapter");
    assert!(empty.backends[1].operations.is_empty());
    let capabilities = [
        AdapterCapability::Changes,
        AdapterCapability::Get,
        AdapterCapability::List,
        AdapterCapability::Claim,
        AdapterCapability::Release,
    ];
    for subset in 0..32 {
        let selected: Vec<_> = capabilities
            .iter()
            .enumerate()
            .filter(|(i, _)| subset & (1 << i) != 0)
            .map(|(_, v)| *v)
            .collect();
        let manifest = AdapterManifest {
            abi: crate::native_adapter::NATIVE_ADAPTER_ABI.into(),
            capabilities: selected.clone(),
            max_frame_bytes: 1024,
        };
        let text = crate::stringify_json(&manifest, false);
        let request = DiscoveryRequest::from_cli(
            base.protocol.clone(),
            None,
            base.output_bytes,
            Some(&text),
        )?;
        let response = discover_capabilities(&request)?;
        assert_eq!(response.backends[1].availability, "declared_only");
        assert_eq!(
            response.backends[1].operations,
            selected
                .iter()
                .map(|v| crate::stringify_json(v, false).trim_matches('"').to_owned())
                .collect::<Vec<_>>()
        );
        assert!(!response.backends[1].operations.contains(&"create".into()));
    }
    for manifest in [
        AdapterManifest {
            abi: "next".into(),
            capabilities: vec![],
            max_frame_bytes: 1,
        },
        AdapterManifest {
            abi: crate::native_adapter::NATIVE_ADAPTER_ABI.into(),
            capabilities: vec![],
            max_frame_bytes: 0,
        },
        AdapterManifest {
            abi: crate::native_adapter::NATIVE_ADAPTER_ABI.into(),
            capabilities: vec![AdapterCapability::Get; 2],
            max_frame_bytes: 1,
        },
        AdapterManifest {
            abi: crate::native_adapter::NATIVE_ADAPTER_ABI.into(),
            capabilities: vec![AdapterCapability::Get; 6],
            max_frame_bytes: 1,
        },
    ] {
        assert_eq!(
            discover_capabilities(&DiscoveryRequest {
                adapter: Some(manifest),
                ..base.clone()
            })
            .err()
            .ok_or("adapter")?
            .code,
            DiscoveryRefusalCode::InvalidAdapter
        );
    }
    for text in [
        "[]",
        "{",
        r#"{"abi":"pm-native-adapter/1","capabilities":["destroy"],"max_frame_bytes":1}"#,
    ] {
        assert_eq!(
            DiscoveryRequest::from_cli(base.protocol.clone(), None, base.output_bytes, Some(text))
                .err()
                .ok_or("adapter shape")?
                .code,
            DiscoveryRefusalCode::InvalidAdapter
        );
    }
    assert!(
        DiscoveryRequest::from_cli(
            base.protocol,
            None,
            base.output_bytes,
            Some(&" ".repeat(DISCOVERY_INPUT_BYTES + 1))
        )
        .is_err()
    );
    Ok(())
}

/// Parser metadata handles every public alias/arity shape without an independent flag table.
#[test]
fn parser_edge_metadata_and_codec_refusal() -> TestResult {
    let mut command = clap::Command::new("edge").arg(
        clap::Arg::new("value")
            .long("value")
            .short('v')
            .alias("alias")
            .short_alias('x'),
    );
    command.build();
    let metadata = arguments(&command);
    let value = metadata
        .iter()
        .find(|arg| arg.id == "value")
        .ok_or("value")?;
    assert_eq!(value.spellings, ["--value", "--alias", "-v", "-x"]);
    assert!(value.description.is_empty());
    let request = DiscoveryRequest::default();
    let mut drifted =
        crate::cli::Cli::command().subcommand(clap::Command::new("unlisted-native-operation"));
    assert_eq!(
        discover_from_parser(&request, &mut drifted)
            .err()
            .ok_or("inventory drift")?
            .code,
        DiscoveryRefusalCode::ContractDrift
    );
    let response = discover_capabilities(&request)?;
    let codec_error = toon_format::ToonError::SerializationError("synthetic codec failure".into());
    assert_eq!(
        response
            .check_encoded(Err(codec_error))
            .err()
            .ok_or("codec refusal")?
            .code,
        DiscoveryRefusalCode::ContractDrift
    );
    let json_ceiling = u32::try_from(response.render(true)?.len() + 1)?;
    assert_eq!(
        discover_capabilities(&DiscoveryRequest {
            output_bytes: json_ceiling,
            ..request
        })
        .err()
        .ok_or("TOON-only ceiling")?
        .code,
        DiscoveryRefusalCode::OutputLimit
    );
    let refusal = DiscoveryRefusal::from(toon_format::ToonError::InvalidInput(
        "synthetic codec failure".into(),
    ));
    assert_eq!(refusal.code, DiscoveryRefusalCode::ContractDrift);
    Ok(())
}

/// Received or caller-modified response metadata cannot increase the renderer's hard ceiling.
#[test]
fn renderer_hard_ceiling_cannot_be_increased_by_received_metadata() -> TestResult {
    let mut response = discover_capabilities(&DiscoveryRequest::default())?;
    response.product_version = "x".repeat(DISCOVERY_MAX_BYTES as usize + 1);
    for budget in [DISCOVERY_MAX_BYTES + 1, u32::MAX] {
        response.output_bytes = budget;
        for json in [false, true] {
            assert!(
                response.render(json).is_err(),
                "a received response cannot raise the hard output ceiling"
            );
        }
    }
    Ok(())
}

/// Extreme numeric inputs cannot enter either output encoding or create nonfinite values.
#[test]
fn seeded_numeric_boundary_fuzz() -> TestResult {
    use proptest::{
        prelude::*,
        test_runner::{Config, RngSeed, TestRunner},
    };
    let mut runner = TestRunner::new(Config {
        cases: 500,
        rng_seed: RngSeed::Fixed(20_261_105),
        failure_persistence: None,
        ..Config::default()
    });
    runner.run(&prop_oneof![any::<u64>(), 1024_u64..=u64::from(DISCOVERY_MAX_BYTES), Just(0), Just(1023), Just(u64::from(DISCOVERY_MAX_BYTES)), Just(u64::from(DISCOVERY_MAX_BYTES)+1)], |value| {
        let input = format!(r#"{{"protocol":"pm-rust/capabilities/v1","operation":"get","output_bytes":{value}}}"#);
        let request = DiscoveryRequest::from_json(input.as_bytes());
        if (1024..=u64::from(DISCOVERY_MAX_BYTES)).contains(&value) {
            let request = request.map_err(|error| TestCaseError::fail(error.to_string()))?;
            match discover_capabilities(&request) {
                Ok(response) => {
                    for json in [false, true] {
                        let rendered = response.render(json).map_err(|error| TestCaseError::fail(error.to_string()))?;
                        prop_assert!(u64::try_from(rendered.len()).unwrap_or(u64::MAX) < value);
                        let decoded: serde_json::Value = if json { serde_json::from_str(&rendered).map_err(|error| TestCaseError::fail(error.to_string()))? } else { toon_format::decode_default(&rendered).map_err(|error| TestCaseError::fail(error.to_string()))? };
                        prop_assert_eq!(&decoded["output_bytes"], &serde_json::json!(value));
                    }
                }
                Err(error) => prop_assert_eq!(error.code, DiscoveryRefusalCode::OutputLimit),
            }
        } else { prop_assert!(request.is_err()); }
        Ok(())
    })?;
    Ok(())
}

/// Descriptors generate consistent schemas and discovery enums match typed producers.
#[test]
fn recovery_schemas_describe_actual_producers() -> TestResult {
    let response = discover_capabilities(&DiscoveryRequest::default())?;
    for schema in &response.recovery_schemas {
        if schema.id == "parser" {
            assert!(schema.wire_schema.is_none());
            continue;
        }
        let wire = schema.wire_schema.as_ref().ok_or("wire schema")?;
        assert_eq!(wire["type"], "object");
        assert_eq!(
            wire["required"],
            serde_json::json!(
                schema
                    .required
                    .iter()
                    .map(|field| &field[0])
                    .collect::<Vec<_>>()
            )
        );
        for [name, _] in schema.required.iter().chain(&schema.optional) {
            assert!(wire["properties"][name].is_object());
        }
    }
    let discovery = response
        .recovery_schemas
        .iter()
        .find(|schema| schema.id == "discovery")
        .ok_or("discovery schema")?;
    let codes = discovery.wire_schema.as_ref().ok_or("discovery schema")?["properties"]["refusal"]
        ["properties"]["code"]["enum"]
        .as_array()
        .ok_or("codes")?;
    for code in [
        DiscoveryRefusalCode::InvalidRequest,
        DiscoveryRefusalCode::IncompatibleProtocol,
        DiscoveryRefusalCode::UnsupportedOperation,
        DiscoveryRefusalCode::InvalidAdapter,
        DiscoveryRefusalCode::OutputLimit,
        DiscoveryRefusalCode::ContractDrift,
    ] {
        let emitted = serde_json::to_value(refuse(code))?;
        assert!(codes.contains(&emitted["code"]));
        let decoded: DiscoveryRefusal = crate::strict_json::from_str(&emitted.to_string())?;
        assert_eq!(decoded.code, code);
    }
    assert_eq!(
        recovery_field_schema("AdmissionLimits")["const"],
        serde_json::to_value(crate::admission::ADMISSION_LIMITS)?
    );
    Ok(())
}
