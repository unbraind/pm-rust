/** Exercise the workflow npm entry point with a real close in a disposable tracker. */
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { copyFileSync, mkdirSync, mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
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

test('release-tag generation explains its Git prerequisite in a plain tracker, including without Git installed', () => {
  const scratch = mkdtempSync(join(tmpdir(), 'pm-rust-changelog-nogit-'));
  const env = { ...process.env, PM_PATH: join(scratch, '.agents/pm'), GIT_CEILING_DIRECTORIES: dirname(scratch) };
  try {
    mkdirSync(join(scratch, 'scripts'));
    for (const file of ['justfile', 'Cargo.toml', 'scripts/changelog.mts']) {
      copyFileSync(join(root, file), join(scratch, file));
    }
    const init = spawnSync(process.execPath, [
      join(root, 'tests/oracle/node_modules/@unbrained/pm-cli/dist/cli.js'),
      'init', '--defaults', '--agent-guidance', 'skip',
    ], { cwd: scratch, env, encoding: 'utf8', timeout: 120_000 });
    assert.equal(init.status, 0, `${init.stdout}\n${init.stderr}`);
    for (const environment of [env, { ...env, PATH: scratch }]) {
      const result = spawnSync(process.execPath, ['scripts/changelog.mts', 'full'], {
        cwd: scratch, env: environment, encoding: 'utf8', timeout: 120_000,
      });
      assert.equal(result.status, 1);
      assert.match(result.stderr, /needs a git repository/);
      assert.match(result.stderr, /Tracker reads and writes still work without git/);
      assert.doesNotMatch(result.stderr, /Command failed: git rev-parse/);
    }
  } finally {
    rmSync(scratch, { recursive: true, force: true });
  }
});
