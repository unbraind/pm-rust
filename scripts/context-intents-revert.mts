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
  writeFileSync(target, fixed);
  const tablePath = join(scratch, 'src/toon_output.rs');
  const tables = readFileSync(tablePath, 'utf8');
  const signature = 'fn table_contract(value: &Value, current: bool) -> Option<(Value, Value)> {';
  if (!tables.includes(signature)) throw new Error('Cannot locate compact table contract');
  writeFileSync(tablePath, tables.replace(signature, `${signature}\n let _ = current; let current = false;`));
  run(scratch, ['--lib', 'current_context_tables_keep_ambiguous_cells_lossless'], ['toon_output::tests::current_context_tables_keep_ambiguous_cells_lossless']);
  writeFileSync(tablePath, tables);
  const packetPath = 'src/orientation_deps/packet.rs';
  const previous = spawnSync('git', ['show', `origin/main:${packetPath}`], { cwd: source, encoding: 'utf8' });
  if (previous.status !== 0) throw new Error('Cannot read previous packet behavior');
  writeFileSync(join(scratch, packetPath), previous.stdout);
  run(scratch, ['--test', 'orientation_differential', 'relationship_packets_match_oracle'], ['relationship_packets_match_oracle']);
} finally {
  rmSync(scratch, { recursive: true, force: true });
}
