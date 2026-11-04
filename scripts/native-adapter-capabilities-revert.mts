import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { cpSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';

const root = resolve(import.meta.dirname, '..');
const scratch = mkdtempSync(join(tmpdir(), 'native-capabilities-revert-'));
try {
  for (const name of ['Cargo.toml', 'Cargo.lock', 'src', 'tests']) cpSync(join(root, name), join(scratch, name), { recursive: true, filter: path => !path.split('/').includes('node_modules') });
  const path = join(scratch, 'src/discovery.rs');
  const source = readFileSync(path, 'utf8');
  const start = source.indexOf('pub fn discover_capabilities(');
  const body = source.indexOf('{', start);
  const end = source.indexOf('\n}', body) + 2;
  assert.ok(start > 0 && body > start && end > body);
  writeFileSync(path, `${source.slice(0, body + 1)}\n    let _ = request;\n    Err(refuse(DiscoveryRefusalCode::UnsupportedOperation))\n}\n${source.slice(end)}`);
  // Dependencies share the existing build cache; all mutated source is disposable.
  const result = spawnSync('cargo', ['+1.90.0', 'test', '--locked', '--test', 'capability_contract', 'discovery_is_complete_bounded_and_tracker_free', '--', '--exact', '--test-threads=2'], {
    cwd: scratch, encoding: 'utf8', timeout: 600000,
    env: { PATH: process.env.PATH, HOME: process.env.HOME, CARGO_BUILD_JOBS: '2', CARGO_TARGET_DIR: join(root, 'target') },
  });
  assert.equal(result.status, 101);
  assert.match(result.stdout, /test discovery_is_complete_bounded_and_tracker_free \.\.\. FAILED/);
  assert.doesNotMatch(result.stderr, /could not compile/);
  console.log(JSON.stringify({ proof: 'replace only discover_capabilities with unsupported-operation refusal in a disposable non-git copy', compile_succeeded: true, reverted_test: 'discovery_is_complete_bounded_and_tracker_free', passed: 0, failed: 1, reviewed_source_untouched: true }));
  writeFileSync(path, source);
  const mainPath = join(scratch, 'src/main.rs');
  const main = readFileSync(mainPath, 'utf8');
  const writerStart = main.indexOf('fn write_discovery_to(');
  const writerBody = main.indexOf('{', writerStart);
  const writerEnd = main.indexOf('\n}', writerBody) + 2;
  assert.ok(writerStart > 0 && writerBody > writerStart && writerEnd > writerBody);
  writeFileSync(mainPath, `${main.slice(0, writerBody + 1)}\n    let _ = (writer, response, json);\n    Ok(())\n}\n${main.slice(writerEnd)}`);
  const delivery = spawnSync('cargo', ['+1.90.0', 'test', '--locked', '--bin', 'pm-rust', 'tests::capabilities_writer_refusals', '--', '--exact', '--test-threads=2'], {
    cwd: scratch, encoding: 'utf8', timeout: 600000,
    env: { PATH: process.env.PATH, HOME: process.env.HOME, CARGO_BUILD_JOBS: '2', CARGO_TARGET_DIR: join(root, 'target') },
  });
  assert.equal(delivery.status, 101);
  assert.match(delivery.stdout, /test tests::capabilities_writer_refusals \.\.\. FAILED/);
  assert.doesNotMatch(delivery.stderr, /could not compile/);
  console.log(JSON.stringify({ proof: 'replace only write_discovery_to with false success in the same disposable copy', compile_succeeded: true, reverted_test: 'tests::capabilities_writer_refusals', passed: 0, failed: 1, reviewed_source_untouched: true }));
} finally { rmSync(scratch, { recursive: true, force: true }); }
