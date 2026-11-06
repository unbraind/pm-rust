import { spawnSync } from 'node:child_process';
import { cpSync, mkdirSync, mkdtempSync, readFileSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
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
const scratch = mkdtempSync(join(tmpdir(), 'pm-rust-context-revert-'));
try {
  const files = spawnSync('git', ['ls-files', '--cached', '--others', '--exclude-standard', '-z'], { cwd: source });
  if (files.status !== 0) throw new Error('Cannot enumerate package source');
  for (const file of new Set(files.stdout.toString().split('\0').filter(Boolean))) {
    const target = join(scratch, file);
    mkdirSync(dirname(target), { recursive: true });
    cpSync(join(source, file), target);
  }
  for (const oracle of ['oracle', 'orientation-oracle']) {
    symlinkSync(resolve('tests', oracle, 'node_modules'), join(scratch, 'tests', oracle, 'node_modules'), 'junction');
  }
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
  const packetPath = 'src/orientation_deps/packet.rs';
  const packets = readFileSync(join(scratch, packetPath), 'utf8');
  if (!packets.includes('self.packets.get(&budget)')) throw new Error('Cannot locate producer memoization');
  writeFileSync(join(scratch, packetPath), packets.replace('self.packets.get(&budget)', 'self.packets.get(&usize::MAX)'));
  run(scratch, ['--lib', 'packet_retry_cache_bounds_work_and_storage'], ['orientation_deps::packet::tests::packet_retry_cache_bounds_work_and_storage']);
  writeFileSync(join(scratch, packetPath), packets);
  const previous = spawnSync('git', ['apply', '--reverse', join(scratch, 'tests/fixtures/orientation-packet-root-priority.patch')], { cwd: scratch });
  if (previous.status !== 0) throw new Error('Cannot apply the stored packet behavior revert');
  run(scratch, ['--test', 'orientation_differential', 'relationship_packets_match_oracle'], ['relationship_packets_match_oracle']);
} finally {
  rmSync(scratch, { recursive: true, force: true });
}
