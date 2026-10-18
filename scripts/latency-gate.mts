import { spawnSync } from 'node:child_process';
import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { performance } from 'node:perf_hooks';
import { fileURLToPath } from 'node:url';

/** Build 3,000 deterministic items with payloads omitted from brief projections. */
function generate(root: string): void {
  const tracker = join(root, '.agents', 'pm');
  const items = join(tracker, 'custom-types', 'nested');
  mkdirSync(items, { recursive: true });
  writeFileSync(join(tracker, 'settings.json'), '{"id_prefix":"bench-"}\n');
  for (let index = 0; index < 3000; index++) {
    const id = `bench-${String(index).padStart(4, '0')}`;
    const content = [
      `id: ${id}`, `title: Generated item ${index}`, 'type: Task',
      `status: ${index % 3 === 0 ? 'closed' : 'open'}`, `priority: ${index % 5}`,
      'tags[1]: benchmark', 'created_at: "2026-08-22T10:00:00.000Z"',
      'updated_at: "2026-08-22T10:00:00.000Z"', 'comments[4]{text,author}:',
      ...Array.from({ length: 4 }, () => `  "${'synthetic payload '.repeat(20)}",benchmark-agent`),
      `body: "${'synthetic body '.repeat(100)}"`, '',
    ].join('\n');
    writeFileSync(join(items, `${id}.toon`), content);
  }
}

/** Launch 30 fresh release processes per command and enforce nearest-rank p95. */
function main(): void {
  const repo = resolve(dirname(fileURLToPath(import.meta.url)), '..');
  const binary = join(repo, 'target', 'release', process.platform === 'win32' ? 'pm-rust.exe' : 'pm-rust');
  const cases = [
    { name: 'get', args: ['get', 'bench-2999'], ceiling: 50 },
    { name: 'list', args: ['list', '--limit', '20'], ceiling: 150 },
    { name: 'context', args: ['context', '--limit', '10'], ceiling: 250 },
  ];
  const root = mkdtempSync(join(tmpdir(), 'pm-rust-latency-'));
  let failed = false;
  try {
    generate(root);
    const home = join(root, 'isolated-home');
    mkdirSync(home);
    const env = { ...process.env, PM_SENTRY_DISABLED: '1', PM_TELEMETRY_SOURCE_CONTEXT: 'test',
      PM_AUTHOR: 'benchmark-agent', HOME: home, USERPROFILE: home, XDG_CONFIG_HOME: join(home, 'config') };
    for (const { name, args, ceiling } of cases) {
      const samples: number[] = [];
      for (let run = 0; run < 30; run++) {
        const start = performance.now();
        const child = spawnSync(binary, ['--workspace', root, ...args], { cwd: root, env, encoding: 'utf8' });
        const elapsed = performance.now() - start;
        if (child.error || child.status !== 0 || !child.stdout) {
          throw new Error(`${name} failed: ${child.error?.message ?? child.stderr}`);
        }
        samples.push(elapsed);
      }
      const sorted = [...samples].sort((a, b) => a - b);
      const p95 = sorted[Math.ceil(0.95 * sorted.length) - 1];
      const p50 = (sorted[14] + sorted[15]) / 2;
      console.log(JSON.stringify({ command: name, items: 3000, runs: samples.length,
        p50_ms: p50, p95_ms: p95, required_p95_below_ms: ceiling, pass: p95 < ceiling, samples_ms: samples }));
      failed ||= p95 >= ceiling;
    }
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
  process.exitCode = failed ? 1 : 0;
}

main();
