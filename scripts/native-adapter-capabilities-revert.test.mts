/** Exercise the revert runner's real subprocess and artifact boundary without compiling Rust. */
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';

for (const mode of ['existing', 'absent', 'first-failure', 'second-failure']) {
  test(`capability revert isolates checkout artifacts: ${mode}`, () => {
    const scratch = mkdtempSync(join(tmpdir(), 'capability-runner-test-'));
    try {
      const root = join(scratch, 'repo');
      for (const name of ['scripts', 'src', 'tests', 'target/debug']) mkdirSync(join(root, name), { recursive: true });
      for (const name of ['Cargo.toml', 'Cargo.lock', 'build.rs']) writeFileSync(join(root, name), 'fixture');
      writeFileSync(join(root, 'src/discovery.rs'), '/* fixture */\npub fn discover_capabilities() {\n    fixed()\n}\n');
      writeFileSync(join(root, 'src/main.rs'), '/* fixture */\nfn write_discovery_to() {\n    fixed()\n}\n');
      const runner = join(root, 'scripts/native-adapter-capabilities-revert.mts');
      writeFileSync(runner, readFileSync(new URL('./native-adapter-capabilities-revert.mts', import.meta.url)));
      const binary = join(root, 'target/debug/pm-rust');
      if (mode !== 'absent') writeFileSync(binary, 'fixed executable');
      const shim = join(scratch, 'cargo-shim.mjs');
      writeFileSync(shim, `import childProcess from 'node:child_process';
import { syncBuiltinESMExports } from 'node:module';
import { mkdirSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
childProcess.spawnSync = (command, args, options) => {
if (command !== 'cargo') throw new Error('unexpected tool');
const target = options.env.CARGO_TARGET_DIR;
mkdirSync(join(target, 'debug'), {recursive:true});
writeFileSync(join(target, 'debug/pm-rust'), 'sabotaged executable');
const delivery = args.includes('--bin');
const fail = ${JSON.stringify(mode)} === (delivery ? 'second-failure' : 'first-failure');
return {status:fail ? 0 : 101, stderr:'', stdout:'test ' + (delivery ? 'tests::capabilities_writer_refusals' : 'discovery_is_complete_bounded_and_tracker_free') + ' ... ' + (fail ? 'ok' : 'FAILED')};
};
syncBuiltinESMExports();
`);
      const childEnv: NodeJS.ProcessEnv = { HOME: scratch };
      for (const name of ['SystemRoot', 'SystemDrive', 'windir', 'TEMP', 'TMP']) {
        const value = process.env[name];
        if (value !== undefined) childEnv[name] = value;
      }
      const result = spawnSync(process.execPath, ['--import', shim, runner], {
        cwd: root, encoding: 'utf8', timeout: 30000,
        env: childEnv,
      });
      assert.equal(result.error, undefined);
      assert.equal(result.status, mode.endsWith('failure') ? 1 : 0);
      if (mode === 'absent') assert.equal(existsSync(binary), false);
      else assert.equal(readFileSync(binary, 'utf8'), 'fixed executable');
    } finally { rmSync(scratch, { recursive: true, force: true }); }
  });
}
