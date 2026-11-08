/** Prove review safety classes with passing controls and independent disposable mutations. */
import assert from 'node:assert/strict';
import { mkdtempSync, readFileSync, symlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { assertNamedTests, checkedGit, copyWorkingTree, escapeTestName } from './context-revert-helpers.mts';
import { ProofProcesses, verifyAsyncMutation } from './context-proof-process.mts';

const source = process.cwd();
const scratch = mkdtempSync(join(tmpdir(), 'pm-context-safety-revert-'));
const processes = new ProofProcesses();
try {
  copyWorkingTree(source, scratch);
  checkedGit(scratch, ['init', '--quiet'], 'scratch source enumeration');
  for (const oracle of ['oracle', 'orientation-oracle']) symlinkSync(join(source, 'tests', oracle, 'node_modules'), join(scratch, 'tests', oracle, 'node_modules'), 'junction');
  const nodeFile = 'scripts/context-review-safety.test.mts';
  const cases: { name: string; path: string; before: string; after: string; test: string; rust?: string[]; multiple?: boolean }[] = [
    { name: 'JSON snapshots', path: 'scripts/context-real-helpers.mts', before: " || entry.name.endsWith('.json')", after: '', test: 'JSON snapshot additions removals and changes are authoritative in every custom directory' },
    { name: 'oracle provenance', path: 'scripts/context-real-helpers.mts', before: 'result.stdout.trim() !== expected', after: 'false', test: 'real-copy oracle provenance refuses mismatched missing failed or noisy driver versions before side effects' },
    { name: 'Windows selection call', path: 'scripts/context-review-revert.mts', before: 'supportedProofs(cases, process.argv[2])', after: 'cases', test: 'Windows signal proof selection preserves every supported case and reports skips' },
    { name: 'Windows selection', path: 'scripts/context-revert-helpers.mts', before: "platform !== 'win32'", after: 'true', test: 'Windows signal proof selection preserves every supported case and reports skips' },
    ...['context-revert-helpers.mts', 'native-adapter-revert.mts', 'native-adapter-capabilities-revert.mts', 'native-adapter-capabilities-review-revert.mts', 'native-adapter-changes-revert.mts', 'perf-revert-proof.mts'].map(file => ({ name: `symlink isolation: ${file}`, path: `scripts/${file}`, before: file === 'native-adapter-changes-revert.mts' ? 'dereference:true' : 'dereference: true', after: 'dereference:false', test: 'proof copies materialize absolute relative and directory symlinks before any mutation', multiple: file === 'native-adapter-changes-revert.mts' })),
    { name: 'dependency junction exclusion', path: 'scripts/context-revert-helpers.mts', before: "includes('node_modules')", after: "includes('excluded_dependencies')", multiple: true, test: 'proof copies materialize absolute relative and directory symlinks before any mutation' },
    { name: 'descendant groups', path: 'scripts/context-proof-process.mts', before: 'process.kill(-pid, signal)', after: 'process.kill(pid, signal)', test: 'all proof runners stop descendants before cleanup: context-intents-revert.mts SIGTERM' },
    { name: 'Windows descendant trees', path: 'scripts/context-proof-process.mts', before: "'/T', '/F'", after: "'/F'", test: 'Windows proof interruption terminates the complete tree or retains scratch inputs' },
    { name: 'SDK refusal warnings', path: 'src/orientation_context.rs', before: '                if settings_fallback {', after: '                if false {', test: 'orientation_context::tests::context_settings_warning_survives_refusals', rust: ['--lib'] },
    { name: 'CLI refusal warnings', path: 'src/main.rs', before: 'attach_settings_warning(&mut payload, settings_warning);', after: 'attach_settings_warning(&mut payload, None);', multiple: true, test: 'context_invalid_stored_activity_limits_match_current_oracle', rust: ['--test', 'orientation_differential'] },
  ];
  let executed = 0;
  for (const proof of cases) {
    if (process.argv[2] && !proof.name.includes(process.argv[2])) continue;
    if (process.platform === 'win32' && proof.name === 'descendant groups') { console.log('Skipped Unix descendant proof; run on a Unix host'); continue; }
    const path = join(scratch, proof.path);
    const fixed = readFileSync(path, 'utf8');
    assert.ok(fixed.includes(proof.before), `stale mutation: ${proof.name}`);
    if (!proof.multiple) assert.equal(fixed.split(proof.before).length, 2, `unique mutation: ${proof.name}`);
    const args = proof.rust
      ? ['test', '-j', '2', '--locked', ...proof.rust, proof.test, '--', '--exact', '--test-threads=2']
      : ['--test', '--test-reporter=tap', '--test-concurrency=2', `--test-name-pattern=^${escapeTestName(proof.test)}$`, nodeFile];
    await verifyAsyncMutation(async reverted => {
      const outcome = await processes.run(proof.rust ? 'cargo' : process.execPath, args, {
        cwd: scratch, timeout: 900_000,
        env: { ...process.env, CARGO_BUILD_JOBS: '2', RUST_TEST_THREADS: '2', CARGO_TARGET_DIR: join(scratch, 'target'), PM_RUST_REQUIRE_PUBLISHED_CLI: '1' },
      });
      assertNamedTests(outcome, [proof.test], reverted, !proof.rust);
      console.log(JSON.stringify({ proof: proof.name, test: proof.test, reverted, result: reverted ? 'named assertion failure' : 'pass' }));
    }, () => writeFileSync(path, fixed.replaceAll(proof.before, proof.after)), () => writeFileSync(path, fixed));
    executed++;
  }
  assert.ok(executed > 0, 'proof filter selected no cases');
  console.log(JSON.stringify({ proved: executed, restored: true }));
} catch (error) { if (!processes.wasInterrupted) throw error; }
finally { await processes.cleanup(scratch); }
