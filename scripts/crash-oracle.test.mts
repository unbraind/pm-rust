import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { chmodSync, existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { test } from 'node:test';
import { exactState, phasePairs, transactionPhase } from './crash-oracle.mts';
import type { TransactionPhase } from './crash-oracle.mts';
import { CRASH_CLOCK_IMPORT, execute } from './crash-process.mts';
import { compareRecovery, runCrashCorpus } from './crash-harness.mts';
import { snapshot } from './crash-storage.mts';
import type { Snapshot } from './crash-storage.mts';

/** Own a plain non-Git tracker for every real-process test. */
function temporary(run: (directory: string) => void): void {
  const directory = mkdtempSync(join(tmpdir(), 'pm-crash-oracle-'));
  try { run(directory); } finally { rmSync(directory, { recursive: true, force: true }); }
}
/** Independently damage one canonical part of an otherwise acknowledged state. */
function damage(state: Snapshot, mutation: string): Snapshot {
  const changed = new Map(state);
  const item = 'tasks/oracle-probe.toon'; const history = 'history/oracle-probe.jsonl';
  if (mutation === 'delete-item') changed.delete(item);
  if (mutation === 'drop-field') changed.set(item, Buffer.from(changed.get(item)!.toString().replace(/^priority:.*\r?\n/m, '')));
  const lines = changed.get(history)!.toString().trimEnd().split('\n');
  if (mutation === 'truncate-event') changed.set(history, Buffer.from(lines.slice(0, -1).join('\n') + '\n'));
  if (mutation === 'reorder-history') changed.set(history, Buffer.from([...lines].reverse().join('\n') + '\n'));
  assert.equal(exactState(state, changed), false, mutation);
  return changed;
}

for (const scenario of ['update', 'comment'] as const) {
  test(`exact state oracle kills all recovered and returned ${scenario} data-loss mutations`, { timeout: 60000 }, () => temporary(root => {
    const implementation = { executable: process.execPath, args: ['--import', CRASH_CLOCK_IMPORT,
      resolve('tests/oracle/node_modules/@unbrained/pm-cli/dist/cli.js')], trailing: ['--json', '--no-extensions'] };
    /** Run real acknowledged operations in a tracker with Git discovery disabled by execute. */
    function command(args: string[]): Snapshot {
      const result = execute(implementation, args, root, 15000);
      assert.equal(result.status, 0, result.stderr.toString());
      return snapshot(join(root, '.agents/pm'));
    }
    command(['init', '--defaults', '--prefix', 'oracle-', '--agent-guidance', 'skip']);
    const pre = command(['create', 'task', 'Oracle probe', '--id', 'oracle-probe', '--priority', '1', '--create-mode', 'progressive']);
    const post = command(scenario === 'update' ? ['update', 'oracle-probe', '--description', 'Committed change'] : ['comment', 'oracle-probe', 'Committed comment']);
    const recovered = command(scenario === 'update' ? ['comment', 'oracle-probe', 'Acknowledged restart'] : ['update', 'oracle-probe', '--description', 'Acknowledged restart']);
    const returned = command(scenario === 'update' ? ['update', 'oracle-probe', '--priority', '2'] : ['comment', 'oracle-probe', 'Acknowledged return']);
    assert.equal(existsSync(join(root, '.git')), false);
    assert.equal(exactState(pre, post), false);
    for (const state of [post, recovered, returned]) {
      assert.equal(exactState(state, new Map(state)), true);
      for (const mutation of ['delete-item', 'truncate-event', 'drop-field', 'reorder-history']) damage(state, mutation);
      const journal = new Map(state); journal.set('runtime/transactions/unexpected.json', Buffer.from('{}'));
      assert.equal(exactState(state, journal), false);
      const other = new Map(state); other.set('tasks/other.toon', Buffer.from('id: other\n'));
      assert.equal(exactState(state, other), false);
      const runtime = new Map(state); runtime.set('runtime/owned.json', Buffer.from('{}'));
      assert.equal(exactState(state, runtime), false);
    }
    // A zero-exit final return may not truncate the already acknowledged restart event.
    assert.equal(exactState(returned, recovered), false);
    assert.equal(exactState(post, pre), false);
  }));
}

test('transaction phase comes from commit events, including resumed calls and history write opens', () => {
  const id = 'oracle-probe';
  const journal = `/tracker/.agents/pm/runtime/transactions/update-${id}.json`;
  const item = `/tracker/.agents/pm/tasks/${id}.toon`;
  const history = `/tracker/.agents/pm/history/${id}.jsonl`;
  const native = `10 link("stage", "${journal}") = 0\n`;
  const published = `20 rename("stage", "${item}") = 0\n`;
  for (const runtime of ['native', 'published'] as const) {
    const commit = runtime === 'native' ? native : published;
    assert.equal(transactionPhase('10 rename("cache.tmp", "cache") = 0\n', runtime, id), 'before-commit');
    assert.equal(transactionPhase(commit.replace('= 0', '= -1 EIO'), runtime, id), 'before-commit');
    assert.equal(transactionPhase(commit, runtime, id), 'after-commit');
    assert.equal(transactionPhase(commit.replace(' = 0', ' <unfinished ...>\n10 <... '+(runtime === 'native' ? 'link' : 'rename')+' resumed> ) = 0').replace(/^20 /, '10 '), runtime, id), 'after-commit');
    assert.equal(transactionPhase(commit + `10 write(5<${history}>, "event", 5) = 5\n`, runtime, id), 'during-cleanup');
    assert.equal(transactionPhase(`10 openat(AT_FDCWD, "${history}", O_WRONLY|O_CREAT|O_APPEND, 0666) = ?\n`, runtime, id), 'after-commit');
    assert.equal(transactionPhase(`10 openat(AT_FDCWD, "${history}", O_RDONLY) = ?\n`, runtime, id), 'before-commit');
  }
  // Publishing an item is not the native journal commit point, nor vice versa.
  assert.equal(transactionPhase(published, 'native', id), 'before-commit');
  assert.equal(transactionPhase(native, 'published', id), 'before-commit');
});

test('phase parity pairs equivalent cuts with different ordinals and rejects ordinal aliases', () => {
  const cuts: { phase: TransactionPhase; implementation: 'native' | 'published'; boundary: number; syscall: string }[] = [
    { implementation: 'native', phase: 'before-commit', syscall: 'fsync', boundary: 1 },
    { implementation: 'native', phase: 'after-commit', syscall: 'rename', boundary: 1 },
    { implementation: 'native', phase: 'during-cleanup', syscall: 'fsync', boundary: 6 },
    { implementation: 'published', phase: 'before-commit', syscall: 'rename', boundary: 1 },
    { implementation: 'published', phase: 'after-commit', syscall: 'openat', boundary: 9 },
    { implementation: 'published', phase: 'after-commit', syscall: 'rename', boundary: 2 },
    { implementation: 'published', phase: 'during-cleanup', syscall: 'rename', boundary: 12 },
  ];
  const pairs = phasePairs(cuts);
  assert.equal(pairs.length, 4);
  assert.ok(pairs.every(([a, b]) => a.phase === b.phase));
  for (const [native, published] of pairs) {
    const nativeBytes = new Map([['tasks/probe.toon', Buffer.from(native.phase === 'before-commit' ? 'PRE' : 'POST')]]);
    const publishedBytes = new Map([['tasks/probe.toon', Buffer.from(published.phase === 'before-commit' ? 'PRE' : 'POST')]]);
    const state = native.phase === 'before-commit' ? 'before' : 'committed';
    assert.equal(compareRecovery(nativeBytes, publishedBytes, 'ack', 'ack', state, state, new Map(), new Map()).ok, true);
  }
  assert.ok(pairs.some(([a, b]) => a.boundary === 1 && b.boundary === 9));
  assert.ok(pairs.some(([a, b]) => a.boundary === 6 && b.boundary === 12));
  assert.ok(!pairs.some(([a, b]) => a.syscall === 'rename' && a.boundary === 1 && b.syscall === 'rename' && b.boundary === 1));
  assert.equal(phasePairs(cuts.filter(cut => cut.phase === 'during-cleanup' && cut.implementation === 'native')).length, 0);
});

for (const stage of ['restart', 'return']) for (const mutation of ['delete-item', 'truncate-event', 'drop-field', 'reorder-history']) {
  test(`live exact oracle kills ${stage} ${mutation} despite exit zero`, { skip: process.platform !== 'linux', timeout: 60000 }, () => temporary(root => {
    const wrapper = join(root, 'native');
    const program = `#!${process.execPath}\nconst fs=require('node:fs');const cp=require('node:child_process');const args=process.argv.slice(2);const r=cp.spawnSync(${JSON.stringify(resolve('target/debug/pm-rust'))},args,{stdio:'inherit'});if(r.status===0&&fs.existsSync('kill.trace')&&args[0]===${JSON.stringify(stage === 'restart' ? 'comment' : 'update')}){const item='.agents/pm/tasks/crash-crash-probe.toon';const history='.agents/pm/history/crash-crash-probe.jsonl';const mutation=${JSON.stringify(mutation)};if(mutation==='delete-item')fs.unlinkSync(item);if(mutation==='drop-field')fs.writeFileSync(item,fs.readFileSync(item,'utf8').replace(/^priority:.*\\r?\\n/m,''));const lines=fs.readFileSync(history,'utf8').trimEnd().split('\\n');if(mutation==='truncate-event')fs.writeFileSync(history,lines.slice(0,-1).join('\\n')+'\\n');if(mutation==='reorder-history')fs.writeFileSync(history,lines.reverse().join('\\n')+'\\n');}process.exit(r.status??1);\n`;
    writeFileSync(wrapper, program); chmodSync(wrapper, 0o755);
    const report = runCrashCorpus({ native: wrapper, artifacts: join(root, 'evidence'), scenarios: ['update'],
      only: { implementation: 'native', scenario: 'update', syscall: 'fsync', ordinal: 1 } });
    const row = report.cases[0];
    assert.equal(row.restartStatus, 0);
    if (stage === 'return') { assert.equal(row.returnStatus, 0); assert.equal(row.restartExact, true); }
    assert.equal(stage === 'restart' ? row.restartExact : row.returnExact, false);
    assert.equal(row.ok, false);
    assert.equal(report.ok, false);
    assert.equal(report.failuresRetained, 1);
  }));
}


test('phase oracle receipt pins mutation tables and independently applicable reverts', () => {
  const receipt = JSON.parse(readFileSync('docs/CRASH_ORACLE_2026_10_25.json', 'utf8'));
  assert.equal(receipt.schema, 'pm-rust/crash-oracle-v1');
  assert.equal(receipt.mutations.length, 4);
  for (const row of receipt.mutations) {
    assert.equal(row.realStateChecks, 6);
    assert.equal(row.realStateKilled, 6);
    assert.equal(row.liveRestartKilled, 1);
    assert.equal(row.liveReturnKilled, 1);
    assert.equal(row.survived, 0);
  }
  assert.equal(receipt.equivalence.length, 4);
  assert.ok(receipt.equivalence.every((row: { pass: boolean }) => row.pass));
  for (const proof of receipt.proofs) {
    assert.equal(proof.negativeExit, 1);
    assert.equal(proof.restoredExit, 0);
    assert.equal(createHash('sha256').update(readFileSync(proof.file)).digest('hex'), proof.sourceSha256);
    assert.equal(createHash('sha256').update(readFileSync(proof.patch)).digest('hex'), proof.patchSha256);
    const checked = spawnSync('git', ['apply', '--check', proof.patch], { encoding: 'utf8' });
    assert.equal(checked.status, 0, checked.stderr);
  }
  for (const [file, hash] of Object.entries(receipt.sourceFiles)) assert.equal(createHash('sha256').update(readFileSync(file)).digest('hex'), hash);
  assert.equal(receipt.publicCopy.sourceUnchanged, true);
  assert.ok(receipt.publicCopy.cases.every((row: { phase: string }) => receipt.phases.includes(row.phase)));
});
