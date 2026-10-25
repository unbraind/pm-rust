import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync, unlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { test } from 'node:test';
import { verifyHistoryPrefixes } from './crash-history-prefix.mts';

const oracle = fileURLToPath(new URL('../tests/oracle/node_modules/@unbrained/pm-cli/dist/cli.js', import.meta.url));
const checker = fileURLToPath(new URL('./crash-history-prefix.mts', import.meta.url));

/** Run real Git or the pinned PM executable against an owned disposable tracker. */
function command(root: string, executable: string, args: string[]): void {
  const result = spawnSync(executable, args, { cwd: root, encoding: 'utf8', env: {
    ...Object.fromEntries(Object.entries(process.env).filter(([name]) => !name.startsWith('PM_') && !['NODE_OPTIONS', 'GIT_DIR', 'GIT_WORK_TREE', 'GIT_COMMON_DIR'].includes(name))), HOME: join(root, 'home'), GIT_CONFIG_GLOBAL: process.platform === 'win32' ? 'NUL' : '/dev/null',
    GIT_CONFIG_NOSYSTEM: '1', GIT_CEILING_DIRECTORIES: dirname(root),
    GIT_AUTHOR_NAME: 'SteveBot', GIT_COMMITTER_NAME: 'SteveBot',
    GIT_AUTHOR_EMAIL: '1153461+unbraind@users.noreply.github.com', GIT_COMMITTER_EMAIL: '1153461+unbraind@users.noreply.github.com',
    PM_SENTRY_DISABLED: '1', PM_TELEMETRY_SOURCE_CONTEXT: 'test', DO_NOT_TRACK: '1',
  } });
  assert.equal(result.status, 0, result.stderr + result.stdout);
}

/** Initialize a real plain-directory PM tracker before any optional Git setup. */
function temporary(callback: (root: string) => void): void {
  const root = mkdtempSync(join(tmpdir(), 'pm-history-prefix-'));
  try {
    command(root, process.execPath, [oracle, 'init', '--defaults', '--prefix', 'prefix-', '--agent-guidance', 'skip', '--no-extensions']);
    callback(root);
  } finally { rmSync(root, { recursive: true, force: true }); }
}

test('history prefix gate rejects replaced and deleted main bytes and accepts real PM appends', () => temporary(root => {
  command(root, 'git', ['init', '-q']);
  /** Mutate only the initialized synthetic tracker through the published CLI. */
  const pm = (...args: string[]): void => command(root, process.execPath, [oracle, ...args, '--author', 'conformance-agent', '--no-extensions']);
  pm('create', 'task', 'Synthetic prefix probe', '--id', 'prefix-probe', '--create-mode', 'progressive');
  const path = '.agents/pm/history/prefix-probe.jsonl';
  command(root, 'git', ['add', '.agents/pm/settings.json', '.agents/pm/tasks/prefix-probe.toon', path]);
  command(root, 'git', ['commit', '-qm', 'Synthetic history baseline']);
  command(root, 'git', ['branch', 'baseline']);
  const original = readFileSync(join(root, path));
  pm('comment', 'prefix-probe', 'A real appended event');
  const appended = readFileSync(join(root, path));
  assert.ok(appended.subarray(0, original.length).equals(original));
  assert.equal(verifyHistoryPrefixes(root, 'baseline').checked, 1);
  writeFileSync(join(root, path), appended.subarray(original.length));
  assert.throws(() => verifyHistoryPrefixes(root, 'baseline'), /history_prefix_replaced/);
  writeFileSync(join(root, path), appended);
  unlinkSync(join(root, path));
  assert.throws(() => verifyHistoryPrefixes(root, 'baseline'), /history_prefix_missing/);
  writeFileSync(join(root, path), appended);
  pm('create', 'task', 'Branch-only prefix probe', '--id', 'prefix-second', '--create-mode', 'progressive');
  command(root, 'git', ['add', '.agents/pm/tasks/prefix-second.toon', '.agents/pm/history/prefix-second.jsonl']);
  assert.deepEqual(verifyHistoryPrefixes(root, 'baseline'), { ok: true, base: 'baseline', checked: 1, branchOnly: 1 });
  assert.throws(() => verifyHistoryPrefixes(root, 'missing-base'), /history_prefix_git_refused/);
}));

test('history prefix gate names its Git requirement in a plain tracker without a Git executable', () => temporary(root => {
  assert.throws(() => verifyHistoryPrefixes(root), /needs a git repository.*tracker reads and writes still work/);
  const result = spawnSync(process.execPath, [checker], { cwd: root, encoding: 'utf8', env: {
    ...process.env, PATH: join(root, 'no-tools'), GIT_CEILING_DIRECTORIES: dirname(root),
  } });
  assert.equal(result.status, 1);
  assert.match(result.stderr, /needs a git repository.*tracker reads and writes still work/);
}));
