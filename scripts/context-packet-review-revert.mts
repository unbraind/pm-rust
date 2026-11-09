import { ProofProcesses, verifyAsyncMutation } from './context-proof-process.mts';
import { assertNamedTests, copyWorkingTree, checkedGit } from './context-revert-helpers.mts';
import { mkdtempSync, readFileSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

/** Require the focused regression to pass, or fail at its named assertion under mutation. */
async function check(root: string, name: string, reverted: boolean): Promise<void> {
  const command = ['test', '-j', '2', '--locked', '--lib', name];
  const result = await processes.run('cargo', command, {
    cwd: root, maxBuffer: 16 * 1024 * 1024,
    env: { ...process.env, CARGO_BUILD_JOBS: '2', RUST_TEST_THREADS: '2', CARGO_TARGET_DIR: join(root, 'target') },
  });
  assertNamedTests(result, [`orientation_deps::packet::tests::${name}`], reverted);
  console.log(JSON.stringify({ command: ['cargo', ...command], reverted, result: reverted ? 'named assertion failure' : 'pass' }));
}

const source = process.cwd();
const scratch = mkdtempSync(join(tmpdir(), 'pm-rust-packet-review-'));
const processes = new ProofProcesses();
try {
  copyWorkingTree(source, scratch);
  const path = join(scratch, 'src/orientation_deps/packet.rs');
  const fixed = readFileSync(path, 'utf8');
  const mutations = [
    {
      name: 'packet_direct_reservations_are_reused',
      before: 'let edge_cost = traversal.reservations.get(id).copied().unwrap_or_default();',
      after: `let edge_cost: usize = direct.iter()
        .filter(|(_, edge)| text(edge, "source") == *id || text(edge, "target") == *id)
        .map(|(_, edge)| cost(edge)).sum();`,
    },
    {
      name: 'packet_render_cost_keys_are_bounded',
      before: 'if shape.len() <= (2 * 1024 * 1024usize).saturating_sub(cache.cost_bytes) {',
      after: 'if true {',
    },
  ];
  for (const mutation of mutations) {
    if (!fixed.includes(mutation.before)) throw new Error(`Cannot locate mutation for ${mutation.name}`);
    await verifyAsyncMutation(async reverted => check(scratch, mutation.name, reverted),
      () => writeFileSync(path, fixed.replace(mutation.before, mutation.after)),
      () => writeFileSync(path, fixed), processes);
  }
  const patch = join(scratch, 'tests/fixtures/orientation-packet-root-priority.patch');
  checkedGit(scratch, ['apply', '--reverse', '--check', patch], 'The retained packet behavior revert fixture no longer applies');
  console.log('Packet behavior revert fixture applies to the fixed implementation.');
} catch (error) {
  if (!processes.wasInterrupted) throw error;
} finally {
  await processes.cleanup(scratch);
}
