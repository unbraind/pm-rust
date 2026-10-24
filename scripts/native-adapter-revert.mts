import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { readFileSync, writeFileSync } from 'node:fs';

const path = new URL('../src/native_adapter.rs', import.meta.url);
const original = readFileSync(path, 'utf8');
const cases = [
  { name: 'capability admission', test: 'admission_refuses_before_transport_or_writes',
    before: '!manifest\n        .capabilities\n        .contains(&request.operation.capability())', after: 'manifest.capabilities.is_empty()' },
  { name: 'ABI compatibility', test: 'versions_fields_and_limits_are_fail_closed',
    before: 'abi != NATIVE_ADAPTER_ABI', after: 'abi == "unsupported-by-fixtures"' },
  { name: 'receipt preservation', test: 'canonical_outcomes_survive_the_byte_boundary',
    before: 'Ok(reply)\n}', after: 'Ok(AdapterReply { outcome: AdapterOutcome::Success { output: Value::Null }, ..reply })\n}' },
];
try {
  for (const proof of cases) {
    assert.equal(original.split(proof.before).length, 2, `Unique behavioral edit required: ${proof.name}`);
    writeFileSync(path, original.replace(proof.before, proof.after));
    const result = spawnSync('cargo', ['test', '--locked', '--test', 'native_adapter_contract', proof.test, '--', '--exact'], {
      env: { ...process.env, CARGO_BUILD_JOBS: '1' }, encoding: 'utf8', maxBuffer: 4 * 1024 * 1024,
    });
    assert.notEqual(result.status, 0, `${proof.name} must fail when behavior is reverted`);
    assert.match(result.stdout, /test result: FAILED/, 'A compile or launch failure is not revert proof');
    console.log(JSON.stringify({ behavior: proof.name, test: proof.test, failed_as_required: true }));
    writeFileSync(path, original);
  }
} finally {
  writeFileSync(path, original);
}
