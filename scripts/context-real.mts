import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { readFileSync, readdirSync, rmSync, writeFileSync, existsSync } from 'node:fs';
import { resolve, join } from 'node:path';
import { performance } from 'node:perf_hooks';

/** Read a required explicit harness argument. */
function argument(name: string): string {
  const value = process.argv[process.argv.indexOf(name) + 1];
  if (!process.argv.includes(name) || !value) throw new Error(`Required: ${name}`);
  return value;
}

const root = resolve(argument('--copy'));
if (!existsSync(join(root, '.pm-rust-read-copy'))) throw new Error('Only a marked scratch tracker copy is accepted');
const driver = resolve(argument('--driver'));
const binary = resolve(argument('--binary'));
const binaryHash = createHash('sha256').update(readFileSync(binary)).digest('hex');
const clock = '2026-10-19T12:00:00.000Z';
const home = join(root, 'synthetic-home');
const environment = {
  PATH: process.env.PATH, HOME: home, USERPROFILE: home,
  PM_AUTHOR: 'conformance-agent', PM_CONTEXT_USAGE_DISABLED: '1',
  PM_SENTRY_DISABLED: '1', PM_TELEMETRY_SOURCE_CONTEXT: 'test',
  DO_NOT_TRACK: '1', FIXED_CLOCK: clock,
};

/** Hash authoritative item and append-only history files without retaining their contents. */
function authoritative(directory: string, prefix = ''): Map<string, string> {
  const hashes = new Map<string, string>();
  for (const entry of readdirSync(directory, { withFileTypes: true })) {
    const relative = join(prefix, entry.name);
    if (['runtime', 'locks', 'transactions', 'checkpoints', 'search'].includes(entry.name)) continue;
    if (entry.isDirectory()) {
      for (const [key, value] of authoritative(join(directory, entry.name), relative)) hashes.set(key, value);
    } else if (entry.name.endsWith('.toon') || entry.name.endsWith('.jsonl')) {
      hashes.set(relative, createHash('sha256').update(readFileSync(join(directory, entry.name))).digest('hex'));
    }
  }
  return hashes;
}

/** Run one fresh process; private output remains exclusively in the marked copy. */
function run(runtime: 'native' | 'oracle', args: string[]): { bytes: Buffer; ms: number; status: number | null } {
  const started = performance.now();
  const result = spawnSync(runtime === 'native' ? binary : process.execPath,
    runtime === 'native' ? [...args, ...(args[0] === 'context' ? ['--timestamp', clock] : [])] : [driver, ...args],
    { cwd: root, env: environment, maxBuffer: 64 * 1024 * 1024 });
  const ms = performance.now() - started;
  if (result.error) throw new Error(`${runtime} process failed`);
  return { bytes: Buffer.concat([result.stdout, Buffer.from('\0'), result.stderr]), ms, status: result.status };
}

const tracker = join(root, '.agents/pm');
const before = authoritative(tracker);
let comparisons = 0;
rmSync(join(tracker, 'runtime'), { recursive: true, force: true });
// Select representative relationship roots from the marked copy without publishing IDs.
const candidates = [...before.keys()].filter(key => key.endsWith('.toon')).sort();
const roots = new Set<string>();
for (const pattern of [/^parent: (?!null|\s*$)/m, /^deps\[[1-9]/m, /^blocked_by: (?!null|\s*$)/m]) {
  const selected = candidates.find(key => pattern.test(readFileSync(join(tracker, key), 'utf8')));
  if (selected) roots.add(selected.split(/[\\/]/).at(-1)!.slice(0, -5));
}
if (roots.size === 0) throw new Error('The real-copy packet matrix needs relationship-bearing roots');
let packetComparisons = 0;
for (const id of roots) {
  for (const controls of [[], ['--summary'], ['--node-limit', '1'], ['--direction', 'incoming'], ['--token-budget', '800']]) {
    for (const format of [[], ['--json']]) {
      const args = ['deps', id, '--format', 'context', ...controls, ...format];
      const oracle = run('oracle', args);
      const native = run('native', args);
      if (oracle.status !== 0 || native.status !== 0 || !oracle.bytes.equals(native.bytes)) {
        writeFileSync(join(root, 'oracle-output.private'), oracle.bytes);
        writeFileSync(join(root, 'native-output.private'), native.bytes);
        throw new Error(`Real-copy packet parity failed at comparison ${packetComparisons + 1}`);
      }
      comparisons += 1;
      packetComparisons += 1;
    }
  }
  let cursor: string | undefined;
  for (let page = 0; page < 3; page += 1) {
    const args = ['deps', id, '--format', 'context', '--node-limit', '1', '--json', ...(cursor ? ['--cursor', cursor] : [])];
    const oracle = run('oracle', args);
    const native = run('native', args);
    if (oracle.status !== 0 || native.status !== 0 || !oracle.bytes.equals(native.bytes)) throw new Error('Real-copy packet continuation parity failed');
    comparisons += 1;
    packetComparisons += 1;
    const parsed: { context: { meta: { nextCursor?: string } } } = JSON.parse(oracle.bytes.toString().split('\0')[0]);
    cursor = parsed.context.meta.nextCursor;
    if (!cursor) break;
  }
}

console.log(JSON.stringify({ phase: 'packet-parity', comparisons, packet_comparisons: packetComparisons }));

const cases = [
  [], ['--limit', '0'], ['--limit', '1'], ['--limit', '10'],
  ['--limit', '3', '--token-budget', '100'], ['--token-budget', '256'],
];
for (const depth of [[], ['--depth', 'deep']]) {
  for (const controls of cases) {
    for (const format of [[], ['--json']]) {
      const args = ['context', ...depth, ...controls, ...format];
      const oracle = run('oracle', args);
      const native = run('native', args);
      comparisons += 1;
      if (oracle.status !== native.status || !oracle.bytes.equals(native.bytes)) {
        writeFileSync(join(root, 'oracle-output.private'), oracle.bytes);
        writeFileSync(join(root, 'native-output.private'), native.bytes);
        throw new Error(`Parity failed: ${args.join(' ')} (private diagnostics retained in copy)`);
      }
    }
  }
  let cursor: string | undefined;
  for (let page = 0; page < 3; page += 1) {
    const args = ['context', ...depth, '--limit', '3', '--json', ...(cursor ? ['--after', cursor] : [])];
    const oracle = run('oracle', args);
    const native = run('native', args);
    comparisons += 1;
    if (oracle.status !== native.status || !oracle.bytes.equals(native.bytes)) throw new Error(`Continuation parity failed at page ${page + 1}`);
    const parsed: { next_cursor?: string } = JSON.parse(oracle.bytes.toString().split('\0')[0]);
    cursor = parsed.next_cursor;
    if (!cursor) break;
  }
}

console.log(JSON.stringify({ phase: 'context-parity', comparisons }));

/** Return the nearest-rank latency percentile. */
function percentile(values: number[], quantile: number): number {
  const sorted = [...values].sort((a, b) => a - b);
  return Math.round(sorted[Math.ceil(sorted.length * quantile) - 1] * 10) / 10;
}

const sampleCount = Number(argument('--samples'));
if (!Number.isInteger(sampleCount) || sampleCount < 1) throw new Error('Samples must be positive');
const samples: { command: string; runtime: string; round: number; ms: number }[] = [];
const timings = [];
for (const depth of [[], ['--depth', 'deep']]) {
  const args = ['context', ...depth, '--limit', '10'];
  rmSync(join(tracker, 'runtime'), { recursive: true, force: true });
  run('oracle', args);
  run('native', args);
  for (let round = 0; round < sampleCount; round += 1) {
    const order: ('native' | 'oracle')[] = round % 2 === 0 ? ['native', 'oracle'] : ['oracle', 'native'];
    for (const runtime of order) {
      const result = run(runtime, args);
      if (result.status !== 0) throw new Error(`Benchmark failed: ${runtime}`);
      samples.push({ command: args.join(' '), runtime, round, ms: result.ms });
    }
    if ((round + 1) % 5 === 0) console.log(JSON.stringify({ phase: 'timing', command: args.join(' '), rounds: round + 1 }));
  }
  for (const runtime of ['native', 'oracle']) {
    const values = samples.filter(row => row.command === args.join(' ') && row.runtime === runtime).map(row => row.ms);
    timings.push({ command: args.join(' '), runtime, samples: values.length, p50_ms: percentile(values, 0.5), p95_ms: percentile(values, 0.95) });
  }
}
const after = authoritative(tracker);
if (createHash('sha256').update(readFileSync(binary)).digest('hex') !== binaryHash) throw new Error('Native binary changed during the run');
if (before.size !== after.size || [...before].some(([key, hash]) => after.get(key) !== hash)) throw new Error('Authoritative copy changed');
const receipt = {
  oracle: '2026.10.14', clock, comparisons, context_comparisons: comparisons - packetComparisons, packet_comparisons: packetComparisons, packet_roots: roots.size,
  item_count: [...before.keys()].filter(key => key.endsWith('.toon')).length,
  authoritative_files: before.size, authoritative_unchanged: true,
  binary_sha256: binaryHash,
  timings, samples,
};
writeFileSync(argument('--output'), `${JSON.stringify(receipt, null, 2)}\n`);
console.log(JSON.stringify({ comparisons, authoritative_unchanged: true, timings }));
