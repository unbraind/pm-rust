import { spawn, execFileSync, type ChildProcess } from 'node:child_process';
import { createHash } from 'node:crypto';
import { cpSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { setTimeout } from 'node:timers/promises';

const sourceRoot = join(dirname(fileURLToPath(import.meta.url)), '..');
const repo = mkdtempSync(join(tmpdir(), 'pm-perf-revert-'));
const ownedGroups = new Set<number>();
process.on('exit', () => {
  // A failed drain retains the scratch tree rather than removing live compiler inputs.
  if (ownedGroups.size === 0) rmSync(repo, { recursive: true, force: true });
});
for (const name of ['Cargo.toml', 'Cargo.lock', 'build.rs', 'src', 'tests', 'examples']) {
  cpSync(join(sourceRoot, name), join(repo, name), { recursive: true, filter: path => !path.split(/[\\/]/).includes('node_modules') });
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
let activeChild: ChildProcess | undefined;
let interrupted: 'SIGINT' | 'SIGTERM' | undefined;
let escalation: ReturnType<typeof globalThis.setTimeout> | undefined;

/** Signal the complete owned Cargo process group, including compiler descendants. */
function stopGroup(pid: number, signal: NodeJS.Signals): void {
  try { process.kill(-pid, signal); } catch (error) {
    if ((error as NodeJS.ErrnoException).code !== 'ESRCH') throw error;
  }
}

/** Wait until owned workers have stopped; zombies have no executable work or open files. */
async function drainGroups(): Promise<void> {
  for (const pid of ownedGroups) {
    stopGroup(pid, 'SIGTERM');
    const started = performance.now();
    for (;;) {
      const processes = execFileSync('ps', ['-eo', 'pgid=,stat='], { encoding: 'utf8' });
      const live = processes.split('\n').some(row => {
        const [group, state] = row.trim().split(/\s+/);
        return Number(group) === pid && !state.startsWith('Z');
      });
      if (!live) break;
      if (performance.now() - started >= 1000) stopGroup(pid, 'SIGKILL');
      if (performance.now() - started >= 5000) throw new Error('Owned Cargo workers did not stop; scratch retained');
      await setTimeout(10);
    }
    ownedGroups.delete(pid);
  }
}

/** Execute a real regression, distinguishing its assertion from compiler or tool failures. */
async function run(test: { name: string; failure: string }, reverted: boolean): Promise<void> {
  if (interrupted) throw new Error('Revert proof interrupted before the next invocation');
  const args = ['+1.90.0', 'test', '-j', '2', '--quiet', '--locked', '--lib', test.name, '--', '--exact', '--test-threads=2'];
  const child = await new Promise<{ status: number; stdout: string }>((resolve, reject) => {
    const invocation = spawn('cargo', args, { cwd: repo, detached: process.platform !== 'win32', env: { ...process.env, CARGO_BUILD_JOBS: '2', RUST_TEST_THREADS: '2', CARGO_TARGET_DIR: join(repo, 'target') }, stdio: ['ignore', 'pipe', 'pipe'] });
    activeChild = invocation;
    let stdout = '';
    let bytes = 0;
    let overflow = false;
    /** Bound compiler diagnostics while continuing to drain both pipes. */
    const receive = (chunk: Buffer, output: boolean): void => {
      bytes += chunk.length;
      if (bytes > 16 * 1024 * 1024) {
        overflow = true;
        interrupt('SIGTERM');
      } else if (output) stdout += chunk.toString();
    };
    invocation.stdout.on('data', (chunk: Buffer) => receive(chunk, true));
    invocation.stderr.on('data', (chunk: Buffer) => receive(chunk, false));
    invocation.once('error', reject);
    invocation.once('close', (status, signal) => {
      if (signal === 'SIGINT' || signal === 'SIGTERM') interrupt(signal);
      if (signal || status === null || overflow) reject(new Error('Revert-control process could not complete'));
      else resolve({ status, stdout });
    });
    if (process.platform !== 'win32' && activeChild.pid !== undefined) ownedGroups.add(activeChild.pid);
  }).finally(async () => { await drainGroups(); activeChild = undefined; });
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
function interrupt(signal: 'SIGINT' | 'SIGTERM'): void {
  interrupted ??= signal;
  for (const pid of ownedGroups) stopGroup(pid, signal);
  if (process.platform === 'win32') activeChild?.kill(signal);
  escalation ??= globalThis.setTimeout(() => {
    for (const pid of ownedGroups) stopGroup(pid, 'SIGKILL');
    if (process.platform === 'win32') activeChild?.kill('SIGKILL');
  }, 1000);
  escalation.unref();
}

for (const signal of ['SIGINT', 'SIGTERM'] as const) {
  process.on(signal, () => interrupt(signal));
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

} catch (error) {
  if (!interrupted) throw error;
  process.exitCode = interrupted === 'SIGINT' ? 130 : 143;
} finally {
  await drainGroups();
  if (escalation) clearTimeout(escalation);
  restore();
}
