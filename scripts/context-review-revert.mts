/** Independent review mutations run only in a disposable source copy with real pinned oracles. */
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { cpSync, mkdirSync, mkdtempSync, readFileSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { checkedGit } from './context-revert-helpers.mts';

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
  const args = node ? target.slice(1) : ['test', '-j', '2', '--locked', ...target, name, '--', '--exact', '--test-threads=2'];
  const result = spawnSync(node ? process.execPath : 'cargo', args, {
    cwd: scratch, env: childEnv, encoding: 'utf8', timeout: 900_000, maxBuffer: 8 * 1024 * 1024,
  });
  const output = `${result.stdout}\n${result.stderr}`;
  assert.ifError(result.error);
  assert.equal(result.status, reverted ? (node ? 1 : 101) : 0, output);
  assert.doesNotMatch(output, /could not compile/);
  assert.ok(node ? output.includes(reverted ? 'fail 1' : 'fail 0') : output.includes(`test ${name} ... ${reverted ? 'FAILED' : 'ok'}`), output);
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
    { path: 'scripts/context-revert-helpers.mts', before: "${result.stderr ?? ''}", after: '',
      target: ['node', '--test', '--test-concurrency=2', 'scripts/context-intents-revert.test.mts'], test: 'revert Git failures preserve diagnostics for source scans and patch checks' },
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
    { path: 'src/read_output.rs', before: '            let measured = estimate(result, false) as u64;', after: '            let measured = 0;',
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
    check(proof.target, proof.test, false);
    try {
      writeFileSync(path, fixed.replace(proof.before, proof.after));
      check(proof.target, proof.test, true);
    } finally { writeFileSync(path, fixed); }
    check(proof.target, proof.test, false);
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
