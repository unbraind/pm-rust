import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { chmodSync, existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { verifyEvidenceCommits } from './crash-evidence-commits.mts';
import { runCrashCorpus } from './crash-harness.mts';

/** Run one bounded tool and require its declared exit code. */
function command(executable: string, args: string[], status = 0): string {
  const result = spawnSync(executable, args, { encoding: 'utf8', timeout: 300000, maxBuffer: 8 * 1024 * 1024 });
  assert.equal(result.error, undefined);
  assert.equal(result.status, status, `${executable}: ${result.stderr}`);
  return result.stdout;
}
/** Hash text or bytes without storing machine-specific coordinates. */
function sha(value: string | Buffer): string { return createHash('sha256').update(value).digest('hex'); }
/** Produce an applicable patch with repository-relative headers, never scratch paths. */
function controlPatch(file: string, original: Buffer | string, changed: string, root: string): string {
  const oldFile = join(root, 'before'); const newFile = join(root, 'after');
  writeFileSync(oldFile, original); writeFileSync(newFile, changed);
  return command('git', ['diff', '--no-index', '--no-ext-diff', oldFile, newFile], 1)
    .replace(/^diff --git .*$/m, `diff --git a/${file} b/${file}`)
    .replace(/^--- .*$/m, `--- a/${file}`).replace(/^\+\+\+ .*$/m, `+++ b/${file}`)
    .replace(/^ $/gm, '');
}
/** Make only the private archived input read-only. */
function readOnly(path: string): void {
  for (const entry of readdirSync(path, { withFileTypes: true })) {
    const child = join(path, entry.name);
    if (entry.isDirectory()) readOnly(child);
    else { assert.ok(entry.isFile()); chmodSync(child, 0o444); }
  }
  chmodSync(path, 0o555);
}
/** Restore permissions only under the owned scratch directory before cleanup. */
function writable(path: string): void {
  chmodSync(path, 0o700);
  for (const entry of readdirSync(path, { withFileTypes: true })) {
    const child = join(path, entry.name);
    if (entry.isDirectory()) writable(child); else chmodSync(child, 0o600);
  }
}

if (process.platform !== 'linux') throw new Error('crash-review-proof requires Linux; just crash gives a typed platform refusal elsewhere.');
const git = spawnSync('git', ['rev-parse', '--is-inside-work-tree'], { encoding: 'utf8' });
if (git.status !== 0) throw new Error('crash-review-proof needs a git repository; the working directory is not inside one. just crash still works without git.');
const root = mkdtempSync(join(tmpdir(), 'pm-crash-controls-'));
try {
  const sourceCommit = command('git', ['rev-parse', 'HEAD']).trim();
  command('git', ['diff', '--quiet', 'HEAD', '--', 'scripts', 'src', 'tests', 'Cargo.toml', 'Cargo.lock']);
  verifyEvidenceCommits(process.cwd(), [{ receipt: 'docs/CRASH_REVIEW_2026_10_23.json', field: 'provenance.sourceCommit', commit: sourceCommit }]);
  const publicCommit = command('git', ['rev-parse', 'origin/main']).trim();
  const controls = [
    { name: 'stable-write-boundary', file: 'scripts/crash-process.mts', before: ", UV_THREADPOOL_SIZE: '1'", after: "", pattern: 'single-worker published boundary' },
    { name: 'empty-replay-dispatch', file: 'scripts/crash-cli.mts', before: 'const report = values.replay !== undefined ?', after: 'const report = values.replay ?', pattern: 'empty replay options' },
    { name: 'empty-replay-exclusion', file: 'scripts/crash-cli.mts', before: 'values.source !== undefined && values.replay !== undefined', after: 'values.source && values.replay', pattern: 'empty replay options' },
    { name: 'default-failure-details', file: 'scripts/crash-output.mts', before: 'report.cases.filter(row => row.failure)', after: 'report.cases.filter(() => false)', pattern: 'default crash output|TOON output' },
    { name: 'strict-toon-failures', file: 'scripts/crash-output.mts', before: "`failures[${failures.length}]{scenario,implementation,syscall,boundary,code,remedy}:`,\n    ...failures.map(row => `  ${row.scenario},${row.implementation},${row.syscall},${row.boundary},${row.failure!.code},${JSON.stringify(row.failure!.remedy)}`)", after: "...failures.map(row => `failure: ${JSON.stringify(row.failure)}`)", pattern: 'TOON output' },
    { name: 'workspace-cleanup', file: 'scripts/crash-harness.mts', before: 'rmSync(killedDirectory, { recursive: true, force: true });', after: '// Retain completed workspace (negative control).', pattern: 'completed case workspaces' },
    { name: 'portable-clock-import', file: 'scripts/crash-process.mts', before: "new URL('./crash-clock.mts', import.meta.url).href", after: "decodeURIComponent(new URL('./crash-clock.mts', import.meta.url).pathname)", pattern: 'clock disclosure' },
    { name: 'runtime-snapshot', file: 'scripts/crash-storage.mts', before: "['locks', 'transactions', 'checkpoints', 'search']", after: "['runtime', 'locks', 'transactions', 'checkpoints', 'search']", pattern: 'runtime transaction' },
    { name: 'cross-operation', file: 'scripts/crash-harness.mts', before: "scenario === 'update' ? ['comment', id, 'Restart synthetic sentinel', ...common] :\n      ['update', id, '--description', 'Restart synthetic description', ...common]", after: "scenario === 'update' ? ['update', id, '--priority', '1', ...common] :\n      ['comment', id, 'Restart synthetic sentinel', ...common]", pattern: 'independent (update|comment) cross-operation' },
    { name: 'published-injection', file: 'scripts/crash-harness.mts', before: "counts.push({ syscall: 'openat', count: 1 });", after: "// Disabled history-open boundary (negative control).", pattern: 'published history-open' },
    { name: 'recovery-comparison', file: 'scripts/crash-harness.mts', before: 'ok: Object.values(result).every(Boolean)', after: 'ok: true', pattern: 'cross-runtime recovery comparison' },
    { name: 'clock-disclosure', file: 'scripts/crash-harness.mts', before: "dateNow: 'fixed'", after: "dateNow: 'real'", pattern: 'clock disclosure' },
    { name: 'reduction-refusal', file: 'scripts/crash-harness.mts', before: 'reduceFailure(report, () => runCrashCorpus({ ...options, source: undefined }));', after: 'report.failuresRetained = runCrashCorpus({ ...options, source: undefined }).failuresRetained;', pattern: 'synthetic reduction refusal' },
  ];
  mkdirSync('docs/controls/crash-review', { recursive: true });
  const resume = process.argv[2];
  assert.ok(resume === undefined || resume === '--resume-controls', 'Use --resume-controls only with the owned local checkpoint.');
  const checkpoint = '.crash-failures/review-controls.json';
  const saved: unknown = resume && existsSync(checkpoint) ? JSON.parse(readFileSync(checkpoint, 'utf8')) : [];
  assert.ok(Array.isArray(saved));
  const results: { name: string; file: string; sourceSha256: string; patch: string; patchSha256: string; command: string[]; negativeExit: number; restoredExit: number }[] = [];
  for (const control of controls) {
    const original = readFileSync(control.file);
    const text = original.toString();
    assert.equal(text.split(control.before).length, 2, control.name);
    const changed = text.replace(control.before, control.after);
    const diff = controlPatch(control.file, original, changed, root);
    const patch = `docs/controls/crash-review/${control.name}.patch`;
    writeFileSync(patch, diff);
    const args = ['--test', `--test-name-pattern=${control.pattern}`, 'scripts/crash-review.test.mts'];
    const recorded: unknown = saved.find((entry: unknown) => typeof entry === 'object' && entry !== null && 'name' in entry && entry.name === control.name);
    const result = { name: control.name, file: control.file, sourceSha256: sha(original), patch, patchSha256: sha(diff),
      command: ['node', ...args], negativeExit: 1, restoredExit: 0 };
    if (JSON.stringify(recorded) === JSON.stringify(result)) {
      results.push(result);
      console.log(`${control.name}: retained completed negative=1 restored=0`);
      continue;
    }
    let negative: string;
    try { writeFileSync(control.file, changed); negative = command(process.execPath, args, 1); }
    finally { writeFileSync(control.file, original); }
    const restored = command(process.execPath, args);
    assert.match(negative, /(?:fail|not ok|✖)/);
    assert.match(restored, /(?:pass|ok|✔)/);
    assert.deepEqual(readFileSync(control.file), original);
    results.push(result);
    mkdirSync('.crash-failures', { recursive: true });
    writeFileSync(checkpoint, JSON.stringify(results, null, 2) + '\n');
    console.log(`${control.name}: negative=1 restored=0`);
  }
  const archive = join(root, 'public.tar'); const copy = join(root, 'public'); mkdirSync(copy);
  command('git', ['archive', '--format=tar', '--output', archive, publicCommit, '.agents/pm']);
  command('tar', ['-xf', archive, '-C', copy]); readOnly(join(copy, '.agents/pm'));
  const report = runCrashCorpus({ native: resolve('target/debug/pm-rust'), source: join(copy, '.agents/pm'), artifacts: join(root, 'evidence'), timeoutMs: 60000 });
  assert.equal(report.sourceUnchanged, true); assert.equal(report.ok, false);
  const inputs = command('git', ['ls-files', 'scripts/crash-*.mts', 'tests/harness', 'tests/fixtures/crash-review', 'src', 'Cargo.toml', 'Cargo.lock', 'rust-toolchain.toml', 'justfile', 'README.md', 'docs/CRASH_HARNESS.md']).trim().split('\n');
  const provenance = { sourceRepository: 'https://github.com/unbraind/pm-rust', sourceCommit, publicTrackerCommit: publicCommit,
    sourceFiles: Object.fromEntries(inputs.map(path => [path, sha(readFileSync(path))])),
    toolchain: { node: process.version, rustc: command('rustc', ['+1.90.0', '--version']).trim(), nightly: command('rustc', ['+nightly-2026-08-06', '--version']).trim(),
      cargo: command('cargo', ['+1.90.0', '--version']).trim(), strace: command('strace', ['--version']).split('\n')[0] },
    reproduction: ['git checkout <sourceCommit>', 'just oracle-install', 'npm ci --prefix tests/harness --ignore-scripts', 'cargo +1.90.0 build --locked --bin pm-rust', 'node scripts/crash-review-proof.mts'],
    oracleLockSha256: sha(readFileSync('tests/oracle/package-lock.json')), harnessLockSha256: sha(readFileSync('tests/harness/package-lock.json')) };
  const file = 'docs/CRASH_REVIEW_2026_10_23.json';
  const artifactArgs = ['--test', '--test-name-pattern=review receipt', 'scripts/crash-review.test.mts'];
  const artifactControl = { name: 'legacy-receipt-format', file, patch: 'docs/controls/crash-review/legacy-receipt-format.patch', patchSha256: '', command: ['node', ...artifactArgs], negativeExit: 1, restoredExit: 0 };
  const receipt = { schema: 'pm-rust/crash-review-v1', provenance, controls: results, publicCopy: report, artifactControl };
  const template = JSON.stringify(receipt, null, 2) + '\n';
  const oldSchema = '"schema": "pm-rust/crash-review-v1"';
  const patch = controlPatch(file, template, template.replace(oldSchema, '"schema": "legacy-receipt"'), root);
  writeFileSync(artifactControl.patch, patch); artifactControl.patchSha256 = sha(patch);
  const complete = JSON.stringify(receipt, null, 2) + '\n';
  try { writeFileSync(file, complete.replace(oldSchema, '"schema": "legacy-receipt"')); command(process.execPath, artifactArgs, 1); }
  finally { writeFileSync(file, complete); }
  command(process.execPath, artifactArgs);
  console.log(JSON.stringify({ controls: results.length, kills: report.killed, sourceItems: report.sourceItems, sourceUnchanged: report.sourceUnchanged }));
} finally { writable(root); rmSync(root, { recursive: true, force: true }); }
