import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { existsSync, readFileSync, mkdtempSync, mkdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';
import { argument, authoritative, relationshipRoots } from './context-real-helpers.mts';
import { measure } from './latency-measure.mts';

test('README advertises every supported native context depth', () => {
  const readme = readFileSync(new URL('../README.md', import.meta.url), 'utf8');
  assert.match(readme, /`context` at brief, standard, deep and full depth/);
});

test('orientation documentation scopes defaults and omissions by depth and settings', () => {
  const orientation = readFileSync(new URL('../docs/NATIVE_ORIENTATION.md', import.meta.url), 'utf8')
    .replace(/\s+/g, ' ');
  assert.match(orientation, /configured default depth.*when no default is configured, it uses brief depth/);
  assert.match(orientation, /omission receipt for sections not selected by the requested depth or workspace settings/);
  assert.match(orientation, /Brief omits all ten optional section groups; full selects all ten/);
  assert.match(orientation, /Deep may still omit sections disabled in workspace settings/);
});

test('required options reject absent, trailing, empty and option-token values', () => {
  for (const args of [[], ['--copy'], ['--copy', ''], ['--copy', '--driver', 'fixture']]) {
    assert.throws(() => argument('--copy', args), /Required: --copy/);
  }
  assert.equal(argument('--copy', ['--copy', 'fixture']), 'fixture');
});

test('real-copy options fail before tracker changes or parity processes', () => {
  const root = mkdtempSync(join(tmpdir(), 'pm-rust-options-'));
  try {
    const runtime = join(root, '.agents/pm/runtime');
    mkdirSync(runtime, { recursive: true });
    writeFileSync(join(root, '.pm-rust-read-copy'), 'synthetic fixture\n');
    const retained = join(runtime, 'retained.txt');
    writeFileSync(retained, 'unchanged\n');
    const invoked = join(root, 'invoked');
    const driver = join(root, 'driver.mjs');
    writeFileSync(driver, `import { writeFileSync } from 'node:fs'; writeFileSync(${JSON.stringify(invoked)}, 'invoked'); process.exit(1);`);
    const output = join(root, 'receipt.json');
    const base = ['--copy', root, '--driver', driver, '--binary', process.execPath];
    const cases = [
      { args: ['--output', output], error: /Required: --samples/ },
      { args: ['--samples', '--output', output], error: /Required: --samples/ },
      ...['0', '-1', '1.5', 'NaN', 'Infinity', '9007199254740992'].map(value => ({
        args: ['--samples', value, '--output', output], error: /Samples must be a positive safe integer/,
      })),
      { args: ['--samples', '1'], error: /Required: --output/ },
      { args: ['--samples', '1', '--output'], error: /Required: --output/ },
      { args: ['--samples', '1', '--output', '--unknown'], error: /Required: --output/ },
    ];
    for (const { args, error } of cases) {
      const result = spawnSync(process.execPath, [fileURLToPath(new URL('./context-real.mts', import.meta.url)), ...base, ...args], { encoding: 'utf8' });
      assert.equal(result.error, undefined);
      assert.equal(result.status, 1);
      assert.match(result.stderr, error);
      assert.equal(readFileSync(retained, 'utf8'), 'unchanged\n');
      assert.equal(existsSync(invoked), false);
      assert.equal(existsSync(output), false);
    }
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('packet roots require local targets even when an external ID collides', () => {
  const items = [
    { id: 'a-global', dependencies: [{ id: 'target', source_kind: 'global' }] },
    { id: 'b-external', dependencies: [{ id: 'target', source_kind: 'external' }] },
    { id: 'c-url', dependencies: [{ id: 'https://example.invalid' }] },
    { id: 'd-missing', parent: 'absent' },
    { id: 'e-local', dependencies: [{ id: 'TARGET', source_kind: 'project' }] },
    { id: 'f-parent', parent: 'target' },
    { id: 'g-blocked', blocked_by: 'target' },
    { id: 'target' },
  ];
  assert.deepEqual([...relationshipRoots(items)], ['f-parent', 'e-local', 'g-blocked']);
  assert.equal(relationshipRoots(items.slice(0, 4)).size, 0);
});

test('latency uses five warm-ups and the median of three nearest-rank p95 windows', () => {
  let calls = 0;
  const result = measure(() => {
    const index = calls++ - 5;
    if (index < 0) return 10000;
    const window = Math.floor(index / 30);
    return window === 0 ? 400 : window === 1 ? 200 : 210;
  });
  assert.equal(calls, 95);
  assert.equal(result.p95_ms, 210);
  assert.equal(result.p50_ms, 210);
  assert.deepEqual(result.windows_ms.map(values => values.length), [30, 30, 30]);
  assert.equal(result.windows_ms[0][0], 400);
});

test('sustained latency violations still fail the unchanged bound', () => {
  const result = measure(() => 251);
  assert.equal(result.p95_ms < 250, false);
  let index = -5;
  const tail = measure(() => ++index % 30 >= 28 ? 300 : 200);
  assert.equal(tail.p50_ms, 200);
  assert.equal(tail.p95_ms, 300);
});


test('authoritative snapshots exclude caches only at the tracker root', () => {
  const root = mkdtempSync(join(tmpdir(), 'pm-rust-snapshot-'));
  try {
    for (const name of ['runtime', 'locks', 'transactions', 'checkpoints', 'search']) {
      for (const prefix of ['', 'custom-types']) {
        const folder = join(root, prefix, name);
        mkdirSync(folder, { recursive: true });
        writeFileSync(join(folder, 'item.toon'), 'title: before\n');
        writeFileSync(join(folder, 'item.jsonl'), '{}\n');
      }
    }
    const before = authoritative(root);
    assert.equal(before.size, 10);
    for (const name of ['runtime', 'locks', 'transactions', 'checkpoints', 'search']) {
      for (const extension of ['toon', 'jsonl']) {
        const relative = join('custom-types', name, `item.${extension}`);
        assert.ok(before.has(relative));
        writeFileSync(join(root, name, `item.${extension}`), 'cache changed\n');
        assert.deepEqual(authoritative(root), before);
        writeFileSync(join(root, relative), 'authoritative changed\n');
        assert.notEqual(authoritative(root).get(relative), before.get(relative));
        writeFileSync(join(root, relative), extension === 'toon' ? 'title: before\n' : '{}\n');
      }
    }
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
