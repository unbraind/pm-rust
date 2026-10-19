import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { argument, relationshipRoots } from './context-real-helpers.mts';
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
