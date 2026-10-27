/** Behavioral regressions run only in an owned disposable copy, never the reviewed checkout. */
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { cpSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';

const root = resolve(import.meta.dirname, '..');
const cases = [
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

/** One bounded subprocess result; compilation failures never count as regressions. */
interface Outcome { status: number | null; stdout: string; stderr: string; error?: Error; }

/** Run in a fresh owned POSIX process group with a bounded compiler/test lifetime. */
async function revertedTest(test: string, childEnv: NodeJS.ProcessEnv): Promise<Outcome> {
  return await new Promise(resolveResult => {
    const child=spawn('cargo',['+1.90.0','test','--locked','--lib',`changes::tests::${test}`,'--','--exact','--test-threads=2'],
      {cwd:scratch,env:childEnv,detached:process.platform!=='win32',stdio:['ignore','pipe','pipe']});
    let stdout=''; let stderr=''; let bytes=0; let error: Error | undefined;
    let escalation: ReturnType<typeof setTimeout> | undefined;
    /** Signal only this invocation's own group, never another compiler/test process. */
    const stop=(signal: NodeJS.Signals): void => {
      try {
        if (process.platform!=='win32' && child.pid!==undefined) process.kill(-child.pid,signal);
        else child.kill(signal);
      } catch { /* The owned process group already exited. */ }
    };
    const timer=setTimeout(()=> {
      error=new Error('bounded compilation/test timeout'); stop('SIGTERM');
      escalation=setTimeout(()=>stop('SIGKILL'),5000); escalation.unref();
    },900_000);
    /** Admit bounded diagnostics without exposing them in successful proof receipts. */
    const receive=(chunk: string, stream: 'stdout' | 'stderr'): void => {
      bytes+=Buffer.byteLength(chunk);
      if (bytes>2_097_152) { error=new Error('diagnostic byte ceiling'); stop('SIGKILL'); return; }
      if (stream==='stdout') stdout+=chunk; else stderr+=chunk;
    };
    child.stdout.setEncoding('utf8').on('data',(chunk: string)=>receive(chunk,'stdout'));
    child.stderr.setEncoding('utf8').on('data',(chunk: string)=>receive(chunk,'stderr'));
    child.on('error',(failure: Error)=> { error=failure; });
    child.on('close',status=> {
      clearTimeout(timer); if (escalation) clearTimeout(escalation);
      if (error) stop('SIGKILL');
      resolveResult({status,stdout,stderr,error});
    });
  });
}

try {
  for (const path of ['Cargo.toml','Cargo.lock','build.rs','src']) cpSync(join(root,path),join(scratch,path),{recursive:true});
  mkdirSync(join(scratch,'tests'));
  for (const path of ['support','fixtures']) cpSync(join(root,'tests',path),join(scratch,'tests',path),{recursive:true});
  const original=readFileSync(source,'utf8');
  const childEnv: NodeJS.ProcessEnv = {};
  for (const name of ['PATH','HOME','CARGO_HOME','RUSTUP_HOME','LD_LIBRARY_PATH','SYSTEMROOT','WINDIR','TMPDIR','TEMP','TMP']) {
    const value=process.env[name];
    if (value!==undefined) childEnv[name]=value;
  }

  childEnv.CARGO_TARGET_DIR=join(root,'target');
  childEnv.CARGO_BUILD_JOBS='2';
  childEnv.PM_SENTRY_DISABLED='1';
  childEnv.PM_TELEMETRY_SOURCE_CONTEXT='test';
  for (const proof of proofs) {
    assert.equal(original.split(proof.before).length,2,`unique behavioral change: ${proof.name}`);
    writeFileSync(source,original.replace(proof.before,proof.after));
    const result=await revertedTest(proof.test,childEnv);
    assert.equal(result.error,undefined,`${proof.name} must finish within its bounded compilation window`);
    assert.notEqual(result.status,0,`${proof.name} must fail`);
    assert.match(result.stdout,/test result: FAILED/u,`compile/launch failures are not revert proof: ${result.stderr}`);
    console.log(JSON.stringify({behavior:proof.name,test:proof.test,failed_as_required:true}));
  }
} finally { rmSync(scratch,{recursive:true,force:true}); }
