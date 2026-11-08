import { spawnSync } from 'node:child_process';

/** Run Git without discarding the diagnostic that explains a failed revert or source scan. */
export function checkedGit(root: string, args: string[], message: string): string {
  const result = spawnSync('git', args, { cwd: root, encoding: 'utf8' });
  if (result.error || result.status !== 0) {
    throw new Error(`${message}: ${result.error?.message ?? ''}${result.stderr ?? ''}`);
  }
  return result.stdout;
}
