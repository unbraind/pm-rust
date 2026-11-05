import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { chmodSync, mkdirSync, mkdtempSync, readdirSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { performance } from 'node:perf_hooks';
import { PmClient, certifyCompleteListResult } from '../tests/oracle/node_modules/@unbrained/pm-cli/dist/sdk/index.js';
import { fingerprint, snapshot } from './crash-storage.mts';

/** Require an explicitly chosen public repository and native executable. */
function argument(flag: string): string {
  const index = process.argv.indexOf(flag);
  const value = index < 0 ? undefined : process.argv[index + 1];
  assert.ok(value, `${flag} is required`);
  return resolve(value);
}
/** Set permissions only on this validator's exclusively owned scratch copy. */
function permissions(root: string, readonly: boolean): void {
  for (const entry of readdirSync(root, { withFileTypes: true })) {
    const path = join(root, entry.name);
    if (entry.isDirectory()) permissions(path, readonly);
    else { assert.ok(entry.isFile()); chmodSync(path, readonly ? 0o400 : 0o600); }
  }
  chmodSync(root, readonly ? 0o500 : 0o700);
}
interface Inventory {
  protocol: string;
  operations: { name: string; provenance: string; arguments: { id: string; spellings?: string[] }[]; sdk: string[] }[];
  backends: { kind: string; availability: string; operations: string[] }[];
}
const repo = argument('--public-repo');
const binary = argument('--binary');
const example = argument('--example');
const scratch = mkdtempSync(join(tmpdir(), 'native-capabilities-validation-'));
const root = join(scratch, 'copy');
const tracker = join(root, '.agents/pm');
let calls = 0;
let outputBytes = 0;
let maxBytes = 0;
/** Run native executables with no Git, Node, shell or inherited credentials. */
function run(executable: string, args: string[]): Buffer {
  const result = spawnSync(executable, args, { cwd: root, timeout: 30000, maxBuffer: 2 * 131072,
    env: { PATH: scratch, HOME: join(scratch, 'empty-home'), GIT_CEILING_DIRECTORIES: dirname(root) } });
  assert.equal(result.status, 0, `native invocation failed: ${args[0]}`);
  calls++; outputBytes += result.stdout.length; maxBytes = Math.max(maxBytes, result.stdout.length);
  return result.stdout;
}
try {
  mkdirSync(root);
  const commit = spawnSync('git', ['-C', repo, 'rev-parse', 'origin/main'], { encoding: 'utf8' });
  assert.equal(commit.status, 0); assert.match(commit.stdout.trim(), /^[a-f0-9]{40}$/);
  const archive = spawnSync('git', ['-C', repo, 'archive', '--format=tar', commit.stdout.trim(), '.agents/pm'], { maxBuffer: 32 * 1024 * 1024 });
  assert.equal(archive.status, 0);
  const extracted = spawnSync('tar', ['-xf', '-', '-C', root], { input: archive.stdout });
  assert.equal(extracted.status, 0);
  const archived = snapshot(tracker);
  const client = new PmClient({ cwd: root, pmRoot: tracker, noExtensions: true });
  const complete = certifyCompleteListResult(await client.listAllComplete({ includeBody: true }));
  const sdkBefore = complete.items;
  const frozen = snapshot(tracker);
  for (const [name, bytes] of archived) assert.deepEqual(frozen.get(name), bytes, "SDK preparation must preserve every archived byte");
  const preparationFiles = frozen.size - archived.size;
  permissions(tracker, true);
  const started = performance.now();
  const json = run(binary, ['capabilities', '--json']);
  const inventory: Inventory = JSON.parse(json.toString());
  assert.equal(inventory.protocol, 'pm-rust/capabilities/v1');
  assert.deepEqual(inventory.backends[1].operations, []);
  assert.equal(inventory.backends[1].availability, 'requires_adapter');
  const toon = run(binary, ['capabilities']);
  assert.ok(toon.includes(Buffer.from(inventory.protocol)));
  assert.deepEqual(JSON.parse(run(example, []).toString()), inventory, 'SDK example and CLI inventory must agree');
  let selectedMax = 0;
  for (const operation of inventory.operations) {
    const selected = run(binary, ['capabilities', '--operation', operation.name, '--json']);
    selectedMax = Math.max(selectedMax, selected.length);
    const response: Inventory = JSON.parse(selected.toString());
    assert.deepEqual(response.operations, [operation]);
    assert.deepEqual(response.backends[0].operations,
      ['capabilities', 'adapter-check', 'help'].includes(operation.name) ? [] : [operation.name]);
    assert.deepEqual(response.backends[1].operations, []);
  }
  // Real reads guided by the discovered native operation/flag surface.
  const names = new Set(inventory.operations.map(operation => operation.name));
  for (const name of ['get', 'list', 'history', 'search', 'deps', 'assurance', 'context', 'explain', 'read-session']) assert.ok(names.has(name));
  const nativeList = JSON.parse(run(binary, ['list', '--all', '--json', '--output-budget', 'unbounded', '--output-limit', 'unbounded', '--no-truncate']).toString()) as { items: { id: string }[] };
  assert.deepEqual(nativeList.items.map(item => item.id).sort(), complete.items.map(item => item.id).sort());
  for (const item of complete.items) {
    const native = JSON.parse(run(binary, ['get', item.id]).toString()) as Record<string, unknown>;
    for (const key of ['id', 'title', 'type', 'status', 'priority', 'description', 'body']) assert.deepEqual(native[key], item[key as keyof typeof item]);
  }
  for (const args of [['history', complete.items[0].id, '--json', '--limit', '3'], ['deps', complete.items[0].id, '--json'],
    ['search', 'native', '--limit', '3', '--json'], ['assurance', 'list', 'measurement', '--json'], ['context', '--limit', '3', '--json'],
    ['explain', 'list', '--json']]) run(binary, args);
  const elapsed = Math.round(performance.now() - started);
  assert.deepEqual(certifyCompleteListResult(await client.listAllComplete({ includeBody: true })).items, sdkBefore, 'Complete SDK read-back must preserve every native/foreign field and relationship');
  assert.equal(fingerprint(snapshot(tracker)), fingerprint(frozen));
  console.log(JSON.stringify({ protocol: inventory.protocol, public_commit: commit.stdout.trim(),
    source_items: complete.complete_list.item_count, source_files: frozen.size, sdk_preparation_files: preparationFiles,
    source_bytes: [...frozen.values()].reduce((sum, bytes) => sum + bytes.length, 0),
    operations: inventory.operations.length, oracle_slices: inventory.operations.filter(op => op.provenance === 'oracle_slice').length,
    native_only: inventory.operations.filter(op => op.provenance === 'native_only').length,
    argument_contracts: inventory.operations.reduce((sum, op) => sum + op.arguments.length, 0),
    native_calls: calls, json_bytes: json.length, toon_bytes: toon.length, max_selected_json_bytes: selectedMax,
    total_output_bytes: outputBytes, largest_output_bytes: maxBytes, elapsed_ms: elapsed,
    unchanged_source_sha256: fingerprint(frozen), no_git_or_node_on_native_path: true,
  }, null, 2));
} finally {
  permissions(root, false);
  rmSync(scratch, { recursive: true, force: true });
}
