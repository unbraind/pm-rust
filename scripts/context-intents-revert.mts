import { ProofProcesses, verifyAsyncMutation } from './context-proof-process.mts';
import { assertNamedTests, copyWorkingTree, checkedGit } from './context-revert-helpers.mts';
import { existsSync, mkdtempSync, readFileSync, symlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';

/** Run exactly the required assertions with oracle setup mandatory in every control. */
async function run(root: string, args: string[], expected: string[], reverted: boolean): Promise<void> {
  const result = await processes.run('cargo', ['test', '-j', '2', '--locked', ...args], {
    cwd: root, maxBuffer: 16 * 1024 * 1024,
    env: { ...process.env, CARGO_BUILD_JOBS: '2', RUST_TEST_THREADS: '2', CARGO_TARGET_DIR: join(root, 'target'), PM_RUST_REQUIRE_PUBLISHED_CLI: '1' },
  });
  assertNamedTests(result, expected, reverted);
  console.log(JSON.stringify({ command: ['cargo', 'test', '-j', '2', '--locked', ...args], tests: expected, reverted }));
}

/** Independently prove a mutation against passing controls in the disposable source copy. */
async function prove(path: string, mutated: string, tests: { args: string[]; names: string[] }[]): Promise<void> {
  const fixed = readFileSync(path, 'utf8');
  await verifyAsyncMutation(
    async reverted => { for (const test of tests) await run(scratch, test.args, test.names, reverted); },
    () => writeFileSync(path, mutated),
    () => writeFileSync(path, fixed),
  );
}

const source = process.cwd();
const dependencies = ['oracle', 'orientation-oracle'].map(oracle => {
  const modules = resolve(source, 'tests', oracle, 'node_modules');
  if (!existsSync(modules)) throw new Error(`Missing ${oracle} dependencies; run just oracle-install`);
  return { oracle, modules };
});
const scratch = mkdtempSync(join(tmpdir(), 'pm-rust-context-revert-'));
const processes = new ProofProcesses();
try {
  copyWorkingTree(source, scratch);
  for (const { oracle, modules } of dependencies) {
    symlinkSync(modules, join(scratch, 'tests', oracle, 'node_modules'), 'junction');
  }
  const preflightPath = join(scratch, 'scripts/context-intents-revert.mts');
  const preflight = readFileSync(preflightPath, 'utf8');
  const guard = 'if (!existsSync(modules)) throw new Error(`Missing ${oracle} dependencies; run just oracle-install`);';
  if (!preflight.includes(guard)) throw new Error('Cannot locate oracle dependency preflight');
  const preflightCommand = ['--test', '--test-reporter=tap', '--test-name-pattern=revert preflight names the missing oracle installation', 'scripts/context-intents-revert.test.mts'];
  const preflightTest = 'revert preflight names the missing oracle installation';
  await verifyAsyncMutation(async reverted => {
    const result = await processes.run(process.execPath, preflightCommand, { cwd: scratch, timeout: 30_000 });
    assertNamedTests(result, [preflightTest], reverted, true);
  }, () => writeFileSync(preflightPath, preflight.replace(guard, '')), () => writeFileSync(preflightPath, preflight));
  const target = join(scratch, 'src/orientation_context/intent.rs');
  const fixed = readFileSync(target, 'utf8');
  const start = fixed.indexOf('pub(super) fn prepare(');
  const body = fixed.indexOf('{', start);
  let end = body + 1;
  let depth = 1;
  for (; end < fixed.length && depth > 0; end += 1) {
    if (fixed[end] === '{') depth += 1;
    if (fixed[end] === '}') depth -= 1;
  }
  if (start < 0 || depth !== 0) throw new Error('Cannot locate intent preparation');
  await prove(target, `${fixed.slice(0, body)}{ let _ = (workspace, options, global); Ok(None) }${fixed.slice(end)}`, [
    { args: ['--lib', 'core_intent_boundaries_and_budget_receipts'], names: ['orientation_context::intent::tests::core_intent_boundaries_and_budget_receipts'] },
    { args: ['--lib', 'context_intent_sdk_budget_boundaries'], names: ['orientation_context::tests::context_intent_sdk_budget_boundaries'] },
    { args: ['--test', 'orientation_differential', 'context_intents_'], names: ['context_intents_and_sections_match_current_oracle', 'context_intents_empty_tracker_match_current_oracle', 'context_intents_wire_budget_match_current_oracle'] },
    { args: ['--test', 'orientation_differential', 'context_reviewed_contracts_match_current_oracle'], names: ['context_reviewed_contracts_match_current_oracle'] },
  ]);
  const tablePath = join(scratch, 'src/toon_output.rs');
  const tables = readFileSync(tablePath, 'utf8');
  const signature = 'fn table_contract(value: &Value, current: bool) -> Option<(Value, Value)> {';
  if (!tables.includes(signature)) throw new Error('Cannot locate compact table contract');
  await prove(tablePath, tables.replace(signature, `${signature}\n let _ = current; let current = false;`), [
    { args: ['--lib', 'current_context_tables_keep_ambiguous_cells_lossless'], names: ['toon_output::tests::current_context_tables_keep_ambiguous_cells_lossless'] },
  ]);
  const current = 'value["output_default"] == "toon" || value["edge_basis"].is_string()';
  if (!tables.includes(current)) throw new Error('Cannot locate current renderer selection');
  await prove(tablePath, tables.replace(current, 'false'), [
    { args: ['--lib', 'current_context_tables_keep_ambiguous_cells_lossless'], names: ['toon_output::tests::current_context_tables_keep_ambiguous_cells_lossless'] },
  ]);
  const sectionsPath = join(scratch, 'src/orientation_context/sections.rs');
  const sections = readFileSync(sectionsPath, 'utf8');
  if (!sections.includes('settings["context"]["activity_limit"]')) throw new Error('Cannot locate configured activity limit');
  await prove(sectionsPath, sections.replace('settings["context"]["activity_limit"]', 'json!(10)'), [
    { args: ['--lib', 'configured_sections_preserve_activity_errors_and_stale_order'], names: ['orientation_context::sections::tests::configured_sections_preserve_activity_errors_and_stale_order'] },
  ]);
  const cutoff = '!activity_before(text(&entry, "ts"), now)';
  if (!sections.includes(cutoff)) throw new Error('Cannot locate activity timestamp boundary');
  await prove(sectionsPath, sections.replace(cutoff, 'super::date_compare(text(&entry, "ts"), now).is_ge()'), [
    { args: ['--test', 'orientation_differential', 'context_reviewed_contracts_match_current_oracle'], names: ['context_reviewed_contracts_match_current_oracle'] },
  ]);
  const emptyTimestamp = 'text(&entry, "ts").is_empty() || ';
  if (!sections.includes(emptyTimestamp)) throw new Error('Cannot locate empty activity timestamp refusal');
  await prove(sectionsPath, sections.replace(emptyTimestamp, ''), [
    { args: ['--lib', 'section_activity_storage_and_ordering'], names: ['orientation_context::sections::tests::section_activity_storage_and_ordering'] },
  ]);
  const contextPath = join(scratch, 'src/orientation_context.rs');
  const context = readFileSync(contextPath, 'utf8');
  const stabilize = 'crate::read_output::stabilize_orientation(&mut result, options.toon);';
  if (!context.includes(stabilize)) throw new Error('Cannot locate current intent receipt stabilization');
  await prove(contextPath, context.replace(stabilize, 'crate::list_intent::stabilize(&mut result);'), [
    { args: ['--test', 'orientation_differential', 'context_reviewed_contracts_match_current_oracle'], names: ['context_reviewed_contracts_match_current_oracle'] },
  ]);
  const prepareCall = 'intent::prepare(self, &mut prepared, global.as_deref())?';
  if (!context.includes(prepareCall)) throw new Error('Cannot locate SDK intent refusal propagation');
  await prove(contextPath, context.replace(prepareCall, prepareCall.replace('?', '.unwrap_or(None)')), [
    { args: ['--lib', 'context_intent_sdk_budget_boundaries'], names: ['orientation_context::tests::context_intent_sdk_budget_boundaries'] },
  ]);
  const settingsStart = context.indexOf('        if settings', context.indexOf('let mut settings ='));
  const settingsEnd = context.indexOf('        let settings_fallback =', settingsStart);
  if (settingsStart < 0 || settingsEnd < 0 || !context.slice(settingsStart, settingsEnd).includes('settings.context')) {
    throw new Error('Cannot locate invalid context settings boundary');
  }
  await prove(contextPath, context.slice(0, settingsStart) + context.slice(settingsEnd), [
    { args: ['--lib', 'context_settings_shape_boundaries'], names: ['orientation_context::tests::context_settings_shape_boundaries'] },
  ]);
  const activityValidation = '"--activity-limit must be a non-negative integer".to_owned(),\n            )?;';
  if (!context.includes(activityValidation)) throw new Error('Cannot locate activity limit validation');
  await prove(contextPath, context.replace(activityValidation, activityValidation.replace(')?;', ').unwrap_or_default();')), [
    { args: ['--lib', 'context_settings_shape_boundaries'], names: ['orientation_context::tests::context_settings_shape_boundaries'] },
  ]);
  const outputPath = join(scratch, 'src/read_output.rs');
  const output = readFileSync(outputPath, 'utf8');
  const contextAlias = 'else if command == "context" {\n            "--section"';
  if (!output.includes(contextAlias)) throw new Error('Cannot locate context section alias receipt');
  await prove(outputPath, output.replace(contextAlias, contextAlias.replace('"--section"', '"--brief"')), [
    { args: ['--lib', 'amount_and_alias_receipts_preserve_requested_dimensions'], names: ['read_output::tests::amount_and_alias_receipts_preserve_requested_dimensions'] },
  ]);
  const retainedIntent = '        stabilize_intent(result);';
  if (!output.includes(retainedIntent)) throw new Error('Cannot locate retained intent estimate update');
  await prove(outputPath, output.replace(retainedIntent, ''), [
    { args: ['--lib', 'context_intent_sdk_budget_boundaries'], names: ['orientation_context::tests::context_intent_sdk_budget_boundaries'] },
  ]);
  const controlsPath = join(scratch, 'src/orientation.rs');
  const controls = readFileSync(controlsPath, 'utf8');
  const trimming = String.raw`raw.trim_matches(|c: char| c == '\u{feff}' || c != '\u{0085}' && c.is_whitespace())`;
  if (!controls.includes(trimming)) throw new Error('Cannot locate ECMAScript option trimming');
  await prove(controlsPath, controls.replace(trimming, 'raw.trim()'), [
    { args: ['--test', 'orientation_differential', 'context_control_whitespace_matches_current_oracle'], names: ['context_control_whitespace_matches_current_oracle'] },
  ]);
  const diagnosticsPath = join(scratch, 'src/main.rs');
  const diagnostics = readFileSync(diagnosticsPath, 'utf8');
  if (!diagnostics.includes('command[1] == "deps"')) throw new Error('Cannot locate orientation diagnostic scope');
  await prove(diagnosticsPath, diagnostics.replace('command[1] == "deps"', 'true'), [
    { args: ['--bin', 'pm-rust', 'current_orientation_diagnostics_preserve_other_commands'], names: ['tests::current_orientation_diagnostics_preserve_other_commands'] },
  ]);
  const retryGuard = '.filter(|&index| {\n                args.get(index + 1)\n                    .is_some_and(|value| !value.starts_with("--"))\n            })';
  if (!diagnostics.includes(retryGuard)) throw new Error('Cannot locate trailing section retry guard');
  await prove(diagnosticsPath, diagnostics.replace(retryGuard, ''), [
    { args: ['--bin', 'pm-rust', 'orientation_error_recovery_contract'], names: ['tests::orientation_error_recovery_contract'] },
    { args: ['--test', 'orientation_differential', 'context_reviewed_contracts_match_current_oracle'], names: ['context_reviewed_contracts_match_current_oracle'] },
  ]);
  const retryValue = '|value| !value.starts_with("--")';
  if (!diagnostics.includes(retryValue)) throw new Error('Cannot locate section retry value guard');
  await prove(diagnosticsPath, diagnostics.replace(retryValue, '|_| true'), [
    { args: ['--bin', 'pm-rust', 'orientation_error_recovery_contract'], names: ['tests::orientation_error_recovery_contract'] },
    { args: ['--test', 'orientation_differential', 'context_reviewed_contracts_match_current_oracle'], names: ['context_reviewed_contracts_match_current_oracle'] },
  ]);
  const packetPath = 'src/orientation_deps/packet.rs';
  const packets = readFileSync(join(scratch, packetPath), 'utf8');
  if (!packets.includes('self.packets.get(&budget)')) throw new Error('Cannot locate producer memoization');
  await prove(join(scratch, packetPath), packets.replace('self.packets.get(&budget)', 'self.packets.get(&usize::MAX)'), [
    { args: ['--lib', 'packet_retry_cache_bounds_work_and_storage'], names: ['orientation_deps::packet::tests::packet_retry_cache_bounds_work_and_storage'] },
  ]);
  await verifyAsyncMutation(async reverted => run(scratch, ['--test', 'orientation_differential', 'relationship_packets_match_oracle'], ['relationship_packets_match_oracle'], reverted),
    () => { checkedGit(scratch, ['apply', '--reverse', join(scratch, 'tests/fixtures/orientation-packet-root-priority.patch')], 'Cannot apply the stored packet behavior revert'); },
    () => writeFileSync(join(scratch, packetPath), packets));
} catch (error) {
  if (!processes.wasInterrupted) throw error;
} finally {
  await processes.cleanup(scratch);
}
