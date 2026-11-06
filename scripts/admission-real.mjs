/** Measure a read-only copy of this public repository's tracker in a non-git directory. */
import assert from 'node:assert/strict';
import { mkdtemp, readdir, readFile, chmod, rm } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { createRequire } from 'node:module';
import { spawnSync } from 'node:child_process';
process.env.PM_SENTRY_DISABLED='1';
process.env.PM_TELEMETRY_SOURCE_CONTEXT='test';
const repo=fileURLToPath(new URL('../',import.meta.url));
const oracle=createRequire(new URL('../tests/oracle/package.json',import.meta.url));
const sdk=await import(pathToFileURL(oracle.resolve('@unbrained/pm-cli/sdk')).href);
const scratch=await mkdtemp(path.join(tmpdir(),'native-admission-public-'));
const root=path.join(scratch,'.agents/pm');
/** A complete sorted byte inventory also verifies that no runtime files appear. */
async function inventory(directory,prefix='') {
  const output=[];
  for (const entry of (await readdir(directory,{withFileTypes:true})).sort((a,b)=>a.name.localeCompare(b.name))) {
    const relative=path.join(prefix,entry.name), full=path.join(directory,entry.name);
    if(entry.isDirectory()) output.push(...await inventory(full,relative));
    else { const bytes=await readFile(full); output.push({relative,bytes:bytes.length,sha256:createHash('sha256').update(bytes).digest('hex')}); }
  }
  return output;
}
/** Protect the copy after SDK reads; restore directory permissions only for owned cleanup. */
async function modes(directory,readonly) {
  if(!readonly) await chmod(directory,0o700);
  for(const entry of await readdir(directory,{withFileTypes:true})) {
    const full=path.join(directory,entry.name);
    if(entry.isDirectory()) await modes(full,readonly); else await chmod(full,readonly?0o400:0o600);
  }
  if(readonly) await chmod(directory,0o500);
}
try {
  const archive=spawnSync('git',['archive','origin/main','.agents/pm'],{cwd:repo,maxBuffer:64*1024*1024});
  assert.equal(archive.status,0,'public tracker archive failed');
  assert.equal(spawnSync('tar',['-x','-C',scratch],{input:archive.stdout}).status,0,'scratch extraction failed');
  const archiveBaseline=await inventory(root);
  const listed=await sdk.listAllComplete({includeBody:true},{pmRoot:root,noExtensions:true});
  const ids=listed.items.slice(0,3).map(item=>item.id);
  assert.equal(ids.length,3);
  const states=await Promise.all(ids.map(id=>sdk.get(id,{}, {pmRoot:root,noExtensions:true})));
  // Published SDK reads may create disposable metadata caches and usage receipts.
  // Certify state first, then freeze the complete prepared copy before native work.
  const baseline=await inventory(root);
  const prepared=new Map(baseline.map(file=>[file.relative,file.sha256]));
  assert.ok(archiveBaseline.every(file=>prepared.get(file.relative)===file.sha256),'SDK preparation changed public source bytes');
  const archivedNames=new Set(archiveBaseline.map(file=>file.relative));
  const extraFiles=baseline.filter(file=>!archivedNames.has(file.relative));
  assert.ok(extraFiles.every(file=>file.relative.startsWith(`runtime${path.sep}`)),'SDK preparation added non-runtime data');
  const preparationFiles=extraFiles.length;
  await modes(root,true);
  const mutations=[...ids.map(id=>({op:'update',id,options:{description:'Read-only admission validation'}})),{op:'update',id:'pm-rust-absent-admission-target',options:{title:'Missing target'}}];
  const input=JSON.stringify({author:'fixture-agent',mutations});
  const started=performance.now();
  const binary=process.argv[2]??path.join(repo,'target/debug/pm-rust');
  const native=spawnSync(binary,['--workspace',scratch,'preview-item-mutations','--json'],{input,encoding:'utf8',env:{...process.env,PATH:'',GIT_CEILING_DIRECTORIES:path.dirname(scratch)}});
  const elapsed=performance.now()-started;
  assert.equal(native.status,0,native.stderr);
  const preview=JSON.parse(native.stdout);
  assert.equal(preview.ok,false);
  assert.equal(preview.source_rows,listed.items.length);
  assert.deepEqual(preview.diagnostics.map(row=>row.state),['admitted','admitted','admitted','refused']);
  const after=await inventory(root);
  const expected=new Map(baseline.map(file=>[file.relative,file]));
  const actual=new Map(after.map(file=>[file.relative,file]));
  const differences=[...new Set([...expected.keys(),...actual.keys()])].filter(name=>expected.get(name)?.sha256!==actual.get(name)?.sha256);
  assert.equal(differences.length,0,JSON.stringify(differences));
  assert.deepEqual((await sdk.listAllComplete({includeBody:true},{pmRoot:root,noExtensions:true})).items,listed.items);
  assert.deepEqual(await Promise.all(ids.map(id=>sdk.get(id,{}, {pmRoot:root,noExtensions:true}))),states);
  const ref=spawnSync('git',['rev-parse','origin/main'],{cwd:repo,encoding:'utf8'});
  console.log(JSON.stringify({corpus:'public unbraind/pm-rust tracker',ref:ref.stdout.trim(),tracker_items:listed.items.length,source_rows:preview.source_rows,source_bytes_per_observation:preview.source_bytes,source_entries_per_observation:preview.source_entries,tracker_files:archiveBaseline.length,tracker_bytes:archiveBaseline.reduce((n,file)=>n+file.bytes,0),preparation_runtime_files:preparationFiles,frozen_files:baseline.length,input_bytes:Buffer.byteLength(input),output_bytes:Buffer.byteLength(native.stdout),admitted:3,refused:1,elapsed_ms:Math.round(elapsed),source_bytes_unchanged:true,sdk_readback_unchanged:true,git_and_node_on_native_path:false}));
} finally { await modes(root,false).catch(()=>{}); await rm(scratch,{recursive:true,force:true}); }
