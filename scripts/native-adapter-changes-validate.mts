/** Validate incremental transport against canonical SDK events on a read-only public tracker copy. */
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { chmodSync, cpSync, mkdirSync, mkdtempSync, readdirSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { performance } from 'node:perf_hooks';
import { listMutationEvents, PmClient, certifyCompleteListResult } from '../tests/oracle/node_modules/@unbrained/pm-cli/dist/sdk/index.js';
import type { MutationEvent } from '../tests/oracle/node_modules/@unbrained/pm-cli/dist/sdk/index.js';
import { fingerprint, snapshot } from './crash-storage.mts';

/** Require an explicitly selected public source and built native binary. */
function argument(flag: string): string {
  const index = process.argv.indexOf(flag);
  const value = index < 0 ? undefined : process.argv[index + 1];
  assert.ok(value, `${flag} is required`);
  return resolve(value);
}

/** Change permissions exclusively inside this validation's owned scratch copy. */
function permissions(root: string, readonly: boolean): void {
  for (const entry of readdirSync(root, { withFileTypes: true })) {
    const path = join(root, entry.name);
    if (entry.isDirectory()) permissions(path, readonly);
    else { assert.ok(entry.isFile()); chmodSync(path, readonly ? 0o400 : 0o600); }
  }
  chmodSync(root, readonly ? 0o500 : 0o700);
}

/** Shared native output shape; canonical events come from the pinned SDK. */
interface Page {
  protocol: string;
  events: MutationEvent[];
  skipped: Array<{ item_id: string; version: number; reason: string }>;
  has_more: boolean;
  next_cursor: string;
}

const source = argument('--source');
const binary = argument('--binary');
const scratch = mkdtempSync(join(tmpdir(), 'native-changes-validation-'));
const root = join(scratch, 'copy');
const tracker = join(root, '.agents/pm');
let locked = false;
try {
  mkdirSync(join(root, '.agents'), { recursive: true });
  cpSync(source, tracker, { recursive: true, filter: path => !['runtime', 'search', 'locks', 'transactions', 'checkpoints'].includes(path.split('/').at(-1) ?? '') });
  const original = snapshot(tracker);
  const client = new PmClient({ cwd: root, pmRoot: tracker, noExtensions: true });
  const items = certifyCompleteListResult(await client.listAllComplete());
  const expected: MutationEvent[] = [];
  let since: string | undefined;
  for (;;) {
    const page = await listMutationEvents({ cwd: root, pmRoot: tracker, since, limit: 1000, cursorMode: 'batch' });
    expected.push(...page.events);
    if (!page.has_more) break;
    assert.ok(page.next_cursor);
    since = page.next_cursor;
  }
  const before = snapshot(tracker);
  const digest = fingerprint(before);
  locked = true; permissions(tracker, true);
  const start = performance.now();
  const actual: MutationEvent[] = [];
  let cursor: string | undefined;
  let pages = 0;
  let bytes = 0;
  let maxPageBytes = 0;
  let maxCursorBytes = 0;
  let toonChecks = 0;
  for (;;) {
    const args = ['--workspace', root, 'changes', '--limit', '20', '--json', ...(cursor ? ['--cursor', cursor] : [])];
    const output = spawnSync(binary, args, { cwd: root, timeout: 30_000, maxBuffer: 2_097_152,
      env: { PATH: scratch, HOME: join(scratch, 'empty-home'), GIT_CEILING_DIRECTORIES: dirname(root) } });
    assert.equal(output.status, 0, output.stderr.toString());
    assert.ok(output.stdout.length <= 65_536);
    const page: Page = JSON.parse(output.stdout.toString());
    assert.equal(page.protocol, 'pm-rust/changes/1');
    assert.equal(page.skipped.length, 0);
    assert.ok(page.events.length <= 20);
    actual.push(...page.events);
    bytes += output.stdout.length;
    maxPageBytes = Math.max(maxPageBytes, output.stdout.length);
    maxCursorBytes = Math.max(maxCursorBytes, Buffer.byteLength(page.next_cursor));
    if (pages === 0 || !page.has_more) {
      const toon = spawnSync(binary, args.filter(value => value !== '--json'), { cwd:root, timeout:30_000,
        env: { PATH:scratch, GIT_CEILING_DIRECTORIES:dirname(root) } });
      assert.equal(toon.status,0);
      assert.ok(toon.stdout.length <= 65_536);
      assert.match(toon.stdout.toString(),/pm-rust\/changes\/1/);
      toonChecks++;
    }
    cursor = page.next_cursor;
    assert.ok(++pages <= expected.length + 1);
    if (!page.has_more) break;
  }
  const idle = spawnSync(binary, ['--workspace', root, 'changes', '--json', '--cursor', cursor!], {
    cwd:root, timeout:30_000, env: { PATH:scratch, GIT_CEILING_DIRECTORIES:dirname(root) } });
  assert.equal(idle.status,0);
  const empty: Page = JSON.parse(idle.stdout.toString());
  assert.equal(empty.events.length,0);
  assert.equal(empty.next_cursor,cursor);
  const byIdentity = (left: MutationEvent, right: MutationEvent): number => left.item_id < right.item_id ? -1 : left.item_id > right.item_id ? 1 : left.version - right.version;
  assert.deepEqual(actual.sort(byIdentity),expected.sort(byIdentity));
  assert.equal(new Set(actual.map(event => `${event.item_id}:${event.version}`)).size,actual.length);
  assert.equal(fingerprint(snapshot(tracker)),digest);
  console.log(JSON.stringify({ source:'public-fleet-tracker-copy', source_items:items.complete_list.item_count,
    source_files:original.size, source_bytes:[...original.values()].reduce((sum,value)=>sum+value.length,0),
    events:actual.length, pages, toon_checks:toonChecks, idle_events:empty.events.length, output_bytes:bytes,
    max_page_bytes:maxPageBytes, max_cursor_bytes:maxCursorBytes, elapsed_ms:Math.round(performance.now()-start),
    unchanged_source_sha256:digest, sdk_event_projection_equal:true, no_git_or_node_on_native_path:true },null,2));
} finally {
  if (locked) permissions(root,false);
  rmSync(scratch,{recursive:true,force:true});
}
