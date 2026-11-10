import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { test } from 'node:test';

test('standalone crash-check installs locked oracle before harness typecheck', () => {
  const result = spawnSync('just', ['--dry-run', 'crash-check'], { encoding: 'utf8' });
  assert.equal(result.status, 0, result.stderr);
  const commands = result.stdout + result.stderr;
  const oracle = commands.indexOf('npm ci --prefix tests/oracle');
  const harness = commands.indexOf('npm ci --prefix tests/harness');
  const typecheck = commands.indexOf('tsc -p tests/harness/tsconfig.json');
  assert.ok(oracle >= 0 && oracle < harness && harness < typecheck,
    'locked oracle and harness must install before typecheck');
});
