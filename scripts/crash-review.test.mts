import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { chmodSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { test } from 'node:test';
import { CLOCK_POLICY, compareRecovery, replayCrashFailure } from './crash-harness.mts';
import { journals, materialize, sameBytes, snapshot } from './crash-storage.mts';

/** Own a disposable directory, never a source tracker. */
function temporary(callback: (directory: string) => void): void {
  const directory = mkdtempSync(join(tmpdir(), 'pm-crash-review-'));
  try { callback(directory); } finally { rmSync(directory, { recursive: true, force: true }); }
}

test('runtime transaction snapshots retain recovery intent and detect journal changes', () => temporary(root => {
  const files = new Map([['runtime/transactions/update-probe.json', Buffer.from('{"version":1}\n')], ['runtime/index.json', Buffer.from('{}')]]);
  materialize(files, root);
  const before = snapshot(root);
  assert.equal(before.size, 2);
  assert.equal(journals(before).size, 1);
  writeFileSync(join(root, 'runtime/transactions/update-probe.json'), '{"version":2}\n');
  assert.equal(sameBytes(journals(before), journals(snapshot(root))), false);
}));

test('cross-runtime recovery comparison rejects different bytes receipts states and journals', () => {
  const before = new Map([['tasks/probe.toon', Buffer.from('before')]]);
  const committed = new Map([['tasks/probe.toon', Buffer.from('committed')]]);
  const empty = new Map<string, Buffer>();
  const pending = new Map([['runtime/transactions/update-probe.json', Buffer.from('{}')]]);
  assert.equal(compareRecovery(before, before, 'ack', 'ack', 'before', 'before', empty, empty).ok, true);
  for (const result of [
    compareRecovery(before, committed, 'ack', 'ack', 'before', 'before', empty, empty),
    compareRecovery(before, before, 'ack', 'other', 'before', 'before', empty, empty),
    compareRecovery(before, before, 'ack', 'ack', 'before', 'committed', empty, empty),
    compareRecovery(before, before, 'ack', 'ack', 'before', 'before', pending, empty),
  ]) assert.equal(result.ok, false);
});

test('clock disclosure matches frozen Date.now and live performance elapsed time', () => {
  const code = 'const first=Date.now();const elapsed=performance.now();await new Promise(r=>setTimeout(r,30));console.log(JSON.stringify({first,last:Date.now(),wall:new Date().toISOString(),elapsed:performance.now()-elapsed}));';
  const child = spawnSync(process.execPath, ['--import', resolve('scripts/crash-clock.mts'), '--input-type=module', '-e', code], { encoding: 'utf8' });
  assert.equal(child.status, 0, child.stderr);
  const result = JSON.parse(child.stdout);
  assert.equal(result.first, result.last);
  assert.equal(result.wall, CLOCK_POLICY.metadata);
  assert.ok(result.elapsed >= 20);
  assert.equal(CLOCK_POLICY.dateNow, 'fixed');
  assert.match(CLOCK_POLICY.limitation, /TTL\/wait ages are frozen/);
  const guide = readFileSync(resolve('docs/CRASH_HARNESS.md'), 'utf8');
  assert.match(guide, /Date\.now\(\).*frozen/);
});

for (const scenario of ['update', 'comment'] as const) {
  test(`independent ${scenario} cross-operation replay proves pending journal and later refusal`,
    { skip: process.platform !== 'linux', timeout: 180000 }, () => temporary(root => {
      const source = resolve('tests/fixtures/crash-review');
      const before = snapshot(source);
      const report = replayCrashFailure({ native: resolve('target/debug/pm-rust'), artifacts: join(root, 'evidence') }, join(source, `${scenario}.json`));
      assert.equal(report.ok, false);
      assert.equal(report.killed, 1);
      const row = report.cases[0];
      assert.equal(row.restartStatus, 0);
      assert.ok(row.returnStatus !== 0);
      assert.equal(row.journalsAtKill, 1);
      assert.equal(row.journalsAfterRestart, 1);
      assert.equal(row.journalsAfterReturn, 1);
      assert.equal(row.failure?.code, 'pending_journal');
      assert.equal(report.failuresRetained, 1);
      assert.ok(row.acknowledged && row.historyPreserved);
      assert.equal(report.sourceUnchanged, true);
      assert.ok(sameBytes(before, snapshot(source)));
    }));
}

test('published history-open injection detects post-publication history loss despite exit-zero restart',
  { skip: process.platform !== 'linux', timeout: 180000 }, () => temporary(root => {
    const report = replayCrashFailure({ native: resolve('target/debug/pm-rust'), artifacts: join(root, 'evidence') },
      resolve('tests/fixtures/crash-review/published-history.json'));
    assert.equal(report.ok, false);
    const row = report.cases[0];
    assert.equal(row.syscall, 'openat');
    assert.equal(row.implementation, 'published');
    assert.equal(row.restartStatus, 0);
    assert.equal(row.state, 'diverged');
    assert.equal(row.failure?.code, 'durable_divergence');
    assert.equal(report.failuresRetained, 1);
  }));

test('synthetic reduction refusal preserves measured failure and CLI exit one',
  { skip: process.platform !== 'linux', timeout: 180000 }, () => temporary(root => {
    const wrapper = join(root, 'trace-controller');
    writeFileSync(wrapper, '#!/bin/sh\nparent="${PWD%/*}"\nsaved="${0}.root"\nif [ -f "$saved" ]; then\n read -r previous < "$saved"\n if [ "$previous" != "$parent" ]; then exit 79; fi\nelse\n printf "%s\\n" "$parent" > "$saved"\nfi\nexec strace "$@"\n');
    chmodSync(wrapper, 0o755);
    const result = spawnSync(process.execPath, [resolve('scripts/crash-cli.mts'), '--native', resolve('target/debug/pm-rust'),
      '--artifacts', join(root, 'evidence'), '--replay', resolve('tests/fixtures/crash-review/update.json'), '--strace', wrapper, '--json', '--full'],
    { encoding: 'utf8', timeout: 120000 });
    assert.equal(result.status, 1, result.stderr + result.stdout);
    const report = JSON.parse(result.stdout);
    assert.equal(report.killed, 1);
    assert.equal(report.cases[0].journalsAfterRestart, 1);
    assert.equal(report.reduction.status, 'refused');
    assert.equal(report.reduction.code, 'baseline_failed');
    assert.equal(report.failuresRetained, 0);
  }));

test('review receipt provides source toolchain and independently applicable control patches', () => {
  const receipt = JSON.parse(readFileSync(resolve('docs/CRASH_REVIEW_2026_10_23.json'), 'utf8'));
  assert.equal(receipt.schema, 'pm-rust/crash-review-v1');
  assert.match(receipt.provenance.sourceCommit, /^[a-f0-9]{40}$/);
  assert.equal(spawnSync('git', ['cat-file', '-e', receipt.provenance.sourceCommit], { encoding: 'utf8' }).status, 0);
  assert.match(receipt.provenance.publicTrackerCommit, /^[a-f0-9]{40}$/);
  for (const name of ['node', 'rustc', 'nightly', 'cargo', 'strace']) assert.ok(receipt.provenance.toolchain[name]);
  assert.ok(receipt.provenance.reproduction.includes('node scripts/crash-review-proof.mts'));
  assert.equal(receipt.controls.length, 6);
  for (const control of receipt.controls) {
    const patch = readFileSync(control.patch);
    assert.equal(createHash('sha256').update(patch).digest('hex'), control.patchSha256);
    assert.equal(control.negativeExit, 1);
    assert.equal(control.restoredExit, 0);
    assert.equal(createHash('sha256').update(readFileSync(control.file)).digest('hex'), control.sourceSha256);
    const checked = spawnSync('git', ['apply', '--check', control.patch], { encoding: 'utf8' });
    assert.equal(checked.status, 0, checked.stderr);
  }
  const artifact = receipt.artifactControl;
  assert.equal(createHash('sha256').update(readFileSync(artifact.patch)).digest('hex'), artifact.patchSha256);
  assert.equal(artifact.negativeExit, 1);
  assert.equal(artifact.restoredExit, 0);
  assert.equal(spawnSync('git', ['apply', '--check', artifact.patch], { encoding: 'utf8' }).status, 0);
});
