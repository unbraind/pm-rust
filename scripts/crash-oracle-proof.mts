import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { parseArgs } from 'node:util';
import { counts, EXPECTED_CHECKS, LIVE_CHECKS, MUTATIONS, PHASE_CHECK, PROOF_PATTERN, REAL_CHECKS, runChecks } from './crash-proof-results.mts';
import { verifyEvidenceCommits } from './crash-evidence-commits.mts';
import { runCrashCorpus } from './crash-harness.mts';

/** Run one bounded real command and retain sanitized metadata only. */
function command(executable: string, args: string[], expected = 0): string {
  const result = spawnSync(executable, args, { encoding: 'utf8', timeout: 300000, maxBuffer: 8 * 1024 * 1024 });
  assert.equal(result.error, undefined);
  assert.equal(result.status, expected, result.stdout + result.stderr);
  return result.stdout;
}
/** Hash exact source or patch bytes for independent replay. */
function hash(bytes: Buffer | string): string { return createHash('sha256').update(bytes).digest('hex'); }

/** Record observed test results and independent behavioral controls. */
function main(): void {
  const { values } = parseArgs({ options: { 'checks-only': { type: 'boolean' }, 'test-name-pattern': { type: 'string' } } });
  if (values['checks-only']) {
    const result = runChecks(values['test-name-pattern'] ?? PROOF_PATTERN);
    const measured = counts(result.checks);
    console.log(JSON.stringify({ ...result, counts: measured }));
    if (result.exit !== 0 || measured.not_run || measured.failed) process.exitCode = 1;
    return;
  }
  if (values['test-name-pattern']) throw new Error('--test-name-pattern requires --checks-only');
  if (process.platform !== 'linux') throw new Error('crash-oracle-proof needs Linux and strace; the oracle unit tests work everywhere.');
  if (spawnSync('git', ['rev-parse', '--is-inside-work-tree']).status !== 0) throw new Error('crash-oracle-proof needs a git repository; just crash and plain tracker operations still work without git.');
  const sourceCommit = command('git', ['rev-parse', 'HEAD']).trim();
  command('git', ['diff', '--quiet', 'HEAD', '--', 'scripts', 'src', 'tests', 'Cargo.toml', 'Cargo.lock']);
  verifyEvidenceCommits(process.cwd(), [{ receipt: 'docs/CRASH_ORACLE_2026_10_25.json', field: 'sourceCommit', commit: sourceCommit }]);
  const root = mkdtempSync(join(tmpdir(), 'pm-crash-oracle-proof-'));
  const file = 'scripts/crash-oracle.mts';
  const original = readFileSync(file);
  try {
    const controls = [
      { name: 'exact-state', before: 'return sameBytes(transactionState(expected), transactionState(actual));',
        after: `return [...expected].filter(([name]) => name.startsWith('history/')).every(([name, bytes]) => {
      const seed = Buffer.from(bytes.toString().split('\\n')[0] + '\\n');
      return actual.get(name)?.subarray(0, seed.length).equals(seed);
    });`, pattern: 'exact state oracle kills all|live exact oracle kills', expected: [...REAL_CHECKS, ...LIVE_CHECKS] },
      { name: 'phase-pairing', before: 'published.phase === native.phase',
        after: 'published.syscall === native.syscall && published.boundary === native.boundary', pattern: 'phase parity', expected: [PHASE_CHECK] },
    ];
    mkdirSync('docs/controls/crash-oracle', { recursive: true });
    const proofs = [];
    for (const control of controls) {
      assert.equal(original.toString().split(control.before).length, 2);
      const changed = original.toString().replace(control.before, control.after);
      const oldFile = join(root, 'old'); const newFile = join(root, 'new');
      writeFileSync(oldFile, original); writeFileSync(newFile, changed);
      const patch = command('git', ['diff', '--no-index', '--no-ext-diff', oldFile, newFile], 1)
        .replace(/^diff --git .*$/m, `diff --git a/${file} b/${file}`)
        .replace(/^--- .*$/m, `--- a/${file}`).replace(/^\+\+\+ .*$/m, `+++ b/${file}`).replace(/^ $/gm, '');
      const patchFile = `docs/controls/crash-oracle/${control.name}.patch`;
      writeFileSync(patchFile, patch);
      let negative;
      try { writeFileSync(file, changed); negative = runChecks(control.pattern, control.expected); }
      finally { writeFileSync(file, original); }
      const restored = runChecks(control.pattern, control.expected);
      console.log(JSON.stringify({ control: control.name, negative: counts(negative.checks), restored: counts(restored.checks) }));
      if (negative.exit !== 1 || counts(negative.checks).not_run || restored.exit !== 0 || counts(restored.checks).not_run || counts(restored.checks).failed) console.error(JSON.stringify({ negative, restored }));
      assert.equal(negative.exit, 1);
      assert.ok(negative.checks.every(row => row.status !== 'not_run'), 'every negative check must execute');
      assert.ok(negative.checks.some(row => row.status === 'failed'), 'the behavioral revert must be detected');
      assert.equal(restored.exit, 0);
      assert.ok(restored.checks.every(row => row.status === 'passed'), 'every restored check must execute and pass');
      proofs.push({ name: control.name, file, sourceSha256: hash(original), patch: patchFile, patchSha256: hash(patch),
        command: restored.command, negativeExit: negative.exit, restoredExit: restored.exit, negative, restored });
      console.log(`${control.name}: reverted=1 restored=0`);
    }
    const publicCommit = command('git', ['rev-parse', 'origin/main']).trim();
    const archive = join(root, 'public.tar'); const copy = join(root, 'public'); mkdirSync(copy);
    command('git', ['archive', '--format=tar', '--output', archive, publicCommit, '.agents/pm']);
    command('tar', ['-xf', archive, '-C', copy]);
    const publicCopy = runCrashCorpus({ native: resolve('target/debug/pm-rust'), source: join(copy, '.agents/pm'),
      artifacts: join(root, 'evidence'), timeoutMs: 60000 });
    assert.equal(publicCopy.sourceUnchanged, true);
    assert.ok(publicCopy.cases.filter(row => row.implementation === 'native').every(row => row.ok && row.restartExact && row.returnExact));
    // Every paired failure must be backed by an independently failing case, never an ordinal alias.
    assert.ok(publicCopy.recoveryParity.filter(row => !row.ok).every(pair => publicCopy.cases.some(row => !row.ok && row.scenario === pair.scenario && row.phase === pair.phase)));
    const checks = proofs.flatMap(proof => proof.restored.checks);
    assert.equal(checks.length, EXPECTED_CHECKS.length);
    const mutations = MUTATIONS.map(mutation => {
      const real = checks.filter(row => REAL_CHECKS.includes(row.name) && row.name.endsWith(mutation));
      const live = checks.filter(row => LIVE_CHECKS.includes(row.name) && row.name.includes(` ${mutation} `));
      return { mutation, realStateChecks: counts(real).executed, realStateKilled: counts(real).passed,
        liveRestartKilled: live.filter(row => row.name.includes(' restart ') && row.status === 'passed').length,
        liveReturnKilled: live.filter(row => row.name.includes(' return ') && row.status === 'passed').length,
        failed: counts([...real, ...live]).failed, not_run: counts([...real, ...live]).not_run };
    });
    const receipt = { schema: 'pm-rust/crash-oracle-v2', sourceCommit, publicTrackerCommit: publicCommit,
      parsingSource: 'Node test:pass/test:fail events via scripts/crash-test-reporter.mts (JSON lines); skip/todo/absent = not_run',
      counts: counts(checks),
      sourceFiles: Object.fromEntries(command('git', ['ls-files', 'scripts/crash-*.mts', 'src', 'Cargo.toml', 'Cargo.lock', 'tests/oracle/package-lock.json', 'tests/harness/package-lock.json']).trim().split('\n').map(path => [path, hash(readFileSync(path))])),
      definition: 'Exact canonical tracker inventory and bytes, including every complete history and pending journal; phase chooses the clean PRE or POST plus the acknowledged restart and return operations.',
      phases: ['before-commit', 'after-commit', 'during-cleanup'],
      mutations, equivalence: checks.find(row => row.name === PHASE_CHECK), proofs, publicCopy };
    writeFileSync('docs/CRASH_ORACLE_2026_10_25.json', JSON.stringify(receipt, null, 2) + '\n');
    console.log(JSON.stringify({ kills: publicCopy.killed, casesPassed: publicCopy.cases.filter(row => row.ok).length,
      parity: publicCopy.recoveryParity.length, parityPassed: publicCopy.recoveryParity.filter(row => row.ok).length,
      retained: publicCopy.failuresRetained, sourceItems: publicCopy.sourceItems, sourceUnchanged: publicCopy.sourceUnchanged }));
  } finally { writeFileSync(file, original); rmSync(root, { recursive: true, force: true }); }

}
if (process.argv[1] && pathToFileURL(resolve(process.argv[1])).href === import.meta.url) main();
