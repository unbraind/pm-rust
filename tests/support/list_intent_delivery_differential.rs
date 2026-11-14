//! Real published delivery API parity for receipt-overhead snapshot continuations.

use crate::{ListOptions, PmRustError, published_cli};
use base64::Engine as _;
use serde_json::{Value, json};
use std::{fs, path::Path};

/// Uses identical producer snapshots at the native and published delivery boundaries.
fn source(padding: bool) -> Value {
    let mut source = super::final_receipt_boundary_source();
    for row in source["items"].as_array_mut().into_iter().flatten() {
        row["title"] = json!("x".repeat(106));
    }
    if padding {
        source["filters"]["padding"] = json!("z".repeat(104));
    }
    source["total"] = json!(75);
    source["has_more"] = json!(true);
    source["next_cursor"] = json!(crate::pagination::encode(
        &json!({"version":1,"fingerprint":"fp","after_id":"demo-42","after_index":42}),
    ));
    source
}

/// Runs the installed SDK's actual delivery and rendering functions without replacing either.
fn published(
    package: &Path,
    directory: &Path,
    source: &Value,
) -> Result<Value, Box<dyn std::error::Error>> {
    let input = directory.join("input.json");
    fs::write(&input, serde_json::to_vec(source)?)?;
    let driver = directory.join("delivery.mjs");
    fs::write(
        &driver,
        format!(
            r"
import {{readFileSync}} from 'node:fs';
import {{pathToFileURL}} from 'node:url';
const {{attachReadOutputContracts}}=await import(pathToFileURL({}));
const {{formatBuiltInOutput}}=await import(pathToFileURL({}));
const source=JSON.parse(readFileSync(process.argv[2],'utf8'));
const options={{for:'triage',outputBudget:'1000',limit:'30',outputLimit:'100',full:true,resolvedOutputFormat:'toon'}};
const first=attachReadOutputContracts('list',options,structuredClone(source));
const cursor=first.output_budget_truncation.continuations[0].cursor;
let retry;
try {{const result=attachReadOutputContracts('list',{{...options,outputCursor:cursor}},structuredClone(source));retry={{ok:true,result,wire:formatBuiltInOutput(result,'toon')}};}}
catch(error) {{retry={{ok:false,detail:error.message,context:error.context}};}}
console.log(JSON.stringify({{first,wire:formatBuiltInOutput(first,'toon'),retry}}));
",
            serde_json::to_string(&package.join("dist/sdk/context-intent-contracts.js"))?,
            serde_json::to_string(&package.join("dist/core/output/output.js"))?
        ),
    )?;
    let mut command = published_cli::oracle_command(
        std::env::var("PM_NODE_INTERPRETER").unwrap_or_else(|_| "node".to_owned()),
    );
    crate::test_process_environment::isolate_environment(&mut command);
    let output = command
        .env("DO_NOT_TRACK", "1")
        .arg(driver)
        .arg(input)
        .output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(serde_json::from_slice(&output.stdout)?)
}

/// Exercises every native assertion even when the optional published installation is absent.
fn native(source: &Value, padding: bool) -> Result<Value, Box<dyn std::error::Error>> {
    let options = ListOptions {
        intent: Some("triage".to_owned()),
        output_budget: Some("1000".to_owned()),
        limit: Some("30".to_owned()),
        output_limit: Some("100".to_owned()),
        full: true,
        toon: true,
        ..Default::default()
    };
    let initial = super::super::attach(source.clone(), &options, 1000);
    let initial_fingerprint = initial.row_fingerprint();
    let initial_rows = initial["items"].clone();
    let initial_receipt = initial["context_intent"].clone();
    let first_pass = super::super::finalize(initial, &options, None)?;
    assert_eq!(
        super::super::settled(&first_pass, &initial_receipt, &options).is_some(),
        padding
    );
    let first = super::super::deliver(source.clone().into(), &options, &options, Some(1000), None)?;
    let raw = first["output_budget_truncation"]["continuations"][0]["cursor"]
        .as_str()
        .ok_or("continuation")?;
    let cursor: Value =
        serde_json::from_slice(&base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(raw)?)?;
    assert!(cursor["o"].as_u64().is_some_and(|offset| offset > 0));
    assert_eq!(cursor["f"] != initial_fingerprint, padding);
    let mut retry = options.clone();
    retry.output_cursor = Some(raw.to_owned());
    let actual = super::super::deliver(source.clone().into(), &retry, &retry, Some(1000), None);
    let retry = if padding {
        let error = actual.err().ok_or("reprojected replay was admitted")?;
        let PmRustError::ReadCursor { code, detail } = error else {
            return Err(format!("unexpected {error}").into());
        };
        assert_eq!(code, "read_output_cursor_stale");
        json!({"ok":false,"detail":detail,"context":{"code":code}})
    } else {
        let result = actual?;
        let offset = usize::try_from(cursor["o"].as_u64().ok_or("offset")?)?;
        assert_eq!(result["items"][0]["id"], initial_rows[offset]["id"]);
        let wire = crate::stringify_toon(&result)?;
        json!({"ok":true,"result":result.value,"wire":wire})
    };
    let wire = crate::stringify_toon(&first)?;
    Ok(json!({"first":first.value,"wire":wire,"retry":retry}))
}

/// A real nonzero reprojected cursor refuses unchanged replay, matching the published SDK.
#[test]
fn round9_reprojected_continuation_matches_published() -> Result<(), Box<dyn std::error::Error>> {
    let oracle = published_cli::published_cli_or_skip("intent delivery continuation");
    for padding in [false, true] {
        let source = source(padding);
        let actual = native(&source, padding)?;
        if let Some(oracle) = &oracle {
            let directory = tempfile::tempdir()?;
            let expected = published(&oracle.package_root, directory.path(), &source)?;
            assert_eq!(actual, expected);
        }
    }
    Ok(())
}
