import { checkedGit } from './context-revert-helpers.mts';
import { spawnSync } from 'node:child_process';
import { cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';

/** Run a focused command without publishing compiler paths or fixture output. */
function run(root: string, args: string[], expected: string[]): void {
  const result = spawnSync('cargo', ['test', '-j', '2', '--locked', ...args], {
    cwd: root, encoding: 'utf8', maxBuffer: 16 * 1024 * 1024,
    env: { ...process.env, CARGO_BUILD_JOBS: '2', RUST_TEST_THREADS: '2' },
  });
  const output = `${result.stdout}\n${result.stderr}`;
  if (result.error || result.status === 0 || !output.includes('test result: FAILED') || expected.some(name => !output.includes(`${name} ... FAILED`))) {
    throw new Error('Behavioral revert did not produce the required assertion failures');
  }
  console.log(JSON.stringify({ command: ['cargo', 'test', '-j', '2', '--locked', ...args], assertion_failures: expected, revert_confirmed: true }));
}

const source = process.cwd();
const dependencies = ['oracle', 'orientation-oracle'].map(oracle => {
  const modules = resolve(source, 'tests', oracle, 'node_modules');
  if (!existsSync(modules)) throw new Error(`Missing ${oracle} dependencies; run just oracle-install`);
  return { oracle, modules };
});
const scratch = mkdtempSync(join(tmpdir(), 'pm-rust-context-revert-'));
try {
  const files = checkedGit(source, ['ls-files', '--cached', '--others', '--exclude-standard', '-z'], 'Cannot enumerate package source');
  for (const file of new Set(files.split('\0').filter(Boolean))) {
    const target = join(scratch, file);
    mkdirSync(dirname(target), { recursive: true });
    cpSync(join(source, file), target);
  }
  for (const { oracle, modules } of dependencies) {
    symlinkSync(modules, join(scratch, 'tests', oracle, 'node_modules'), 'junction');
  }
  const preflightPath = join(scratch, 'scripts/context-intents-revert.mts');
  const preflight = readFileSync(preflightPath, 'utf8');
  const guard = 'if (!existsSync(modules)) throw new Error(`Missing ${oracle} dependencies; run just oracle-install`);';
  if (!preflight.includes(guard)) throw new Error('Cannot locate oracle dependency preflight');
  writeFileSync(preflightPath, preflight.replace(guard, ''));
  const preflightCommand = ['--test', '--test-reporter=tap', 'scripts/context-intents-revert.test.mts'];
  const preflightResult = spawnSync(process.execPath, preflightCommand, { cwd: scratch, encoding: 'utf8', timeout: 30_000 });
  const preflightTest = 'revert preflight names the missing oracle installation';
  if (preflightResult.error || preflightResult.status !== 1 || !preflightResult.stdout.includes(`not ok 1 - ${preflightTest}`)) {
    throw new Error('Oracle dependency preflight revert did not produce its assertion failure');
  }
  console.log(JSON.stringify({ command: ['node', ...preflightCommand], assertion_failures: [preflightTest], revert_confirmed: true }));
  writeFileSync(preflightPath, preflight);
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
  writeFileSync(target, `${fixed.slice(0, body)}{ let _ = (workspace, options, global); Ok(None) }${fixed.slice(end)}`);
  run(scratch, ['--lib', 'core_intent_boundaries_and_budget_receipts'], ['orientation_context::intent::tests::core_intent_boundaries_and_budget_receipts']);
  run(scratch, ['--lib', 'context_intent_sdk_budget_boundaries'], ['orientation_context::tests::context_intent_sdk_budget_boundaries']);
  run(scratch, ['--test', 'orientation_differential', 'context_intents_'], ['context_intents_and_sections_match_current_oracle', 'context_intents_empty_tracker_match_current_oracle', 'context_intents_wire_budget_match_current_oracle']);
  run(scratch, ['--test', 'orientation_differential', 'context_reviewed_contracts_match_current_oracle'], ['context_reviewed_contracts_match_current_oracle']);
  writeFileSync(target, fixed);
  const tablePath = join(scratch, 'src/toon_output.rs');
  const tables = readFileSync(tablePath, 'utf8');
  const signature = 'fn table_contract(value: &Value, current: bool) -> Option<(Value, Value)> {';
  if (!tables.includes(signature)) throw new Error('Cannot locate compact table contract');
  writeFileSync(tablePath, tables.replace(signature, `${signature}\n let _ = current; let current = false;`));
  run(scratch, ['--lib', 'current_context_tables_keep_ambiguous_cells_lossless'], ['toon_output::tests::current_context_tables_keep_ambiguous_cells_lossless']);
  writeFileSync(tablePath, tables);
  const current = 'value["output_default"] == "toon" || value["edge_basis"].is_string()';
  if (!tables.includes(current)) throw new Error('Cannot locate current renderer selection');
  writeFileSync(tablePath, tables.replace(current, 'false'));
  run(scratch, ['--lib', 'current_context_tables_keep_ambiguous_cells_lossless'], ['toon_output::tests::current_context_tables_keep_ambiguous_cells_lossless']);
  writeFileSync(tablePath, tables);
  const sectionsPath = join(scratch, 'src/orientation_context/sections.rs');
  const sections = readFileSync(sectionsPath, 'utf8');
  if (!sections.includes('settings["context"]["activity_limit"]')) throw new Error('Cannot locate configured activity limit');
  writeFileSync(sectionsPath, sections.replace('settings["context"]["activity_limit"]', 'json!(10)'));
  run(scratch, ['--lib', 'configured_sections_preserve_activity_errors_and_stale_order'], ['orientation_context::sections::tests::configured_sections_preserve_activity_errors_and_stale_order']);
  writeFileSync(sectionsPath, sections);
  const cutoff = '!activity_before(text(&entry, "ts"), now)';
  if (!sections.includes(cutoff)) throw new Error('Cannot locate activity timestamp boundary');
  writeFileSync(sectionsPath, sections.replace(cutoff, 'super::date_compare(text(&entry, "ts"), now).is_ge()'));
  run(scratch, ['--test', 'orientation_differential', 'context_reviewed_contracts_match_current_oracle'], ['context_reviewed_contracts_match_current_oracle']);
  writeFileSync(sectionsPath, sections);
  const emptyTimestamp = 'text(&entry, "ts").is_empty() || ';
  if (!sections.includes(emptyTimestamp)) throw new Error('Cannot locate empty activity timestamp refusal');
  writeFileSync(sectionsPath, sections.replace(emptyTimestamp, ''));
  run(scratch, ['--lib', 'section_activity_storage_and_ordering'], ['orientation_context::sections::tests::section_activity_storage_and_ordering']);
  writeFileSync(sectionsPath, sections);
  const contextPath = join(scratch, 'src/orientation_context.rs');
  const context = readFileSync(contextPath, 'utf8');
  const stabilize = 'crate::read_output::stabilize_orientation(&mut result, options.toon);';
  if (!context.includes(stabilize)) throw new Error('Cannot locate current intent receipt stabilization');
  writeFileSync(contextPath, context.replace(stabilize, 'crate::list_intent::stabilize(&mut result);'));
  run(scratch, ['--test', 'orientation_differential', 'context_reviewed_contracts_match_current_oracle'], ['context_reviewed_contracts_match_current_oracle']);
  writeFileSync(contextPath, context);
  const prepareCall = 'intent::prepare(self, &mut prepared, global.as_deref())?';
  if (!context.includes(prepareCall)) throw new Error('Cannot locate SDK intent refusal propagation');
  writeFileSync(contextPath, context.replace(prepareCall, prepareCall.replace('?', '.unwrap_or(None)')));
  run(scratch, ['--lib', 'context_intent_sdk_budget_boundaries'], ['orientation_context::tests::context_intent_sdk_budget_boundaries']);
  writeFileSync(contextPath, context);
  const settingsStart = context.indexOf('        if settings', context.indexOf('let mut settings ='));
  const settingsEnd = context.indexOf('        let depth = trim_control(', settingsStart);
  if (settingsStart < 0 || settingsEnd < 0 || !context.slice(settingsStart, settingsEnd).includes('settings.context')) {
    throw new Error('Cannot locate invalid context settings boundary');
  }
  writeFileSync(contextPath, context.slice(0, settingsStart) + context.slice(settingsEnd));
  run(scratch, ['--lib', 'context_settings_shape_boundaries'], ['orientation_context::tests::context_settings_shape_boundaries']);
  writeFileSync(contextPath, context);
  const activityValidation = '"--activity-limit must be a non-negative integer".to_owned(),\n            )?;';
  if (!context.includes(activityValidation)) throw new Error('Cannot locate activity limit validation');
  writeFileSync(contextPath, context.replace(activityValidation, activityValidation.replace(')?;', ').unwrap_or_default();')));
  run(scratch, ['--lib', 'context_settings_shape_boundaries'], ['orientation_context::tests::context_settings_shape_boundaries']);
  writeFileSync(contextPath, context);
  const outputPath = join(scratch, 'src/read_output.rs');
  const output = readFileSync(outputPath, 'utf8');
  const contextAlias = 'else if command == "context" {\n            "--section"';
  if (!output.includes(contextAlias)) throw new Error('Cannot locate context section alias receipt');
  writeFileSync(outputPath, output.replace(contextAlias, contextAlias.replace('"--section"', '"--brief"')));
  run(scratch, ['--lib', 'amount_and_alias_receipts_preserve_requested_dimensions'], ['read_output::tests::amount_and_alias_receipts_preserve_requested_dimensions']);
  writeFileSync(outputPath, output);
  const retainedIntent = 'if result["context_intent"]["result_omitted"] != true {';
  if (!output.includes(retainedIntent)) throw new Error('Cannot locate retained intent estimate update');
  writeFileSync(outputPath, output.replace(retainedIntent, 'if false {'));
  run(scratch, ['--lib', 'context_intent_sdk_budget_boundaries'], ['orientation_context::tests::context_intent_sdk_budget_boundaries']);
  writeFileSync(outputPath, output);
  const controlsPath = join(scratch, 'src/orientation.rs');
  const controls = readFileSync(controlsPath, 'utf8');
  const trimming = String.raw`raw.trim_matches(|c: char| c == '\u{feff}' || c != '\u{0085}' && c.is_whitespace())`;
  if (!controls.includes(trimming)) throw new Error('Cannot locate ECMAScript option trimming');
  writeFileSync(controlsPath, controls.replace(trimming, 'raw.trim()'));
  run(scratch, ['--test', 'orientation_differential', 'context_control_whitespace_matches_current_oracle'], ['context_control_whitespace_matches_current_oracle']);
  writeFileSync(controlsPath, controls);
  const diagnosticsPath = join(scratch, 'src/main.rs');
  const diagnostics = readFileSync(diagnosticsPath, 'utf8');
  if (!diagnostics.includes('command[1] == "deps"')) throw new Error('Cannot locate orientation diagnostic scope');
  writeFileSync(diagnosticsPath, diagnostics.replace('command[1] == "deps"', 'true'));
  run(scratch, ['--bin', 'pm-rust', 'current_orientation_diagnostics_preserve_other_commands'], ['tests::current_orientation_diagnostics_preserve_other_commands']);
  writeFileSync(diagnosticsPath, diagnostics);
  const retryGuard = '.filter(|&index| {\n                args.get(index + 1)\n                    .is_some_and(|value| !value.starts_with("--"))\n            })';
  if (!diagnostics.includes(retryGuard)) throw new Error('Cannot locate trailing section retry guard');
  writeFileSync(diagnosticsPath, diagnostics.replace(retryGuard, ''));
  run(scratch, ['--bin', 'pm-rust', 'orientation_error_recovery_contract'], ['tests::orientation_error_recovery_contract']);
  run(scratch, ['--test', 'orientation_differential', 'context_reviewed_contracts_match_current_oracle'], ['context_reviewed_contracts_match_current_oracle']);
  writeFileSync(diagnosticsPath, diagnostics);
  const retryValue = '|value| !value.starts_with("--")';
  if (!diagnostics.includes(retryValue)) throw new Error('Cannot locate section retry value guard');
  writeFileSync(diagnosticsPath, diagnostics.replace(retryValue, '|_| true'));
  run(scratch, ['--bin', 'pm-rust', 'orientation_error_recovery_contract'], ['tests::orientation_error_recovery_contract']);
  run(scratch, ['--test', 'orientation_differential', 'context_reviewed_contracts_match_current_oracle'], ['context_reviewed_contracts_match_current_oracle']);
  writeFileSync(diagnosticsPath, diagnostics);
  const packetPath = 'src/orientation_deps/packet.rs';
  const packets = readFileSync(join(scratch, packetPath), 'utf8');
  if (!packets.includes('self.packets.get(&budget)')) throw new Error('Cannot locate producer memoization');
  writeFileSync(join(scratch, packetPath), packets.replace('self.packets.get(&budget)', 'self.packets.get(&usize::MAX)'));
  run(scratch, ['--lib', 'packet_retry_cache_bounds_work_and_storage'], ['orientation_deps::packet::tests::packet_retry_cache_bounds_work_and_storage']);
  writeFileSync(join(scratch, packetPath), packets);
  checkedGit(scratch, ['apply', '--reverse', join(scratch, 'tests/fixtures/orientation-packet-root-priority.patch')], 'Cannot apply the stored packet behavior revert');
  run(scratch, ['--test', 'orientation_differential', 'relationship_packets_match_oracle'], ['relationship_packets_match_oracle']);
} finally {
  rmSync(scratch, { recursive: true, force: true });
}
