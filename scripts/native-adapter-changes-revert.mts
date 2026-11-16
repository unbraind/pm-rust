import { assertionFailureDiagnostic } from './context-proof-diagnostics.mts';
import { proofCopyFilter, registerCargoConfiguration } from './context-revert-helpers.mts';
import { absentLink } from './context-revert-helpers.mts';
/** Behavioral regressions run only in an owned disposable copy, never the reviewed checkout. */
import assert from 'node:assert/strict';
import { assertNamedTests } from './context-revert-helpers.mts';
import { ProofProcesses } from './context-proof-process.mts';
import { cpSync, mkdirSync, mkdtempSync, readFileSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';

const root = resolve(import.meta.dirname, '..');
const cases = [
  { name:'UTF-8 filename admission', test:'history_filename_bytes_validate_before_filesystem_access',
    before:'.and_then(|name| name.to_str())\n        .map(str::to_owned)', after:'.map(|name| name.to_string_lossy().into_owned())' },
  { name:'duplicate event admission', test:'duplicate_backend_positions_refuse_before_cursor_admission',
    before:'        if !unique {', after:'        if false {' },
  { name:'delayed reply correlation', test:'delayed_older_pages_cannot_rewind_a_reader',
    before:'request_id: format!("changes-{digest}"),', after:'request_id: "changes".into(),' },
  { name:'exclusive resume', test:'stale_prefixes_and_partial_appends',
    before:'let previous = cursor.prefixes.get(id);', after:'let previous: Option<&Prefix> = None;' },
  { name:'prefix integrity', test:'stale_prefixes_and_partial_appends',
    before:'crate::history::sha256_digest(&raw[..bytes]) != prefix.hash', after:'false' },
  { name:'cancellation before checkpoint admission', test:'backend_refusals_and_cancelled_exchange_retain_checkpoint',
    before:'page.validate(&self.request)?;\n        cancelled(&self.cancellation)?;', after:'page.validate(&self.request)?;' },
  { name:'canonical field projection', test:'seeded_500_sequences_preserve_physical_events_and_foreign_data',
    before:'patch_count: entry.get("patch")?.as_array()?.len() as u64,', after:'patch_count: 0,' },
];
const selectedCase=process.argv[2];
const proofs=selectedCase===undefined ? cases : cases.filter(proof=>proof.name===selectedCase);
assert.ok(process.argv.length<=3 && proofs.length>0,'pass one documented proof name or omit it for all proofs');
const scratch=mkdtempSync(join(tmpdir(),'native-changes-revert-'));
const source=join(scratch,'src/changes.rs');

registerCargoConfiguration(root, scratch);
const processes = new ProofProcesses();

/** Run the selected regression with the shared descendant and interruption safety contract. */
async function revertedTest(test: string, childEnv: NodeJS.ProcessEnv) {
  return processes.run('cargo', ['+1.90.0', 'test', '-j', '2', '--locked', '--lib', `changes::tests::${test}`, '--', '--exact', '--test-threads=2'],
    { cwd: scratch, env: childEnv, maxBuffer: 2_097_152 });
}

try {
  for (const path of ['Cargo.toml','Cargo.lock','build.rs','src','examples']) cpSync(join(root,path),join(scratch,path),{recursive:true,dereference:true,filter:path=>!absentLink(path) && proofCopyFilter(path)});
  mkdirSync(join(scratch,'tests'));
  for (const path of ['support','fixtures']) cpSync(join(root,'tests',path),join(scratch,'tests',path),{recursive:true,dereference:true,filter:path=>!absentLink(path) && proofCopyFilter(path)});
  const original=readFileSync(source,'utf8');
  const childEnv: NodeJS.ProcessEnv = {};
  for (const name of ['PATH','HOME','CARGO_HOME','RUSTUP_HOME','LD_LIBRARY_PATH','SYSTEMROOT','WINDIR','TMPDIR','TEMP','TMP']) {
    const value=process.env[name];
    if (value!==undefined) childEnv[name]=value;
  }

  childEnv.CARGO_TARGET_DIR=join(scratch,'target');
  childEnv.CARGO_BUILD_JOBS='2';
  childEnv.PM_SENTRY_DISABLED='1';
  childEnv.PM_TELEMETRY_SOURCE_CONTEXT='test';
  for (const proof of proofs) {
    assert.equal(original.split(proof.before).length,2,`unique behavioral change: ${proof.name}`);
    assertNamedTests(await revertedTest(proof.test,childEnv), [`changes::tests::${proof.test}`], false, false, assertionFailureDiagnostic(scratch, [`changes::tests::${proof.test}`]));
    try {
      writeFileSync(source,original.replace(proof.before,proof.after));
      assertNamedTests(await revertedTest(proof.test,childEnv), [`changes::tests::${proof.test}`], true, false, assertionFailureDiagnostic(scratch, [`changes::tests::${proof.test}`]));
    } finally { await processes.settled(); writeFileSync(source,original); }
    assertNamedTests(await revertedTest(proof.test,childEnv), [`changes::tests::${proof.test}`], false, false, assertionFailureDiagnostic(scratch, [`changes::tests::${proof.test}`]));
    console.log(JSON.stringify({behavior:proof.name,test:proof.test,failed_as_required:true}));
  }
} catch (error) {
  if (!processes.wasInterrupted) throw error;
} finally { await processes.cleanup(scratch); }
