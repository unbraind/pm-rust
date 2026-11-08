/** Prove review regressions with independent behavior reverts in an isolated non-git copy. */
import assert from 'node:assert/strict';
import { assertNamedTests, verifyMutation } from './context-revert-helpers.mts';
import { spawnSync } from 'node:child_process';
import { cpSync, mkdirSync, mkdtempSync, readFileSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';

const root = resolve(import.meta.dirname, '..');
const scratch = mkdtempSync(join(tmpdir(), 'capabilities-review-revert-'));
const library = join(scratch, 'src/discovery.rs');

/** Replace one uniquely identified behavior, refusing stale mutation anchors. */
function replace(source: string, before: string, after: string): string {
  assert.equal(source.split(before).length, 2, `unique anchor: ${before}`);
  return source.replace(before, after);
}

try {
  for (const name of ['Cargo.toml', 'Cargo.lock', 'build.rs', 'src', 'tests', 'scripts', 'examples']) {
    cpSync(join(root, name), join(scratch, name), { recursive: true, filter: path => !path.split(/[\\/]/).includes('node_modules') });
  }
  // Only the already installed, pinned test oracle is shared; no build artifacts are shared.
  symlinkSync(join(root, 'tests/oracle/node_modules'), join(scratch, 'tests/oracle/node_modules'), 'dir');
  const original = readFileSync(library, 'utf8');
  const mainPath = join(scratch, 'src/main.rs');
  const originalMain = readFileSync(mainPath, 'utf8');
  const runner = join(scratch, 'scripts/native-adapter-capabilities-revert.mts');
  const runnerSource = readFileSync(runner, 'utf8');
  const nodeTests = join(scratch, 'scripts/native-adapter-capabilities-revert.test.mts');
  const baseline = spawnSync(process.execPath, ['--test', '--test-reporter=tap', '--test-concurrency=2', nodeTests], { encoding: 'utf8', timeout: 60000 });
  assertNamedTests(baseline, ['capability revert isolates checkout artifacts: existing'], false, true);
  writeFileSync(runner, runnerSource.replaceAll("CARGO_TARGET_DIR: join(scratch, 'target')", "CARGO_TARGET_DIR: join(root, 'target')"));
  const isolation = spawnSync(process.execPath, ['--test', '--test-reporter=tap', '--test-concurrency=2', nodeTests], {
    encoding: 'utf8', env: { PATH: process.env.PATH, HOME: process.env.HOME }, timeout: 60000,
  });
  assertNamedTests(isolation, ['capability revert isolates checkout artifacts: existing'], true, true);
  assert.match(isolation.stdout, /sabotaged executable/);
  console.log(JSON.stringify({ behavior: 'checkout binary isolation', failed_as_required: true }));
  writeFileSync(runner, runnerSource);

  // Exercise the actual copy callbacks with Windows and POSIX input on every host.
  for (const name of ['native-adapter-capabilities-revert.mts', 'native-adapter-capabilities-review-revert.mts']) {
    const path = join(scratch, 'scripts', name);
    const fixed = readFileSync(path, 'utf8');
    writeFileSync(path, replace(fixed, "path.split(/[\\\\/]/)", "path.split('/')"));
    const result = spawnSync(process.execPath, ['--test', '--test-reporter=tap', '--test-concurrency=2', '--test-name-pattern=capability copy filters', join(scratch, 'scripts/native-adapter-capabilities-revert.test.mts')], {
      encoding: 'utf8', env: { PATH: process.env.PATH, HOME: process.env.HOME }, timeout: 60000,
    });
    assertNamedTests(result, [`capability copy filters both path separators: ${name}`], true, true);
    assert.match(result.stdout, /tests 2/);
    assert.match(result.stdout, /pass 1/);
    assert.match(result.stdout, /fail 1/);
    console.log(JSON.stringify({ behavior: `separator-independent copy: ${name}`, passed: 1, failed: 1 }));
    writeFileSync(path, fixed);
  }

  const local = '    let local = operations\n        .iter()\n        .filter(|op| !["capabilities", "adapter-check", "help"].contains(&op.name.as_str()))\n        .map(|op| op.name.clone())\n        .collect();\n';
  const remote = '            .filter(|name: &String| {\n                request\n                    .operation\n                    .as_ref()\n                    .is_none_or(|selected| selected == name)\n            })\n';
  const cases = [
    { name: 'nested adapter SDK refusal classification', target: ['--lib'], test: 'discovery::tests::nested_adapter_refusal_codes_match_cli',
      mutate: (source: string) => replace(source, '        if let Some(adapter) = value.get("adapter").filter(|adapter| !adapter.is_null()) {\n            serde_json::from_value::<AdapterManifest>(adapter.clone())\n                .map_err(|_| refuse(DiscoveryRefusalCode::InvalidAdapter))?;\n        }\n', '') },
    { name: 'nested adapter actual CLI parity', target: ['--test', 'capability_contract'], test: 'nested_adapter_refusals_match_sdk_and_cli',
      mutate: (source: string) => replace(source, '        if let Some(adapter) = value.get("adapter").filter(|adapter| !adapter.is_null()) {\n            serde_json::from_value::<AdapterManifest>(adapter.clone())\n                .map_err(|_| refuse(DiscoveryRefusalCode::InvalidAdapter))?;\n        }\n', '') },
    { name: 'reciprocal parser conflicts', target: ['--lib'], test: 'discovery::tests::parser_conflicts_are_reciprocal',
      mutate: (source: string) => replace(source, '                            || command\n                                .get_arg_conflicts_with(other)\n                                .iter()\n                                .any(|value| value.get_id() == arg.get_id())', '') },
    { name: 'independent missing-default parser oracle', target: ['--lib'], test: 'discovery::tests::missing_defaults_match_bare_parser_execution',
      mutate: (source: string) => replace(source, 'strings(&[crate::cli::VALIDATE_CLOSE_DEFAULT])', 'strings(&["incorrect-default"])') },
    { name: 'object-only request', target: ['--lib'], test: 'discovery::tests::discovery_rejects_positional_struct_arrays',
      mutate: (source: string) => replace(source, '        if !value.is_object() {\n            return Err(refuse(DiscoveryRefusalCode::InvalidRequest));\n        }\n', '') },
    { name: 'object-only nested adapter', target: ['--lib'], test: 'discovery::tests::discovery_rejects_positional_struct_arrays',
      mutate: (source: string) => replace(source, '        if value\n            .get("adapter")\n            .is_some_and(|adapter| !adapter.is_null() && !adapter.is_object())\n        {\n            return Err(refuse(DiscoveryRefusalCode::InvalidAdapter));\n        }\n', '') },
    { name: 'object-only CLI adapter', target: ['--lib'], test: 'discovery::tests::discovery_rejects_positional_struct_arrays',
      mutate: (source: string) => replace(source, '                if !value.is_object() {\n                    return Err(refuse(DiscoveryRefusalCode::InvalidAdapter));\n                }\n', '') },
    { name: 'lifecycle wrapper SDK mappings', target: ['--lib'], test: 'discovery::tests::ownership_sdk_mappings_include_public_entry_points',
      mutate: (source: string) => replace(source, '        "start-task" => (\n            &[\n                "Workspace::start_task",', '        "start-task" => (\n            &[\n') },
    { name: 'claim option SDK mappings', target: ['--lib'], test: 'discovery::tests::ownership_sdk_mappings_include_public_entry_points',
      mutate: (source: string) => replace(source, '        "claim" => (\n            &[\n                "Workspace::claim_with_options",', '        "claim" => (\n            &[\n') },
    { name: 'selected local backend', target: ['--lib'], test: 'discovery::tests::selected_operation_filters_every_backend',
      mutate: (source: string) => replace(replace(source, local, ''), '    let operations: Vec<_> = all\n', `${local.replace('local = operations', 'local = all')}    let operations: Vec<_> = all\n`) },
    { name: 'selected adapter backend', target: ['--lib'], test: 'discovery::tests::selected_operation_filters_every_backend',
      mutate: (source: string) => replace(source, remote, '') },
    { name: 'ownership wire encodings', target: ['--test', 'capability_contract'], test: 'ownership_encodings_match_default_cli_output',
      mutate: (source: string) => replace(source, '    } else if [\n        "get",\n        "create",\n        "update",\n        "comment",\n        "claim",\n        "release",\n        "start-task",\n        "pause-task",\n    ]\n    .contains(&name)\n    {', '    } else if ["get", "create", "update", "comment"].contains(&name) {') },
    { name: 'session producer comparison', target: ['--test', 'capability_contract'], test: 'session_refusal_producer_matches_recovery_descriptor',
      mutate: (source: string) => replace(source, '("detail", "string"),', '("renamed_detail", "string"),') },
    { name: 'discovery producer refusal preservation', target: ['--bin', 'pm-rust'], test: 'tests::capabilities_writer_refusals', main: true,
      mutate: (source: string) => replace(source, '    let rendered = response?.render(json)?;',
        '    let rendered = response.or_else(|_| pm_rust::discovery::discover_capabilities(&Default::default()))?.render(json)?;') },
  ];
  const childEnv: NodeJS.ProcessEnv = { CARGO_BUILD_JOBS: '2', CARGO_TARGET_DIR: join(scratch, 'target'), RUST_TEST_THREADS: '2' };
  for (const name of ['PATH', 'HOME', 'CARGO_HOME', 'RUSTUP_HOME', 'LD_LIBRARY_PATH', 'SYSTEMROOT', 'WINDIR', 'TMPDIR', 'TEMP', 'TMP']) {
    const value = process.env[name];
    if (value !== undefined) childEnv[name] = value;
  }
  mkdirSync(join(scratch, 'target'), { recursive: true });
  for (const proof of cases) {
    writeFileSync(library, original);
    writeFileSync(mainPath, originalMain);
    verifyMutation(reverted => {
      const result = spawnSync('cargo', ['+1.90.0', 'test', '-j', '2', '--locked', ...proof.target, proof.test, '--', '--exact', '--test-threads=2'], {
        cwd: scratch, env: childEnv, encoding: 'utf8', timeout: 900000, maxBuffer: 4 * 1024 * 1024,
      });
      assertNamedTests(result, [proof.test], reverted);
    }, () => {
      if (proof.main) writeFileSync(mainPath, proof.mutate(originalMain));
      else writeFileSync(library, proof.mutate(original));
    }, () => {
      writeFileSync(library, original);
      writeFileSync(mainPath, originalMain);
    });
    console.log(JSON.stringify({ behavior: proof.name, test: proof.test, passed: 0, failed: 1, compile_succeeded: true }));
  }
  writeFileSync(library, original);
  writeFileSync(mainPath, originalMain);
  // Rerun every affected Rust regression after restoring the implementation.
  for (const target of [['--lib', 'discovery::'], ['--test', 'capability_contract'], ['--bin', 'pm-rust', 'tests::capabilities_writer_refusals']]) {
    const result = spawnSync('cargo', ['+1.90.0', 'test', '--locked', ...target, '--', '--test-threads=2'], {
      cwd: scratch, env: childEnv, encoding: 'utf8', timeout: 900000, maxBuffer: 4 * 1024 * 1024,
    });
    assert.equal(result.status, 0, result.stderr);
    console.log(result.stdout.match(/test result: ok\..*/)?.[0]);
  }
} finally { rmSync(scratch, { recursive: true, force: true }); }
