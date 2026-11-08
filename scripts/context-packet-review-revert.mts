import { assertNamedTests, checkedGit, verifyMutation } from './context-revert-helpers.mts';
import { spawnSync } from 'node:child_process';
import { cpSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';

/** Require the focused regression to pass, or fail at its named assertion under mutation. */
function check(root: string, name: string, reverted: boolean): void {
  const command = ['test', '-j', '2', '--locked', '--lib', name];
  const result = spawnSync('cargo', command, {
    cwd: root, encoding: 'utf8', maxBuffer: 16 * 1024 * 1024,
    env: { ...process.env, CARGO_BUILD_JOBS: '2', RUST_TEST_THREADS: '2', CARGO_TARGET_DIR: join(root, 'target') },
  });
  assertNamedTests(result, [`orientation_deps::packet::tests::${name}`], reverted);
  console.log(JSON.stringify({ command: ['cargo', ...command], reverted, result: reverted ? 'named assertion failure' : 'pass' }));
}

const source = process.cwd();
const scratch = mkdtempSync(join(tmpdir(), 'pm-rust-packet-review-'));
try {
  const files = checkedGit(source, ['ls-files', '--cached', '--others', '--exclude-standard', '-z'], 'Cannot enumerate package source');
  for (const file of new Set(files.split('\0').filter(Boolean))) {
    const target = join(scratch, file);
    mkdirSync(dirname(target), { recursive: true });
    cpSync(join(source, file), target);
  }
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
    verifyMutation(reverted => check(scratch, mutation.name, reverted),
      () => writeFileSync(path, fixed.replace(mutation.before, mutation.after)),
      () => writeFileSync(path, fixed));
  }
  const patch = join(scratch, 'tests/fixtures/orientation-packet-root-priority.patch');
  checkedGit(scratch, ['apply', '--reverse', '--check', patch], 'The retained packet behavior revert fixture no longer applies');
  console.log('Packet behavior revert fixture applies to the fixed implementation.');
} finally {
  rmSync(scratch, { recursive: true, force: true });
}
