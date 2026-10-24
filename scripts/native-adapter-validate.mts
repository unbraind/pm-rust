import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { chmodSync, cpSync, mkdirSync, mkdtempSync, readdirSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { performance } from 'node:perf_hooks';
import { PmClient, certifyCompleteListResult } from '../tests/oracle/node_modules/@unbrained/pm-cli/dist/sdk/index.js';
import { fingerprint, snapshot } from './crash-storage.mts';

/** Read a required explicit validation argument. */
function argument(flag: string): string {
  const index = process.argv.indexOf(flag);
  const value = index < 0 ? undefined : process.argv[index + 1];
  assert.ok(value, `${flag} is required`);
  return resolve(value);
}

/** Change permissions only on this program's exclusively owned scratch copy. */
function permissions(root: string, readonly: boolean): void {
  for (const entry of readdirSync(root, { withFileTypes: true })) {
    const path = join(root, entry.name);
    if (entry.isDirectory()) permissions(path, readonly);
    else { assert.ok(entry.isFile()); chmodSync(path, readonly ? 0o400 : 0o600); }
  }
  chmodSync(root, readonly ? 0o500 : 0o700);
}

interface Frame {
  abi: string;
  request_id: string;
  outcome: { status: 'success'; output: Record<string, unknown> };
}

const source = argument('--source');
const example = argument('--example');
const binary = argument('--binary');
const scratch = mkdtempSync(join(tmpdir(), 'native-adapter-validation-'));
const root = join(scratch, 'copy');
const tracker = join(root, '.agents/pm');
const manifest = { abi: 'pm-native-adapter/1', capabilities: ['get', 'list'], max_frame_bytes: 1048576 };
let calls = 0;
let wireBytes = 0;
let cliChecks = 0;
let maxCliBytes = 0;
let maxFrameBytes = 0;

/** Exchange with a real native adapter with no Git or Node executable available. */
function exchange(operation: Record<string, unknown>): Record<string, unknown> {
  const request = { abi: manifest.abi, request_id: `validation-${++calls}`, operation };
  const response = spawnSync(example, [root], {
    input: JSON.stringify(request), cwd: root, timeout: 30000, maxBuffer: 2 * manifest.max_frame_bytes,
    env: { PATH: scratch, HOME: join(scratch, 'empty-home'), GIT_CEILING_DIRECTORIES: dirname(root) },
  });
  assert.equal(response.status, 0, 'Native adapter exchange must succeed');
  wireBytes += response.stdout.length;
  maxFrameBytes = Math.max(maxFrameBytes, response.stdout.length);
  const reply: Frame = JSON.parse(response.stdout.toString());
  assert.equal(reply.request_id, request.request_id);
  assert.equal(reply.outcome.status, 'success');
  assert.equal(reply.abi, manifest.abi);
  // Check every transcript in both CLI encodings, without leaking result bodies.
  for (const json of [false, true]) {
    const checked = spawnSync(binary, ['adapter-check', ...(json ? ['--json'] : [])], {
      input: JSON.stringify({ manifest, request, reply }), cwd: root, timeout: 30000,
      env: { PATH: scratch, GIT_CEILING_DIRECTORIES: dirname(root) },
    });
    assert.equal(checked.status, 0, 'ABI transcript must validate');
    maxCliBytes = Math.max(maxCliBytes, checked.stdout.length);
    assert.ok(checked.stdout.length < 512);
    if (json) assert.equal((JSON.parse(checked.stdout.toString()) as { dispatched: boolean }).dispatched, false);
    cliChecks++;
  }
  return reply.outcome.output;
}

try {
  mkdirSync(join(root, '.agents'), { recursive: true });
  cpSync(source, tracker, { recursive: true, filter: path => !['runtime', 'search', 'locks', 'transactions', 'checkpoints'].includes(path.split('/').at(-1) ?? '') });
  const original = fingerprint(snapshot(tracker));
  const client = new PmClient({ cwd: root, pmRoot: tracker, noExtensions: true });
  const complete = certifyCompleteListResult(await client.listAllComplete());
  assert.equal(fingerprint(snapshot(tracker)), original, 'SDK reference must preserve authoritative bytes');
  const before = snapshot(tracker);
  permissions(tracker, true);
  const started = performance.now();
  const ids: string[] = [];
  let after: string | undefined;
  let outputCursor: string | undefined;
  let sdkAfter: string | undefined;
  let sdkOutputCursor: string | undefined;
  let pages = 0;
  do {
    const output = exchange({ action: 'list', limit: 10, ...(after ? { after } : {}), ...(outputCursor ? { output_cursor: outputCursor } : {}) });
    assert.ok(Array.isArray(output.items));
    ids.push(...output.items.map((row: { id: string }) => row.id));
    const expected = await client.list({ excludeTerminal: false, brief: true, limit: '10',
      outputBudget: '4000', ...(sdkAfter ? { after: sdkAfter } : {}), ...(sdkOutputCursor ? { outputCursor: sdkOutputCursor } : {}) });
    assert.ok('items' in expected);
    assert.deepEqual(output.items.map((row: { id: string }) => row.id), expected.items.map(row => row.id));
    assert.deepEqual(output.omission_receipt, ('omission_receipt' in expected ? expected.omission_receipt : undefined), 'Canonical omission receipt must survive, including its absence');
    // SDK compact defaults and native CLI defaults have separate cursor fingerprints.
    const sdkNext = expected.next_cursor ?? undefined;
    const sdkOutputContinuation = 'continuation_kind' in expected && expected.continuation_kind === 'output_cursor';
    sdkAfter = sdkOutputContinuation ? undefined : sdkNext;
    sdkOutputCursor = sdkOutputContinuation ? sdkNext : undefined;
    const next = typeof output.next_cursor === 'string' ? output.next_cursor : undefined;
    after = output.continuation_kind === 'output_cursor' ? undefined : next;
    outputCursor = output.continuation_kind === 'output_cursor' ? next : undefined;
    assert.ok(++pages <= complete.complete_list.item_count + 1, 'Continuation must make bounded progress');
  } while (after || outputCursor);
  assert.deepEqual([...ids].sort(), complete.items.map(item => item.id).sort());
  assert.equal(new Set(ids).size, ids.length);
  for (const id of ids) {
    const actual = exchange({ action: 'get', id });
    const expected = await client.get(id);
    assert.ok('item' in expected);
    assert.equal(actual.id, expected.item.id);
    assert.equal(actual.title, expected.item.title);
    assert.equal(actual.status, expected.item.status);
  }
  assert.equal(fingerprint(snapshot(tracker)), fingerprint(before), 'Read-only source must remain unchanged');
  console.log(JSON.stringify({ protocol: manifest.abi, source: 'public-fleet-tracker-copy',
    source_items: ids.length, source_files: before.size,
    source_bytes: [...before.values()].reduce((sum, value) => sum + value.length, 0),
    pages, adapter_calls: calls, cli_checks: cliChecks, output_bytes: wireBytes,
    max_frame_bytes: maxFrameBytes, max_cli_receipt_bytes: maxCliBytes,
    elapsed_ms: Math.round(performance.now() - started),
    unchanged_source_sha256: original, no_git_or_node_on_native_path: true,
  }, null, 2));
} finally {
  permissions(root, false);
  rmSync(scratch, { recursive: true, force: true });
}
