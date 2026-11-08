import { spawnSync } from 'node:child_process';
import assert from 'node:assert/strict';

/** Minimal subprocess outcome accepted by both synchronous and bounded async runners. */
export interface TestOutcome {
  status: number | null;
  stdout: string | null;
  stderr: string | null;
  error?: Error;
}

/** Require each named result, rejecting setup failures, skipped tests and unrelated failures. */
export function assertNamedTests(result: TestOutcome, names: string[], reverted: boolean, node = false): void {
  assert.ifError(result.error);
  const output = `${result.stdout ?? ''}\n${result.stderr ?? ''}`;
  assert.equal(result.status, reverted ? (node ? 1 : 101) : 0, `Unexpected test exit status\n${output}`);
  assert.ok(names.length > 0, 'A proof must select named tests');
  assert.doesNotMatch(output, /could not compile/);
  for (const name of names) {
    const escaped = name.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
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
