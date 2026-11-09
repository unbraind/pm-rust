import { ProofProcesses } from './context-proof-process.mts';
import { createHash } from 'node:crypto';
import { cpSync, existsSync, lstatSync, mkdirSync, mkdtempSync, readFileSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const sourceRoot = join(dirname(fileURLToPath(import.meta.url)), '..');
const repo = mkdtempSync(join(tmpdir(), 'pm-perf-revert-'));
const processes = new ProofProcesses();
try {
for (const name of ['Cargo.toml', 'Cargo.lock', 'build.rs', 'src', 'tests', 'examples']) {
  cpSync(join(sourceRoot, name), join(repo, name), { recursive: true, dereference: true, filter: path => !path.split(/[\\/]/).includes('node_modules') && !(lstatSync(path).isSymbolicLink() && !existsSync(path)) });
}
const ownershipPath = join(repo, 'src/ownership.rs');
const mutationPath = join(repo, 'src/mutation.rs');
const originalOwnership = readFileSync(ownershipPath, 'utf8');
const originalMutation = readFileSync(mutationPath, 'utf8');
const lineage = '    let parents = workspace.read_parent_index()?;';
const materialize = `    let parents: BTreeMap<String, Option<String>> = workspace
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
if (originalOwnership.split(lineage).length !== 2 || originalMutation.split(fastPath).length !== 2) {
  throw new Error('Revert proof does not match the candidate functions; no files changed');
}

const cases = [
  { name: 'ownership::tests::ownership_lineage_scans_corpus_once', failure: 'lineage retained the full corpus payloads' },
  { name: 'mutation::tests::locator_metadata_work_depends_on_directories_and_targets', failure: 'unrelated files reached live metadata checks' },
];
const results: { test: string; mode: string; command: string; status: number | null; expected_result: boolean }[] = [];
/** Execute a real regression, distinguishing its assertion from compiler or tool failures. */
async function run(test: { name: string; failure: string }, reverted: boolean): Promise<void> {
  const args = ['+1.90.0', 'test', '-j', '2', '--quiet', '--locked', '--lib', test.name, '--', '--exact', '--test-threads=2'];
  const child = await processes.run('cargo', args, { cwd: repo, env: { ...process.env, CARGO_BUILD_JOBS: '2', RUST_TEST_THREADS: '2', CARGO_TARGET_DIR: join(repo, 'target') } });
  const output = child.stdout ?? '';
  const expected = !child.error && (reverted
    ? child.status === 101 && output.includes(test.failure) && output.includes('0 passed; 1 failed;')
    : child.status === 0 && output.includes('1 passed; 0 failed;'));
  results.push({ test: test.name, mode: reverted ? 'behavior reverted' : 'restored', command: `cargo ${args.join(' ')}`, status: child.status, expected_result: expected });
  console.log(JSON.stringify(results.at(-1)));
  if (!expected) throw new Error(`Unexpected ${reverted ? 'negative' : 'positive'} control result for ${test.name}`);
}

/** Restore the exact original bytes even after a failing assertion or tool invocation. */
function restore(): void {
  writeFileSync(ownershipPath, originalOwnership);
  writeFileSync(mutationPath, originalMutation);
}

try {
  for (const test of cases) await run(test, false);

  try {
    // Keep the test observations and all safety checks. Revert only the two
    // optimized behaviors: full payload retention and per-entry metadata work.
    writeFileSync(ownershipPath, originalOwnership.replaceAll(lineage, materialize));
    writeFileSync(mutationPath, originalMutation.replace(fastPath, ''));
    for (const test of cases) await run(test, true);
  } finally {
    await processes.settled();
    restore();
  }
  for (const test of cases) await run(test, false);
  if (readFileSync(ownershipPath, 'utf8') !== originalOwnership || readFileSync(mutationPath, 'utf8') !== originalMutation) {
    throw new Error('Candidate source bytes were not restored');
  }
  mkdirSync(join(sourceRoot, 'target'), { recursive: true });
  writeFileSync(join(sourceRoot, 'target/native-revert-proof.json'), `${JSON.stringify({
    complete: true, candidate_commit: process.env.BENCH_HEAD_SHA ?? null,
    source_restored: true, source_sha256: {
      ownership: createHash('sha256').update(originalOwnership).digest('hex'),
      mutation: createHash('sha256').update(originalMutation).digest('hex'),
    }, results,
  }, null, 2)}\n`);

} finally {
  await processes.settled();
  restore();
}
} catch (error) {
  if (!processes.wasInterrupted) throw error;
} finally { await processes.cleanup(repo); }
