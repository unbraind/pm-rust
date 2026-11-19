import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { copyFileSync, mkdirSync, mkdtempSync, readFileSync, rmSync, unlinkSync, writeFileSync } from 'node:fs';
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
    GIT_AUTHOR_NAME: 'unbrained', GIT_COMMITTER_NAME: 'unbrained',
    GIT_AUTHOR_EMAIL: '1153461+unbraind@users.noreply.github.com', GIT_COMMITTER_EMAIL: '1153461+unbraind@users.noreply.github.com',
    PM_SENTRY_DISABLED: '1', PM_TELEMETRY_SOURCE_CONTEXT: 'test', DO_NOT_TRACK: '1',
  } });
  assert.equal(result.status, 0, result.stderr + result.stdout);
}

/** Initialize a real plain-directory PM tracker before any optional Git setup. */
async function temporary(callback: (root: string) => void | Promise<void>, prefix = 'prefix-'): Promise<void> {
  const root = mkdtempSync(join(tmpdir(), 'pm-history-prefix-'));
  try {
    command(root, process.execPath, [oracle, 'init', '--defaults', '--prefix', prefix, '--agent-guidance', 'skip', '--no-extensions']);
    await callback(root);
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

/** Reproduce the actual bj7a/unih discontinuities using pm's retained originals, then run its real repair. */
async function realRepairs(callback: (root: string, paths: string[]) => void | Promise<void>): Promise<void> {
  await temporary(async root => {
    command(root, 'git', ['init', '-q']);
    mkdirSync(join(root, '.agents/pm/features'), { recursive: true });
    command(root, process.execPath, [oracle, 'create', 'task', 'Ordinary probe', '--id', 'pm-rust-ordinary', '--create-mode', 'progressive', '--no-extensions']);
    command(root, 'git', ['add', '.agents/pm/tasks/pm-rust-ordinary.toon', '.agents/pm/history/pm-rust-ordinary.jsonl']);
    const paths = ['bj7a', 'unih'].map(suffix => {
      const id = `pm-rust-${suffix}`;
      const path = `.agents/pm/history/${id}.jsonl`;
      const item = `.agents/pm/features/${id}.toon`;
      copyFileSync(new URL(`../${item}`, import.meta.url), join(root, item));
      const repaired = readFileSync(new URL(`../${path}`, import.meta.url), 'utf8').trimEnd().split('\n').map(line => JSON.parse(line));
      const marker = repaired.findIndex(entry => entry.op === 'history_repair');
      assert.ok(marker > 0, 'real repair must retain its audit marker');
      const original = repaired.slice(0, marker).map(entry => entry.reanchor_evidence?.at(-1)?.record ?? entry);
      writeFileSync(join(root, path), original.map(entry => JSON.stringify(entry)).join('\n') + '\n');
      command(root, 'git', ['add', item, path]);
      return path;
    });
    command(root, 'git', ['commit', '-qm', 'Real history discontinuity baseline']);
    command(root, 'git', ['branch', 'baseline']);
    for (const suffix of ['bj7a', 'unih']) {
      command(root, process.execPath, [oracle, 'history-repair', `pm-rust-${suffix}`, '--author', 'conformance-agent', '--no-extensions']);
    }
    await callback(root, paths);
  }, 'pm-rust-');
}

test('history prefix gate accepts the real bj7a and unih pm history-repair re-anchors', () => realRepairs((root, paths) => {
  assert.deepEqual(verifyHistoryPrefixes(root, 'baseline'), { ok: true, base: 'baseline', checked: 2, branchOnly: 0 });
  for (const [index, path] of paths.entries()) {
    const records = readFileSync(join(root, path), 'utf8').trimEnd().split('\n').map(line => JSON.parse(line));
    assert.match(records.at(-1).message, new RegExp(`re-anchored ${index === 0 ? 14 : 17} entries`));
  }
  // A valid repair never exempts another history stream from the original byte-prefix rule.
  const ordinary = '.agents/pm/history/pm-rust-ordinary.jsonl';
  writeFileSync(join(root, ordinary), '{}\n');
  assert.throws(() => verifyHistoryPrefixes(root, 'baseline'), /history_prefix_replaced.*pm-rust-ordinary/);
}));

test('history prefix gate rejects lossy, unaudited and unverifiable repairs', async t => realRepairs(async (root, paths) => {
  for (const path of paths) {
    const repaired = readFileSync(join(root, path), 'utf8');
    const records = (): Record<string, unknown>[] => repaired.trimEnd().split('\n').map(line => JSON.parse(line));
    const reject = (mutate: (entries: Record<string, unknown>[]) => void): void => {
      const entries = records(); mutate(entries);
      writeFileSync(join(root, path), entries.map(entry => JSON.stringify(entry)).join('\n') + '\n');
      assert.throws(() => verifyHistoryPrefixes(root, 'baseline'), /history_prefix_replaced/);
      writeFileSync(join(root, path), repaired);
    };
    await t.test(`${path}: dropped record`, () => reject(entries => { entries.splice(2, 1); }));
    await t.test(`${path}: reordered records`, () => reject(entries => { [entries[2], entries[3]] = [entries[3], entries[2]]; }));
    await t.test(`${path}: changed non-hash field`, () => reject(entries => { entries[0].author = 'altered-agent'; }));
    await t.test(`${path}: missing marker`, () => reject(entries => { entries.pop(); }));
    await t.test(`${path}: unsealed marker`, () => reject(entries => { delete entries.at(-1)!.record_hash; }));
    await t.test(`${path}: marker on an invalid chain`, () => reject(entries => { entries.at(-1)!.after_hash = '0'.repeat(64); }));
    await t.test(`${path}: unrelated marker`, () => reject(entries => { entries.at(-1)!.message = 'An unrelated repair'; }));
    await t.test(`${path}: wrong repair count`, () => reject(entries => { entries.at(-1)!.message = 'history-repair re-anchored 1 entry.'; }));
    await t.test(`${path}: missing retained evidence`, () => reject(entries => { entries.find(entry => entry.reanchor_evidence)!.reanchor_evidence = []; }));
    await t.test(`${path}: changed retained original`, () => reject(entries => {
      const evidence = entries.find(entry => entry.reanchor_evidence)!.reanchor_evidence as { record: Record<string, unknown> }[];
      evidence.at(-1)!.record.author = 'altered-agent';
    }));
    // A fully sealed repaired chain still fails if its paired item differs.
    const item = join(root, path.replace('/history/', '/features/').replace('.jsonl', '.toon'));
    const saved = readFileSync(item, 'utf8');
    await t.test(`${path}: sealed chain does not replay to paired item`, () => {
      writeFileSync(item, saved.replace(/^title:.*$/m, 'title: Changed item after repair'));
      assert.throws(() => verifyHistoryPrefixes(root, 'baseline'), /history_prefix_replaced/);
      writeFileSync(item, saved);
    });
  }
  assert.equal(verifyHistoryPrefixes(root, 'baseline').checked, 2);
}));
