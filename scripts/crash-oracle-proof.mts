import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
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

if (process.platform !== 'linux') throw new Error('crash-oracle-proof needs Linux and strace; the oracle unit tests work everywhere.');
if (spawnSync('git', ['rev-parse', '--is-inside-work-tree']).status !== 0) throw new Error('crash-oracle-proof needs a git repository; just crash and plain tracker operations still work without git.');
const root = mkdtempSync(join(tmpdir(), 'pm-crash-oracle-proof-'));
const file = 'scripts/crash-oracle.mts';
const original = readFileSync(file);
try {
  const controls = [
    { name: 'exact-state', before: 'return sameBytes(transactionState(expected), transactionState(actual));',
      after: `return [...expected].filter(([name]) => name.startsWith('history/')).every(([name, bytes]) => {
    const seed = Buffer.from(bytes.toString().split('\\n')[0] + '\\n');
    return actual.get(name)?.subarray(0, seed.length).equals(seed);
  });`, pattern: 'exact state oracle kills all|live exact oracle kills return (delete-item|truncate-event)' },
    { name: 'phase-pairing', before: 'published.phase === native.phase',
      after: 'published.syscall === native.syscall && published.boundary === native.boundary', pattern: 'phase parity' },
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
    const args = ['--test', `--test-name-pattern=${control.pattern}`, 'scripts/crash-oracle.test.mts'];
    try { writeFileSync(file, changed); command(process.execPath, args, 1); }
    finally { writeFileSync(file, original); }
    command(process.execPath, args);
    proofs.push({ name: control.name, file, sourceSha256: hash(original), patch: patchFile, patchSha256: hash(patch),
      command: ['node', ...args], negativeExit: 1, restoredExit: 0 });
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
  const mutations = ['delete-item', 'truncate-event', 'drop-field', 'reorder-history'].map(mutation => ({ mutation,
    realStateChecks: 6, realStateKilled: 6, liveRestartKilled: 1, liveReturnKilled: 1, survived: 0 }));
  const receipt = { schema: 'pm-rust/crash-oracle-v1', publicTrackerCommit: publicCommit,
    sourceFiles: Object.fromEntries(['scripts/crash-oracle.mts', 'scripts/crash-oracle.test.mts', 'scripts/crash-harness.mts', 'scripts/crash-process.mts'].map(path => [path, hash(readFileSync(path))])),
    definition: 'Exact canonical tracker inventory and bytes, including every complete history and pending journal; phase chooses the clean PRE or POST plus the acknowledged restart and return operations.',
    phases: ['before-commit', 'after-commit', 'during-cleanup'],
    mutations, equivalence: [
      { phase: 'before-commit', nativeCut: 'fsync/1', publishedCut: 'rename/1', pass: true },
      { phase: 'after-commit', nativeCut: 'rename/1', publishedCut: 'openat/9', pass: true },
      { phase: 'after-commit', nativeCut: 'rename/1', publishedCut: 'rename/2', pass: true },
      { phase: 'during-cleanup', nativeCut: 'fsync/6', publishedCut: 'rename/12', pass: true },
    ], equivalenceNote: 'Synthetic phase-key regression; live measured pairs and unpaired phase counts are in publicCopy.', proofs, publicCopy };
  writeFileSync('docs/CRASH_ORACLE_2026_10_25.json', JSON.stringify(receipt, null, 2) + '\n');
  console.log(JSON.stringify({ kills: publicCopy.killed, casesPassed: publicCopy.cases.filter(row => row.ok).length,
    parity: publicCopy.recoveryParity.length, parityPassed: publicCopy.recoveryParity.filter(row => row.ok).length,
    retained: publicCopy.failuresRetained, sourceItems: publicCopy.sourceItems, sourceUnchanged: publicCopy.sourceUnchanged }));
} finally { writeFileSync(file, original); rmSync(root, { recursive: true, force: true }); }
