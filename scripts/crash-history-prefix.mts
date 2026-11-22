import { spawnSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { join, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { isDeepStrictEqual, parseArgs } from 'node:util';

import type { RedactionApproval } from './crash-history-redaction.mts';
import type { HistoryEntry } from '../tests/oracle/node_modules/@unbrained/pm-cli/dist/types.js';

type HistoryRecord = Record<string, unknown>;
const require = createRequire(import.meta.url);
const oracle = fileURLToPath(new URL('../tests/oracle/node_modules/@unbrained/pm-cli/dist/cli.js', import.meta.url));
const policyPath = '.github/history-redactions.json';

/** Load approvals exclusively from a regular blob committed to the candidate's HEAD. */
function redactionApprovals(git: (args: string[]) => Buffer): Map<string, RedactionApproval> {
  const approvals = new Map<string, RedactionApproval>();
  const blob = git(['ls-tree', 'HEAD', '--', policyPath]).toString();
  if (!blob) return approvals;
  const invalid = new Error(`history_prefix_replaced: invalid redaction approval in ${policyPath}`);
  if (!blob.startsWith('100644 blob ')) throw invalid;
  let entries: unknown;
  try { entries = JSON.parse(git(['show', `HEAD:${policyPath}`]).toString()); }
  catch { throw invalid; }
  if (!Array.isArray(entries)) throw invalid;
  const fields = ['item', 'literal', 'replacement', 'reason', 'approved', 'author', 'marker_hash'];
  for (const rule of entries) {
    if (!rule || Object.keys(rule).length !== fields.length
        || !fields.every(key => typeof rule[key] === 'string' && rule[key].trim() !== '' && rule[key] === rule[key].trim())
        || !/^[a-z0-9][a-z0-9-]*$/.test(rule.item) || !/^[a-f0-9]{64}$/.test(rule.marker_hash)
        || rule.replacement.includes(rule.literal) || rule.literal.includes(rule.replacement)
        || approvals.has(rule.item)) throw invalid;
    approvals.set(rule.item, rule);
  }
  return approvals;
}

/** Accept only the hash-only re-anchor and retained-record evidence written by pm history-repair. */
function auditedReanchor(before: HistoryRecord[], after: HistoryRecord[]): boolean {
  const marker = after.at(-1)!;
  if (after.length !== before.length + 1 || marker.op !== 'history_repair' || marker.event_class !== 'maintenance'
      || typeof marker.message !== 'string' || typeof marker.author !== 'string' || !marker.author.trim()
      || typeof marker.ts !== 'string' || !Array.isArray(marker.patch)
      || marker.record_hash_version !== 1 || typeof marker.record_hash !== 'string' || !/^[a-f0-9]{64}$/.test(marker.record_hash)) return false;
  // This is buildHistoryRepairMessage's hash-only form; patch repairs are outside this exception.
  const named = /^history-repair re-anchored ([1-9]\d*) entr(?:y|ies)(?:, reconciled chain with on-disk item)?\.$/.exec(marker.message);
  if (!named) return false;
  let rewritten = 0;
  const payload = (entry: HistoryRecord): HistoryRecord => Object.fromEntries(Object.entries(entry)
    .filter(([key]) => !['before_hash', 'after_hash', 'record_hash', 'reanchor_evidence'].includes(key)));
  for (const [index, entry] of before.entries()) {
    const next = after[index];
    if (isDeepStrictEqual(entry, next)) continue;
    if (!isDeepStrictEqual(payload(entry), payload(next))) return false;
    const prior = entry.reanchor_evidence ?? [];
    const evidence = next.reanchor_evidence;
    if (!Array.isArray(prior) || !Array.isArray(evidence) || evidence.length !== prior.length + 1
        || !isDeepStrictEqual(evidence.slice(0, -1), prior)) return false;
    const retained = evidence.at(-1) as HistoryRecord | null;
    if (!retained || !isDeepStrictEqual(retained.record, entry)
        || !['before_hash', 'after_hash', 'record_hash', 'hash_algorithm', 'item_hash_version', 'record_hash_version']
          .every(key => isDeepStrictEqual(retained[key], entry[key]))) return false;
    if (entry.before_hash === next.before_hash && entry.after_hash === next.after_hash) return false;
    rewritten++;
  }
  return rewritten === Number(named[1]);
}

/** Verify the exact main prefix transformation, then have PM replay the complete candidate and paired item. */
function auditedRewrite(root: string, path: string, original: Buffer, current: Buffer, env: NodeJS.ProcessEnv,
  approvals: Map<string, RedactionApproval>): boolean {
  try {
    const records = (bytes: Buffer): (HistoryRecord & HistoryEntry)[] => bytes.toString('utf8').replace(/\n$/, '').split('\n').map(line => {
      const value: unknown = JSON.parse(line);
      if (value === null || typeof value !== 'object' || Array.isArray(value)) throw new Error('invalid history record');
      return value as HistoryRecord & HistoryEntry;
    });
    const before = records(original);
    const after = records(current);
    const item = path.slice('.agents/pm/history/'.length, -'.jsonl'.length);
    if (!auditedReanchor(before, after)) {
      const rule = approvals.get(item);
      if (!rule) return false;
      // The pinned oracle is required only when a prefix has actually been rewritten.
      const { matchesRedaction } = require('./crash-history-redaction.mts') as typeof import('./crash-history-redaction.mts');
      if (!matchesRedaction(before, after, rule, original, current)) return false;
    }
    // PM owns record sealing, retained-evidence integrity, full chain replay and item comparison.
    // Ignore inherited PM routing/configuration overrides so verification reads this exact pair.
    const verifierEnv = Object.fromEntries(Object.entries(env).filter(([name]) => !name.startsWith('PM_')));
    const result = spawnSync(process.execPath, [oracle, '--pm-path', join(root, '.agents/pm'), 'history',
      item, '--verify', '--strict-exit', '--limit', '1', '--json', '--no-extensions'],
    { cwd: root, env: { ...verifierEnv, PM_SENTRY_DISABLED: '1', PM_TELEMETRY_SOURCE_CONTEXT: 'test' },
      encoding: 'utf8', timeout: 30000, maxBuffer: 8 * 1024 * 1024 });
    if (result.status !== 0) return false;
    const { verification } = JSON.parse(result.stdout);
    return verification?.ok === true && verification.current_matches_latest === true && verification.entries === after.length;
  } catch { return false; }
}

/** A complete comparison of every changed history path with the selected main tree. */
export interface HistoryPrefixReceipt { ok: true; base: string; checked: number; branchOnly: number }

/** Compare raw bytes, with independently verified PM repair and approved redaction exceptions. */
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
  const approvals = redactionApprovals(git);
  const paths = git(['diff', '--no-ext-diff', '--no-textconv', '--no-renames', '--name-only', '-z', base, '--', '.agents/pm/history']).toString().split('\0').filter(Boolean);
  let checked = 0; let branchOnly = 0;
  for (const path of paths) {
    if (git(['ls-tree', '-z', base, '--', path]).length === 0) { branchOnly++; continue; }
    const original = git(['show', `${base}:${path}`]);
    let current: Buffer;
    try { current = readFileSync(join(root, path)); }
    catch { throw new Error(`history_prefix_missing: ${path}; keep main's history file and append new events.`); }
    if (!current.subarray(0, original.length).equals(original)
        && (!path.endsWith('.jsonl') || !auditedRewrite(root, path, original, current, env, approvals))) {
      throw new Error(`history_prefix_replaced: ${path}; restore the main item/history pair and re-apply intended changes through pm commands, or supply a verified hash-only pm history-repair re-anchor or an approved pm history-redact event.`);
    }
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
