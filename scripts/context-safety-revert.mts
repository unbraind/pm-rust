import { assertionFailureDiagnostic } from './context-proof-diagnostics.mts';
/** Prove review safety classes with passing controls and independent disposable mutations. */
import assert from 'node:assert/strict';
import { existsSync, mkdtempSync, readFileSync, symlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { assertNamedTests, checkedGit, copyWorkingTree, escapeTestName, supportedProofs } from './context-revert-helpers.mts';
import { ProofProcesses, verifyAsyncMutation } from './context-proof-process.mts';

const source = process.cwd();
for (const oracle of ['oracle', 'orientation-oracle']) {
  if (!existsSync(join(source, 'tests', oracle, 'node_modules', '@unbrained/pm-cli/dist/cli.js'))) throw new Error(`Missing ${oracle} dependencies; run just oracle-install`);
}
const scratch = mkdtempSync(join(tmpdir(), 'pm-context-safety-revert-'));
const processes = new ProofProcesses();
try {
  copyWorkingTree(source, scratch);
  checkedGit(scratch, ['init', '--quiet'], 'scratch source enumeration');
  for (const oracle of ['oracle', 'orientation-oracle']) symlinkSync(join(source, 'tests', oracle, 'node_modules'), join(scratch, 'tests', oracle, 'node_modules'), 'junction');
  const nodeFile = 'scripts/context-review-safety.test.mts';
  const cases: { name: string; path: string; before: string; after: string; test: string; rust?: string[]; multiple?: boolean; unixOnly?: boolean; nodeFile?: string }[] = [
    { name: 'simulated ownership scope', path: 'scripts/context-review-safety.test.mts', before: 'PM_PROOF_PROCESS_REGISTRY: root,', after: 'PM_PROOF_PROCESS_REGISTRY: process.env.PM_PROOF_PROCESS_REGISTRY ?? root,', test: 'simulated process registries never leak into the parent ownership scope' },
    { name: 'timeout evidence refusal', path: 'scripts/context-revert-helpers.mts', before: '  assert.doesNotMatch(output, /testTimeoutFailure|cancelledByParent|test timed out after|cancelled by parent/i);', after: '', test: 'revert proof rejects unrelated failures skipped tests and invalid controls', nodeFile: 'scripts/context-intents-revert.test.mts' },
    { name: 'dependency delivery contract', path: 'src/orientation_deps.rs', before: 'output.current_contract = true;', after: 'output.current_contract = false;', test: 'orientation_deps::tests::dependency_wire_measurement_matches_delivery', rust: ['--lib'] },
    { name: 'lossless operator workspace paths', path: 'src/main.rs', before: 'prefix.push(raw.to_owned());', after: 'prefix.push(raw.to_string_lossy().into_owned().into());', multiple: true, test: 'tests::parser_refusal_workspace_selection_matches_native_scope', rust: ['--bin', 'pm-rust'] },
    ...['stdoutDecoder', 'stderrDecoder'].map(decoder => ({ name: `Unicode diagnostic capture: ${decoder}`, path: 'scripts/context-proof-process.mts', before: `${decoder}.write(chunk)`, after: 'chunk.toString()', test: 'proof diagnostics preserve Unicode across independent stream chunk boundaries' })),
    { name: 'portable failed-reap simulation', path: 'scripts/context-review-safety.test.mts', before: "Object.defineProperty(process,'platform',{value:'darwin'});\nlet unavailable=true;", after: 'let unavailable=true;', test: 'failed worker reap retains mutated inputs until termination is confirmed' },
    { name: 'Windows-host Unix simulations', path: 'scripts/context-review-safety.test.mts', before: "Object.defineProperty(process,'platform',{value:'darwin'});", after: '', multiple: true, test: 'signal errors require positive termination evidence before restoration' },
    { name: 'Darwin signal evidence', path: 'scripts/context-proof-process.mts', before: "if (code === 'EPERM') {", after: 'if (false) {', test: 'signal errors require positive termination evidence before restoration' },
    { name: 'Darwin live worker protection', path: 'scripts/context-proof-process.mts', before: 'if (!this.live(pid, group)) return;', after: 'if (true) return;', test: 'signal errors require positive termination evidence before restoration' },
    { name: 'Darwin escalation handoff', path: 'scripts/context-proof-process.mts', before: 'if (escalation) clearTimeout(escalation);\n            drained = this.drain();', after: 'drained = this.drain();', test: 'leader exit transfers escalation to the drain without a duplicate timer' },
    { name: 'review proof target admission', path: 'scripts/context-review-revert.mts', before: 'let changed = compact_strings_with_intent(&mut target.value, preserve_intent);', after: 'compact_strings(&mut result.value);', test: 'all managed mutation targets match the live package: context-review-revert.mts', nodeFile: 'scripts/context-proof-definitions.test.mts' },
    { name: 'safety proof target admission', path: 'scripts/context-safety-revert.mts', before: "before: " + "'let omitted = measure(&result, options.toon).max(legacy_estimate);'", after: "before: 'measure(&result, options.toon).max(legacy_estimate)'", test: 'all managed mutation targets match the live package: context-safety-revert.mts', nodeFile: 'scripts/context-proof-definitions.test.mts' },
    { name: 'deferred restoration', path: 'scripts/context-proof-process.mts', before: 'await owner?.settled(); restore();', after: 'restore();', test: 'failed worker reap retains mutated inputs until termination is confirmed' },
    { unixOnly: true, name: 'spawn ownership before constructors', path: 'scripts/context-proof-process.mts', before: 'options?.detached && child.pid !== undefined', after: 'false', test: 'outer cleanup owns delayed nested constructors on exit' },
    { unixOnly: true, name: 'runner restoration grace', path: 'scripts/context-proof-process.mts', before: 'this.runners.has(pid)', after: 'false', multiple: true, test: 'outer cleanup owns delayed nested constructors on SIGTERM' },
    ...['context-revert-helpers.mts', 'native-adapter-revert.mts', 'native-adapter-capabilities-revert.mts', 'native-adapter-capabilities-review-revert.mts', 'native-adapter-changes-revert.mts'].map(file => ({ unixOnly: true, name: `dangling links: ${file}`, path: `scripts/${file}`, before: file === 'context-revert-helpers.mts' ? ' || absentLink(path)' : '!absentLink(path)', after: file === 'context-revert-helpers.mts' ? '' : 'true', multiple: file === 'native-adapter-changes-revert.mts', test: 'proof copies skip dangling file and directory links across every runner' })),
    { unixOnly: true, name: 'dangling links: perf', path: 'scripts/perf-revert-proof.mts', before: '!(lstatSync(path).isSymbolicLink() && !existsSync(path))', after: 'true', test: 'proof copies skip dangling file and directory links across every runner' },
    { name: 'top-level-only intent protection', path: 'src/read_output.rs', before: 'compact_strings_at(row, false, false)', after: 'compact_strings_at(row, preserve_intent, false)', multiple: true, test: 'read_output::tests::item_metadata_named_context_intent_compacts_under_budgets', rust: ['--lib'] },
    { name: 'JavaScript column order', path: 'src/toon_output.rs', before: 'flat_map(javascript_keys)', after: 'flat_map(|map| map.keys().map(String::as_str))', test: 'toon_output::tests::current_compact_tables_round_trip_through_pinned_sdk', rust: ['--lib'] },
    { name: 'parser warning preservation', path: 'src/main.rs', before: '(args[0] == "context")', after: '(false)', test: 'context_parser_refusals_preserve_settings_warnings', rust: ['--test', 'orientation_differential'] },
    { name: 'whole-settings fallback', path: 'src/settings.rs', before: 'value: if fallback { json!({}) } else { value },', after: 'value: if fallback { json!({"schema":value["schema"]}) } else { value },', test: 'orientation_context::tests::context_stored_settings_fallback_precedes_overrides', rust: ['--lib'] },
    { name: 'JSON snapshots', path: 'scripts/context-real-helpers.mts', before: " || entry.name.endsWith('.json')", after: '', test: 'JSON snapshot additions removals and changes are authoritative in every custom directory' },
    { name: 'oracle provenance', path: 'scripts/context-real-helpers.mts', before: 'result.stdout.trim() !== expected', after: 'false', test: 'real-copy oracle provenance refuses mismatched missing failed or noisy driver versions before side effects' },
    { name: 'Windows selection call', path: 'scripts/context-review-revert.mts', before: 'supportedProofs(cases, process.argv[2])', after: 'cases', test: 'Windows signal proof selection preserves every supported case and reports skips' },
    { name: 'Windows selection', path: 'scripts/context-revert-helpers.mts', before: "platform !== 'win32'", after: 'true', test: 'Windows signal proof selection preserves every supported case and reports skips' },
    ...['context-revert-helpers.mts', 'native-adapter-revert.mts', 'native-adapter-capabilities-revert.mts', 'native-adapter-capabilities-review-revert.mts', 'native-adapter-changes-revert.mts', 'perf-revert-proof.mts'].map(file => ({ name: `symlink isolation: ${file}`, path: `scripts/${file}`, before: file === 'native-adapter-changes-revert.mts' ? 'dereference:true' : 'dereference: true', after: 'dereference:false', test: 'proof copies materialize absolute relative and directory symlinks before any mutation', unixOnly: true, multiple: file === 'native-adapter-changes-revert.mts' })),
    { unixOnly: true, name: 'dependency junction exclusion', path: 'scripts/context-revert-helpers.mts', before: "includes('node_modules')", after: "includes('excluded_dependencies')", multiple: true, test: 'proof copies materialize absolute relative and directory symlinks before any mutation' },
    { name: 'descendant groups', path: 'scripts/context-proof-process.mts', before: 'process.kill(-pid, signal)', after: 'process.kill(pid, signal)', test: 'all proof runners stop descendants before cleanup: context-intents-revert.mts SIGTERM' },
    { name: 'Windows descendant trees', path: 'scripts/context-proof-process.mts', before: 'const stop = windowsJobStop(owned.marker);', after: 'const stop = windowsJobStop("unrelated-job");', test: 'Windows proof interruption terminates the complete tree or retains scratch inputs' },
    { name: 'nested ownership', path: 'scripts/context-proof-process.mts', before: 'this.collectNested();', after: '', multiple: true, test: 'outer ownership drains nested detached groups on SIGTERM' },
    { unixOnly: true, name: 'nested runner ownership', path: 'scripts/context-proof-process.mts', before: 'this.runners.set(pid, path)', after: 'void pid', test: 'outer ownership drains nested detached groups on exit' },
    { name: 'Windows orphan ownership', path: 'scripts/context-proof-process.mts', before: "if (process.platform !== 'win32') {\n            // Drain now owns escalation.", after: "if (process.platform === 'win32' && pid !== undefined) this.forget(pid); if (process.platform !== 'win32') {\n            // Drain now owns escalation.", test: 'Windows ownership persists after leader exit until inherited pipes are stopped' },
    { name: 'Windows kernel job ownership', path: 'scripts/context-proof-process.mts', before: 'windowsJob(command, args, marker)', after: '{ command, args }', test: 'Windows ownership persists after leader exit until inherited pipes are stopped' },
    { name: 'safety selection call', path: 'scripts/context-safety-revert.mts', before: '\n  for (const proof of supportedProofs(cases, process.argv[2])) {', after: '\n  for (const proof of cases) {', test: 'Windows safety selection excludes every unsupported mutation before its baseline' },
    { name: 'symlink platform selection', path: 'scripts/context-revert-helpers.mts', before: 'proof.unixOnly || ', after: '', test: 'Windows safety selection excludes every unsupported mutation before its baseline' },
    { name: 'optional dependency timing', path: 'scripts/context-real.mts', before: "unavailableTimings.push({ command: label, reason: unavailable }); continue;", after: "throw new Error('Benchmark preflight failed');", test: 'real-copy receipts survive infeasible timing while rejecting parity and runtime failures' },
    { name: 'generic refusal warnings', path: 'src/main.rs', before: 'if json && warning.is_some() {', after: 'if false {', test: 'context_item_faults_preserve_settings_warning_in_every_encoding', rust: ['--test', 'orientation_differential'] },
    { name: 'intent declaration collections', path: 'src/read_output.rs', before: '((preserve_intent && key == "context_intent")', after: '((false)', test: 'read_output::tests::intent_declarations_are_immutable_during_wire_compaction', rust: ['--lib'] },
    { name: 'intent delivery policy', path: 'src/read_output.rs', before: 'compact(&mut result, true)', after: 'compact(&mut result, false)', test: 'read_output::tests::intent_declarations_are_immutable_during_wire_compaction', rust: ['--lib'] },
    { name: 'omitted receipt compatibility', path: 'src/read_output.rs', before: 'let omitted = measure(&result, options.toon).max(legacy_estimate);', after: 'let omitted = measure(&result, options.toon).min(legacy_estimate);', test: 'context_omitted_intent_receipt_matches_current_oracle', rust: ['--test', 'orientation_differential'] },
    { name: 'list contract delivery', path: 'src/read_output.rs', before: 'preserve_intent: true', after: 'preserve_intent: false', test: 'read_output::tests::intent_declarations_are_immutable_during_wire_compaction', rust: ['--lib'] },
    { name: 'list contract omission receipt', path: 'src/read_output.rs', before: 'let omitted_estimate = measure(&result, options.toon).max(legacy_estimate);', after: 'let omitted_estimate = measure(&result, options.toon).min(legacy_estimate);', test: 'triage_omitted_intent_receipts_match_published', rust: ['--test', 'pagination_differential'] },
    { name: 'intent declaration strings', path: 'src/read_output.rs', before: '|| (preserve_intent && key == "context_intent")', after: '', test: 'read_output::tests::intent_declarations_are_immutable_during_wire_compaction', rust: ['--lib'] },
    { name: 'scratch path identity', path: 'scripts/harness.test.mts', before: 'fs.realpathSync(path) !== fs.realpathSync(marker.root)', after: 'path !== marker.root', test: 'revert proof restores both source files on SIGTERM (stubborn descendant)', nodeFile: 'scripts/harness.test.mts' },
    { name: 'SDK refusal warnings', path: 'src/orientation_context.rs', before: '                if settings_fallback\n', after: '                if false\n', test: 'orientation_context::tests::context_settings_warning_survives_refusals', rust: ['--lib'] },
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
      assertNamedTests(outcome, [proof.test], reverted, !proof.rust, assertionFailureDiagnostic(scratch, [proof.test], !proof.rust));
      console.log(JSON.stringify({ proof: proof.name, test: proof.test, reverted, result: reverted ? 'named assertion failure' : 'pass' }));
    }, () => writeFileSync(path, fixed.replaceAll(proof.before, proof.after)), () => writeFileSync(path, fixed), processes);
    executed++;
  }
  assert.ok(executed > 0, 'proof filter selected no cases');
  console.log(JSON.stringify({ proved: executed, restored: true }));
} catch (error) { if (!processes.wasInterrupted) throw error; }
finally { await processes.cleanup(scratch); }
