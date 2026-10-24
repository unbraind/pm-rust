import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { chmodSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { test } from 'node:test';
import { boundaryCount, execute, killedAtBoundary } from './crash-process.mts';
import { CLOCK_POLICY, compareRecovery, replayCrashFailure, runCrashCorpus } from './crash-harness.mts';
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
  test(`independent ${scenario} cross-operation replay recovers journals before return`,
    { skip: process.platform !== 'linux', timeout: 180000 }, () => temporary(root => {
      const source = resolve('tests/fixtures/crash-review');
      const before = snapshot(source);
      const wrapper = join(root, 'native');
      const calls = join(root, 'native-calls.jsonl');
      writeFileSync(wrapper, `#!${process.execPath}\nconst fs=require('node:fs');const cp=require('node:child_process');fs.appendFileSync(${JSON.stringify(calls)},JSON.stringify(process.argv.slice(2))+'\\n');const r=cp.spawnSync(${JSON.stringify(resolve('target/debug/pm-rust'))},process.argv.slice(2),{stdio:'inherit'});process.exit(r.status??1);\n`);
      chmodSync(wrapper, 0o755);
      const report = replayCrashFailure({ native: wrapper, artifacts: join(root, 'evidence') }, join(source, `${scenario}.json`));
      assert.equal(report.ok, true, JSON.stringify(report.cases));
      assert.equal(report.killed, 1);
      const row = report.cases[0];
      assert.equal(row.restartStatus, 0);
      assert.equal(row.returnStatus, 0);
      const commands = readFileSync(calls, 'utf8').trim().split('\n').map(line => JSON.parse(line)[0]);
      assert.deepEqual(commands.slice(-2), [scenario === 'update' ? 'comment' : 'update', scenario]);
      assert.equal(row.journalsAtKill, 1);
      assert.equal(row.journalsAfterRestart, 0);
      assert.equal(row.journalsAfterReturn, 0);
      assert.equal(row.failure, undefined);
      assert.equal(report.failuresRetained, 0);
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
      '--artifacts', join(root, 'evidence'), '--replay', resolve('tests/fixtures/crash-review/published-history.json'), '--strace', wrapper, '--json', '--full'],
    { encoding: 'utf8', timeout: 120000 });
    assert.equal(result.status, 1, result.stderr + result.stdout);
    const report = JSON.parse(result.stdout);
    assert.equal(report.killed, 1);
    assert.equal(report.cases[0].state, 'diverged');
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
  assert.equal(receipt.controls.length, 11);
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


test('single-worker published boundary keeps successive async publications on one traced task',
  { skip: process.platform !== 'linux' }, () => temporary(root => {
    const code = `const assert=require('node:assert/strict');const fs=require('node:fs/promises');assert.equal(process.env.UV_THREADPOOL_SIZE,'1');(async()=>{await fs.writeFile('cache.tmp','derived');await fs.rename('cache.tmp','cache');await fs.writeFile('item.tmp','durable');await fs.rename('item.tmp','item');})().catch(()=>process.exitCode=1);`;
    const child = { executable: process.execPath, args: ['-e', code] };
    const reference = execute(child, [], root, 15000, join(root, 'reference.trace'));
    assert.equal(reference.status, 0, reference.stderr.toString());
    assert.equal(boundaryCount(reference.trace, 'rename'), 2);
    const interrupted = execute(child, [], root, 15000, join(root, 'interrupt.trace'), { syscall: 'rename', ordinal: 2 });
    assert.equal(killedAtBoundary(interrupted, { syscall: 'rename', ordinal: 2 }), true);
  }));

test('empty replay options refuse before normal corpus admission', () => temporary(root => {
  for (const args of [['--replay', ''], ['--source', '', '--replay', ''], ['--source', '', '--replay', 'missing']]) {
    const result = spawnSync(process.execPath, [resolve('scripts/crash-cli.mts'), '--native', 'missing', '--artifacts', join(root, 'evidence'), ...args, '--json'], { encoding: 'utf8' });
    assert.equal(result.status, 2, result.stderr);
    const refusal = JSON.parse(result.stdout);
    assert.equal(refusal.code, 'invalid_options');
    assert.match(refusal.remedy, args.length === 2 ? /readable retained case/ : /only one/);
  }
}));

test('default crash output explains parity and case failure remedies',
  { skip: process.platform !== 'linux', timeout: 180000 }, () => temporary(root => {
    const result = spawnSync(process.execPath, [resolve('scripts/crash-cli.mts'), '--native', resolve('target/debug/pm-rust'), '--artifacts', join(root, 'evidence'), '--replay', resolve('tests/fixtures/crash-review/published-history.json')], { encoding: 'utf8', timeout: 120000 });
    assert.equal(result.status, 1, result.stderr + result.stdout);
    assert.match(result.stdout, /parity\[1\]\{scenario,bytes,acknowledgement\}/);
    assert.match(result.stdout, /failure: .*"code":"durable_divergence".*"remedy":"Replay/);
}));

test('completed case workspaces are removed before later process admission',
  { skip: process.platform !== 'linux', timeout: 180000 }, () => temporary(root => {
    const wrapper = join(root, 'bounded-native');
    writeFileSync(wrapper, `#!${process.execPath}\nconst fs=require('node:fs');const path=require('node:path');const cp=require('node:child_process');const cases=fs.readdirSync(path.dirname(process.cwd()),{withFileTypes:true}).filter(e=>e.isDirectory()&&/^case-[0-9]+$/.test(e.name));if(cases.length>2)process.exit(79);const r=cp.spawnSync(${JSON.stringify(resolve('target/debug/pm-rust'))},process.argv.slice(2),{stdio:'inherit'});process.exit(r.status??1);\n`);
    chmodSync(wrapper, 0o755);
    const report = runCrashCorpus({ native: wrapper, artifacts: join(root, 'evidence'), scenarios: ['update'] });
    assert.ok(report.cases.filter(row => row.implementation === 'native').every(row => row.ok), JSON.stringify(report.cases));
    assert.ok(report.killed > 1);
}));
