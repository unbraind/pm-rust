import { assertionFailureDiagnostic } from './context-proof-diagnostics.mts';
import { proofCopyFilter, registerCargoConfiguration } from './context-revert-helpers.mts';
import { absentLink } from './context-revert-helpers.mts';
import { ProofProcesses, verifyAsyncMutation } from './context-proof-process.mts';
import assert from 'node:assert/strict';
import { cpSync, mkdtempSync, readFileSync, symlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { assertNamedTests } from './context-revert-helpers.mts';

const root = resolve(import.meta.dirname, '..');
const scratch = mkdtempSync(join(tmpdir(), 'native-adapter-revert-'));

const abi = join(scratch, 'src/native_adapter.rs');
const cli = join(scratch, 'src/main.rs');
const strict = join(scratch, 'src/strict_json.rs');
registerCargoConfiguration(root, scratch);
const processes = new ProofProcesses();
try {
for (const name of ['Cargo.toml', 'Cargo.lock', 'build.rs', 'src', 'tests', 'examples']) cpSync(join(root, name), join(scratch, name), { recursive: true, dereference: true, filter: path => !path.split(/[\\/]/).includes('node_modules') && !absentLink(path) && proofCopyFilter(path) });
for (const oracle of ['oracle', 'orientation-oracle']) symlinkSync(join(root, 'tests', oracle, 'node_modules'), join(scratch, 'tests', oracle, 'node_modules'), 'junction');
const originals = new Map([abi, cli, strict].map(path => [path, readFileSync(path, 'utf8')]));
const strictBefore = '    let mut reader = serde_json::Deserializer::from_slice(bytes);\n    let StrictValue(value) = StrictValue::deserialize(&mut reader)?;\n    reader.end()?;\n    serde_json::from_value(value)';
const strictAfter = '    serde_json::from_slice(bytes)';
const cases = [
  { path: strict, target: ['--test', 'native_adapter_contract'], name: 'recursive duplicate admission', test: 'strict_json_duplicate_corpus_matches_live_client', before: strictBefore, after: strictAfter },
  { path: strict, target: ['--test', 'strict_json_contract'], name: 'validation before typed field filtering', test: 'strict_json_validates_before_typed_fields_are_ignored', before: strictBefore, after: strictAfter },
  { path: strict, target: ['--test', 'strict_json_contract'], name: 'strict tracker decode audit', test: 'strict_json_tracker_decodes_refuse_duplicates_without_git', before: strictBefore, after: strictAfter },
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
    before: '    writer.flush()?;\n    Ok(())\n}\n\n/// Renders an admitted response',
    after: '    let _ = writer.flush();\n    Ok(())\n}\n\n/// Renders an admitted response' },
];
  const selected = process.argv[2];
  const proofs = cases.filter(proof => selected === undefined || proof.name === selected);
  assert.ok(proofs.length > 0, 'Unknown adapter proof name');
  for (const proof of proofs) {
    const original = originals.get(proof.path);
    assert.ok(original);
    assert.equal(original.split(proof.before).length, 2, `Unique behavioral edit required: ${proof.name}`);
    await verifyAsyncMutation(async reverted => {
      const result = await processes.run('cargo', ['+1.90.0', 'test', '-j', '2', '--locked', ...proof.target, proof.test, '--', '--exact', '--test-threads=2'], {
        cwd: scratch, env: { ...process.env, CARGO_BUILD_JOBS: '2', RUST_TEST_THREADS: '2', CARGO_TARGET_DIR: join(scratch, 'target') }, maxBuffer: 4 * 1024 * 1024,
      });
      assertNamedTests(result, [proof.test], reverted, false, assertionFailureDiagnostic(scratch, [proof.test]));
    }, () => writeFileSync(proof.path, original.replace(proof.before, proof.after)), () => writeFileSync(proof.path, original), processes);
    console.log(JSON.stringify({ behavior: proof.name, test: proof.test, failed_as_required: true }));
  }
} catch (error) {
  if (!processes.wasInterrupted) throw error;
} finally {
  await processes.cleanup(scratch);
}
