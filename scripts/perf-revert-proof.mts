import { execFile, type ChildProcess } from 'node:child_process';
import { createHash } from 'node:crypto';
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const repo = join(dirname(fileURLToPath(import.meta.url)), '..');
const ownershipPath = join(repo, 'src/ownership.rs');
const mutationPath = join(repo, 'src/mutation.rs');
const originalOwnership = readFileSync(ownershipPath, 'utf8');
const originalMutation = readFileSync(mutationPath, 'utf8');
const lineage = '        let parents = self.read_parent_index()?;';
const materialize = `        let parents: BTreeMap<String, Option<String>> = self
        .read_items()?
        .into_iter()
        .map(|item| (item.metadata.id, item.metadata.parent))
        .collect();`;
const fastPath = `        let name = entry.file_name().to_string_lossy().into_owned();
        // The listing already knows these are unrelated regular files. Keep
        // live metadata checks for targets, directories, symlinks and unknown
        // types, including a target moved after this directory snapshot.
        if name != target && entry.file_type().is_ok_and(|kind| kind.is_file()) {
            continue;
        }
`;
if (originalOwnership.split(lineage).length !== 3 || originalMutation.split(fastPath).length !== 2) {
  throw new Error('Revert proof does not match the candidate functions; no files changed');
}

const cases = [
  { name: 'ownership::tests::ownership_lineage_scans_corpus_once', failure: 'lineage retained the full corpus payloads' },
  { name: 'mutation::tests::locator_metadata_work_depends_on_directories_and_targets', failure: 'unrelated files reached live metadata checks' },
];
const results: { test: string; mode: string; command: string; status: number | null; expected_result: boolean }[] = [];
let activeChild: ChildProcess | undefined;

/** Execute a real regression, distinguishing its assertion from compiler or tool failures. */
async function run(test: { name: string; failure: string }, reverted: boolean): Promise<void> {
  const args = ['+1.90.0', 'test', '--quiet', '--locked', '--lib', test.name];
  const child = await new Promise<{ status: number; stdout: string }>((resolve, reject) => {
    activeChild = execFile('cargo', args, { cwd: repo, encoding: 'utf8', maxBuffer: 16 * 1024 * 1024 }, (error, stdout) => {
      activeChild = undefined;
      if (error?.signal === 'SIGINT' || error?.signal === 'SIGTERM') interrupt(error.signal);
      if (error && (typeof error.code !== 'number' || error.signal || error.killed)) {
        reject(new Error('Revert-control process could not complete'));
      } else {
        resolve({ status: error ? Number(error.code) : 0, stdout });
      }
    });
  });
  const expected = reverted
    ? child.status === 101 && child.stdout.includes(test.failure) && child.stdout.includes('0 passed; 1 failed;')
    : child.status === 0 && child.stdout.includes('1 passed; 0 failed;');
  results.push({ test: test.name, mode: reverted ? 'behavior reverted' : 'restored', command: `cargo ${args.join(' ')}`, status: child.status, expected_result: expected });
  console.log(JSON.stringify(results.at(-1)));
  if (!expected) throw new Error(`Unexpected ${reverted ? 'negative' : 'positive'} control result for ${test.name}`);
}

/** Restore the exact original bytes even after a failing assertion or tool invocation. */
function restore(): void {
  writeFileSync(ownershipPath, originalOwnership);
  writeFileSync(mutationPath, originalMutation);
}

/** Restore and exit consistently whichever side of an interrupt is observed first. */
function interrupt(signal: 'SIGINT' | 'SIGTERM'): never {
  activeChild?.kill(signal);
  restore();
  process.exit(128 + (signal === 'SIGINT' ? 2 : 15));
}

for (const signal of ['SIGINT', 'SIGTERM'] as const) {
  process.on(signal, () => interrupt(signal));
}

try {
  // Keep the test observations and all safety checks. Revert only the two
  // optimized behaviors: full payload retention and per-entry metadata work.
  writeFileSync(ownershipPath, originalOwnership.replaceAll(lineage, materialize));
  writeFileSync(mutationPath, originalMutation.replace(fastPath, ''));
  for (const test of cases) await run(test, true);
} finally {
  restore();
}
for (const test of cases) await run(test, false);
if (readFileSync(ownershipPath, 'utf8') !== originalOwnership || readFileSync(mutationPath, 'utf8') !== originalMutation) {
  throw new Error('Candidate source bytes were not restored');
}
mkdirSync(join(repo, 'target'), { recursive: true });
writeFileSync(join(repo, 'target/native-revert-proof.json'), `${JSON.stringify({
  complete: true, candidate_commit: process.env.BENCH_HEAD_SHA ?? null,
  source_restored: true, source_sha256: {
    ownership: createHash('sha256').update(originalOwnership).digest('hex'),
    mutation: createHash('sha256').update(originalMutation).digest('hex'),
  }, results,
}, null, 2)}\n`);
