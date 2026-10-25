import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { test } from 'node:test';
import { counts, EXPECTED_CHECKS, parseResults, reconcileResults, runChecks } from './crash-proof-results.mts';

test('subset recorder reports absent checks as not_run and refuses certification', () => {
  const result = spawnSync(process.execPath, ['scripts/crash-oracle-proof.mts', '--checks-only', '--test-name-pattern=phase parity'], { encoding: 'utf8', timeout: 30000 });
  assert.equal(result.status, 1, result.stderr);
  const receipt = JSON.parse(result.stdout);
  assert.deepEqual(receipt.counts, { expected: EXPECTED_CHECKS.length, executed: 1, passed: 1, failed: 0, not_run: EXPECTED_CHECKS.length - 1 });
  assert.ok(receipt.checks.filter((row: { name: string }) => row.name.startsWith('live ')).every((row: { status: string }) => row.status === 'not_run'));
});

test('real runner distinguishes failure skip todo and absent checks in a non-git directory', () => {
  const root = mkdtempSync(join(tmpdir(), 'pm-proof-results-'));
  try {
    const file = join(root, 'checks.test.mjs');
    writeFileSync(file, "import {test} from 'node:test';test('pass',()=>{});test('fail',()=>{throw Error('control')});test('skip',{skip:true},()=>{});test('todo',{todo:true},()=>{});");
    const oldCeiling = process.env.GIT_CEILING_DIRECTORIES;
    process.env.GIT_CEILING_DIRECTORIES = root;
    try {
      const run = runChecks('.', ['pass', 'fail', 'skip', 'todo', 'absent'], resolve(file));
      assert.equal(run.exit, 1);
      assert.deepEqual(counts(run.checks), { expected: 5, executed: 2, passed: 1, failed: 1, not_run: 3 });
    } finally { if (oldCeiling === undefined) delete process.env.GIT_CEILING_DIRECTORIES; else process.env.GIT_CEILING_DIRECTORIES = oldCeiling; }
    assert.deepEqual(reconcileResults([], ['absent']), [{ name: 'absent', status: 'not_run' }]);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('malformed and duplicate terminal events cannot certify a check', () => {
  for (const output of ['bad json', '{}', '{"type":"test:pass","data":{}}', '{"type":"test:start","data":{"name":"check"}}']) assert.throws(() => parseResults(output));
  const event = JSON.stringify({ type: 'test:pass', data: { name: 'duplicate' } });
  assert.throws(() => parseResults(event + '\n' + event), /Duplicate/);
});
