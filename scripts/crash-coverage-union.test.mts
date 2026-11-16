import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';
import { collectProfiles, restoreProfiles } from './ci-coverage-profiles.mts';

// The ordinary crash gate has no LLVM component. CI's coverage merge job requires this fixture.
test('native LLVM profile union equals a combined run for every file and all four dimensions',
  { skip: process.env.PM_TEST_COVERAGE_MERGE !== 'required' }, () => {
    const root = mkdtempSync(join(tmpdir(), 'llvm-union-'));
    try {
      const toolchain = 'nightly-2026-08-06';
      const sysroot = execFileSync('rustc', [`+${toolchain}`, '--print', 'sysroot'], { encoding: 'utf8' }).trim();
      const version = execFileSync('rustc', [`+${toolchain}`, '-Vv'], { encoding: 'utf8' });
      const host = version.match(/^host: (.+)$/m)![1];
      const tool = (name: string) => join(sysroot, 'lib/rustlib', host, 'bin', name + (process.platform === 'win32' ? '.exe' : ''));
      writeFileSync(join(root, 'branches.rs'), 'pub fn side(positive: bool) -> u8 {\n if positive {\n  1\n } else {\n  2\n }\n}\n');
      writeFileSync(join(root, 'main.rs'), 'mod branches;\nfn main() {\n println!("{}", branches::side(std::env::args().len() > 1));\n}\n');
      const binary = join(root, 'fixture' + (process.platform === 'win32' ? '.exe' : ''));
      execFileSync('rustc', [`+${toolchain}`, '--edition=2024', '-C', 'instrument-coverage', '-Z', 'coverage-options=branch', join(root, 'main.rs'), '-o', binary]);
      const dirs = ['left', 'right', 'combined', 'merged'].map(name => { const path = join(root, name); mkdirSync(path); return path; });
      const run = (dir: string, args: string[]) => execFileSync(binary, args, { env: { ...process.env, LLVM_PROFILE_FILE: join(dir, '%m.profraw') } });
      run(dirs[0], ['positive']); run(dirs[1], []);
      run(dirs[2], ['positive']); run(dirs[2], []);
      const report = (dir: string) => {
        const profile = join(dir, 'all.profdata');
        execFileSync(tool('llvm-profdata'), ['merge', '-sparse', '--failure-mode=any', '-o', profile, ...readdirSync(dir).filter(name => name.endsWith('.profraw')).map(name => join(dir, name))]);
        return JSON.parse(execFileSync(tool('llvm-cov'), ['export', binary, `--instr-profile=${profile}`], { encoding: 'utf8' }));
      };
      const left = report(dirs[0]), right = report(dirs[1]);
      assert.ok(left.data[0].totals.branches.percent < 100 && right.data[0].totals.branches.percent < 100);
      const artifacts = [1, 2].map(shard => join(root, `artifact-${shard}`));
      artifacts.forEach((artifact, index) => collectProfiles(dirs[index], artifact, String(index + 1), 'fixture'));
      assert.equal(restoreProfiles(artifacts, dirs[3], 'fixture'), 2);
      const merged = report(dirs[3]);
      assert.deepEqual(merged.data[0].files, report(dirs[2]).data[0].files, 'every file, branch, segment and counter matches the combined run');
      for (const dimension of ['lines', 'functions', 'regions', 'branches']) assert.equal(merged.data[0].totals[dimension].percent, 100);
      assert.throws(() => restoreProfiles(artifacts, join(root, 'wrong-build'), 'other-build'), /identity/);
      writeFileSync(join(artifacts[1], 'profiles/0.profraw'), 'corrupt');
      assert.throws(() => restoreProfiles(artifacts, join(root, 'corrupt'), 'fixture'), /digest/);
      assert.equal(readFileSync(join(root, 'branches.rs'), 'utf8').includes('if positive'), true);
    } finally { rmSync(root, { recursive: true, force: true }); }
  });

test('Cargo target shards rebuild mapping and restore only shard hits at unchanged thresholds',
  { skip: process.env.PM_TEST_COVERAGE_MERGE !== 'required' }, () => {
    const root = mkdtempSync(join(tmpdir(), 'cargo-union-'));
    try {
      mkdirSync(join(root, 'src')); mkdirSync(join(root, 'tests'));
      writeFileSync(join(root, 'Cargo.toml'), '[package]\nname="pm-rust"\nversion="0.0.0"\nedition="2024"\n');
      writeFileSync(join(root, 'src/lib.rs'), 'pub fn branch(yes: bool) -> u8 {\n if yes {\n  1\n } else {\n  2\n }\n}\n');
      writeFileSync(join(root, 'tests/left.rs'), '#[test] fn left() { assert_eq!(pm_rust::branch(true), 1); }\n');
      writeFileSync(join(root, 'tests/right.rs'), '#[test] fn right() { assert_eq!(pm_rust::branch(false), 2); }\n');
      const env: NodeJS.ProcessEnv = { ...process.env, CARGO_TARGET_DIR: join(root, 'target'), CARGO_BUILD_JOBS: '2', CARGO_INCREMENTAL: '0' };
      for (const key of ['CARGO_LLVM_COV_TARGET_DIR', 'CARGO_LLVM_COV_BUILD_DIR', 'LLVM_PROFILE_FILE', 'RUSTFLAGS', 'CARGO_ENCODED_RUSTFLAGS']) delete env[key];
      const cargo = (args: string[]) => execFileSync('cargo', ['+nightly-2026-08-06', ...args], { cwd: root, env, encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] });
      cargo(['generate-lockfile', '--offline']);
      const rawRoot = join(root, 'target/llvm-cov-target');
      const artifacts = [join(root, 'artifact-1'), join(root, 'artifact-2')];
      for (const [index, target] of ['left', 'right'].entries()) {
        cargo(['llvm-cov', '--locked', '--branch', '--no-report', '--test', target]);
        collectProfiles(rawRoot, artifacts[index], String(index + 1), 'fixture-cargo');
        cargo(['llvm-cov', 'clean', '--workspace']);
      }
      cargo(['llvm-cov', '--locked', '--branch', '--no-report', '--all-targets', '--', '--list']);
      cargo(['llvm-cov', 'clean', '--profraw-only']);
      assert.ok(restoreProfiles(artifacts, rawRoot, 'fixture-cargo') >= 2);
      const merged = JSON.parse(cargo(['llvm-cov', 'report', '--json', '--failure-mode=any']));
      for (const dimension of ['lines', 'functions', 'regions', 'branches']) assert.equal(merged.data[0].totals[dimension].percent, 100);
      const combined = JSON.parse(cargo(['llvm-cov', '--locked', '--branch', '--all-targets', '--json']));
      assert.deepEqual(merged.data[0].files, combined.data[0].files, 'rebuilding all targets preserves exact per-file hits');
    } finally { rmSync(root, { recursive: true, force: true }); }
  });
