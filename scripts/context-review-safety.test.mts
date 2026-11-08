/** Adversarial boundary tests for every proof runner and real-copy receipt. */
import assert from 'node:assert/strict';
import { spawn, spawnSync } from 'node:child_process';
import { existsSync, lstatSync, mkdirSync, mkdtempSync, readFileSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';
import { setTimeout } from 'node:timers/promises';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { authoritative, verifiedOracleVersion } from './context-real-helpers.mts';
import { checkedGit, copyWorkingTree, supportedProofs } from './context-revert-helpers.mts';
import { verifyAsyncMutation } from './context-proof-process.mts';

const packageRoot = fileURLToPath(new URL('..', import.meta.url));
const runners = ['context-intents-revert.mts', 'context-packet-review-revert.mts', 'context-review-revert.mts', 'context-safety-revert.mts', 'native-adapter-revert.mts', 'native-adapter-capabilities-revert.mts', 'native-adapter-capabilities-review-revert.mts', 'native-adapter-changes-revert.mts', 'perf-revert-proof.mts'];

/** Build a public synthetic source copy with shared read-only oracle installations. */
function fixture(root: string): string {
  const source = join(root, 'source');
  mkdirSync(source);
  copyWorkingTree(packageRoot, source);
  checkedGit(source, ['init', '--quiet'], 'fixture init');
  writeFileSync(join(source, '.git/info/exclude'), 'node_modules\n');
  for (const oracle of ['oracle', 'orientation-oracle']) symlinkSync(join(packageRoot, 'tests', oracle, 'node_modules'), join(source, 'tests', oracle, 'node_modules'), 'junction');
  return source;
}

test('Windows signal proof selection preserves every supported case and reports skips', () => {
  const cases = [{ test: 'ordinary rust test' }, { test: 'runner SIGTERM proof' }, { test: 'runner SIGINT proof' }, { test: 'ordinary node test' }];
  const receipts: string[] = [];
  assert.deepEqual(supportedProofs(cases, undefined, 'win32', message => receipts.push(message)), [cases[0], cases[3]]);
  assert.equal(receipts.length, 2);
  assert.ok(receipts.every(message => message.includes('Unix host')));
  for (const signal of ['SIGTERM', 'SIGINT']) assert.throws(() => supportedProofs(cases, signal, 'win32'), /requires a Unix host/);
  assert.deepEqual(supportedProofs(cases, undefined, 'linux'), cases);
  assert.deepEqual(supportedProofs(cases, 'ordinary rust', 'win32'), [cases[0]]);
  const root = mkdtempSync(join(tmpdir(), 'pm-proof-platform-'));
  try {
    const shim = join(root, 'windows-selection.mjs');
    writeFileSync(shim, `import childProcess from 'node:child_process';
import { syncBuiltinESMExports } from 'node:module';
Object.defineProperty(process, 'platform', { value: 'win32' });
childProcess.spawn = () => { throw new Error('unsupported proof reached a baseline subprocess'); };
syncBuiltinESMExports();
`);
    const result = spawnSync(process.execPath, ['--import', pathToFileURL(shim).href, join(packageRoot, 'scripts/context-review-revert.mts'), 'SIGTERM'], {
      cwd: packageRoot, encoding: 'utf8', timeout: 30_000, env: { ...process.env, TEMP: root, TMP: root },
    });
    assert.equal(result.status, 1);
    assert.match(result.stderr, /Selected signal proof requires a Unix host/);
    assert.doesNotMatch(result.stderr, /unsupported proof reached a baseline subprocess/);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('async proof controls restore exactly and never mutate after a failed baseline', async () => {
  const events: string[] = [];
  await assert.rejects(verifyAsyncMutation(async () => { throw new Error('baseline'); }, () => events.push('mutate'), () => events.push('restore')));
  assert.equal(events.length, 0);
  await verifyAsyncMutation(async reverted => { events.push(reverted ? 'failure' : 'pass'); }, () => events.push('mutate'), () => events.push('restore'));
  assert.deepEqual(events, ['pass', 'mutate', 'failure', 'restore', 'pass']);
  events.length = 0;
  await assert.rejects(verifyAsyncMutation(async reverted => { if (reverted) throw new Error('wrong failure'); }, () => events.push('mutate'), () => events.push('restore')));
  assert.deepEqual(events, ['mutate', 'restore']);
});

test('JSON snapshot additions removals and changes are authoritative in every custom directory', () => {
  const root = mkdtempSync(join(tmpdir(), 'pm-json-snapshot-'));
  try {
    for (const folder of ['tasks', 'history', 'custom-types/runtime', 'custom-types/locks']) {
      mkdirSync(join(root, folder), { recursive: true });
      const path = join(root, folder, 'original.json');
      writeFileSync(path, '{}');
      const before = authoritative(root);
      for (const mutation of ['change', 'remove', 'add']) {
        const added = join(root, folder, 'added.json');
        if (mutation === 'change') writeFileSync(path, '{"changed":true}');
        if (mutation === 'remove') rmSync(path);
        if (mutation === 'add') writeFileSync(added, '{}');
        assert.notDeepEqual(authoritative(root), before, `${folder}: ${mutation}`);
        writeFileSync(path, '{}');
        rmSync(added, { force: true });
        assert.deepEqual(authoritative(root), before);
      }
    }
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('real-copy oracle provenance refuses mismatched missing failed or noisy driver versions before side effects', () => {
  const root = mkdtempSync(join(tmpdir(), 'pm-oracle-provenance-'));
  try {
    const driver = join(root, 'driver.mjs');
    const environment = { PATH: process.env.PATH, PM_SENTRY_DISABLED: '1', PM_TELEMETRY_SOURCE_CONTEXT: 'test' };
    assert.equal(verifiedOracleVersion(join(packageRoot, 'tests/orientation-oracle/node_modules/@unbrained/pm-cli/dist/cli.js'), root, environment), '2026.11.5');
    for (const response of ["console.log('2026.11.4')", "console.log('')", "console.log('2026.11.5'); process.exit(2)", "console.log('2026.11.5'); console.error('warning')", "throw new Error('unavailable')"]) {
      writeFileSync(driver, response);
      assert.throws(() => verifiedOracleVersion(driver, root, environment), /must verify the pinned oracle/);
      mkdirSync(join(root, '.agents/pm/runtime'), { recursive: true });
      writeFileSync(join(root, '.pm-rust-read-copy'), 'fixture');
      const retained = join(root, '.agents/pm/runtime/retained');
      writeFileSync(retained, 'unchanged');
      const output = join(root, 'receipt.json');
      const result = spawnSync(process.execPath, [join(packageRoot, 'scripts/context-real.mts'), '--copy', root, '--driver', driver, '--binary', process.execPath, '--samples', '1', '--output', output], { encoding: 'utf8' });
      assert.equal(result.status, 1);
      assert.match(result.stderr, /must verify the pinned oracle/);
      assert.equal(readFileSync(retained, 'utf8'), 'unchanged');
      assert.equal(existsSync(output), false);
    }
    writeFileSync(driver, "console.log('2026.11.5')");
    assert.equal(verifiedOracleVersion(driver, root, environment), '2026.11.5');
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('proof copies materialize absolute relative and directory symlinks before any mutation', { skip: process.platform === 'win32' }, () => {
  const root = mkdtempSync(join(tmpdir(), 'pm-proof-symlink-'));
  try {
    const source = join(root, 'source'); const destination = join(root, 'copy');
    mkdirSync(source); mkdirSync(destination); mkdirSync(join(source, 'original'));
    checkedGit(source, ['init', '--quiet'], 'fixture init');
    writeFileSync(join(source, 'original/target.rs'), 'original');
    symlinkSync(join(source, 'original/target.rs'), join(source, 'absolute.rs'));
    symlinkSync('original/target.rs', join(source, 'relative.rs'));
    symlinkSync(join(source, 'original'), join(source, 'directory'));
    symlinkSync(join(source, 'original'), join(source, 'node_modules'));
    mkdirSync(join(source, 'nested'));
    symlinkSync(join(source, 'original'), join(source, 'nested/node_modules'));
    copyWorkingTree(source, destination);
    assert.equal(existsSync(join(destination, 'node_modules')), false);
    assert.equal(existsSync(join(destination, 'nested/node_modules')), false);
    for (const name of ['absolute.rs', 'relative.rs', 'directory/target.rs']) {
      assert.equal(lstatSync(join(destination, name)).isSymbolicLink(), false);
      writeFileSync(join(destination, name), 'mutation');
      assert.equal(readFileSync(join(source, 'original/target.rs'), 'utf8'), 'original');
    }
    // Execute every copy callback against a real symlink, rather than inspecting its spelling.
    const shim = join(root, 'copy-shim.mjs');
    writeFileSync(shim, `import assert from 'node:assert/strict';
import fs from 'node:fs';
import { syncBuiltinESMExports } from 'node:module';
const original = fs.cpSync;
fs.cpSync = (_source, _destination, options) => {
  const source = ${JSON.stringify(join(source, 'absolute.rs'))};
  const target = ${JSON.stringify(join(root, 'callback-copy.rs'))};
  fs.rmSync(target, {force:true});
  original(source, target, options);
  assert.equal(fs.lstatSync(target).isSymbolicLink(), false, 'copied mutation targets must be regular files');
  fs.writeFileSync(target, 'mutation');
  assert.equal(fs.readFileSync(source, 'utf8'), 'original');
  throw new Error('symlink isolation verified');
};
syncBuiltinESMExports();
`);
    for (const runner of runners) {
      const result = spawnSync(process.execPath, ['--import', pathToFileURL(shim).href, join(packageRoot, 'scripts', runner)], { cwd: packageRoot, encoding: 'utf8', timeout: 30_000 });
      assert.equal(result.status, 1, runner);
      assert.match(result.stderr, /symlink isolation verified/, runner);
    }
  } finally { rmSync(root, { recursive: true, force: true }); }
});

for (const runner of runners) {
  for (const signal of ['SIGINT', 'SIGTERM'] as const) {
    test(`all proof runners stop descendants before cleanup: ${runner} ${signal}`, { skip: process.platform === 'win32', timeout: 20_000 }, async () => {
      const root = mkdtempSync(join(tmpdir(), 'pm-proof-process-'));
      let owned: { pid: number; worker: number; root: string } | undefined;
      const source = fixture(root);
      const shim = join(root, 'spawn-shim.mjs');
      const fake = join(root, 'cargo.mjs');
      writeFileSync(fake, `import { spawn } from 'node:child_process';
import { existsSync, renameSync, writeFileSync } from 'node:fs';
import { setTimeout } from 'node:timers/promises';
const marker = process.env.PM_PROOF_MARKER;
const worker = spawn(process.execPath, ['--input-type=module', '-e', \`
import { existsSync, writeFileSync } from 'node:fs';
const marker = process.env.PM_PROOF_MARKER;
for (const signal of ['SIGINT','SIGTERM']) process.on(signal, () => {
  setTimeout(() => { writeFileSync(marker + '.stopped', JSON.stringify(existsSync(process.cwd()))); process.exit(0); }, 100);
});
writeFileSync(marker + '.worker', String(process.pid));
setInterval(() => {}, 1000);
\`], { stdio: ['ignore', 'inherit', 'inherit'] });
while (!existsSync(marker + '.worker')) await setTimeout(5);
writeFileSync(marker + '.stage', JSON.stringify({ pid: process.pid, worker: worker.pid, root: process.cwd() }));
renameSync(marker + '.stage', marker);
setInterval(() => {}, 1000);
`);
      writeFileSync(shim, `import childProcess from 'node:child_process';
import { syncBuiltinESMExports } from 'node:module';
const original = childProcess.spawn;
childProcess.spawn = (command, args, options) => original(process.execPath, [${JSON.stringify(fake)}], { ...options, env: { ...options.env, PM_PROOF_MARKER: process.env.PM_PROOF_MARKER } });
syncBuiltinESMExports();
`);
      const marker = join(root, 'ready');
      const child = spawn(process.execPath, ['--import', pathToFileURL(shim).href, join(source, 'scripts', runner), ...(runner === 'context-review-revert.mts' ? ['context_stored_settings_fallback'] : [])], {
        cwd: source, stdio: ['ignore', 'pipe', 'pipe'], env: { ...process.env, PM_PROOF_MARKER: marker },
      });
      let stderr = ''; child.stderr.on('data', chunk => { stderr += String(chunk); }); child.stdout.resume();
      const finished = new Promise<number | null>((resolve, reject) => { child.once('error', reject); child.once('exit', resolve); });
      try {
        for (let i = 0; !existsSync(marker) && i < 1000; i++) { if (child.exitCode !== null) break; await setTimeout(10); }
        assert.ok(existsSync(marker), stderr);
        owned = JSON.parse(readFileSync(marker, 'utf8')) as typeof owned;
        child.kill(signal);
        const deadline = globalThis.setTimeout(() => child.kill('SIGKILL'), 8000);
        try { assert.equal(await finished, signal === 'SIGINT' ? 130 : 143, stderr); }
        finally { clearTimeout(deadline); }
        assert.ok(owned);
        assert.equal(readFileSync(`${marker}.stopped`, 'utf8'), 'true', 'worker stops with its scratch inputs present');
        assert.equal(existsSync(owned.root), false, 'scratch is removed after worker termination');
      } finally {
        if (child.exitCode === null) child.kill('SIGKILL');
        if (owned) {
          for (const pid of [-owned.pid, owned.pid, owned.worker]) { try { process.kill(pid, 'SIGKILL'); } catch { /* already drained */ } }
        }
        await finished;
        if (owned) rmSync(owned.root, { recursive: true, force: true });
        rmSync(root, { recursive: true, force: true });
      }
    });
  }
}

/** Exercise the Windows branch on every host, including inability to prove tree termination. */
test('Windows proof interruption terminates the complete tree or retains scratch inputs', () => {
  const root = mkdtempSync(join(tmpdir(), 'pm-proof-windows-'));
  try {
    for (const fail of [false, true]) {
      const script = join(root, 'windows.mjs');
      writeFileSync(script, `import assert from 'node:assert/strict';
import childProcess from 'node:child_process';
import { EventEmitter } from 'node:events';
import { PassThrough } from 'node:stream';
import { syncBuiltinESMExports } from 'node:module';
import { existsSync, mkdirSync } from 'node:fs';
Object.defineProperty(process, 'platform', { value: 'win32' });
const calls = [];
childProcess.execFileSync = (command, args) => {
  assert.equal(command, 'taskkill');
  assert.deepEqual(args, ['/pid', '424242', '/T', '/F']);
  calls.push(args);
  if (${fail}) throw new Error('tree termination unavailable');
};
childProcess.spawn = () => {
  const child = new EventEmitter(); child.pid = 424242;
  child.stdout = new PassThrough(); child.stderr = new PassThrough();
  setImmediate(() => {
    process.emit('SIGTERM');
    child.emit('exit', null, 'SIGTERM');
    child.stdout.end(); child.stderr.end(); child.emit('close', null);
  });
  return child;
};
syncBuiltinESMExports();
import { ProofProcesses } from ${JSON.stringify(pathToFileURL(join(packageRoot, 'scripts/context-proof-process.mts')).href)};
const processes = new ProofProcesses();
const scratch = ${JSON.stringify(join(root, 'scratch'))};
mkdirSync(scratch);
await assert.rejects(processes.run('cargo', []));
assert.ok(calls.length > 0);
if (${fail}) await assert.rejects(processes.cleanup(scratch), /scratch retained/);
else await processes.cleanup(scratch);
assert.equal(existsSync(scratch), ${fail});
process.exitCode = 0;
`);
      const result = spawnSync(process.execPath, [script], { encoding: 'utf8', timeout: 10_000 });
      assert.equal(result.status, 0, result.stderr);
      rmSync(join(root, 'scratch'), { force: true, recursive: true });
    }
  } finally { rmSync(root, { recursive: true, force: true }); }
});
