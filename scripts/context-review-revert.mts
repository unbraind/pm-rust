/** Independent review mutations run only in a disposable source copy with real pinned oracles. */
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { cpSync, mkdirSync, mkdtempSync, readFileSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { assertNamedTests, checkedGit, escapeTestName, verifyMutation } from './context-revert-helpers.mts';

const source = process.cwd();
const scratch = mkdtempSync(join(tmpdir(), 'pm-context-review-'));
const childEnv: NodeJS.ProcessEnv = {
  CARGO_BUILD_JOBS: '2', RUST_TEST_THREADS: '2', PM_RUST_REQUIRE_PUBLISHED_CLI: '1',
  CARGO_TARGET_DIR: join(scratch, 'target'),
};
for (const name of ['PATH', 'HOME', 'CARGO_HOME', 'RUSTUP_HOME', 'LD_LIBRARY_PATH', 'SystemRoot', 'SystemDrive', 'windir', 'TMPDIR', 'TEMP', 'TMP']) {
  const value = process.env[name];
  if (value !== undefined) childEnv[name] = value;
}

/** Require a real named test result; compiler failures and empty selections are rejected. */
function check(target: string[], name: string, reverted: boolean): void {
  const node = target[0] === 'node';
  const args = node ? ['--test-reporter=tap', `--test-name-pattern=^${escapeTestName(name)}$`, ...target.slice(1)] : ['test', '-j', '2', '--locked', ...target, name, '--', '--exact', '--test-threads=2'];
  const result = spawnSync(node ? process.execPath : 'cargo', args, {
    cwd: scratch, env: childEnv, encoding: 'utf8', timeout: 900_000, maxBuffer: 8 * 1024 * 1024,
  });
  assertNamedTests(result, [name], reverted, node);
  console.log(JSON.stringify({ command: [node ? 'node' : 'cargo', ...args], reverted, result: reverted ? 'named behavioral failure' : 'pass' }));
}

try {
  for (const file of checkedGit(source, ['ls-files', '--cached', '--others', '--exclude-standard', '-z'], 'Cannot enumerate package source').split('\0').filter(Boolean)) {
    const path = join(scratch, file);
    mkdirSync(dirname(path), { recursive: true });
    cpSync(join(source, file), path);
  }
  for (const oracle of ['oracle', 'orientation-oracle']) {
    symlinkSync(resolve(source, 'tests', oracle, 'node_modules'), join(scratch, 'tests', oracle, 'node_modules'), 'junction');
  }
  const cases = [
    { path: 'scripts/context-revert-helpers.mts', before: "${message}: ${result.error?.message ?? ''}${result.stderr ?? ''}", after: "${message}: ${result.error?.message ?? ''}",
      target: ['node', '--test', '--test-concurrency=2', 'scripts/context-intents-revert.test.mts'], test: 'revert Git failures preserve diagnostics for source scans and patch checks' },
    { path: 'scripts/context-revert-helpers.mts', before: '  check(false);\n  try {', after: '  try {',
      target: ['node', '--test', '--test-concurrency=2', 'scripts/context-intents-revert.test.mts'], test: 'revert proof rejects unrelated failures skipped tests and invalid controls' },
    { path: 'justfile', before: 'crash-check: oracle-install', after: 'crash-check:',
      target: ['node', '--test', '--test-concurrency=2', 'scripts/context-intents-revert.test.mts'], test: 'standalone harness check installs the orientation oracle first' },
    { path: 'justfile', before: 'orientation-revert-proof: oracle-install', after: 'orientation-revert-proof:',
      target: ['node', '--test', '--test-concurrency=2', 'scripts/context-intents-revert.test.mts'], test: 'standalone harness check installs the orientation oracle first' },
    { path: 'src/orientation_context/intent.rs', before: '            read_output::stabilize_intent(&mut result);', after: '            read_output::update(&mut result, "context_intent", false);',
      target: ['--lib'], test: 'orientation_context::intent::tests::retained_context_intent_feasibility_matches_projection' },
    { path: 'src/list_intent.rs', before: '        result.value = Value::Object(map);\n        read_output::update(&mut result, "context_intent", false);', after: '        result.value = Value::Object(map);\n        read_output::stabilize_intent(&mut result);',
      target: ['--lib'], test: 'list_intent::tests::legacy_list_intent_feasibility_preserves_published_projection' },
    { path: 'src/orientation_deps/packet.rs', before: 'if used + reserved + tokens + edge_cost > budget {', after: 'if false {',
      target: ['--lib'], test: 'orientation_deps::packet::tests::packet_direct_reservations_are_reused' },
    { path: 'scripts/context-revert-helpers.mts', before: '  for (const name of names) {', after: '  for (const name of names.filter(() => false)) {',
      target: ['node', '--test', '--test-concurrency=2', 'scripts/context-intents-revert.test.mts'], test: 'revert proof rejects unrelated failures skipped tests and invalid controls' },
    { path: 'scripts/perf-revert-proof.mts', before: 'for (const test of cases) await run(test, false);\n\ntry {', after: 'try {',
      target: ['node', '--test', '--test-concurrency=2', 'scripts/harness.test.mts'], test: 'revert proof restores both source files on SIGTERM (parent only)' },
    { path: 'scripts/perf-revert-proof.mts', before: '  activeChild?.kill(signal);\n  restore();', after: '  activeChild?.kill(signal);',
      target: ['node', '--test', '--test-concurrency=2', 'scripts/harness.test.mts'], test: 'revert proof restores both source files on SIGTERM (parent only)' },
    { path: 'scripts/perf-revert-proof.mts', before: "CARGO_TARGET_DIR: join(repo, 'target')", after: "CARGO_TARGET_DIR: join(sourceRoot, 'target')",
      target: ['node', '--test', '--test-concurrency=2', 'scripts/harness.test.mts'], test: 'revert proof restores both source files on SIGTERM (parent only)' },
    ...['sections', 'activity_limit'].map(id => ({ path: 'src/discovery.rs', before: `                "${id}",\n`, after: '',
      target: ['--test', 'capability_contract'], test: 'discovery_is_complete_bounded_and_tracker_free' })),
    { path: 'tests/orientation_differential.rs', before: '        return Ok(None);\n    }\n    let current =', after: '        return Ok(Some(root.join("invalid-driver.mjs")));\n    }\n    let current =',
      target: ['--test', 'orientation_differential'], test: 'orientation_oracles_are_gated_independently' },
    { path: 'src/orientation_context/intent.rs', before: '            _ => None,', after: '            _ => Some(Self::Orient),',
      target: ['--test', 'orientation_differential'], test: 'context_unknown_intents_have_explicit_cli_boundaries' },
    { path: 'src/orientation_context.rs', before: '        let depth = trim_control(', after: '        let _ = std::fs::write(self.pm_root().join("settings.json"), settings.to_string());\n        let depth = trim_control(',
      target: ['--test', 'orientation_differential'], test: 'context_successful_reads_preserve_settings_bytes' },
    { path: 'src/read_output.rs', before: '    if measured > budget {\n        let changed = compact_strings(&mut result.value);', after: '    if false && measured > budget {\n        let changed = compact_strings(&mut result.value);',
      target: ['--lib'], test: 'orientation_context::tests::context_intent_sdk_budget_boundaries' },
    { path: 'src/read_output.rs', before: '        let measured = estimate(result, false) as u64;', after: '        let measured = 0;',
      target: ['--lib'], test: 'read_output::tests::stabilized_orientation_receipts_respect_final_ceilings' },
    { path: 'src/read_output.rs', before: '        stabilize_orientation(result, toon);', after: '        update_output(result, toon);',
      target: ['--lib'], test: 'read_output::tests::stabilized_orientation_receipts_respect_final_ceilings' },
    { path: 'src/read_output.rs', before: '        && measure(&result, options.toon) > budget', after: '        && false && measure(&result, options.toon) > budget',
      target: ['--lib'], test: 'read_output::tests::shared_read_commands_bound_final_self_referential_receipts' },
    { path: 'src/orientation_deps/packet.rs', before: '(2 * 1024 * 1024usize).saturating_sub(self.packet_bytes)', after: '(2 * 1024 * 1024usize)',
      target: ['--lib'], test: 'orientation_deps::packet::tests::packet_producer_cache_cumulative_admission' },
    { path: 'src/orientation_deps/packet.rs', before: 'json!(query.tokens.to_string().len())', after: 'json!(0)',
      target: ['--lib'], test: 'orientation_deps::packet::tests::packet_cached_rendering_preserves_budget_digit_widths' },
    ...[
      { before: '.join(";")', after: '.join(",")' },
      { before: 'absent.push(json!([index, key]));', after: 'if false { absent.push(json!([index, key])); }' },
      { before: '.map(|(i, _)| i)', after: '.map(|(i, _)| i + 1)' },
    ].map(mutation => ({ path: 'src/toon_output.rs', ...mutation,
      target: ['--lib'], test: 'toon_output::tests::current_compact_tables_round_trip_through_pinned_sdk' })),
  ];
  let executed = 0;
  for (const proof of cases) {
    if (process.argv[2] && !proof.test.includes(process.argv[2])) continue;
    executed += 1;
    const path = join(scratch, proof.path);
    const fixed = readFileSync(path, 'utf8');
    assert.equal(fixed.split(proof.before).length, 2, `stale or ambiguous mutation: ${proof.path}: ${proof.before}`);
    verifyMutation(reverted => check(proof.target, proof.test, reverted),
      () => writeFileSync(path, fixed.replace(proof.before, proof.after)),
      () => writeFileSync(path, fixed));
  }
  // Verify the actual history tests with only their baseline oracle available.
  assert.ok(executed > 0, 'review proof filter selected no mutations');
  if (!process.argv[2]) {
    rmSync(join(scratch, 'tests/orientation-oracle/node_modules'));
    for (const name of ['history_invalid_filename_matches_oracle', 'history_reserved_patch_paths_match_oracle', 'history_matches_oracle', 'bounded_history_and_refusals_match_oracle']) {
      check(['--test', 'orientation_differential'], name, false);
    }
  }
} finally { rmSync(scratch, { recursive: true, force: true }); }
