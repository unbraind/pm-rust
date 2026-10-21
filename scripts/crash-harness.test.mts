import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { existsSync, mkdtempSync, mkdirSync, readdirSync, readFileSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { test } from 'node:test';
import { CrashRefusal, replayCrashFailure, runCrashCorpus } from './crash-harness.mts';
import { boundaryCount, execute, killedAtBoundary } from './crash-process.mts';
import { fingerprint, materialize, preservesHistory, recoveredState, sameBytes, snapshot } from './crash-storage.mts';

/** Own and clean a real plain-directory tracker fixture. */
function temporary(callback: (root: string) => void): void {
  const root = mkdtempSync(join(tmpdir(), 'pm-rust-crash-test-'));
  try { callback(root); } finally { rmSync(root, { recursive: true, force: true }); }
}

test('history preservation rejects truncation, replacement and missing streams byte-exactly', () => {
  const before = new Map([['history/probe.jsonl', Buffer.from('{"op":"create"}\n')]]);
  assert.equal(preservesHistory(before, before), true);
  assert.equal(preservesHistory(before, new Map([['history/probe.jsonl', Buffer.from('{"op":"create"}\n{}\n')]])), true);
  for (const replacement of ['', '{"op":"update"}\n', '{"op":"create"}']) {
    assert.equal(preservesHistory(before, new Map([['history/probe.jsonl', Buffer.from(replacement)]])), false);
  }
  assert.equal(preservesHistory(before, new Map()), false);
});

test('successful restart cannot lose its own history while a safe refusal may retain the seed', () => temporary(root => {
  materialize(new Map([['history/probe.jsonl', Buffer.from('seed\n')]]), root);
  const seed = snapshot(root);
  writeFileSync(join(root, 'history/probe.jsonl'), 'seed\nrestart\n');
  const before = snapshot(root);
  writeFileSync(join(root, 'history/probe.jsonl'), 'seed\nmutation\nrestart\n');
  const committed = snapshot(root);
  assert.equal(recoveredState(seed, before, committed, seed, 0), 'diverged');
  assert.equal(recoveredState(seed, before, committed, seed, 4), 'before');
  assert.equal(recoveredState(seed, before, committed, before, 0), 'before');
  assert.equal(recoveredState(seed, before, committed, committed, 0), 'committed');
}));

test('inventories include nested custom items but exclude only root caches', () => temporary(root => {
  const files = new Map([
    ['settings.json', Buffer.from('{}')], ['tasks/probe.toon', Buffer.from('id: probe\n')],
    ['history/probe.jsonl', Buffer.from('{}\n')], ['custom/runtime/probe.toon', Buffer.from('id: nested\n')],
  ]);
  materialize(files, root);
  for (const cache of ['runtime', 'locks', 'transactions', 'checkpoints', 'search']) {
    mkdirSync(join(root, cache));
    writeFileSync(join(root, cache, 'cache.json'), 'private');
  }
  assert.equal(sameBytes(files, snapshot(root)), true);
  assert.equal(fingerprint(files), fingerprint(new Map([...files].reverse())));
  assert.notEqual(fingerprint(files), fingerprint(new Map([...files, ['extra', Buffer.from('')]])));
  assert.equal(sameBytes(files, new Map([...files, ['extra', Buffer.from('')]])), false);
  assert.equal(sameBytes(files, new Map([...files].slice(1))), false);
  assert.equal(sameBytes(files, new Map([...files].map(([key]) => [key, Buffer.from('changed')]))), false);
}));

test('snapshot refuses symlinks without following their target', { skip: process.platform === 'win32' }, () => temporary(root => {
  writeFileSync(join(root, 'real'), 'untouched');
  symlinkSync(join(root, 'real'), join(root, 'link'));
  assert.throws(() => snapshot(root), /symlink/);
  assert.equal(readFileSync(join(root, 'real'), 'utf8'), 'untouched');
}));

test('materialization refuses traversal before creating any files', () => temporary(root => {
  for (const name of ['../outside', '/absolute', 'tasks/../outside', 'a\\b', 'C:/drive']) {
    assert.throws(() => materialize(new Map([[name, Buffer.from('bad')]]), root), /unsafe_filename/);
  }
  assert.equal(readdirSync(root).length, 0);
}));

test('boundary inventory counts task-local entries and ignores resumed or unrelated calls', () => {
  const trace = '  10 write(3, "a", 1) = 1\n10 fsync(3) = 0\n11 fsync(4 <unfinished ...>\n11 <... fsync resumed>) = 0\n10 fsync(3) = 0\n12 rename("a", "b") = 0\n';
  assert.equal(boundaryCount(trace, 'fsync'), 2);
  assert.equal(boundaryCount(trace, 'rename'), 1);
  assert.equal(boundaryCount('', 'fsync'), 0);
});

test('ordinary exits and non-boundary deaths cannot masquerade as injected kills', () => {
  const receipt = { status: null, signal: 'SIGKILL', stdout: Buffer.alloc(0), stderr: Buffer.alloc(0), trace: '10 fsync(3) = ?\n10 +++ killed by SIGKILL +++\n' };
  assert.equal(killedAtBoundary(receipt, { syscall: 'fsync', ordinal: 1 }), true);
  assert.equal(killedAtBoundary({ ...receipt, status: 0 }, { syscall: 'fsync', ordinal: 1 }), false);
  assert.equal(killedAtBoundary({ ...receipt, trace: '10 +++ killed by SIGKILL +++\n' }, { syscall: 'fsync', ordinal: 1 }), false);
  assert.equal(killedAtBoundary(receipt, { syscall: 'rename', ordinal: 1 }), false);
});

test('invalid SDK options refuse before launching or reading any tracker', () => {
  for (const patch of [{ native: '' }, { artifacts: '' }, { maxBoundaries: 0 }, { maxBoundaries: 65 },
    { timeoutMs: 99 }, { timeoutMs: Infinity }, { scenarios: [] }, { scenarios: ['update', 'update'] as const }]) {
    assert.throws(() => runCrashCorpus({ native: 'missing', artifacts: 'unused', ...patch }),
      (error: unknown) => error instanceof CrashRefusal && error.code === 'invalid_options' && error.remedy.length > 0);
  }
});

test('CLI help and refusals are bounded, structured and omit input path contents', () => {
  const cli = resolve('scripts/crash-cli.mts');
  const help = spawnSync(process.execPath, [cli, '--help'], { encoding: 'utf8' });
  assert.equal(help.status, 0);
  assert.match(help.stdout, /Usage:/);
  for (const args of [[], ['--unknown'], ['--native'], ['--native', 'sensitive-sentinel']]) {
    const result = spawnSync(process.execPath, [cli, ...args, '--json'], { encoding: 'utf8' });
    assert.equal(result.status, 2);
    assert.equal(JSON.parse(result.stdout).code, 'invalid_options');
    assert.doesNotMatch(result.stdout, /sensitive-sentinel/);
    assert.equal(result.stderr, '');
    assert.ok(result.stdout.length < 500);
  }
});

test('real child timeout and missing executable are explicit failures', () => temporary(root => {
  assert.throws(() => execute({ executable: process.execPath, args: ['-e', 'setTimeout(()=>{},10000)'] }, [], root, 100), /timeout/);
  assert.throws(() => execute({ executable: join(root, 'absent'), args: [] }, [], root, 100), /process_unavailable/);
}));

test('replay refuses unknown manifests without executing their stored commands', () => temporary(root => {
  const manifest = join(root, 'case.json');
  const options = { native: 'unused', artifacts: join(root, 'evidence') };
  for (const value of ['invalid', '{}', '{"schema":"pm-rust/crash-v1","only":{"scenario":"shell"},"command":"danger"}']) {
    writeFileSync(manifest, value);
    assert.throws(() => replayCrashFailure(options, manifest), (error: unknown) => error instanceof CrashRefusal && error.code === 'invalid_options');
    assert.equal(existsSync(options.artifacts), false);
  }
}));

test('artifact destinations cannot mutate the source through relative names or symlink aliases',
  { skip: process.platform !== 'linux' }, () => temporary(root => {
    const source = join(root, 'source');
    mkdirSync(source);
    writeFileSync(join(source, 'settings.json'), '{"id_prefix":"safe-"}');
    const alias = join(root, 'alias');
    symlinkSync(source, alias);
    for (const artifacts of [source, join(source, 'evidence'), join(source, '..named'), join(alias, 'evidence')]) {
      assert.throws(() => runCrashCorpus({ source, artifacts, native: 'unused' }),
        (error: unknown) => error instanceof CrashRefusal && error.code === 'unsafe_source' && error.remedy.includes('outside the read-only'));
    }
    assert.deepEqual(readdirSync(source), ['settings.json']);
    assert.equal(readFileSync(join(source, 'settings.json'), 'utf8'), '{"id_prefix":"safe-"}');
  }));

test('live syscall injection kills a real writing child and restart preserves earlier acknowledged bytes',
  { skip: process.platform !== 'linux' }, () => temporary(root => {
    const durable = join(root, 'acknowledged');
    writeFileSync(durable, 'acknowledged\n');
    const program = { executable: process.execPath, args: ['-e', 'const fs=require("node:fs");const f=fs.openSync("candidate","w");fs.writeSync(f,"candidate");fs.fsyncSync(f);'] };
    const reference = execute(program, [], root, 5000, join(root, 'reference.trace'));
    assert.equal(reference.status, 0);
    assert.equal(boundaryCount(reference.trace, 'fsync'), 1);
    const killed = execute(program, [], root, 5000, join(root, 'kill.trace'), { syscall: 'fsync', ordinal: 1 });
    assert.equal(killedAtBoundary(killed, { syscall: 'fsync', ordinal: 1 }), true);
    const restarted = execute({ executable: process.execPath, args: ['-e', 'process.stdout.write(require("node:fs").readFileSync("acknowledged"))'] }, [], root, 5000);
    assert.equal(restarted.status, 0);
    assert.equal(restarted.stdout.toString(), 'acknowledged\n');
    assert.equal(existsSync(join(root, '.git')), false);
  }));

test('real pm corpus preserves acknowledged history and independently replays retained failures without git',
  { skip: process.platform !== 'linux', timeout: 120000 }, () => temporary(root => {
    const tools = join(root, 'tools');
    mkdirSync(tools);
    symlinkSync(process.execPath, join(tools, 'node'));
    const located = spawnSync('which', ['strace'], { encoding: 'utf8' });
    assert.equal(located.status, 0, 'Linux crash tests require strace');
    symlinkSync(located.stdout.trim(), join(tools, 'strace'));
    const savedPath = process.env.PATH;
    process.env.PATH = tools;
    try {
      const options = { native: resolve('target/debug/pm-rust'), artifacts: join(root, 'evidence') };
      const report = runCrashCorpus(options);
      assert.equal(report.killed, report.boundaries.reduce((count, row) => count + row.count, 0));
      assert.ok(report.killed > 0);
      assert.ok(report.cases.every(row => row.historyPreserved && row.acknowledged));
      assert.ok(report.cases.some(row => row.ok));
      assert.ok(report.parity.every(row => row.bytes && row.acknowledgement));
      const failed = report.cases.filter(row => !row.ok);
      assert.ok(failed.every(row => row.implementation === 'native' && row.syscall === 'fsync' && row.boundary === 1 && row.restartStatus === 4), 'Only independently reproduced incomplete-lock restart refusals are expected');
      assert.equal(report.failuresRetained, failed.length);
      if (failed.length > 0) {
        assert.equal(report.ok, false);
        const caseName = readdirSync(options.artifacts)[0];
        const evidence = join(options.artifacts, caseName, 'case.json');
        const before = readFileSync(evidence);
        const replay = replayCrashFailure({ ...options, artifacts: join(root, 'replay') }, evidence);
        assert.equal(replay.killed, 1);
        assert.equal(replay.ok, false);
        assert.equal(replay.cases[0].historyPreserved, true);
        assert.equal(replay.cases[0].durableSha256, JSON.parse(before.toString()).row.durableSha256);
        assert.deepEqual(readFileSync(evidence), before);
      }
      assert.equal(existsSync(join(root, '.git')), false);
    } finally { process.env.PATH = savedPath; }
  }));
