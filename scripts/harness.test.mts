import assert from 'node:assert/strict';
import { spawn, spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { chmodSync, copyFileSync, existsSync, readFileSync, mkdtempSync, mkdirSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { basename, join } from 'node:path';
import { test } from 'node:test';
import { setTimeout } from 'node:timers/promises';
import { fileURLToPath } from 'node:url';
import { argument, authoritative, relationshipRoots } from './context-real-helpers.mts';
import { measure } from './latency-measure.mts';
import { measureRuntimes } from './bench-schedule.mts';
import { benchmarkOracleVersion } from './bench-oracle-version.mts';

for (const signal of ['SIGINT', 'SIGTERM'] as const) {
  for (const delivery of ['process group', 'parent only', 'cargo only'] as const) {
    test(`revert proof restores both source files on ${signal} (${delivery})`, { skip: process.platform === 'win32', timeout: 15000 }, async () => {
      // Separate delivery targets exercise either callback arriving first, plus terminal process groups.
      const root = mkdtempSync(join(tmpdir(), 'pm-rust-interrupt-'));
      const originals = new Map(['ownership', 'mutation'].map(name => [name,
        readFileSync(new URL(`../src/${name}.rs`, import.meta.url), 'utf8')]));
      mkdirSync(join(root, 'src'));
      mkdirSync(join(root, 'scripts'));
      mkdirSync(join(root, 'bin'));
      for (const [name, bytes] of originals) writeFileSync(join(root, `src/${name}.rs`), bytes);
      const script = join(root, 'scripts/perf-revert-proof.mts');
      writeFileSync(script, readFileSync(new URL('./perf-revert-proof.mts', import.meta.url)));
      const cargo = join(root, 'bin/cargo');
      const cargoProgram = join(root, 'cargo.mjs');
      writeFileSync(cargoProgram, "import { renameSync, writeFileSync } from 'node:fs';\nwriteFileSync('ready.stage', String(process.pid));\nrenameSync('ready.stage', 'ready');\nsetInterval(() => {}, 1000);\n");
      writeFileSync(cargo, '#!/bin/sh\nexec "$PM_PROOF_NODE" "$PM_PROOF_CHILD"\n');
      chmodSync(cargo, 0o755);
      const child = spawn(process.execPath, [script], {
        cwd: root, detached: true, stdio: 'ignore',
        env: { ...process.env, PATH: `${join(root, 'bin')}:${process.env.PATH ?? ''}`,
          PM_PROOF_NODE: process.execPath, PM_PROOF_CHILD: cargoProgram },
      });
      const completed = new Promise<{ code: number | null; signal: NodeJS.Signals | null }>((resolve, reject) => {
        child.once('error', reject);
        child.once('exit', (code, signal) => resolve({ code, signal }));
      });
      try {
        for (let attempt = 0; !existsSync(join(root, 'ready')) && attempt < 500; attempt++) await setTimeout(10);
        assert.ok(existsSync(join(root, 'ready')), 'cargo never reached the rewritten-source phase');
        const rawPid = readFileSync(join(root, 'ready'), 'utf8');
        assert.match(rawPid, /^[1-9][0-9]*$/);
        const cargoPid = Number(rawPid);
        assert.ok(Number.isSafeInteger(cargoPid) && cargoPid > 1, 'cargo PID must identify an ordinary child process');
        for (const [name, bytes] of originals) assert.notEqual(readFileSync(join(root, `src/${name}.rs`), 'utf8'), bytes);
        assert.ok(child.pid && child.pid > 1);
        const target = delivery === 'process group' ? -child.pid : delivery === 'parent only' ? child.pid :
          cargoPid;
        process.kill(target, signal);
        const result = await completed;
        for (const [name, bytes] of originals) assert.equal(readFileSync(join(root, `src/${name}.rs`), 'utf8'), bytes);
        assert.deepEqual(result, { code: signal === 'SIGINT' ? 130 : 143, signal: null });
      } finally {
        if (child.pid) {
          try { process.kill(-child.pid, 'SIGKILL'); } catch { /* The process group already exited. */ }
        }
        await completed;
        rmSync(root, { recursive: true, force: true });
      }
    });
  }
}

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

test('hot-path paired benchmarks reject identical executables without CI metadata', () => {
  const root = mkdtempSync(join(tmpdir(), 'pm-rust-identical-bench-'));
  try {
    mkdirSync(join(root, 'scripts'));
    mkdirSync(join(root, 'before'));
    const packages = join(root, 'tests/bench-oracle/node_modules/@unbrained');
    mkdirSync(packages, { recursive: true });
    // Use the real locked oracle already required by the release gate, with the
    // benchmark's exact source and imports relocated into a disposable non-git root.
    symlinkSync(fileURLToPath(new URL('../tests/oracle/node_modules/@unbrained/pm-cli', import.meta.url)),
      join(packages, 'pm-cli'), 'junction');
    for (const name of ['native-bench.mts', 'bench-oracle.mts', 'bench-oracle-version.mts', 'bench-schedule.mts', 'context-real-helpers.mts']) {
      copyFileSync(new URL(name, import.meta.url), join(root, 'scripts', name));
    }
    // A parity-oracle pin bump can precede the benchmark-oracle bump. The
    // executable-identity refusal must remain independent of either pin.
    writeFileSync(join(root, 'tests/bench-oracle/package.json'), JSON.stringify({ dependencies: { '@unbrained/pm-cli': 'future-pin' } }));
    const baseline = join(root, 'before', basename(process.execPath));
    copyFileSync(process.execPath, baseline);
    const output = join(root, 'receipt.json');
    for (const metadata of [undefined, 'synthetic-commit']) {
      const env = { ...process.env };
      delete env.BENCH_BASE_SHA;
      if (metadata !== undefined) env.BENCH_BASE_SHA = metadata;
      const result = spawnSync(process.execPath, [join(root, 'scripts/native-bench.mts'),
        '--baseline', baseline, '--binary', process.execPath, '--output', output,
        '--sizes', '1', '--samples', '1', '--warmups', '1', '--commands', 'get'],
      { cwd: root, env, encoding: 'utf8', timeout: 15000 });
      assert.equal(result.error, undefined);
      assert.equal(result.status, 1);
      assert.match(result.stderr, /Paired CI builds produced the same executable; check target-directory isolation/);
      assert.equal(result.stdout, '');
      assert.equal(existsSync(output), false, 'self-comparison must not publish a measurement receipt');
      assert.equal(existsSync(join(root, '.git')), false);
    }
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('hot-path benchmark oracle version follows its manifest instead of a source literal', () => {
  const root = mkdtempSync(join(tmpdir(), 'pm-rust-oracle-pin-'));
  try {
    const packageRoot = join(root, 'tests/bench-oracle/node_modules/@unbrained/pm-cli');
    mkdirSync(packageRoot, { recursive: true });
    // Advance real package metadata to model an independent dependency-pin update.
    // The contract reads metadata only and never replaces or mocks SDK code.
    const metadata: { version: string } = JSON.parse(readFileSync(
      new URL('../tests/oracle/node_modules/@unbrained/pm-cli/package.json', import.meta.url), 'utf8'));
    metadata.version = '2099.1.1';
    writeFileSync(join(packageRoot, 'package.json'), JSON.stringify(metadata));
    const manifest = join(root, 'tests/bench-oracle/package.json');
    writeFileSync(manifest, JSON.stringify({ dependencies: { '@unbrained/pm-cli': metadata.version } }));
    assert.equal(benchmarkOracleVersion(root), metadata.version);
    writeFileSync(manifest, JSON.stringify({ dependencies: { '@unbrained/pm-cli': 'different-pin' } }));
    assert.throws(() => benchmarkOracleVersion(root), /Benchmark oracle does not match its declared dependency pin/);
    assert.equal(existsSync(join(root, '.git')), false);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('hot-path benchmark command selections reject unsupported or empty names before fixtures', () => {
  const root = mkdtempSync(join(tmpdir(), 'pm-rust-command-bench-'));
  try {
    mkdirSync(join(root, 'scripts'));
    const packages = join(root, 'tests/bench-oracle/node_modules/@unbrained');
    mkdirSync(packages, { recursive: true });
    symlinkSync(fileURLToPath(new URL('../tests/oracle/node_modules/@unbrained/pm-cli', import.meta.url)),
      join(packages, 'pm-cli'), 'junction');
    for (const name of ['native-bench.mts', 'bench-oracle.mts', 'bench-oracle-version.mts', 'bench-schedule.mts', 'context-real-helpers.mts']) {
      copyFileSync(new URL(name, import.meta.url), join(root, 'scripts', name));
    }
    const output = join(root, 'receipt.json');
    for (const commands of ['unknown', 'get,unknown', ',', 'get,']) {
      const result = spawnSync(process.execPath, [join(root, 'scripts/native-bench.mts'),
        '--binary', process.execPath, '--output', output, '--sizes', '1',
        '--samples', '1', '--warmups', '1', '--commands', commands],
      { cwd: root, encoding: 'utf8', timeout: 15000 });
      assert.equal(result.error, undefined);
      assert.equal(result.status, 1);
      assert.match(result.stderr, /Commands must be nonempty supported names/);
      assert.equal(result.stdout, '');
      assert.equal(existsSync(output), false, 'invalid selections must not publish a receipt');
      assert.equal(existsSync(join(root, '.git')), false);
    }
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('hot-path reproduction identity flag agrees with the retained binary fingerprints', () => {
  const receipt: { identical_native_executables: boolean; fingerprints: { runtime: string; sha256: string }[] } =
    JSON.parse(readFileSync(new URL('../docs/benchmarks/native-hot-path-reproduction-2026-10-21.json', import.meta.url), 'utf8'));
  const before = receipt.fingerprints.find(row => row.runtime === 'before');
  const native = receipt.fingerprints.find(row => row.runtime === 'native');
  assert.ok(before && native);
  assert.equal(receipt.identical_native_executables, before.sha256 === native.sha256);
});

test('hot-path reports reject empty measurements while rendering the retained real receipt', () => {
  const root = mkdtempSync(join(tmpdir(), 'pm-rust-report-'));
  try {
    const input = join(root, 'receipt.json');
    const output = join(root, 'report.md');
    const historical: { rows: { runtime: string }[] } = JSON.parse(readFileSync(
      new URL('../docs/benchmarks/native-hot-paths-2026-10-21.json', import.meta.url), 'utf8'));
    for (const rows of [[], historical.rows.filter(row => row.runtime === 'oracle')]) {
      writeFileSync(input, JSON.stringify({ complete: true, samples: 1, warmups: 1, rows }));
      const result = spawnSync(process.execPath, [fileURLToPath(new URL('./bench-report.mts', import.meta.url)), input, output],
        { cwd: root, encoding: 'utf8', timeout: 15000 });
      assert.equal(result.error, undefined);
      assert.equal(result.status, 1);
      assert.match(result.stderr, /Cannot publish a benchmark receipt without native measurements/);
      assert.equal(existsSync(output), false);
    }
    copyFileSync(new URL('../docs/benchmarks/native-hot-paths-2026-10-21.json', import.meta.url), input);
    const result = spawnSync(process.execPath, [fileURLToPath(new URL('./bench-report.mts', import.meta.url)), input, output],
      { cwd: root, encoding: 'utf8', timeout: 15000 });
    assert.equal(result.error, undefined);
    assert.equal(result.status, 0, result.stderr);
    assert.equal(readFileSync(output, 'utf8'), readFileSync(
      new URL('../docs/benchmarks/native-hot-path-tables-2026-10-21.md', import.meta.url), 'utf8').replace(/\r\n/g, '\n'));
    assert.equal(existsSync(join(root, '.git')), false);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

// These small contracts run inside the existing cross-platform Rust harness gate.
test('hot-path samples retain the executed order after warmups with two or three runtimes', () => {
  for (const names of [['native', 'oracle'], ['before', 'native', 'oracle']]) {
    for (const warmups of [1, 2]) {
      const launches: { name: string; measured: boolean }[] = [];
      const runtimes = names.map(name => ({ name }));
      const results = measureRuntimes(runtimes, warmups, 3, (runtime, measured) => {
        launches.push({ name: runtime.name, measured });
        return { ms: launches.length };
      });
      assert.deepEqual(runtimes.map(runtime => runtime.name), names);
      const expected = Array.from({ length: warmups + 3 }, (_, index) => {
        const round = index - warmups;
        return (round % 2 === 0 ? names : [...names].reverse())
          .map(name => ({ name, measured: round >= 0 }));
      }).flat();
      assert.deepEqual(launches, expected);
      for (const name of names) {
        const samples = results.get(name);
        assert.equal(samples?.length, 3);
        samples?.forEach((sample, round) => {
          const order = round % 2 === 0 ? names : [...names].reverse();
          assert.equal(sample.round, round);
          assert.deepEqual(sample.runtime_order, order);
          const launch = (warmups + round) * names.length + order.indexOf(name);
          assert.equal(sample.ms, launch + 1);
          assert.deepEqual(launches[launch], { name, measured: true });
        });
      }
    }
  }
});

test('hot-path historical order is labeled reconstruction and preserves the original receipt bytes', () => {
  const receipt: { candidate_commit: string; samples: number; fingerprints: { runtime: string }[];
    rows: { samples: { round?: number; runtime_order?: string[] }[] }[];
    runtime_order_source?: { kind: string; candidate_commit: string; original_receipt_sha256: string } } = JSON.parse(readFileSync(new URL('../docs/benchmarks/native-hot-paths-2026-10-21.json', import.meta.url), 'utf8'));
  const source = receipt.runtime_order_source;
  assert.ok(source);
  assert.equal(source.kind, 'reconstructed-from-scheduler');
  assert.equal(source.candidate_commit, receipt.candidate_commit);
  const names: string[] = receipt.fingerprints.map((entry: { runtime: string }) => entry.runtime);
  for (const row of receipt.rows) {
    assert.equal(row.samples.length, receipt.samples);
    row.samples.forEach((sample: { round?: number; runtime_order?: string[] }, round: number) => {
      assert.equal(sample.round, round);
      assert.deepEqual(sample.runtime_order, round % 2 === 0 ? names : [...names].reverse());
      delete sample.round;
      delete sample.runtime_order;
    });
  }
  delete receipt.runtime_order_source;
  assert.equal(createHash('sha256').update(`${JSON.stringify(receipt, null, 2)}\n`).digest('hex'), source.original_receipt_sha256);
  const docs = readFileSync(new URL('../docs/NATIVE_HOT_PATHS.md', import.meta.url), 'utf8');
  assert.match(docs, /`round` and `runtime_order`/);
  assert.match(docs, /not independent observation of the historical launch order/);
});

test('hot-path runner wording describes the actual GitHub-hosted runner', () => {
  const docs = readFileSync(new URL('../docs/NATIVE_HOT_PATHS.md', import.meta.url), 'utf8');
  const workflow = readFileSync(new URL('../.github/workflows/native-bench.yml', import.meta.url), 'utf8');
  assert.match(workflow, /runs-on: ubuntu-latest/);
  assert.match(docs, /ephemeral GitHub-hosted `ubuntu-latest` runner/);
  assert.doesNotMatch(docs, /\bdedicated\b/i);
  assert.match(docs, /GitHub-hosted runner noise is still possible/);
});

test('hot-path documented latency sample total matches every retained window', () => {
  const receipt: { jobs: { latency?: { runs: number; windows_ms: number[][] }[] }[] } = JSON.parse(readFileSync(new URL('../docs/benchmarks/native-hot-path-gates-2026-10-21.json', import.meta.url), 'utf8'));
  const counts = receipt.jobs.flatMap(job => job.latency ? [job.latency.reduce((sum, latency) => {
      const retained = latency.windows_ms.flat().length;
      assert.equal(retained, latency.runs);
      return sum + retained;
    }, 0)] : []);
  assert.deepEqual(counts, [270, 270, 270]);
  const total = counts.reduce((sum, count) => sum + count, 0);
  const docs = readFileSync(new URL('../docs/NATIVE_HOT_PATHS.md', import.meta.url), 'utf8');
  assert.match(docs, new RegExp(`all ${total} existing latency samples \\(270 per platform\\)`));
});

test('hot-path README has one non-git section with writer and context boundaries', () => {
  const readme = readFileSync(new URL('../README.md', import.meta.url), 'utf8');
  assert.equal(readme.match(/^## Without git$/gm)?.length, 1);
  const section = readme.split('## Without git\n')[1].split('\n## ')[0];
  assert.match(section, /tests 50 writers/);
  assert.match(section, /without\na Git executable/);
  assert.match(section, /`context` refuses trackers with 10,000 or more items/);
  assert.match(section, /\]\(docs\/NATIVE_HOT_PATHS.md\)/);
  assert.match(section, /GitHub synchronization and Git history tooling require a Git\nrepository/);
});
