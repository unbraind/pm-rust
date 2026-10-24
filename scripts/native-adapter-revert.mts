import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { readFileSync, writeFileSync } from 'node:fs';

const abi = new URL('../src/native_adapter.rs', import.meta.url);
const cli = new URL('../src/main.rs', import.meta.url);
const originals = new Map([abi, cli].map(path => [path, readFileSync(path, 'utf8')]));
const cases = [
  { path: abi, target: ['--test', 'native_adapter_contract'], name: 'raw reply admission', test: 'raw_frame_seeded_corpus_matches_live_client',
    before: 'let checked =\n                check_adapter_reply(&transcript.manifest, &transcript.request, &recorded)?;',
    after: 'let decoded: AdapterReply = serde_json::from_slice(&recorded).map_err(|_| refusal(AdapterRefusalCode::InvalidFrame))?;\n            let encoded = crate::stringify_json(&decoded, false).into_bytes();\n            let checked = check_adapter_reply(&transcript.manifest, &transcript.request, &encoded)?;' },
  { path: abi, target: ['--test', 'native_adapter_contract'], name: 'capability admission', test: 'admission_refuses_before_transport_or_writes',
    before: '!manifest\n        .capabilities\n        .contains(&request.operation.capability())', after: 'manifest.capabilities.is_empty()' },
  { path: abi, target: ['--test', 'native_adapter_contract'], name: 'ABI compatibility', test: 'versions_fields_and_limits_are_fail_closed',
    before: 'abi != NATIVE_ADAPTER_ABI', after: 'abi == "unsupported-by-fixtures"' },
  { path: abi, target: ['--test', 'native_adapter_contract'], name: 'receipt preservation', test: 'canonical_outcomes_survive_the_byte_boundary',
    before: 'Ok(reply)\n}', after: 'Ok(AdapterReply { outcome: AdapterOutcome::Success { output: Value::Null }, ..reply })\n}' },
  { path: cli, target: ['--test', 'native_adapter_contract'], name: 'tracker-free CLI', test: 'cli_validates_without_tracker_git_node_or_backend_execution',
    before: '    let discover = || Workspace::discover(&cli.workspace);',
    after: '    Workspace::discover(&cli.workspace)?;\n    let discover = || Workspace::discover(&cli.workspace);' },
  { path: cli, target: ['--bin', 'pm-rust'], name: 'CLI flush failure', test: 'tests::native_adapter_stdio_refusals',
    before: '    writer.flush()?;\n    Ok(())\n}\n\n/// Dispatches one parsed command',
    after: '    let _ = writer.flush();\n    Ok(())\n}\n\n/// Dispatches one parsed command' },
];
try {
  for (const proof of cases) {
    const original = originals.get(proof.path);
    assert.ok(original);
    assert.equal(original.split(proof.before).length, 2, `Unique behavioral edit required: ${proof.name}`);
    writeFileSync(proof.path, original.replace(proof.before, proof.after));
    const result = spawnSync('cargo', ['test', '--locked', ...proof.target, proof.test, '--', '--exact'], {
      env: { ...process.env, CARGO_BUILD_JOBS: '1' }, encoding: 'utf8', maxBuffer: 4 * 1024 * 1024,
    });
    assert.notEqual(result.status, 0, `${proof.name} must fail when behavior is reverted`);
    assert.match(result.stdout, /test result: FAILED/, 'A compile or launch failure is not revert proof');
    console.log(JSON.stringify({ behavior: proof.name, test: proof.test, failed_as_required: true }));
    writeFileSync(proof.path, original);
  }
} finally {
  for (const [path, original] of originals) writeFileSync(path, original);
}
