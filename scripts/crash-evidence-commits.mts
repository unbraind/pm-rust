import { spawnSync } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

/** A receipt's exact commit field, retained for actionable failures. */
export interface ReceiptCommit { receipt: string; field: string; commit: string }

/** Discover every commit field in a JSON receipt, including nested provenance. */
export function receiptCommits(value: unknown, receipt: string, field = ''): ReceiptCommit[] {
  if (!value || typeof value !== 'object') return [];
  return Object.entries(value).flatMap(([key, child]) => {
    const location = field ? `${field}.${key}` : key;
    const commitUrl = typeof child === 'string' ? child.match(/^https:\/\/github\.com\/unbraind\/pm-rust\/commit\/([a-f0-9]{40})(?:[/?#]|$)/)?.[1] : undefined;
    if (/(?:commit|head|reverted_to|rust_behavior_proved_at)$/i.test(key) || commitUrl) {
      if (!commitUrl && (typeof child !== 'string' || !/^[a-f0-9]{40}$/.test(child))) throw new Error(`evidence_commit_invalid: ${receipt} ${location}`);
      return [{ receipt, field: location, commit: commitUrl ?? child as string }];
    }
    return receiptCommits(child, receipt, location);
  });
}

/** Verify remote availability and squash-safe retention without borrowing local objects. */
export function verifyEvidenceCommits(directory: string, additional: ReceiptCommit[] = []): { receipts: number; commits: number; retainedBy: Record<string, string> } {
  const env = { ...process.env };
  for (const name of ['GIT_DIR', 'GIT_WORK_TREE', 'GIT_COMMON_DIR', 'GIT_INDEX_FILE', 'GIT_OBJECT_DIRECTORY', 'GIT_ALTERNATE_OBJECT_DIRECTORIES']) delete env[name];
  /** Run a bounded Git command; never publish its potentially private diagnostics. */
  function git(cwd: string, args: string[]): ReturnType<typeof spawnSync> {
    return spawnSync('git', args, { cwd, env, timeout: 120000, maxBuffer: 8 * 1024 * 1024 });
  }
  if (git(directory, ['rev-parse', '--show-toplevel']).status !== 0) throw new Error('evidence-commits needs a git repository and Git executable; plain tracker operations still work without git.');
  const listing = git(directory, ['ls-files', '-z', '--', 'docs']);
  if (listing.status !== 0) throw new Error('evidence-commits needs readable tracked docs.');
  const references = listing.stdout!.toString().split('\0').filter(path => path.endsWith('.json')).flatMap(receipt =>
    receiptCommits(JSON.parse(readFileSync(join(directory, receipt), 'utf8')), receipt)).concat(additional);
  if (!references.length) throw new Error('evidence_commit_empty: no receipt commit fields found');
  const origin = git(directory, ['remote', 'get-url', 'origin']);
  if (origin.status !== 0) throw new Error('evidence-commits needs an origin remote for a fresh full clone.');
  const root = mkdtempSync(join(tmpdir(), 'pm-evidence-commits-'));
  try {
    const clone = join(root, 'clone');
    if (git(root, ['clone', '--no-local', '--no-checkout', '--no-tags', '--', origin.stdout!.toString().trim(), clone]).status !== 0) throw new Error('evidence-commits could not create a fresh full origin clone.');
    const branches = git(clone, ['for-each-ref', '--format=%(refname)', 'refs/remotes/origin/main', 'refs/remotes/origin/evidence/']);
    if (branches.status !== 0) throw new Error('evidence-commits could not inspect retained origin branches.');
    const retained = branches.stdout!.toString().trim().split('\n').filter(Boolean).sort((a, b) => Number(b.endsWith('/main')) - Number(a.endsWith('/main')));
    const retainedBy: Record<string, string> = {};
    for (const reference of references) {
      const label = `${reference.receipt} ${reference.field} ${reference.commit}`;
      if (git(clone, ['cat-file', '-e', `${reference.commit}^{commit}`]).status !== 0) throw new Error(`evidence_commit_unresolvable: ${label}; preserve the source on an evidence/<name> branch before recording.`);
      const branch = retained.find(ref => git(clone, ['merge-base', '--is-ancestor', reference.commit, ref]).status === 0);
      if (!branch) throw new Error(`evidence_commit_unretained: ${label}; source must be on main or an evidence/<name> branch, never only a PR branch or tag.`);
      retainedBy[reference.commit] = branch.replace('refs/remotes/origin/', '');
    }
    return { receipts: new Set(references.map(row => row.receipt)).size, commits: references.length, retainedBy };
  } finally { rmSync(root, { recursive: true, force: true }); }
}

/** Run the release gate with receipt-specific failures and sanitized diagnostics. */
function main(): void {
  try { console.log(JSON.stringify(verifyEvidenceCommits(process.cwd()))); }
  catch (error) { console.error(error instanceof Error ? error.message : 'evidence commit verification failed'); process.exitCode = 1; }
}
if (process.argv[1] && pathToFileURL(resolve(process.argv[1])).href === import.meta.url) main();
