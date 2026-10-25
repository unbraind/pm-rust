import { spawnSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { parseArgs } from 'node:util';

/** A complete comparison of every changed history path with the selected main tree. */
export interface HistoryPrefixReceipt { ok: true; base: string; checked: number; branchOnly: number }

/** Compare raw bytes, including deleted paths; never normalize or rewrite a history stream. */
export function verifyHistoryPrefixes(directory: string, base = 'origin/main'): HistoryPrefixReceipt {
  const env = { ...process.env };
  for (const name of ['GIT_DIR', 'GIT_WORK_TREE', 'GIT_COMMON_DIR', 'GIT_INDEX_FILE']) delete env[name];
  const located = spawnSync('git', ['rev-parse', '--show-toplevel'], { cwd: directory, encoding: 'utf8', env });
  if (located.status !== 0) throw new Error('crash-history-prefix needs a git repository and an available Git executable; tracker reads and writes still work without git.');
  const root = located.stdout.trim();
  /** Read one bounded Git result without invoking shell expansion or external diff helpers. */
  function git(args: string[]): Buffer {
    const result = spawnSync('git', args, { cwd: root, env, maxBuffer: 8 * 1024 * 1024 });
    if (result.status !== 0) throw new Error('history_prefix_git_refused: supply an available base tree with --base and readable Git objects.');
    return result.stdout;
  }
  git(['rev-parse', '--verify', '--end-of-options', `${base}^{tree}`]);
  const paths = git(['diff', '--no-ext-diff', '--no-textconv', '--no-renames', '--name-only', '-z', base, '--', '.agents/pm/history']).toString().split('\0').filter(Boolean);
  let checked = 0; let branchOnly = 0;
  for (const path of paths) {
    if (git(['ls-tree', '-z', base, '--', path]).length === 0) { branchOnly++; continue; }
    const original = git(['show', `${base}:${path}`]);
    let current: Buffer;
    try { current = readFileSync(join(root, path)); }
    catch { throw new Error(`history_prefix_missing: ${path}; keep main's history file and append new events.`); }
    if (!current.subarray(0, original.length).equals(original)) throw new Error(`history_prefix_replaced: ${path}; restore the main item/history pair and re-apply intended changes through pm commands.`);
    checked++;
  }
  return { ok: true, base, checked, branchOnly };
}

/** Run the repository-only gate and preserve an actionable missing-Git refusal. */
function main(): void {
  try {
    const { values } = parseArgs({ options: { base: { type: 'string', default: 'origin/main' } } });
    console.log(JSON.stringify(verifyHistoryPrefixes(process.cwd(), values.base)));
  } catch (error) {
    console.error(error instanceof Error ? error.message : 'history-prefix comparison failed; check the Git base and tracker files.');
    process.exitCode = 1;
  }
}
if (process.argv[1] && pathToFileURL(resolve(process.argv[1])).href === import.meta.url) main();
