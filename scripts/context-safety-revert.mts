/** Prove review safety classes with passing controls and independent disposable mutations. */
import assert from 'node:assert/strict';
import { mkdtempSync, readFileSync, symlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { assertNamedTests, checkedGit, copyWorkingTree, escapeTestName, supportedProofs } from './context-revert-helpers.mts';
import { ProofProcesses, verifyAsyncMutation } from './context-proof-process.mts';

const source = process.cwd();
const scratch = mkdtempSync(join(tmpdir(), 'pm-context-safety-revert-'));
const processes = new ProofProcesses();
try {
  copyWorkingTree(source, scratch);
  checkedGit(scratch, ['init', '--quiet'], 'scratch source enumeration');
  for (const oracle of ['oracle', 'orientation-oracle']) symlinkSync(join(source, 'tests', oracle, 'node_modules'), join(scratch, 'tests', oracle, 'node_modules'), 'junction');
  const nodeFile = 'scripts/context-review-safety.test.mts';
  const cases: { name: string; path: string; before: string; after: string; test: string; rust?: string[]; multiple?: boolean; unixOnly?: boolean; nodeFile?: string }[] = [
    { name: 'JSON snapshots', path: 'scripts/context-real-helpers.mts', before: " || entry.name.endsWith('.json')", after: '', test: 'JSON snapshot additions removals and changes are authoritative in every custom directory' },
    { name: 'oracle provenance', path: 'scripts/context-real-helpers.mts', before: 'result.stdout.trim() !== expected', after: 'false', test: 'real-copy oracle provenance refuses mismatched missing failed or noisy driver versions before side effects' },
    { name: 'Windows selection call', path: 'scripts/context-review-revert.mts', before: 'supportedProofs(cases, process.argv[2])', after: 'cases', test: 'Windows signal proof selection preserves every supported case and reports skips' },
    { name: 'Windows selection', path: 'scripts/context-revert-helpers.mts', before: "platform !== 'win32'", after: 'true', test: 'Windows signal proof selection preserves every supported case and reports skips' },
    ...['context-revert-helpers.mts', 'native-adapter-revert.mts', 'native-adapter-capabilities-revert.mts', 'native-adapter-capabilities-review-revert.mts', 'native-adapter-changes-revert.mts', 'perf-revert-proof.mts'].map(file => ({ name: `symlink isolation: ${file}`, path: `scripts/${file}`, before: file === 'native-adapter-changes-revert.mts' ? 'dereference:true' : 'dereference: true', after: 'dereference:false', test: 'proof copies materialize absolute relative and directory symlinks before any mutation', unixOnly: true, multiple: file === 'native-adapter-changes-revert.mts' })),
    { unixOnly: true, name: 'dependency junction exclusion', path: 'scripts/context-revert-helpers.mts', before: "includes('node_modules')", after: "includes('excluded_dependencies')", multiple: true, test: 'proof copies materialize absolute relative and directory symlinks before any mutation' },
    { name: 'descendant groups', path: 'scripts/context-proof-process.mts', before: 'process.kill(-pid, signal)', after: 'process.kill(pid, signal)', test: 'all proof runners stop descendants before cleanup: context-intents-revert.mts SIGTERM' },
    { name: 'Windows descendant trees', path: 'scripts/context-proof-process.mts', before: "'/T', '/F'", after: "'/F'", test: 'Windows proof interruption terminates the complete tree or retains scratch inputs' },
    { name: 'nested ownership', path: 'scripts/context-proof-process.mts', before: 'this.collectNested();', after: '', multiple: true, test: 'outer ownership drains nested detached groups on SIGTERM' },
    { unixOnly: true, name: 'nested runner ownership', path: 'scripts/context-proof-process.mts', before: 'this.runners.set(pid, path)', after: 'void pid', test: 'outer ownership drains nested detached groups on exit' },
    { name: 'Windows orphan ownership', path: 'scripts/context-proof-process.mts', before: "if (process.platform !== 'win32') drained = this.drain();", after: "if (process.platform === 'win32' && pid !== undefined) this.forget(pid); if (process.platform !== 'win32') drained = this.drain();", test: 'Windows ownership persists after leader exit until inherited pipes are stopped' },
    { name: 'Windows kernel job ownership', path: 'scripts/context-proof-process.mts', before: 'windowsJob(command, args, marker)', after: '{ command, args }', test: 'Windows ownership persists after leader exit until inherited pipes are stopped' },
    { name: 'safety selection call', path: 'scripts/context-safety-revert.mts', before: '\n  for (const proof of supportedProofs(cases, process.argv[2])) {', after: '\n  for (const proof of cases) {', test: 'Windows safety selection excludes every unsupported mutation before its baseline' },
    { name: 'symlink platform selection', path: 'scripts/context-revert-helpers.mts', before: 'proof.unixOnly || ', after: '', test: 'Windows safety selection excludes every unsupported mutation before its baseline' },
    { name: 'optional dependency timing', path: 'scripts/context-real.mts', before: "unavailableTimings.push({ command: label, reason: unavailable }); continue;", after: "throw new Error('Benchmark preflight failed');", test: 'real-copy receipts survive infeasible timing while rejecting parity and runtime failures' },
    { name: 'generic refusal warnings', path: 'src/main.rs', before: 'if json && warning.is_some() {', after: 'if false {', test: 'context_item_faults_preserve_settings_warning_in_every_encoding', rust: ['--test', 'orientation_differential'] },
    { name: 'intent declaration collections', path: 'src/read_output.rs', before: 'preserve_intent && key == "context_intent"', after: 'false', test: 'read_output::tests::intent_declarations_are_immutable_during_wire_compaction', rust: ['--lib'] },
    { name: 'intent delivery policy', path: 'src/read_output.rs', before: 'compact(&mut result, true)', after: 'compact(&mut result, false)', test: 'read_output::tests::intent_declarations_are_immutable_during_wire_compaction', rust: ['--lib'] },
    { name: 'omitted receipt compatibility', path: 'src/read_output.rs', before: 'measure(&result, options.toon).max(legacy_estimate)', after: 'measure(&result, options.toon).min(legacy_estimate)', test: 'context_omitted_intent_receipt_matches_current_oracle', rust: ['--test', 'orientation_differential'] },
    { name: 'list contract delivery', path: 'src/read_output.rs', before: 'preserve_intent: true', after: 'preserve_intent: false', test: 'read_output::tests::intent_declarations_are_immutable_during_wire_compaction', rust: ['--lib'] },
    { name: 'list contract omission receipt', path: 'src/read_output.rs', before: 'let omitted_estimate = measure(&result, options.toon).max(legacy_estimate);', after: 'let omitted_estimate = measure(&result, options.toon).min(legacy_estimate);', test: 'triage_omitted_intent_receipts_match_published', rust: ['--test', 'pagination_differential'] },
    { name: 'intent declaration strings', path: 'src/read_output.rs', before: '"focus_row_states" | "context_intent"', after: '"focus_row_states"', test: 'read_output::tests::intent_declarations_are_immutable_during_wire_compaction', rust: ['--lib'] },
    { name: 'scratch path identity', path: 'scripts/harness.test.mts', before: 'fs.realpathSync(path) !== fs.realpathSync(marker.root)', after: 'path !== marker.root', test: 'revert proof restores both source files on SIGTERM (stubborn descendant)', nodeFile: 'scripts/harness.test.mts' },
    { name: 'SDK refusal warnings', path: 'src/orientation_context.rs', before: '                if settings_fallback {', after: '                if false {', test: 'orientation_context::tests::context_settings_warning_survives_refusals', rust: ['--lib'] },
    { name: 'CLI refusal warnings', path: 'src/main.rs', before: 'attach_settings_warning(&mut payload, settings_warning);', after: 'attach_settings_warning(&mut payload, None);', multiple: true, test: 'context_invalid_stored_activity_limits_match_current_oracle', rust: ['--test', 'orientation_differential'] },
  ];
  let executed = 0;
  for (const proof of supportedProofs(cases, process.argv[2])) {
    const path = join(scratch, proof.path);
    const fixed = readFileSync(path, 'utf8');
    assert.ok(fixed.includes(proof.before), `stale mutation: ${proof.name}`);
    if (!proof.multiple) assert.equal(fixed.split(proof.before).length, 2, `unique mutation: ${proof.name}`);
    const args = proof.rust
      ? ['test', '-j', '2', '--locked', ...proof.rust, proof.test, '--', '--exact', '--test-threads=2']
      : ['--test', '--test-reporter=tap', '--test-concurrency=2', `--test-name-pattern=^${escapeTestName(proof.test)}$`, proof.nodeFile ?? nodeFile];
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
