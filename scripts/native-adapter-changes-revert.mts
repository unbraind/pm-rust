/** Behavioral regressions run only in an owned disposable copy, never the reviewed checkout. */
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { cpSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';

const root = resolve(import.meta.dirname, '..');
const scratch = mkdtempSync(join(tmpdir(), 'native-changes-revert-'));
const source = join(scratch, 'src/changes.rs');
const cases = [
  { name:'exclusive resume', test:'stale_prefixes_and_partial_appends',
    before:'let previous = cursor.prefixes.get(id);', after:'let previous: Option<&Prefix> = None;' },
  { name:'prefix integrity', test:'stale_prefixes_and_partial_appends',
    before:'crate::history::sha256_digest(&raw[..bytes]) != prefix.hash', after:'false' },
  { name:'cancellation before checkpoint admission', test:'backend_refusals_and_cancelled_exchange_retain_checkpoint',
    before:'page.validate(&self.request)?;\n        cancelled(&self.cancellation)?;', after:'page.validate(&self.request)?;' },
  { name:'canonical field projection', test:'seeded_500_sequences_preserve_physical_events_and_foreign_data',
    before:'patch_count: entry.get("patch")?.as_array()?.len() as u64,', after:'patch_count: 0,' },
];
try {
  for (const path of ['Cargo.toml','Cargo.lock','build.rs','src']) cpSync(join(root,path),join(scratch,path),{recursive:true});
  mkdirSync(join(scratch,'tests'));
  for (const path of ['support','fixtures']) cpSync(join(root,'tests',path),join(scratch,'tests',path),{recursive:true});
  const original=readFileSync(source,'utf8');
  const childEnv: NodeJS.ProcessEnv = Object.fromEntries(Object.entries(process.env).filter(([name])=> !/token|secret|password|credential|auth|npm_media_server/iu.test(name)));

  childEnv.CARGO_TARGET_DIR=join(root,'target');
  childEnv.CARGO_BUILD_JOBS='2';
  childEnv.PM_SENTRY_DISABLED='1';
  childEnv.PM_TELEMETRY_SOURCE_CONTEXT='test';
  for (const proof of cases) {
    assert.equal(original.split(proof.before).length,2,`unique behavioral change: ${proof.name}`);
    writeFileSync(source,original.replace(proof.before,proof.after));
    const result=spawnSync('cargo',['+1.90.0','test','--locked','--lib',`changes::tests::${proof.test}`,'--','--exact','--test-threads=2'],{cwd:scratch,env:childEnv,encoding:'utf8',maxBuffer:2_097_152,timeout:180_000});
    assert.notEqual(result.status,0,`${proof.name} must fail`);
    assert.match(result.stdout,/test result: FAILED/u,`compile/launch failures are not revert proof: ${result.stderr}`);
    console.log(JSON.stringify({behavior:proof.name,test:proof.test,failed_as_required:true}));
  }
} finally { rmSync(scratch,{recursive:true,force:true}); }
