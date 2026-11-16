import assert from 'node:assert/strict';
import { execFileSync, spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';
import { auditIdentities } from './ci-identity.mts';
import { targetShards } from './ci-targets.mts';

test('target partition retains every Cargo integration target once, including new targets', () => {
  const names = ['claim_release_differential', 'orientation_differential', 'pagination_differential',
    'search_collections_differential', 'close_differential', 'future_contract', 'read_contract', 'constructor', '__proto__'];
  const shards = targetShards(names);
  assert.deepEqual(shards.flat().sort(), [...names].sort());
  assert.deepEqual(shards, targetShards([...names].reverse()));
  assert.equal(new Set(shards.flat()).size, names.length);
  assert.ok(shards.every(shard => shard.length > 0));
});

test('docs identity gate inspects unreachable authors and committers', () => {
  const root = mkdtempSync(join(tmpdir(), 'ci-identity-'));
  const env = { ...process.env, GIT_CONFIG_NOSYSTEM: '1', GIT_CONFIG_GLOBAL: process.platform === 'win32' ? 'NUL' : '/dev/null' };
  const git = (args: string[], input?: string) => execFileSync('git', args, { cwd: root, env, input, encoding: 'utf8' }).trim();
  try {
    git(['init', '--quiet']);
    mkdirSync(join(root, '.github'));
    writeFileSync(join(root, '.github/approved-git-identities.txt'), 'fixture@example.invalid\n');
    const tree = git(['hash-object', '-t', 'tree', '-w', '--stdin'], '');
    const allowed = 'Fixture <fixture@example.invalid> 1 +0000';
    const foreign = 'Fixture <foreign@example.invalid> 1 +0000';
    const commit = (author: string, committer: string) => git(['hash-object', '-t', 'commit', '-w', '--stdin'],
      `tree ${tree}\nauthor ${author}\ncommitter ${committer}\n\nSynthetic identity fixture\n`);
    commit(allowed, allowed);
    assert.equal(auditIdentities(root), 1);
    commit(allowed, foreign);
    assert.throws(() => auditIdentities(root), /Unapproved committer/);
    commit(foreign, allowed);
    assert.throws(() => auditIdentities(root), /Unapproved (author|committer)/);
  } finally { rmSync(root, { recursive: true, force: true }); }
});


test('harness watchdogs validate scales before launching and preserve integer milliseconds', () => {
  const module = new URL('./crash-test-depth.mts', import.meta.url).href;
  for (const [scale, expected] of [['1', '101'], ['3', '303'], ['1.25', '127']]) {
    const result = execFileSync(process.execPath, ['--input-type=module', '-e', `import {testTime} from '${module}'; console.log(testTime(101));`],
      { encoding: 'utf8', env: { ...process.env, PM_TEST_TIME_SCALE: scale } });
    assert.equal(result.trim(), expected);
  }
  for (const scale of ['0', '-1', 'NaN', 'Infinity']) {
    assert.throws(() => execFileSync(process.execPath, ['--input-type=module', '-e', `import '${module}';`],
      { stdio: 'pipe', env: { ...process.env, PM_TEST_TIME_SCALE: scale } }));
  }
});

test('local strace setup rejects incompatible flags and denied ptrace before publishing PATH', { skip: process.platform !== 'linux' }, () => {
  const root = mkdtempSync(join(tmpdir(), 'ci-strace-'));
  const prefix = join(root, 'syscall-tools');
  const githubPath = join(root, 'github-path');
  const argsPath = join(root, 'probe-args');
  try {
    mkdirSync(join(prefix, 'usr/bin'), { recursive: true });
    mkdirSync(join(prefix, 'bin'));
    const stub = `#!/bin/sh
if [ "$1" = -V ]; then exit 0; fi
printf '%s\\n' "$@" > "$STRACE_PROBE_ARGS"
case "$STRACE_PROBE_MODE" in
  old) for flag do if [ "$flag" = --kill-on-exit ]; then echo 'unrecognized option --kill-on-exit' >&2; exit 1; fi; done ;;
  denied) echo 'ptrace denied' >&2; exit 1 ;;
esac
`;
    for (const target of ['usr/bin/strace', 'bin/strace']) writeFileSync(join(prefix, target), stub, { mode: 0o755 });
    for (const mode of ['old', 'denied', 'supported']) {
      writeFileSync(githubPath, '');
      const result = spawnSync('bash', ['.github/scripts/setup-local-strace.sh'], {
        encoding: 'utf8', env: { ...process.env, RUNNER_TOOL_CACHE: root, RUNNER_TEMP: root, GITHUB_PATH: githubPath,
          STRACE_PROBE_ARGS: argsPath, STRACE_PROBE_MODE: mode },
      });
      assert.equal(result.status, mode === 'supported' ? 0 : 1, result.stderr);
      if (mode === 'supported') assert.equal(readFileSync(githubPath, 'utf8').trim(), join(prefix, 'bin'));
      else {
        assert.match(result.stderr, /strace.*--kill-on-exit.*syscall injection.*ptrace/s);
        assert.equal(readFileSync(githubPath, 'utf8'), '');
      }
    }
    const args = readFileSync(argsPath, 'utf8').trim().split('\n');
    const processSource = readFileSync(new URL('./crash-process.mts', import.meta.url), 'utf8');
    const harnessFlags = ['-f', '--kill-on-exit', '-qq', '-yy', '-s', '4096'];
    assert.ok(processSource.includes(harnessFlags.map(flag => `'${flag}'`).join(', ')));
    assert.deepEqual(args.slice(0, 6), harnessFlags);
    assert.ok(args.includes('trace=rename,fsync,link,linkat,write,writev,unlink,unlinkat'));
    assert.ok(args.includes('inject=rename:signal=SIGKILL:when=65535'));
    assert.deepEqual(args.slice(-2), ['--', 'true']);
  } finally { rmSync(root, { recursive: true, force: true }); }
});
