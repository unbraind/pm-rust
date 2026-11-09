import { spawnSync } from 'node:child_process';
import assert from 'node:assert/strict';
import { cpSync, existsSync, lstatSync, mkdirSync } from 'node:fs';
import { dirname, join } from 'node:path';

/** Minimal subprocess outcome accepted by both synchronous and bounded async runners. */
export interface TestOutcome {
  status: number | null;
  stdout: string | null;
  stderr: string | null;
  error?: Error;
}

/** Escape a literal test name for both subprocess selection and result validation. */
export function escapeTestName(name: string): string {
  return name.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

/** Require each named result, rejecting setup failures, skipped tests and unrelated failures. */
export function assertNamedTests(result: TestOutcome, names: string[], reverted: boolean, node = false): void {
  assert.ifError(result.error);
  const output = `${result.stdout ?? ''}\n${result.stderr ?? ''}`;
  assert.equal(result.status, reverted ? (node ? 1 : 101) : 0, `Unexpected test exit status\n${output}`);
  assert.ok(names.length > 0, 'A proof must select named tests');
  assert.doesNotMatch(output, /could not compile/);
  for (const name of names) {
    const escaped = escapeTestName(name);
    const line = node
      ? `${reverted ? 'not ok' : 'ok'} \\d+ - ${escaped}`
      : `test ${escaped} \\.\\.\\. ${reverted ? 'FAILED' : 'ok'}`;
    assert.match(output, new RegExp(`^${line}\\r?$`, 'm'), `Missing named ${reverted ? 'failure' : 'pass'}: ${name}`);
  }
}

/** Establish a passing control before mutation and rerun it after exact restoration. */
export function verifyMutation(check: (reverted: boolean) => void, mutate: () => void, restore: () => void): void {
  check(false);
  try {
    mutate();
    check(true);
  } finally {
    restore();
  }
  check(false);
}

/** Run Git without discarding the diagnostic that explains a failed revert or source scan. */
export function checkedGit(root: string, args: string[], message: string): string {
  const result = spawnSync('git', args, { cwd: root, encoding: 'utf8' });
  if (result.error || result.status !== 0) {
    throw new Error(`${message}: ${result.error?.message ?? ''}${result.stderr ?? ''}`);
  }
  return result.stdout;
}

/** Treats dangling symlinks like absent entries, tolerating unrelated broken links. */
export function absentLink(source: string): boolean {
  const entry = lstatSync(source, { throwIfNoEntry: false });
  return entry?.isSymbolicLink() === true && !existsSync(source);
}

/** Copy the live worktree, including untracked files and excluding unstaged deletions. */
export function copyWorkingTree(source: string, scratch: string): void {
  const files = checkedGit(source, ['ls-files', '--cached', '--others', '--exclude-standard', '-z'], 'Cannot enumerate package source');
  for (const file of new Set(files.split('\0').filter(Boolean))) {
    if (file.split(/[\\/]/).includes('node_modules')) continue;
    const input = join(source, file);
    if (!lstatSync(input, { throwIfNoEntry: false }) || absentLink(input)) continue;
    const target = join(scratch, file);
    mkdirSync(dirname(target), { recursive: true });
    cpSync(input, target, { dereference: true, recursive: true, filter: path => !path.split(/[\\/]/).includes('node_modules') && !absentLink(path) });
  }
}

/** Filter platform-specific proof contracts before baseline checks, with an explicit skip receipt. */
export function supportedProofs<T extends { test: string; name?: string; unixOnly?: boolean }>(proofs: T[], selected?: string, platform: NodeJS.Platform = process.platform, report: (message: string) => void = console.log): T[] {
  return proofs.filter(proof => {
    if (selected && !(proof.name ?? proof.test).includes(selected)) return false;
    if (platform !== 'win32' || !(proof.unixOnly || /\bSIG(?:INT|TERM)\b/.test(proof.test))) return true;
    const kind = proof.unixOnly ? (/symlink/.test(proof.test) ? 'symlink' : 'process') : 'signal';
    if (selected) throw new Error(`Selected ${kind} proof requires a Unix host`);
    report(JSON.stringify({ skipped: proof.name ?? proof.test, reason: `${kind} proof requires a Unix host` }));
    return false;
  });
}
