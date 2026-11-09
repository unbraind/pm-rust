import { syncSpawnBridge } from './context-test-process-shim.mts';
import { assertNamedTests, copyWorkingTree, checkedGit, verifyMutation } from './context-revert-helpers.mts';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import test from 'node:test';

/** Both missing oracle installations fail before source copying or compilation. */
test('revert preflight names the missing oracle installation', () => {
  const root = mkdtempSync(join(tmpdir(), 'pm-rust-preflight-'));
  try {
    for (const oracle of ['oracle', 'orientation-oracle']) {
      const result = spawnSync(process.execPath, [fileURLToPath(new URL('./context-intents-revert.mts', import.meta.url))], {
        cwd: root, encoding: 'utf8', timeout: 10_000,
      });
      assert.ifError(result.error);
      assert.equal(result.status, 1);
      assert.ok(result.stderr.includes(`Missing ${oracle} dependencies; run just oracle-install`));
      assert.ok(!result.stderr.includes('Cannot enumerate package source'));
      mkdirSync(join(root, 'tests', oracle, 'node_modules'), { recursive: true });
    }
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

/** Real Git failures retain the caller context and the failed path or repository diagnostic. */
test('revert Git failures preserve diagnostics for source scans and patch checks', () => {
  const root = mkdtempSync(join(tmpdir(), 'pm-rust-git-diagnostic-'));
  try {
    assert.throws(() => checkedGit(root, ['ls-files'], 'source scan failed'), /source scan failed:.*not a git repository/s);
    writeFileSync(join(root, 'packet.rs'), 'different packet contents\n');
    const patch = join(root, 'packet.patch');
    writeFileSync(patch, 'diff --git a/packet.rs b/packet.rs\n--- a/packet.rs\n+++ b/packet.rs\n@@ -1 +1 @@\n-original\n+expected\n');
    for (const check of [[], ['--check']]) {
      assert.throws(() => checkedGit(root, ['apply', '--reverse', ...check, patch], 'packet revert failed'), /packet revert failed:.*patch failed: packet.rs:1/s);
    }
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

/** Proof admission requires the selected test in both control and mutation runs. */
test('revert proof rejects unrelated failures skipped tests and invalid controls', () => {
  for (const node of [false, true]) {
    for (const name of ['selected', 'name.with[regex](characters)', 'preflight names setup']) {
      for (const reverted of [false, true]) {
        const line = node ? `${reverted ? 'not ok' : 'ok'} 1 - ${name}` : `test ${name} ... ${reverted ? 'FAILED' : 'ok'}`;
        const status = reverted ? (node ? 1 : 101) : 0;
        const valid = { status, stdout: `${line}\n`, stderr: '' };
        assertNamedTests(valid, [name], reverted, node);
        for (const invalid of [
          { ...valid, stdout: line.replace(name, 'unrelated') },
          { ...valid, stdout: `${line} # SKIP unavailable\n` },
          { ...valid, stdout: `${line} # TODO unavailable\n` },
          { ...valid, stdout: '' },
          { ...valid, status: null },
          { ...valid, stderr: 'could not compile' },
          { ...valid, stderr: "failureType: 'testTimeoutFailure'" },
          { ...valid, stderr: "failureType: 'cancelledByParent'" },
          { ...valid, error: new Error('spawn failed') },
        ]) assert.throws(() => assertNamedTests(invalid, [name], reverted, node));
      }
    }
  }
  const events: string[] = [];
  assert.throws(() => verifyMutation(() => { throw new Error('baseline setup failure'); }, () => events.push('mutate'), () => events.push('restore')));
  assert.equal(events.length, 0, 'a failing baseline must prevent mutation');
  verifyMutation(reverted => events.push(reverted ? 'fail selected' : 'pass selected'), () => events.push('mutate'), () => events.push('restore'));
  assert.deepEqual(events, ['pass selected', 'mutate', 'fail selected', 'restore', 'pass selected']);
  events.length = 0;
  assert.throws(() => verifyMutation(reverted => { if (reverted) throw new Error('wrong failure'); }, () => events.push('mutate'), () => events.push('restore')));
  assert.deepEqual(events, ['mutate', 'restore']);
});

/** Just's dependency graph installs imported runtime modules before standalone harness checks. */
test('standalone harness check installs the orientation oracle first', () => {
  const result = spawnSync('just', ['--dump', '--dump-format', 'json'], {
    cwd: fileURLToPath(new URL('..', import.meta.url)), encoding: 'utf8', timeout: 10_000,
  });
  assert.ifError(result.error);
  assert.equal(result.status, 0, result.stderr);
  const manifest = JSON.parse(result.stdout) as { recipes: Record<string, { dependencies: { recipe: string }[] }> };
  const dependencies = (recipe: string): string[] => manifest.recipes[recipe].dependencies.flatMap(dependency => [dependency.recipe, ...dependencies(dependency.recipe)]);
  for (const recipe of ['crash-check', 'orientation-revert-proof']) {
    for (const oracle of ['oracle-install', 'orientation-oracle-install']) {
      assert.ok(dependencies(recipe).includes(oracle), `${recipe} must install ${oracle}`);
    }
  }
});

/** Exercise the actual runner: a broken oracle/control cannot be certified as a mutation. */
test('intent runner rejects setup failures before certifying a mutation', () => {
  const scratch = mkdtempSync(join(tmpdir(), 'pm-context-baseline-'));
  try {
    const shim = join(scratch, 'cargo-shim.mjs');
    writeFileSync(shim, `import childProcess from 'node:child_process';
import { syncBuiltinESMExports } from 'node:module';
const original = childProcess.spawnSync;
childProcess.spawnSync = (command, args, options) => {
  if (command !== 'cargo') return original(command, args, options);
  if (options.env.PM_RUST_REQUIRE_PUBLISHED_CLI !== '1') throw new Error('oracle must be required');
  return { status: 101, stdout: 'test orientation_context::intent::tests::core_intent_boundaries_and_budget_receipts ... FAILED\\ntest result: FAILED\\n', stderr: 'oracle setup failed' };
};
${syncSpawnBridge}
syncBuiltinESMExports();
`);
    const childEnv = { ...process.env };
    delete childEnv.NODE_TEST_CONTEXT;
    const result = spawnSync(process.execPath, ['--import', pathToFileURL(shim).href, fileURLToPath(new URL('./context-intents-revert.mts', import.meta.url))], {
      cwd: fileURLToPath(new URL('..', import.meta.url)), env: childEnv, encoding: 'utf8', timeout: 30_000,
    });
    assert.ifError(result.error);
    assert.equal(result.status, 1);
    assert.match(result.stderr, /Unexpected test exit status/);
    assert.ok(!result.stdout.includes('"reverted":true'), 'setup failures cannot certify a behavioral revert');
  } finally { rmSync(scratch, { recursive: true, force: true }); }
});

/** Every proof runner copies existing tracked and untracked files despite unstaged deletions. */
test('all worktree proof copies tolerate unstaged deletions', () => {
  const root = mkdtempSync(join(tmpdir(), 'pm-rust-live-copy-'));
  try {
    const source = join(root, 'source');
    mkdirSync(source);
    for (const path of ['scripts', 'src', 'tests/support', 'tests/fixtures', 'tests/oracle/package.json', 'tests/orientation-oracle/package.json', 'Cargo.toml', 'Cargo.lock', 'build.rs', 'justfile']) {
      cpSync(fileURLToPath(new URL(`../${path}`, import.meta.url)), join(source, path), { recursive: true });
    }
    checkedGit(source, ['init', '--quiet'], 'fixture init');
    writeFileSync(join(source, '.git/info/exclude'), 'node_modules\n');
    for (const file of ['retained.txt', 'deleted.txt', 'deleted-dir/child.txt']) {
      mkdirSync(join(source, file, '..'), { recursive: true });
      writeFileSync(join(source, file), file);
    }
    checkedGit(source, ['add', 'retained.txt', 'deleted.txt', 'deleted-dir/child.txt'], 'fixture add');
    rmSync(join(source, 'deleted.txt'));
    rmSync(join(source, 'deleted-dir'), { recursive: true });
    writeFileSync(join(source, 'untracked.txt'), 'untracked');
    for (const oracle of ['oracle', 'orientation-oracle']) {
      mkdirSync(join(source, 'tests', oracle), { recursive: true });
      symlinkSync(fileURLToPath(new URL(`../tests/${oracle}/node_modules`, import.meta.url)), join(source, 'tests', oracle, 'node_modules'), 'junction');
    }
    const copied = join(root, 'copy');
    mkdirSync(copied);
    copyWorkingTree(source, copied);
    assert.equal(readFileSync(join(copied, 'retained.txt'), 'utf8'), 'retained.txt');
    assert.equal(readFileSync(join(copied, 'untracked.txt'), 'utf8'), 'untracked');
    assert.equal(existsSync(join(copied, 'deleted.txt')), false);
    assert.equal(existsSync(join(copied, 'deleted-dir')), false);
    const shim = join(root, 'copy-control.mjs');
    writeFileSync(shim, `import childProcess from 'node:child_process';
import assert from 'node:assert/strict';
import { existsSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import { syncBuiltinESMExports } from 'node:module';
const original = childProcess.spawnSync;
childProcess.spawnSync = (command, args, options) => {
  if (command !== 'cargo' && command !== process.execPath) return original(command, args, options);
  assert.equal(readFileSync(join(options.cwd, 'retained.txt'), 'utf8'), 'retained.txt');
  assert.equal(readFileSync(join(options.cwd, 'untracked.txt'), 'utf8'), 'untracked');
  assert.equal(existsSync(join(options.cwd, 'deleted.txt')), false);
  assert.equal(existsSync(join(options.cwd, 'deleted-dir')), false);
  throw new Error('live-copy control reached');
};
${syncSpawnBridge}
syncBuiltinESMExports();
`);
    const childEnv = { ...process.env };
    delete childEnv.NODE_TEST_CONTEXT;
    for (const script of ['context-intents-revert.mts', 'context-packet-review-revert.mts', 'context-review-revert.mts']) {
      const result = spawnSync(process.execPath, ['--import', pathToFileURL(shim).href, join(source, 'scripts', script)], {
        cwd: source, env: childEnv, encoding: 'utf8', timeout: 30_000,
      });
      assert.ifError(result.error);
      assert.equal(result.status, 1);
      assert.match(result.stderr, /live-copy control reached/, script);
    }
  } finally { rmSync(root, { recursive: true, force: true }); }
});
