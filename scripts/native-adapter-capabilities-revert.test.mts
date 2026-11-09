import { syncSpawnBridge } from './context-test-process-shim.mts';
/** Exercise the revert runner's real subprocess and artifact boundary without compiling Rust. */
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';
import { fileURLToPath, pathToFileURL } from 'node:url';

for (const runner of ['native-adapter-capabilities-revert.mts', 'native-adapter-capabilities-review-revert.mts']) {
  test(`capability copy filters both path separators: ${runner}`, () => {
    const scratch = mkdtempSync(join(tmpdir(), 'capability-copy-test-'));
    try {
      const shim = join(scratch, 'copy-shim.mjs');
      writeFileSync(shim, `import assert from 'node:assert/strict';
import fs from 'node:fs';
import { syncBuiltinESMExports } from 'node:module';
fs.cpSync = (_source, _destination, options) => {
  for (const path of ['tests/oracle/node_modules', 'tests/oracle/node_modules/child', 'C:/repo/node_modules/pkg', 'C:\\\\repo\\\\tests\\\\oracle\\\\node_modules', 'C:\\\\repo\\\\node_modules\\\\pkg', 'tests\\\\oracle/node_modules\\\\pkg']) {
    assert.equal(options.filter(path), false, path);
  }
  for (const path of ['tests/oracle', 'src/discovery.rs', 'C:\\\\repo\\\\src', 'node_modules_backup/pkg', 'tests/my_node_modules/pkg']) {
    assert.equal(options.filter(path), true, path);
  }
  throw new Error('separator corpus verified');
};
syncBuiltinESMExports();
`);
      const result = spawnSync(process.execPath, ['--import', pathToFileURL(shim).href, fileURLToPath(new URL(runner, import.meta.url))], {
        encoding: 'utf8', timeout: 30000, env: { HOME: scratch, TMPDIR: scratch, TEMP: scratch, TMP: scratch },
      });
      assert.equal(result.error, undefined);
      assert.equal(result.status, 1);
      assert.match(result.stderr, /Error: separator corpus verified/);
    } finally { rmSync(scratch, { recursive: true, force: true }); }
  });
}

for (const mode of ['existing', 'absent', 'first-failure', 'second-failure']) {
  test(`capability revert isolates checkout artifacts: ${mode}`, () => {
    const scratch = mkdtempSync(join(tmpdir(), 'capability-runner-test-'));
    try {
      const root = join(scratch, 'repo');
      for (const name of ['scripts', 'src', 'tests', 'examples', 'target/debug']) mkdirSync(join(root, name), { recursive: true });
      for (const name of ['Cargo.toml', 'Cargo.lock', 'build.rs']) writeFileSync(join(root, name), 'fixture');
      writeFileSync(join(root, 'src/discovery.rs'), '/* fixture */\npub fn discover_capabilities() {\n    fixed()\n}\n');
      writeFileSync(join(root, 'src/main.rs'), '/* fixture */\nfn write_discovery_to() {\n    fixed()\n}\n');
      const runner = join(root, 'scripts/native-adapter-capabilities-revert.mts');
      writeFileSync(runner, readFileSync(new URL('./native-adapter-capabilities-revert.mts', import.meta.url)));
      for (const helper of ['context-revert-helpers.mts', 'context-proof-process.mts', 'context-windows-job.mts']) writeFileSync(join(root, 'scripts', helper), readFileSync(new URL(helper, import.meta.url)));
      const binary = join(root, 'target/debug/pm-rust');
      if (mode !== 'absent') writeFileSync(binary, 'fixed executable');
      // A fragment marker also catches path-as-specifier bugs on POSIX hosts.
      const shim = join(scratch, 'cargo # shim.mjs');
      writeFileSync(shim, `import childProcess from 'node:child_process';
import { syncBuiltinESMExports } from 'node:module';
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
childProcess.spawnSync = (command, args, options) => {
if (command !== 'cargo') throw new Error('unexpected tool');
const target = options.env.CARGO_TARGET_DIR;
mkdirSync(join(target, 'debug'), {recursive:true});
writeFileSync(join(target, 'debug/pm-rust'), 'sabotaged executable');
const delivery = args.includes('--bin');
const fail = ${JSON.stringify(mode)} === (delivery ? 'second-failure' : 'first-failure');
const mutated = readFileSync(join(options.cwd, delivery ? 'src/main.rs' : 'src/discovery.rs'), 'utf8').includes(delivery ? 'let _ = (writer' : 'let _ = request');
return {status:mutated && !fail ? 101 : 0, stderr:'', stdout:'test ' + (delivery ? 'tests::capabilities_writer_refusals' : 'discovery_is_complete_bounded_and_tracker_free') + ' ... ' + (mutated && !fail ? 'FAILED' : 'ok')};
};
${syncSpawnBridge}
syncBuiltinESMExports();
`);
      const childEnv: NodeJS.ProcessEnv = { HOME: scratch };
      for (const name of ['SystemRoot', 'SystemDrive', 'windir', 'TEMP', 'TMP']) {
        const value = process.env[name];
        if (value !== undefined) childEnv[name] = value;
      }
      const result = spawnSync(process.execPath, ['--import', pathToFileURL(shim).href, runner], {
        cwd: root, encoding: 'utf8', timeout: 30000,
        env: childEnv,
      });
      assert.equal(result.error, undefined);
      assert.equal(result.status, mode.endsWith('failure') ? 1 : 0, result.stderr);
      if (mode.endsWith('failure')) {
        assert.match(result.stderr, /0\s*!==\s*101/);
        if (mode === 'first-failure') assert.equal(result.stdout, '');
        else assert.equal(JSON.parse(result.stdout).reverted_test, 'discovery_is_complete_bounded_and_tracker_free');
      }
      if (mode === 'absent') assert.equal(existsSync(binary), false);
      else assert.equal(readFileSync(binary, 'utf8'), 'fixed executable');
    } finally { rmSync(scratch, { recursive: true, force: true }); }
  });
}
