/** Exercise the workflow npm entry point with a real close in a disposable tracker. */
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { copyFileSync, mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('../', import.meta.url));

test('the workflow npm script regenerates and verifies a closed scratch item without just', () => {
  const scratch = mkdtempSync(join(tmpdir(), 'pm-rust-changelog-'));
  /** Execute a real process and retain its output when the acceptance fails. */
  function run(command: string, args: string[]): string {
    const result = spawnSync(command, args, {
      cwd: scratch, encoding: 'utf8', timeout: 300_000,
      env: { ...process.env, PM_AUTHOR: 'changelog-fixture', PM_PATH: join(scratch, '.agents/pm') },
    });
    assert.equal(result.status, 0, `${result.error ?? ''}\n${result.stdout}\n${result.stderr}`);
    return result.stdout;
  }
  try {
    run('git', ['clone', '--quiet', '--shared', root, '.']);
    for (const file of ['justfile', 'scripts/changelog.mts', 'tests/oracle/package.json']) {
      copyFileSync(join(root, file), join(scratch, file));
    }
    const cli = join(root, 'tests/oracle/node_modules/@unbrained/pm-cli/dist/cli.js');
    const id = 'pm-rust-changelog-fixture';
    run(process.execPath, [cli, 'create', 'task', 'Scratch closeout changelog witness',
      '--id', id, '--create-mode', 'progressive']);
    run(process.execPath, [cli, 'close', id, 'Verified synthetic closeout', '--validate-close', 'warn']);
    const npm = process.platform === 'win32' ? 'npm.cmd' : 'npm';
    run(npm, ['run', 'changelog:full', '--prefix', 'tests/oracle']);
    assert.match(readFileSync(join(scratch, 'CHANGELOG.md'), 'utf8'), /Scratch closeout changelog witness/);
    run(npm, ['run', 'changelog:full', '--prefix', 'tests/oracle', '--', '--check']);
  } finally {
    rmSync(scratch, { recursive: true, force: true });
  }
});
