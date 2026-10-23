import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { chmodSync, cpSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { performance } from 'node:perf_hooks';
import { PmClient, certifyCompleteListResult, serializeItemDocument } from '../tests/oracle/node_modules/@unbrained/pm-cli/dist/sdk/index.js';
import type { ItemDocument } from '../tests/oracle/node_modules/@unbrained/pm-cli/dist/sdk/index.js';

interface Receipt { protocol: string; source_items: number; source_bytes: number; admitted_memory_bytes: number; memory_ceiling_bytes: number; pages: number; complete: boolean; snapshot_sha256: string }
interface Frame { output: { items: { id: string }[]; next_cursor?: string; continuation_kind?: string }; receipt: Receipt }
const clock = '2026-10-21T12:00:00.000Z';
const ceiling = 512 * 1024 * 1024;
const pageItems = option('--page-items', '1000');
assert.ok(Number.isSafeInteger(Number(pageItems)) && Number(pageItems) > 0);

/** Read an explicit argument or a safe default. */
function option(flag: string, fallback: string): string {
  const index = process.argv.indexOf(flag);
  return index < 0 ? fallback : (process.argv[index + 1] ?? fallback);
}

/** Hash every source byte and relative filename, excluding disposable runtime caches. */
function fingerprint(root: string): string {
  const digest = createHash('sha256');
  /** Visit only the explicit copied tracker, never the host filesystem. */
  function visit(path: string, relative: string): void {
    for (const entry of readdirSync(path, { withFileTypes: true }).sort((a, b) => a.name.localeCompare(b.name))) {
      if (['runtime', 'search', 'locks'].includes(entry.name) && !relative) continue;
      const name = join(relative, entry.name);
      if (entry.isDirectory()) visit(join(path, entry.name), name);
      else if (entry.isFile()) digest.update(name).update(readFileSync(join(path, entry.name)));
      else throw new Error('Source copy must contain only regular files');
    }
  }
  visit(root, '');
  return digest.digest('hex');
}

/** Make the copied tracker read-only for native validation, then restore for cleanup. */
function permissions(root: string, readonly: boolean): void {
  for (const entry of readdirSync(root, { withFileTypes: true })) {
    const path = join(root, entry.name);
    if (entry.isDirectory()) permissions(path, readonly);
    else chmodSync(path, readonly ? 0o400 : 0o600);
  }
  chmodSync(root, readonly ? 0o500 : 0o700);
}

/** Certify complete oracle identities and compare every native page with the real SDK. */
async function measure(root: string, label: string, binary: string, scratch: string): Promise<Record<string, unknown>> {
  const tracker = join(root, '.agents/pm');
  const client = new PmClient({ cwd: root, pmRoot: tracker, noExtensions: true });
  const complete = certifyCompleteListResult(await client.listAllComplete());
  const before = fingerprint(tracker);
  permissions(tracker, true);
  const rssFile = join(scratch, `${label}-rss.txt`);
  console.error(JSON.stringify({ phase: 'native', label, source_items: complete.complete_list.item_count }));
  const started = performance.now();
  const result = spawnSync('/usr/bin/time', ['-f', '%M', '-o', rssFile, binary, '--workspace', root, 'read-session', '--memory-bytes', String(ceiling), '--page-items', pageItems, '--output-tokens', '100000', '--timestamp', clock, '--json'], {
    env: { ...process.env, GIT_CEILING_DIRECTORIES: dirname(root) }, maxBuffer: 64 * 1024 * 1024,
  });
  const elapsedMs = performance.now() - started;
  permissions(tracker, false);
  assert.equal(result.status, 0, result.stderr.toString());
  assert.equal(fingerprint(tracker), before, 'Native source bytes must remain unchanged');
  const frames = result.stdout.toString().trim().split('\n').map(line => JSON.parse(line) as Frame);
  const ids = frames.flatMap(frame => frame.output.items.map(row => row.id));
  assert.equal(new Set(ids).size, complete.complete_list.item_count);
  assert.deepEqual([...ids].sort(), complete.items.map(row => row.id).sort());
  console.error(JSON.stringify({ phase: 'oracle-pages', label, pages: frames.length }));
  let after: string | undefined;
  let sdkAfter: string | undefined;
  const oracleLauncher = fileURLToPath(new URL('../tests/oracle/launch.mts', import.meta.url));
  const clockPreload = pathToFileURL(fileURLToPath(new URL('./read-session-clock.mts', import.meta.url))).href;
  for (const frame of frames) {
    const expected = await client.list({ excludeTerminal: false, brief: true, limit: pageItems, outputBudget: '100000', ...(sdkAfter ? { after: sdkAfter } : {}) });
    assert.ok('items' in expected, 'SDK page must not be omitted');
    assert.deepEqual(frame.output.items.map(row => row.id), expected.items.map(row => row.id), 'Native rows must match the real SDK');
    sdkAfter = expected.next_cursor ?? undefined;
    const oracle = spawnSync(process.execPath, [oracleLauncher, '--path', tracker, 'list', '--all', '--brief', '--limit', pageItems, '--output-budget', '100000', '--json', ...(after ? ['--after', after] : [])], { cwd: root, env: { ...process.env, NODE_OPTIONS: `--import=${clockPreload}`, FIXED_CLOCK: clock }, maxBuffer: 64 * 1024 * 1024 });
    assert.equal(oracle.status, 0, 'Published CLI oracle must succeed');
    assert.equal(JSON.stringify(frame.output), JSON.stringify(JSON.parse(oracle.stdout.toString())), 'Compact native frame must preserve the published CLI JSON payload');
    after = frame.output.next_cursor;
  }
  const last = frames.at(-1);
  assert.ok(last?.receipt.complete);
  assert.equal(last.receipt.source_items, complete.complete_list.item_count);
  assert.ok(last.receipt.admitted_memory_bytes <= ceiling);
  const peakRssKiB = Number(readFileSync(rssFile, 'utf8').trim());
  assert.ok(peakRssKiB * 1024 <= ceiling, 'Measured child RSS exceeds declared memory ceiling');
  return { label, source_items: complete.complete_list.item_count, pages: frames.length, source_bytes: last.receipt.source_bytes, admitted_memory_bytes: last.receipt.admitted_memory_bytes, declared_ceiling_bytes: ceiling, peak_rss_kib: peakRssKiB, elapsed_ms: Math.round(elapsedMs), output_bytes: result.stdout.length, oracle_page_comparisons: frames.length, unchanged_source_sha256: before, snapshot_sha256: last.receipt.snapshot_sha256 };
}

/** Generate a genuine non-git tracker with the SDK, then validate a public copy. */
async function main(): Promise<void> {
  const binary = resolve(option('--binary', 'target/debug/pm-rust'));
  const source = option('--source', '');
  if (!source) throw new Error('--source requires an explicitly selected public tracker copy');
  const scratch = mkdtempSync(join(tmpdir(), 'pm-read-session-validation-'));
  process.env.GIT_CEILING_DIRECTORIES = scratch;
  try {
    const synthetic = join(scratch, 'synthetic');
    mkdirSync(synthetic);
    const client = new PmClient({ cwd: synthetic, pmRoot: join(synthetic, '.agents/pm'), noExtensions: true });
    await client.init('synthetic-', { defaults: true, agentGuidance: 'skip' });
    const count = Number(option('--items', '10000'));
    assert.ok(Number.isSafeInteger(count) && count > 0);
    const tasks = join(synthetic, '.agents/pm/tasks');
    mkdirSync(tasks, { recursive: true });
    for (let index = 0; index < count; index++) {
      const id = `synthetic-${String(index).padStart(5, '0')}`;
      const document: ItemDocument = { metadata: { id, title: `Synthetic item ${index}`, description: "", type: 'Task', status: index % 5 === 0 ? 'closed' : 'open', priority: 2, tags: [], created_at: clock, updated_at: clock }, body: '' };
      writeFileSync(join(tasks, `${id}.toon`), serializeItemDocument(document));
    }
    const copied = join(scratch, 'public-copy');
    mkdirSync(join(copied, '.agents'), { recursive: true });
    cpSync(resolve(source), join(copied, '.agents/pm'), { recursive: true, filter: path => !['runtime', 'search', 'locks'].includes(path.split('/').at(-1) ?? '') });
    const results = [await measure(synthetic, 'synthetic', binary, scratch), await measure(copied, 'public-copy', binary, scratch)];
    console.log(JSON.stringify({ contract: 'pm-rust/read-session-validation/v1', results }, null, 2));
  } finally { rmSync(scratch, { recursive: true, force: true }); }
}

await main();
