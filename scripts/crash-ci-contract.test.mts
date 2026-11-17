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

for (const role of ['allowed', 'committer', 'author'] as const) {
  test(`docs identity gate independently inspects unreachable ${role} identities`, () => {
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
      git(['hash-object', '-t', 'commit', '-w', '--stdin'],
        `tree ${tree}\nauthor ${role === 'author' ? foreign : allowed}\ncommitter ${role === 'committer' ? foreign : allowed}\n\nSynthetic identity fixture\n`);
      if (role === 'allowed') assert.equal(auditIdentities(root), 1);
      else assert.throws(() => auditIdentities(root), role === 'author' ? /Unapproved author/ : /Unapproved committer/);
    } finally { rmSync(root, { recursive: true, force: true }); }
  });
}


test('harness watchdogs validate scales before launching and preserve integer milliseconds', () => {
  const module = new URL('./crash-test-depth.mts', import.meta.url).href;
  for (const [scale, expected] of [['', '101'], ['1', '101'], ['3', '303'], ['1.25', '127']]) {
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
  const root = mkdtempSync(join(tmpdir(), 'ci strace-'));
  const prefix = join(root, 'syscall-tools');
  const githubPath = join(root, 'github-path');
  const argsPath = join(root, 'probe-args');
  try {
    mkdirSync(join(prefix, 'usr/bin'), { recursive: true });
    mkdirSync(join(prefix, 'bin'));
    const stub = `#!/bin/sh
if [ "$1" = -V ]; then exit 0; fi
printf '%s\\n' "$@" > "$STRACE_PROBE_ARGS"
[ "$LD_LIBRARY_PATH" = "$STRACE_EXPECTED_LIBRARY_PATH" ] || exit 2
case "$STRACE_PROBE_MODE" in
  old) for flag do if [ "$flag" = --kill-on-exit ]; then echo 'unrecognized option --kill-on-exit' >&2; exit 1; fi; done ;;
  denied) echo 'ptrace denied' >&2; exit 1 ;;
esac
`;
    writeFileSync(join(prefix, 'usr/bin/strace'), stub, { mode: 0o755 });
    for (const mode of ['old', 'denied', 'supported']) {
      writeFileSync(githubPath, '');
      const result = spawnSync('bash', ['.github/scripts/setup-local-strace.sh'], {
        encoding: 'utf8', env: { ...process.env, RUNNER_TOOL_CACHE: root, RUNNER_TEMP: root, GITHUB_PATH: githubPath,
          STRACE_PROBE_ARGS: argsPath, STRACE_PROBE_MODE: mode,
          STRACE_EXPECTED_LIBRARY_PATH: join(prefix, 'usr/lib', execFileSync('gcc', ['-dumpmachine'], { encoding: 'utf8' }).trim()) },
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

test('hosted tools match local pins and quick/deep jobs only install through rust-setup', () => {
  const action = readFileSync('.github/actions/rust-setup/action.yml', 'utf8');
  const local = readFileSync('.github/scripts/setup-local-rust.sh', 'utf8');
  for (const [tool, version] of [['just', '1.43.0'], ['cargo-audit', '0.23.0'], ['cargo-llvm-cov', '0.8.7']]) {
    assert.ok(action.includes(`tool: ${tool}@${version}\n`), `${tool} hosted pin`);
    assert.ok(local.includes(`install --locked ${tool} --version ${version}`), `${tool} local pin`);
  }
  for (const workflow of ['ci.yml', 'ci-deep.yml']) {
    const ci = readFileSync(`.github/workflows/${workflow}`, 'utf8');
    assert.ok(ci.includes('uses: ./.github/actions/rust-setup'));
    assert.ok(!ci.includes('uses: taiki-e/install-action'), `${workflow} must not overwrite tools outside rust-setup`);
  }
});

test('local installers create a cold tool root and share a bounded lock with clear refusal', { skip: process.platform !== 'linux' }, () => {
  const root = mkdtempSync(join(tmpdir(), 'ci-tool-lock-'));
  const toolRoot = join(root, 'cold-tools');
  const helper = join(process.cwd(), '.github/scripts/setup-local-tool-root.sh');
  const env = { ...process.env, RUNNER_TOOL_CACHE: toolRoot, RUNNER_TOOL_LOCK_TIMEOUT_SECONDS: '0' };
  const script = 'source "$1"; (lock_runner_tools; printf "%s" "$tool_root") 9>"$tool_root/rust.lock"';
  try {
    const cold = spawnSync('bash', ['-euc', script, 'fixture', helper], { encoding: 'utf8', env });
    assert.equal(cold.status, 0, cold.stderr);
    assert.equal(cold.stdout, toolRoot);
    const held = spawnSync('flock', [join(toolRoot, 'rust.lock'), 'bash', '-euc', script, 'fixture', helper],
      { encoding: 'utf8', env, timeout: 5000 });
    assert.equal(held.status, 1, held.stderr);
    assert.match(held.stderr, /Timed out waiting for the shared runner tool installation lock/);
    for (const installer of ['rust', 'strace']) {
      const source = readFileSync(`.github/scripts/setup-local-${installer}.sh`, 'utf8');
      assert.ok(source.includes('/setup-local-tool-root.sh"'));
      assert.ok(source.includes('  lock_runner_tools\n'));
      assert.ok(source.includes(') 9>"$tool_root/rust.lock"'));
    }
  } finally { rmSync(root, { recursive: true, force: true }); }
});
