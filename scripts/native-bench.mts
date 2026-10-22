import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { loadavg, tmpdir } from 'node:os';
import { basename, dirname, join, resolve } from 'node:path';
import { performance } from 'node:perf_hooks';
import { fileURLToPath } from 'node:url';
import { encode } from '../tests/bench-oracle/node_modules/@unbrained/pm-cli/node_modules/@toon-format/toon/dist/index.mjs';
import { authoritative } from './context-real-helpers.mts';
import { measureRuntimes } from './bench-schedule.mts';
import { benchmarkOracleVersion } from './bench-oracle-version.mts';
import { createHistoryEntry } from '../tests/bench-oracle/node_modules/@unbrained/pm-cli/dist/core/history/history.js';
import type { ItemDocument } from '../tests/bench-oracle/node_modules/@unbrained/pm-cli/dist/types/index.js';

const repo = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const clock = '2026-10-21T12:00:00.000Z';

/** Read one explicit option without accepting missing values. */
function option(name: string, fallback: string): string {
  const index = process.argv.indexOf(name);
  if (index < 0) return fallback;
  const value = process.argv[index + 1];
  if (!value || value.startsWith('--')) throw new Error(`Missing ${name} value`);
  return value;
}

/** Reject ambiguous sample counts before creating scratch state. */
function positive(raw: string): number {
  const value = Number(raw);
  if (!Number.isSafeInteger(value) || value < 1) throw new Error('Counts must be positive integers');
  return value;
}

/** Return a nearest-rank percentile, retaining raw samples in the receipt. */
function percentile(values: number[], quantile: number): number {
  return [...values].sort((a, b) => a - b)[Math.ceil(values.length * quantile) - 1];
}

/** Fingerprint binaries and outputs without publishing their filesystem location. */
function hash(bytes: Buffer): string {
  return createHash('sha256').update(bytes).digest('hex');
}

type Sample = { ms: number; rss_kib: number; stdout_bytes: number; stderr_bytes: number; status: number | null };
type Execution = { sample: Sample; stdout: Buffer; stderr: Buffer; storage?: string[] };
type Runtime = { name: string; program: string; prefix: string[]; native: boolean };

/** Generate real canonical TOON with dependencies and nontrivial omitted payloads. */
function generate(root: string, count: number, settings: string): void {
  const tracker = join(root, '.agents/pm');
  for (const directory of ['tasks', 'history', 'runtime']) mkdirSync(join(tracker, directory), { recursive: true });
  writeFileSync(join(tracker, 'settings.json'), settings);
  for (let index = 0; index < count; index++) {
    const id = `bench-${String(index).padStart(5, '0')}`;
    const previous = `bench-${String(Math.max(0, index - 1)).padStart(5, '0')}`;
    const document = {
      id, title: `Generated ${index % 17 === 0 ? 'needle' : 'work'} item ${index}`,
      description: 'Synthetic description with agent context '.repeat(8),
      type: 'Task', status: index % 5 === 0 ? 'closed' : 'open', priority: (index % 5) as ItemDocument['metadata']['priority'],
      tags: ['benchmark', `group-${index % 10}`], created_at: clock, updated_at: clock,
      author: 'benchmark-agent',
      dependencies: index ? [{ id: previous, kind: 'related', created_at: clock }] : [],
      comments: Array.from({ length: 4 }, (_, row) => ({ created_at: clock, author: 'benchmark-agent', text: `Comment ${row}: ${'synthetic payload '.repeat(20)}` })),
      files: [{ path: 'src/example.rs', note: 'Synthetic file evidence', scope: 'project' }],
      tests: [{ command: 'true', scope: 'project' }], body: 'Synthetic body '.repeat(50),
    } satisfies ItemDocument['metadata'] & { body: string };
    writeFileSync(join(tracker, 'tasks', `${id}.toon`), `${encode(document)}\n`);
    const { body, ...metadata } = document;
    const comments = metadata.comments;
    // This is the SDK's legitimate pre-create empty document; persisted item
    // metadata is fully populated above. The API type describes stored items.
    let before: ItemDocument = { metadata: {} as ItemDocument['metadata'], body: '' };
    const history = [];
    for (let count = 0; count <= comments.length; count++) {
      const after: ItemDocument = { metadata: { ...metadata, comments: comments.slice(0, count) }, body };
      history.push(createHistoryEntry({ nowIso: clock, author: 'benchmark-agent', authorSource: 'asserted',
        op: count === 0 ? 'create' : 'comment_add', before, after }));
      before = after;
    }
    writeFileSync(join(tracker, 'history', `${id}.jsonl`), `${history.map(entry => JSON.stringify(entry)).join('\n')}\n`);
  }
}

/** Compare status and both byte streams, including refusals, without normalization. */
function equal(a: Execution, b: Execution): boolean {
  return a.sample.status === b.sample.status && a.stdout.equals(b.stdout) && a.stderr.equals(b.stderr);
}

/** Measure fresh OS processes on warm filesystem pages; reset writes outside timing. */
function main(): void {
  const samples = positive(option('--samples', '20'));
  const warmups = positive(option('--warmups', '2'));
  const sizes = option('--sizes', '1000,10000,50000').split(',').map(positive);
  const output = resolve(option('--output', 'target/native-bench.json'));
  const binary = resolve(option('--binary', 'target/release/pm-rust'));
  const baseline = option('--baseline', '');
  if (baseline && basename(baseline) !== basename(binary)) throw new Error('Baseline and native executable filenames must match: use separate directories');
  const profileValue = option('--profile', 'no');
  if (profileValue !== 'yes' && profileValue !== 'no') throw new Error('Profile must be yes or no');
  const profile = profileValue === 'yes';
  const supported = ['context', 'next', 'list-open', 'search', 'get', 'list', 'claim', 'comment', 'update'];
  const selected = option('--commands', supported.join(',')).split(',');
  if (selected.some(command => !supported.includes(command))) throw new Error('Commands must be nonempty supported names: ' + supported.join(','));
  const runtimes: Runtime[] = [
    ...(baseline ? [{ name: 'before', program: resolve(baseline), prefix: [], native: true }] : []),
    { name: 'native', program: binary, prefix: [], native: true },
    { name: 'oracle', program: process.execPath, prefix: [join(repo, 'scripts/bench-oracle.mts')], native: false },
  ];
  const fingerprints = runtimes.map(runtime => ({ runtime: runtime.name, sha256: hash(readFileSync(runtime.program)) }));
  if (baseline && fingerprints[0].sha256 === fingerprints[1].sha256) throw new Error('Paired CI builds produced the same executable; check target-directory isolation');
  const oracleVersion = benchmarkOracleVersion(repo);
  const scratch = mkdtempSync(join(tmpdir(), 'pm-rust-bench-'));
  const rows: Record<string, unknown>[] = [];
  const loads: number[][] = [];
  const profiles: Record<string, unknown>[] = [];
  /** Checkpoint completed cases, retaining a clearly incomplete receipt on failure. */
  function save(complete: boolean): void {
    mkdirSync(dirname(output), { recursive: true });
    writeFileSync(output, `${JSON.stringify({ schema: 1, complete, oracle: oracleVersion, clock, samples, warmups,
      baseline_commit: process.env.BENCH_BASE_SHA ?? null, candidate_commit: process.env.BENCH_HEAD_SHA ?? null,
      history_entries_per_item: 5, generated_target_history_verified: true,
      runtime_order_source: { kind: 'recorded-at-launch' },
      method: 'alternating fresh processes; warm filesystem; nearest-rank percentiles; GNU time maximum RSS; reset mutations outside timing',
      platform: process.platform, arch: process.arch, node: process.versions.node, fingerprints, load_average: loads, profiles, rows }, null, 2)}\n`);
  }
  const home = join(scratch, 'home');
  mkdirSync(home);
  const env = {
    PATH: process.env.PATH, HOME: home, USERPROFILE: home, XDG_CONFIG_HOME: join(home, 'config'),
    PM_AUTHOR: 'benchmark-agent', PM_SENTRY_DISABLED: '1', PM_TELEMETRY_SOURCE_CONTEXT: 'test',
    PM_CONTEXT_USAGE_DISABLED: '1', DO_NOT_TRACK: '1', FIXED_CLOCK: clock,
    GIT_CEILING_DIRECTORIES: scratch,
  };
  /** Run the oracle init outside every measurement window. */
  function initialize(root: string): string {
    mkdirSync(root, { recursive: true });
    const child = spawnSync(process.execPath, [join(repo, 'scripts/bench-oracle.mts'), 'init', 'bench-', '--defaults', '--agent-guidance', 'skip'], { cwd: root, env });
    if (child.error || child.status !== 0) throw new Error('Synthetic pm init failed');
    const tracker = join(root, '.agents/pm');
    // Only delete seed items in the newly created disposable tracker.
    for (const entry of readdirSync(tracker, { withFileTypes: true })) {
      if (entry.isDirectory()) rmSync(join(tracker, entry.name), { recursive: true, force: true });
    }
    const settings = JSON.parse(readFileSync(join(tracker, 'settings.json'), 'utf8'));
    settings.agent_identity.probes_enabled = false;
    settings.locks.wait_ms = 60000;
    return `${JSON.stringify(settings)}\n`;
  }
  /** Capture peak RSS in a separate file so stderr remains an exact oracle comparison. */
  function run(runtime: Runtime, root: string, args: string[]): Execution {
    const resource = join(scratch, 'resources.txt');
    const nativeArgs = runtime.native && ['context', 'list', 'claim', 'comment', 'update'].includes(args[0]) ? [...args, '--timestamp', clock] : args;
    const started = performance.now();
    const child = spawnSync('/usr/bin/time', ['-f', '%M', '-o', resource, runtime.program, ...runtime.prefix, ...nativeArgs], {
      cwd: root, env, maxBuffer: 64 * 1024 * 1024,
    });
    const ms = performance.now() - started;
    if (child.error || child.signal) throw new Error('Measured process could not complete');
    const rss = readFileSync(resource, 'utf8').trim().split('\n').at(-1) ?? '';
    if (!/^\d+$/.test(rss)) throw new Error('GNU time did not return peak RSS');
    return { sample: { ms, rss_kib: Number(rss), stdout_bytes: child.stdout.length, stderr_bytes: child.stderr.length, status: child.status }, stdout: child.stdout, stderr: child.stderr };
  }
  try {
    for (const size of sizes) {
      const root = join(scratch, `items-${size}`);
      const settings = initialize(root);
      generate(root, size, settings);
      const id = `bench-${String(size - 1).padStart(5, '0')}`;
      const tracker = join(root, '.agents/pm');
      const item = join(tracker, 'tasks', `${id}.toon`);
      const history = join(tracker, 'history', `${id}.jsonl`);
      const verification = spawnSync(process.execPath, [join(repo, 'scripts/bench-oracle.mts'), 'history', id, '--verify', '--strict-exit', '--json'], { cwd: root, env, maxBuffer: 64 * 1024 * 1024 });
      if (verification.error || verification.status !== 0) {
        writeFileSync(join(repo, 'target/bench-fixture.private.txt'), Buffer.concat([verification.stdout, verification.stderr]));
        throw new Error('Generated target history did not verify against the oracle');
      }
      const original = readFileSync(item);
      const originalHistory = readFileSync(history);
      const readSnapshot = authoritative(tracker);
      const cases = [
        ['context', '--limit', '10'], ['next'], ['list-open', '--limit', '20'],
        ['search', 'needle', '--limit', '10'], ['get', id], ['list', '--status', 'open', '--limit', '20'], ['claim', id, '--author', 'benchmark-agent'],
        ['comment', id, 'Measured synthetic comment', '--author', 'benchmark-agent'], ['update', id, '--title', 'Measured synthetic update', '--author', 'benchmark-agent'],
      ];
      for (const args of cases) {
        if (!selected.includes(args[0])) continue;
        const mutates = ['claim', 'comment', 'update'].includes(args[0]);
        if (args[0] === 'claim') {
          const after = authoritative(tracker);
          if (readSnapshot.size !== after.size || [...readSnapshot].some(([path, digest]) => after.get(path) !== digest)) throw new Error('Read commands changed authoritative storage');
        }
        /** Restore the exact initial write target and derived state outside timing. */
        const reset = (): void => {
          if (!mutates) return;
          writeFileSync(item, original);
          writeFileSync(history, originalHistory);
          rmSync(join(tracker, 'runtime'), { recursive: true, force: true });
        };
        const references = new Map<string, Execution>();
        const results = measureRuntimes(runtimes, warmups, samples, (runtime, measured) => {
          reset();
          const result = run(runtime, root, args);
          if (mutates) result.storage = [hash(readFileSync(item)), existsSync(history) ? hash(readFileSync(history)) : 'absent'];
          // The oracle discloses cold validation versus warm cache reads in
          // its completeness receipt. Compare measured warm runs only.
          if (!measured) return result.sample;
          if (!references.has(runtime.name)) references.set(runtime.name, result);
          const reference = references.get(runtime.name);
          if (reference && (!equal(reference, result) || JSON.stringify(reference.storage) !== JSON.stringify(result.storage))) {
            writeFileSync(join(repo, 'target/bench-unstable.private.json'), JSON.stringify({ runtime: runtime.name, command: args[0], expected: { stdout: reference.stdout.toString(), stderr: reference.stderr.toString(), storage: reference.storage }, actual: { stdout: result.stdout.toString(), stderr: result.stderr.toString(), storage: result.storage } }));
            throw new Error(`Unstable output or storage: ${runtime.name} ${args[0]}`);
          }
          return result.sample;
        });
        const expected = references.get('oracle');
        const before = references.get('before');
        if (!expected) throw new Error('Missing oracle execution');
        if (expected.sample.status !== 0) throw new Error(`Oracle ${args[0]} failed; refusal timings cannot represent successful operations`);
        for (const runtime of runtimes) {
          const measured = results.get(runtime.name) ?? [];
          const actual = references.get(runtime.name);
          if (!actual) throw new Error('Missing native execution');
          if (before && runtime.name === 'native' && (!equal(before, actual) || JSON.stringify(before.storage) !== JSON.stringify(actual.storage))) throw new Error(`Optimization changed observable output or storage: ${args[0]}`);
          rows.push({ items: size, command: args[0], runtime: runtime.name,
            p50_ms: percentile(measured.map(sample => sample.ms), 0.5),
            p95_ms: percentile(measured.map(sample => sample.ms), 0.95),
            peak_rss_kib: Math.max(...measured.map(sample => sample.rss_kib)),
            stdout_bytes: actual.stdout.length, stderr_bytes: actual.stderr.length,
            status: actual.sample.status, oracle_byte_identical: equal(expected, actual),
            oracle_storage_identical: mutates ? JSON.stringify(expected.storage) === JSON.stringify(actual.storage) : null,
            stdout_sha256: hash(actual.stdout), stderr_sha256: hash(actual.stderr), samples: measured });
        }
        if (profile) {
          for (const runtime of runtimes.filter(runtime => runtime.native)) {
            reset();
            const trace = join(scratch, 'syscalls.txt');
            const nativeArgs = ['context', 'list', 'claim', 'comment', 'update'].includes(args[0]) ? [...args, '--timestamp', clock] : args;
            const child = spawnSync('strace', ['-c', '-f', '-e', 'trace=openat,read,newfstatat,statx,getdents64,write,fsync,fdatasync', '-o', trace, runtime.program, ...nativeArgs], { cwd: root, env });
            if (child.error || child.signal || !existsSync(trace)) throw new Error('Requested syscall profiling is unavailable');
            profiles.push({ items: size, command: args[0], runtime: runtime.name, status: child.status, summary: readFileSync(trace, 'utf8') });
          }
        }
        loads.push(loadavg());
        save(false);
        console.log(JSON.stringify({ items: size, command: args[0], completed: true }));
      }
    }
    for (const runtime of runtimes) {
      if (hash(readFileSync(runtime.program)) !== fingerprints.find(row => row.runtime === runtime.name)?.sha256) throw new Error('Measured executable changed');
    }
    save(true);
  } finally {
    rmSync(scratch, { recursive: true, force: true });
  }
}

main();
