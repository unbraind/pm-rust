/** Adversarial boundary tests for every proof runner and real-copy receipt. */
import assert from 'node:assert/strict';
import { spawn, spawnSync } from 'node:child_process';
import { existsSync, lstatSync, mkdirSync, mkdtempSync, readFileSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';
import { setTimeout } from 'node:timers/promises';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { authoritative, benchmarkPreflight, verifiedOracleVersion } from './context-real-helpers.mts';
import { checkedGit, copyWorkingTree, supportedProofs } from './context-revert-helpers.mts';
import { ProofProcesses, verifyAsyncMutation } from './context-proof-process.mts';
import { windowsJob } from './context-windows-job.mts';
import { syncSpawnBridge } from './context-test-process-shim.mts';

const packageRoot = fileURLToPath(new URL('..', import.meta.url));
const runners = ['context-intents-revert.mts', 'context-packet-review-revert.mts', 'context-review-revert.mts', 'context-safety-revert.mts', 'native-adapter-revert.mts', 'native-adapter-capabilities-revert.mts', 'native-adapter-capabilities-review-revert.mts', 'native-adapter-changes-revert.mts', 'perf-revert-proof.mts'];

/** Build a public synthetic source copy with shared read-only oracle installations. */
function fixture(root: string): string {
  const source = join(root, 'source');
  mkdirSync(source);
  copyWorkingTree(packageRoot, source);
  checkedGit(source, ['init', '--quiet'], 'fixture init');
  writeFileSync(join(source, '.git/info/exclude'), 'node_modules\n');
  for (const oracle of ['oracle', 'orientation-oracle']) symlinkSync(join(packageRoot, 'tests', oracle, 'node_modules'), join(source, 'tests', oracle, 'node_modules'), 'junction');
  return source;
}

test('Windows signal proof selection preserves every supported case and reports skips', () => {
  const cases = [{ test: 'ordinary rust test' }, { test: 'runner SIGTERM proof' }, { test: 'runner SIGINT proof' }, { test: 'ordinary node test' }];
  const receipts: string[] = [];
  assert.deepEqual(supportedProofs(cases, undefined, 'win32', message => receipts.push(message)), [cases[0], cases[3]]);
  assert.equal(receipts.length, 2);
  assert.ok(receipts.every(message => message.includes('Unix host')));
  for (const signal of ['SIGTERM', 'SIGINT']) assert.throws(() => supportedProofs(cases, signal, 'win32'), /requires a Unix host/);
  assert.deepEqual(supportedProofs(cases, undefined, 'linux'), cases);
  assert.deepEqual(supportedProofs(cases, 'ordinary rust', 'win32'), [cases[0]]);
  const root = mkdtempSync(join(tmpdir(), 'pm-proof-platform-'));
  try {
    const shim = join(root, 'windows-selection.mjs');
    writeFileSync(shim, `import childProcess from 'node:child_process';
import { syncBuiltinESMExports } from 'node:module';
Object.defineProperty(process, 'platform', { value: 'win32' });
childProcess.spawn = () => { throw new Error('unsupported proof reached a baseline subprocess'); };
syncBuiltinESMExports();
`);
    const result = spawnSync(process.execPath, ['--import', pathToFileURL(shim).href, join(packageRoot, 'scripts/context-review-revert.mts'), 'SIGTERM'], {
      cwd: packageRoot, encoding: 'utf8', timeout: 30_000, env: { ...process.env, TEMP: root, TMP: root },
    });
    assert.equal(result.status, 1);
    assert.match(result.stderr, /Selected signal proof requires a Unix host/);
    assert.doesNotMatch(result.stderr, /unsupported proof reached a baseline subprocess/);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('async proof controls restore exactly and never mutate after a failed baseline', async () => {
  const events: string[] = [];
  await assert.rejects(verifyAsyncMutation(async () => { throw new Error('baseline'); }, () => events.push('mutate'), () => events.push('restore')));
  assert.equal(events.length, 0);
  await verifyAsyncMutation(async reverted => { events.push(reverted ? 'failure' : 'pass'); }, () => events.push('mutate'), () => events.push('restore'));
  assert.deepEqual(events, ['pass', 'mutate', 'failure', 'restore', 'pass']);
  events.length = 0;
  await assert.rejects(verifyAsyncMutation(async reverted => { if (reverted) throw new Error('wrong failure'); }, () => events.push('mutate'), () => events.push('restore')));
  assert.deepEqual(events, ['mutate', 'restore']);
});

test('Windows safety selection excludes every unsupported mutation before its baseline', () => {
  const cases = [
    { name: 'safe', test: 'ordinary' },
    ...Array.from({ length: 7 }, (_, i) => ({ name: `symlink-${i}`, test: 'symlink fixture', unixOnly: true })),
    { name: 'descendant groups', test: 'runner SIGTERM' },
    { name: 'nested runner ownership', test: 'outer detached owner exit', unixOnly: true },
    { name: 'later safe', test: 'ordinary' },
  ];
  const receipts: string[] = [];
  assert.deepEqual(supportedProofs(cases, undefined, 'win32', row => receipts.push(row)), [cases[0], cases[10]]);
  assert.equal(receipts.length, 9);
  assert.deepEqual(supportedProofs(cases, undefined, 'linux'), cases);
  for (const name of ['symlink', 'descendant', 'nested']) assert.throws(() => supportedProofs(cases, name, 'win32'), /requires a Unix host/);
  const root = mkdtempSync(join(tmpdir(), 'pm-safety-selection-'));
  try {
    const shim = join(root, 'platform.mjs');
    writeFileSync(shim, `import childProcess from 'node:child_process';
import { syncBuiltinESMExports } from 'node:module';
Object.defineProperty(process, 'platform', {value:'win32'});
childProcess.spawn = () => { throw new Error('unsupported baseline executed'); };
syncBuiltinESMExports();`);
    for (const selected of ['symlink isolation', 'dependency junction', 'nested runner ownership']) {
      const result = spawnSync(process.execPath, ['--import', pathToFileURL(shim).href, join(packageRoot, 'scripts/context-safety-revert.mts'), selected], { cwd: packageRoot, encoding: 'utf8', timeout: 30_000, env: { ...process.env, TEMP: root, TMP: root } });
      assert.equal(result.status, 1);
      assert.match(result.stderr, /requires a Unix host/);
      assert.doesNotMatch(result.stderr, /unsupported baseline executed/);
    }
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('benchmark preflight records only identical dependency budget refusals as unavailable', () => {
  const result = (status: number | null, payload: unknown): { status: number | null; bytes: Buffer } => ({ status, bytes: Buffer.from(`\0${JSON.stringify(payload)}`) });
  const refusal = result(2, { code: 'invalid_argument_value', recovery: { token_budget: { requested_tokens: 5000 } } });
  assert.equal(benchmarkPreflight(refusal, refusal, 'deps', true), 'dependency_token_budget_infeasible');
  const success = result(0, { context: {} });
  assert.equal(benchmarkPreflight(success, success, 'context'), undefined);
  for (const bad of [result(1, {}), result(null, {}), result(2, { code: 'invalid_argument_value' }), result(2, { code: 'runtime_error', recovery: { token_budget: { requested_tokens: 5000 } } })]) {
    assert.throws(() => benchmarkPreflight(bad, bad, 'deps', true), /preflight failed/);
    assert.throws(() => benchmarkPreflight(refusal, bad, 'deps', true), /parity failed/);
  }
  assert.throws(() => benchmarkPreflight(refusal, refusal, 'context'), /preflight failed/);
  assert.throws(() => benchmarkPreflight(success, result(0, { changed: true }), 'context'), /parity failed/);
});

test('real-copy receipts survive infeasible timing while rejecting parity and runtime failures', () => {
  const root = mkdtempSync(join(tmpdir(), 'pm-timing-receipt-'));
  try {
    mkdirSync(join(root,'.agents/pm'),{recursive:true});
    writeFileSync(join(root,'.pm-rust-read-copy'),'synthetic');
    const driver=join(root,'driver.mjs'); writeFileSync(driver,'synthetic driver');
    const shim=join(root,'timing-shim.mjs');
    for (const mode of ['budget','status','output','runtime']) {
      writeFileSync(shim, `import childProcess from 'node:child_process'; import {syncBuiltinESMExports} from 'node:module';
childProcess.spawnSync=(command,args)=>{
const native=command!==process.execPath; const argv=native?args:args.slice(1);
let status=0, stdout=JSON.stringify({context:{meta:{}}}), stderr='';
if(argv[0]==='--version') return {status:0,stdout:'2026.11.5\\n',stderr:''};
if(argv[0]==='list') stdout=JSON.stringify({items:[{id:'sample-a',parent:'sample-b'},{id:'sample-b'}]});
if(argv[0]==='deps' && argv.includes('5000')) {
 status=${mode === 'runtime' ? 1 : 2}; stdout='';
 stderr=JSON.stringify({code:'invalid_argument_value',recovery:{token_budget:{requested_tokens:5000}}});
 if(native && ${mode === 'status'}) status=1;
 if(native && ${mode === 'output'}) stderr+='different';
}
return {status,stdout:Buffer.from(stdout),stderr:Buffer.from(stderr)};
}; syncBuiltinESMExports();`);
      const output=join(root,`receipt-${mode}.json`);
      const result=spawnSync(process.execPath,['--import',pathToFileURL(shim).href,join(packageRoot,'scripts/context-real.mts'),'--copy',root,'--driver',driver,'--binary',driver,'--samples','1','--output',output],{encoding:'utf8',timeout:30000});
      if(mode==='budget') {
        assert.equal(result.status,0,result.stderr);
        const receipt=JSON.parse(readFileSync(output,'utf8')) as {unavailable_timings:{reason:string}[];samples:unknown[];authoritative_unchanged:boolean};
        assert.equal(receipt.unavailable_timings[0].reason,'dependency_token_budget_infeasible');
        assert.equal(receipt.samples.length,8);
        assert.equal(receipt.authoritative_unchanged,true);
      } else {
        assert.equal(result.status,1,result.stderr);
        assert.match(result.stderr,mode==='runtime'?/preflight failed/:/parity failed/);
        assert.equal(existsSync(output),false);
      }
    }
  } finally {rmSync(root,{recursive:true,force:true});}
});

test('Windows ownership persists after leader exit until inherited pipes are stopped', () => {
  const root=mkdtempSync(join(tmpdir(),'pm-windows-orphan-'));
  try {
    const script=join(root,'orphan.mjs');
    writeFileSync(script, `import assert from 'node:assert/strict';
import childProcess from 'node:child_process';import {EventEmitter} from 'node:events';import {PassThrough} from 'node:stream';import {syncBuiltinESMExports} from 'node:module';import {existsSync,mkdirSync} from 'node:fs';
Object.defineProperty(process,'platform',{value:'win32'});
let child, killed=false;
const scratch=${JSON.stringify(join(root,'scratch'))};mkdirSync(scratch);
childProcess.spawn=(command,args)=>{assert.equal(command,'powershell.exe');assert.ok(Buffer.from(args.at(-1),'base64').toString('utf16le').includes('PROC_THREAD_ATTRIBUTE_JOB_LIST'));child=new EventEmitter();child.pid=424242;child.stdout=new PassThrough();child.stderr=new PassThrough();setImmediate(()=>child.emit('exit',0,null));return child;};
childProcess.execFileSync=(command,args)=>{assert.equal(command,'taskkill');assert.deepEqual(args,['/pid','424242','/T','/F']);assert.ok(existsSync(scratch));killed=true;setImmediate(()=>{child.stdout.end();child.stderr.end();child.emit('close',0);});};
syncBuiltinESMExports();
const {ProofProcesses}=await import(${JSON.stringify(pathToFileURL(join(packageRoot,'scripts/context-proof-process.mts')).href)});
const owned=new ProofProcesses();const result=await owned.run('cargo',[],{timeout:50});
assert.ok(killed);assert.match(result.error.message,/timeout/);await owned.cleanup(scratch);assert.equal(existsSync(scratch),false);`);
    const result=spawnSync(process.execPath,[script],{encoding:'utf8',timeout:10000,env:{...process.env,TEMP:root,TMP:root}});
    assert.equal(result.status,0,result.stderr);
  } finally {rmSync(root,{recursive:true,force:true});}
});

test('Windows synchronous controls preserve original job argv and exact diagnostics', () => {
  const root=mkdtempSync(join(tmpdir(),'pm-job-controls-'));
  try {
    const script=join(root,'control.mjs');
    const args=['test','space and "quote"','trailing\\'];
    writeFileSync(script, `import assert from 'node:assert/strict';import childProcess from 'node:child_process';import {syncBuiltinESMExports} from 'node:module';import {mkdirSync} from 'node:fs';
Object.defineProperty(process,'platform',{value:'win32'});
childProcess.spawnSync=(command,args)=>{assert.equal(command,'cargo');assert.deepEqual(args,${JSON.stringify(args)});return {status:0,stdout:'named control',stderr:'original diagnostic'};};
${syncSpawnBridge}
syncBuiltinESMExports();
const {ProofProcesses}=await import(${JSON.stringify(pathToFileURL(join(packageRoot,'scripts/context-proof-process.mts')).href)});
const owned=new ProofProcesses();const scratch=${JSON.stringify(join(root,'scratch'))};mkdirSync(scratch);
try {const result=await owned.run('cargo',${JSON.stringify(args)});assert.ifError(result.error);assert.equal(result.status,0);assert.equal(result.stdout,'named control');assert.equal(result.stderr,'original diagnostic');}finally {await owned.cleanup(scratch);}`);
    const result=spawnSync(process.execPath,[script],{encoding:'utf8',timeout:10000,env:{...process.env,TEMP:root,TMP:root}});
    assert.equal(result.status,0,result.stderr);
  } finally {rmSync(root,{recursive:true,force:true});}
});

for (const ending of ['exit', 'SIGTERM'] as const) {
  test(`outer ownership drains nested detached groups on ${ending}`, { skip: process.platform === 'win32', timeout: 20_000 }, async () => {
    const root = mkdtempSync(join(tmpdir(), 'pm-nested-proof-'));
    const manager = join(root, 'manager.mjs'); const nested = join(root, 'nested.mjs'); const marker = join(root, 'ready');
    const module = JSON.stringify(pathToFileURL(join(packageRoot, 'scripts/context-proof-process.mts')).href);
    const worker = `import {existsSync,writeFileSync} from 'node:fs';
writeFileSync(${JSON.stringify(marker)}, String(process.pid));
process.on('SIGTERM',()=>{writeFileSync(${JSON.stringify(marker + '.stopped')},String(existsSync(${JSON.stringify(join(root,'scratch'))})));process.exit(0)});
setInterval(()=>{},1000);`;
    writeFileSync(nested, `import {setTimeout} from 'node:timers/promises';import {writeFileSync,existsSync} from 'node:fs';import {ProofProcesses} from ${module};
const owned = new ProofProcesses();
try { await owned.run(process.execPath, ['--input-type=module','-e',${JSON.stringify(worker)}]); } finally { await owned.cleanup(${JSON.stringify(join(root, 'nested-scratch'))}); await setTimeout(1200); writeFileSync(${JSON.stringify(marker+'.owner-stopped')},String(existsSync(${JSON.stringify(join(root,'scratch'))}))); }`);
    const parent = `import {spawn} from 'node:child_process'; import {existsSync} from 'node:fs'; import {setTimeout} from 'node:timers/promises';
spawn(process.execPath,[${JSON.stringify(nested)}],{detached:true,stdio:'ignore',env:process.env});
while(!existsSync(${JSON.stringify(marker)})) await setTimeout(5);
${ending === 'exit' ? 'process.exit(0);' : 'setInterval(()=>{},1000);'}`;
    writeFileSync(manager, `import {ProofProcesses} from ${module};
const owned=new ProofProcesses();
try { await owned.run(process.execPath,['--input-type=module','-e',${JSON.stringify(parent)}],{timeout:10000,cwd:${JSON.stringify(join(root,'scratch'))}}); }
catch(error) { if(!owned.wasInterrupted) throw error; }
finally { await owned.cleanup(${JSON.stringify(join(root, 'scratch'))}); }`);
    mkdirSync(join(root, 'scratch')); mkdirSync(join(root, 'nested-scratch'));
    const child = spawn(process.execPath, [manager], { stdio: ['ignore', 'pipe', 'pipe'] });
    child.stdout.resume(); let stderr = ''; child.stderr.on('data', chunk => { stderr += String(chunk); });
    const finished = new Promise<number | null>(resolve => child.once('exit', resolve));
    try {
      for (let i=0; !existsSync(marker) && i<1000; i++) { if(child.exitCode!==null) break; await setTimeout(10); }
      assert.ok(existsSync(marker), stderr);
      if (ending !== 'exit') child.kill(ending);
      assert.equal(await finished, ending === 'exit' ? 0 : 143, stderr);
      assert.equal(readFileSync(marker + '.stopped','utf8'), 'true');
      assert.equal(readFileSync(marker + '.owner-stopped','utf8'), 'true', 'nested restoration finishes before outer input removal');
      assert.equal(existsSync(join(root,'scratch')),false);
    } finally {
      if (child.exitCode===null) child.kill('SIGKILL');
      if(existsSync(marker)) { try { process.kill(Number(readFileSync(marker,'utf8')),'SIGKILL'); } catch { /* drained */ } }
      await finished;
      rmSync(root,{recursive:true,force:true});
    }
  });
}

/** Cleanup must own detached descendants before their delayed constructors run. */
for (const { ending, twoArguments } of [false, true].flatMap(twoArguments =>
  (['exit', 'SIGTERM'] as const).map(ending => ({ ending, twoArguments })))) {
  test(`outer cleanup owns delayed nested constructors on ${ending}${twoArguments ? ' (two-argument spawn)' : ''}`, { skip: process.platform === 'win32', timeout: 20_000 }, async () => {
    const root = mkdtempSync(join(tmpdir(), 'pm-delayed-owner-'));
    const scratch = join(root, 'scratch'); mkdirSync(scratch);
    const marker = join(root, 'ready');
    const nested = join(root, 'nested.mjs');
    const module = JSON.stringify(pathToFileURL(join(packageRoot, 'scripts/context-proof-process.mts')).href);
    writeFileSync(nested, `import {existsSync,writeFileSync} from 'node:fs';import {setTimeout} from 'node:timers/promises';
process.on('SIGTERM',()=>{ void setTimeout(1200).then(()=>{writeFileSync(${JSON.stringify(marker+'.stopped')},String(existsSync(${JSON.stringify(scratch)})));process.exit(0)}); });
writeFileSync(${JSON.stringify(marker)},String(process.pid));
await setTimeout(10000);
const {ProofProcesses}=await import(${module});new ProofProcesses();writeFileSync(${JSON.stringify(marker+'.constructor')},'too late');`);
    const parent = `import {spawn} from 'node:child_process';import {existsSync} from 'node:fs';import {setTimeout} from 'node:timers/promises';
${twoArguments ? `spawn(process.execPath,{detached:true,stdio:'ignore',env:{...process.env,NODE_OPTIONS:(process.env.NODE_OPTIONS??'')+' --import='+${JSON.stringify(JSON.stringify(pathToFileURL(nested).href))}}});` : `spawn(process.execPath,[${JSON.stringify(nested)}],{detached:true,stdio:'ignore'});`}
while(!existsSync(${JSON.stringify(marker)})) await setTimeout(5);
${ending === 'exit' ? 'process.exit(0)' : 'setInterval(()=>{},1000)'}`;
    const manager = join(root, 'manager.mjs');
    writeFileSync(manager, `import {ProofProcesses} from ${module};const owned=new ProofProcesses();
try {await owned.run(process.execPath,['--input-type=module','-e',${JSON.stringify(parent)}]);}
catch(error){if(!owned.wasInterrupted)throw error;}
finally{await owned.cleanup(${JSON.stringify(scratch)});}`);
    const child = spawn(process.execPath,[manager],{stdio:['ignore','ignore','pipe']});
    let diagnostic='';child.stderr.on('data',chunk=>{diagnostic+=String(chunk)});
    const finished = new Promise<number|null>(resolve=>child.once('exit',resolve));
    try {
      for(let i=0;!existsSync(marker)&&i<1000;i++) {if(child.exitCode!==null)break;await setTimeout(10);}
      assert.ok(existsSync(marker),diagnostic);
      if(ending!=='exit')child.kill(ending);
      assert.equal(await finished,ending==='exit'?0:143,diagnostic);
      assert.equal(readFileSync(marker+'.stopped','utf8'),'true','inputs remain through delayed runner termination');
      assert.equal(existsSync(marker+'.constructor'),false,'cleanup precedes the constructor');
      assert.equal(existsSync(scratch),false);
    } finally {
      if(child.exitCode===null)child.kill('SIGKILL');
      if(existsSync(marker)){try{process.kill(-Number(readFileSync(marker,'utf8')),'SIGKILL')}catch{/* already stopped */}}
      await finished;rmSync(root,{recursive:true,force:true});
    }
  });
}

/** A failed reap is a refusal to restore, even when the baseline already passed. */
test('failed worker reap retains mutated inputs until termination is confirmed', () => {
  const root=mkdtempSync(join(tmpdir(),'pm-failed-reap-'));
  try {
    const script=join(root,'reap.mjs');
    writeFileSync(script, `import assert from 'node:assert/strict';import childProcess from 'node:child_process';import {EventEmitter} from 'node:events';import {PassThrough} from 'node:stream';import {syncBuiltinESMExports} from 'node:module';
let unavailable=true;
childProcess.execFileSync=()=>{if(unavailable)throw new Error('termination unavailable');return '';};
process.kill=()=>true;
childProcess.spawn=()=>{const child=new EventEmitter();child.pid=424242;child.stdout=new PassThrough();child.stderr=new PassThrough();setImmediate(()=>{child.emit('exit',0,null);child.stdout.end();child.stderr.end();child.emit('close',0)});return child;};
syncBuiltinESMExports();
const {ProofProcesses,verifyAsyncMutation}=await import(${JSON.stringify(pathToFileURL(join(packageRoot,'scripts/context-proof-process.mts')).href)});
const owned=new ProofProcesses();let input='fixed';
await assert.rejects(verifyAsyncMutation(async reverted=>{if(reverted)await owned.run('cargo',[])},()=>{input='mutated'},()=>{input='fixed'},owned),/termination unavailable/);
assert.equal(input,'mutated','failed reap must not rewrite worker inputs');
unavailable=false;await owned.settled();
await verifyAsyncMutation(async()=>{},()=>{input='mutated'},()=>{input='fixed'},owned);
assert.equal(input,'fixed');await owned.cleanup(${JSON.stringify(join(root,'scratch'))});`);
    const result=spawnSync(process.execPath,[script],{encoding:'utf8',timeout:10000});
    assert.equal(result.status,0,result.stderr);
  } finally {rmSync(root,{recursive:true,force:true});}
});

/** All proof copy callbacks tolerate dangling inputs while materializing usable links. */
test('proof copies skip dangling file and directory links across every runner', { skip: process.platform === 'win32' }, () => {
  const root=mkdtempSync(join(tmpdir(),'pm-dangling-copy-'));
  try {
    const source=join(root,'source');mkdirSync(source);checkedGit(source,['init','--quiet'],'fixture init');
    writeFileSync(join(source,'kept'),'fixed');
    symlinkSync('absent',join(source,'broken'));
    symlinkSync('absent-directory',join(source,'broken-directory'),'dir');
    mkdirSync(join(source,'folder'));symlinkSync('absent',join(source,'folder/broken'));
    checkedGit(source,['add','kept','broken'],'fixture add');
    const copy=join(root,'copy');mkdirSync(copy);copyWorkingTree(source,copy);
    assert.equal(readFileSync(join(copy,'kept'),'utf8'),'fixed');
    for(const name of ['broken','broken-directory','folder/broken'])assert.equal(lstatSync(join(copy,name),{throwIfNoEntry:false}),undefined);
    const shim=join(root,'copy.mjs');
    writeFileSync(shim, `import assert from 'node:assert/strict';import fs from 'node:fs';import {syncBuiltinESMExports} from 'node:module';
const original=fs.cpSync;
fs.cpSync=(_source,_target,options)=>{const target=${JSON.stringify(join(root,'callback'))};fs.rmSync(target,{recursive:true,force:true});original(${JSON.stringify(source)},target,options);assert.equal(fs.readFileSync(target+'/kept','utf8'),'fixed');assert.equal(fs.lstatSync(target+'/broken',{throwIfNoEntry:false}),undefined);throw new Error('dangling copy verified');};syncBuiltinESMExports();`);
    for(const runner of runners){
      const result=spawnSync(process.execPath,['--import',pathToFileURL(shim).href,join(packageRoot,'scripts',runner)],{cwd:packageRoot,encoding:'utf8',timeout:30000});
      assert.equal(result.status,1,runner);assert.match(result.stderr,/dangling copy verified/,runner);
    }
  } finally {rmSync(root,{recursive:true,force:true});}
});

test('Windows kernel jobs drain orphaned descendants before returning output', { skip: process.platform !== 'win32', timeout: 30_000 }, async () => {
  const root = mkdtempSync(join(tmpdir(), 'pm-windows-job-'));
  const owned = new ProofProcesses();
  try {
    const marker = join(root, 'worker');
    const worker = `require('node:fs').writeFileSync(${JSON.stringify(marker)},String(process.pid));setInterval(()=>{},1000);`;
    const parent = `const {spawn}=require('node:child_process'); const fs=require('node:fs'); spawn(process.execPath,['-e',${JSON.stringify(worker)}],{stdio:'inherit'}); const timer=setInterval(()=>{if(fs.existsSync(${JSON.stringify(marker)}))process.exit(0)},10);`;
    const result = await owned.run(process.execPath,['-e',parent],{timeout:15000});
    assert.ifError(result.error); assert.equal(result.status,0);
    const pid=Number(readFileSync(marker,'utf8'));
    assert.throws(()=>process.kill(pid,0));
    const invocation=windowsJob(process.execPath,['space and "quote"','trailing\\'], 'marker');
    assert.equal(invocation.command,'powershell.exe');
  } finally { await owned.cleanup(root); }
});

test('JSON snapshot additions removals and changes are authoritative in every custom directory', () => {
  const root = mkdtempSync(join(tmpdir(), 'pm-json-snapshot-'));
  try {
    for (const folder of ['tasks', 'history', 'custom-types/runtime', 'custom-types/locks']) {
      mkdirSync(join(root, folder), { recursive: true });
      const path = join(root, folder, 'original.json');
      writeFileSync(path, '{}');
      const before = authoritative(root);
      for (const mutation of ['change', 'remove', 'add']) {
        const added = join(root, folder, 'added.json');
        if (mutation === 'change') writeFileSync(path, '{"changed":true}');
        if (mutation === 'remove') rmSync(path);
        if (mutation === 'add') writeFileSync(added, '{}');
        assert.notDeepEqual(authoritative(root), before, `${folder}: ${mutation}`);
        writeFileSync(path, '{}');
        rmSync(added, { force: true });
        assert.deepEqual(authoritative(root), before);
      }
    }
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('real-copy oracle provenance refuses mismatched missing failed or noisy driver versions before side effects', () => {
  const root = mkdtempSync(join(tmpdir(), 'pm-oracle-provenance-'));
  try {
    const driver = join(root, 'driver.mjs');
    const environment = { PATH: process.env.PATH, PM_SENTRY_DISABLED: '1', PM_TELEMETRY_SOURCE_CONTEXT: 'test' };
    assert.equal(verifiedOracleVersion(join(packageRoot, 'tests/orientation-oracle/node_modules/@unbrained/pm-cli/dist/cli.js'), root, environment), '2026.11.5');
    for (const response of ["console.log('2026.11.4')", "console.log('')", "console.log('2026.11.5'); process.exit(2)", "console.log('2026.11.5'); console.error('warning')", "throw new Error('unavailable')"]) {
      writeFileSync(driver, response);
      assert.throws(() => verifiedOracleVersion(driver, root, environment), /must verify the pinned oracle/);
      mkdirSync(join(root, '.agents/pm/runtime'), { recursive: true });
      writeFileSync(join(root, '.pm-rust-read-copy'), 'fixture');
      const retained = join(root, '.agents/pm/runtime/retained');
      writeFileSync(retained, 'unchanged');
      const output = join(root, 'receipt.json');
      const result = spawnSync(process.execPath, [join(packageRoot, 'scripts/context-real.mts'), '--copy', root, '--driver', driver, '--binary', process.execPath, '--samples', '1', '--output', output], { encoding: 'utf8' });
      assert.equal(result.status, 1);
      assert.match(result.stderr, /must verify the pinned oracle/);
      assert.equal(readFileSync(retained, 'utf8'), 'unchanged');
      assert.equal(existsSync(output), false);
    }
    writeFileSync(driver, "console.log('2026.11.5')");
    assert.equal(verifiedOracleVersion(driver, root, environment), '2026.11.5');
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('proof copies materialize absolute relative and directory symlinks before any mutation', { skip: process.platform === 'win32' }, () => {
  const root = mkdtempSync(join(tmpdir(), 'pm-proof-symlink-'));
  try {
    const source = join(root, 'source'); const destination = join(root, 'copy');
    mkdirSync(source); mkdirSync(destination); mkdirSync(join(source, 'original'));
    checkedGit(source, ['init', '--quiet'], 'fixture init');
    writeFileSync(join(source, 'original/target.rs'), 'original');
    symlinkSync(join(source, 'original/target.rs'), join(source, 'absolute.rs'));
    symlinkSync('original/target.rs', join(source, 'relative.rs'));
    symlinkSync(join(source, 'original'), join(source, 'directory'));
    symlinkSync(join(source, 'original'), join(source, 'node_modules'));
    mkdirSync(join(source, 'nested'));
    symlinkSync(join(source, 'original'), join(source, 'nested/node_modules'));
    copyWorkingTree(source, destination);
    assert.equal(existsSync(join(destination, 'node_modules')), false);
    assert.equal(existsSync(join(destination, 'nested/node_modules')), false);
    for (const name of ['absolute.rs', 'relative.rs', 'directory/target.rs']) {
      assert.equal(lstatSync(join(destination, name)).isSymbolicLink(), false);
      writeFileSync(join(destination, name), 'mutation');
      assert.equal(readFileSync(join(source, 'original/target.rs'), 'utf8'), 'original');
    }
    // Execute every copy callback against a real symlink, rather than inspecting its spelling.
    const shim = join(root, 'copy-shim.mjs');
    writeFileSync(shim, `import assert from 'node:assert/strict';
import fs from 'node:fs';
import { syncBuiltinESMExports } from 'node:module';
const original = fs.cpSync;
fs.cpSync = (_source, _destination, options) => {
  const source = ${JSON.stringify(join(source, 'absolute.rs'))};
  const target = ${JSON.stringify(join(root, 'callback-copy.rs'))};
  fs.rmSync(target, {force:true});
  original(source, target, options);
  assert.equal(fs.lstatSync(target).isSymbolicLink(), false, 'copied mutation targets must be regular files');
  fs.writeFileSync(target, 'mutation');
  assert.equal(fs.readFileSync(source, 'utf8'), 'original');
  throw new Error('symlink isolation verified');
};
syncBuiltinESMExports();
`);
    for (const runner of runners) {
      const result = spawnSync(process.execPath, ['--import', pathToFileURL(shim).href, join(packageRoot, 'scripts', runner)], { cwd: packageRoot, encoding: 'utf8', timeout: 30_000 });
      assert.equal(result.status, 1, runner);
      assert.match(result.stderr, /symlink isolation verified/, runner);
    }
  } finally { rmSync(root, { recursive: true, force: true }); }
});

for (const runner of runners) {
  for (const signal of ['SIGINT', 'SIGTERM'] as const) {
    test(`all proof runners stop descendants before cleanup: ${runner} ${signal}`, { skip: process.platform === 'win32', timeout: 20_000 }, async () => {
      const root = mkdtempSync(join(tmpdir(), 'pm-proof-process-'));
      let owned: { pid: number; worker: number; root: string } | undefined;
      const source = fixture(root);
      const shim = join(root, 'spawn-shim.mjs');
      const fake = join(root, 'cargo.mjs');
      writeFileSync(fake, `import { spawn } from 'node:child_process';
import { existsSync, renameSync, writeFileSync } from 'node:fs';
import { setTimeout } from 'node:timers/promises';
const marker = process.env.PM_PROOF_MARKER;
const worker = spawn(process.execPath, ['--input-type=module', '-e', \`
import { existsSync, writeFileSync } from 'node:fs';
const marker = process.env.PM_PROOF_MARKER;
for (const signal of ['SIGINT','SIGTERM']) process.on(signal, () => {
  setTimeout(() => { writeFileSync(marker + '.stopped', JSON.stringify(existsSync(process.cwd()))); process.exit(0); }, 100);
});
writeFileSync(marker + '.worker', String(process.pid));
setInterval(() => {}, 1000);
\`], { stdio: ['ignore', 'inherit', 'inherit'] });
while (!existsSync(marker + '.worker')) await setTimeout(5);
writeFileSync(marker + '.stage', JSON.stringify({ pid: process.pid, worker: worker.pid, root: process.cwd() }));
renameSync(marker + '.stage', marker);
setInterval(() => {}, 1000);
`);
      writeFileSync(shim, `import childProcess from 'node:child_process';
import { syncBuiltinESMExports } from 'node:module';
const original = childProcess.spawn;
childProcess.spawn = (command, args, options) => original(process.execPath, [${JSON.stringify(fake)}], { ...options, env: { ...options.env, PM_PROOF_MARKER: process.env.PM_PROOF_MARKER } });
syncBuiltinESMExports();
`);
      const marker = join(root, 'ready');
      const child = spawn(process.execPath, ['--import', pathToFileURL(shim).href, join(source, 'scripts', runner), ...(runner === 'context-review-revert.mts' ? ['context_stored_settings_fallback'] : [])], {
        cwd: source, stdio: ['ignore', 'pipe', 'pipe'], env: { ...process.env, PM_PROOF_MARKER: marker },
      });
      let stderr = ''; child.stderr.on('data', chunk => { stderr += String(chunk); }); child.stdout.resume();
      const finished = new Promise<number | null>((resolve, reject) => { child.once('error', reject); child.once('exit', resolve); });
      try {
        for (let i = 0; !existsSync(marker) && i < 1000; i++) { if (child.exitCode !== null) break; await setTimeout(10); }
        assert.ok(existsSync(marker), stderr);
        owned = JSON.parse(readFileSync(marker, 'utf8')) as typeof owned;
        child.kill(signal);
        const deadline = globalThis.setTimeout(() => child.kill('SIGKILL'), 8000);
        try { assert.equal(await finished, signal === 'SIGINT' ? 130 : 143, stderr); }
        finally { clearTimeout(deadline); }
        assert.ok(owned);
        assert.equal(readFileSync(`${marker}.stopped`, 'utf8'), 'true', 'worker stops with its scratch inputs present');
        assert.equal(existsSync(owned.root), false, 'scratch is removed after worker termination');
      } finally {
        if (child.exitCode === null) child.kill('SIGKILL');
        if (owned) {
          for (const pid of [-owned.pid, owned.pid, owned.worker]) { try { process.kill(pid, 'SIGKILL'); } catch { /* already drained */ } }
        }
        await finished;
        if (owned) rmSync(owned.root, { recursive: true, force: true });
        rmSync(root, { recursive: true, force: true });
      }
    });
  }
}

/** Exercise the Windows branch on every host, including inability to prove tree termination. */
test('Windows proof interruption terminates the complete tree or retains scratch inputs', () => {
  const root = mkdtempSync(join(tmpdir(), 'pm-proof-windows-'));
  try {
    for (const fail of [false, true]) {
      const script = join(root, 'windows.mjs');
      writeFileSync(script, `import assert from 'node:assert/strict';
import childProcess from 'node:child_process';
import { EventEmitter } from 'node:events';
import { PassThrough } from 'node:stream';
import { syncBuiltinESMExports } from 'node:module';
import { existsSync, mkdirSync } from 'node:fs';
Object.defineProperty(process, 'platform', { value: 'win32' });
const calls = [];
childProcess.execFileSync = (command, args) => {
  assert.equal(command, 'taskkill');
  assert.deepEqual(args, ['/pid', '424242', '/T', '/F']);
  calls.push(args);
  if (${fail}) throw new Error('tree termination unavailable');
};
childProcess.spawn = () => {
  const child = new EventEmitter(); child.pid = 424242;
  child.stdout = new PassThrough(); child.stderr = new PassThrough();
  setImmediate(() => {
    process.emit('SIGTERM');
    child.emit('exit', null, 'SIGTERM');
    child.stdout.end(); child.stderr.end(); child.emit('close', null);
  });
  return child;
};
syncBuiltinESMExports();
import { ProofProcesses } from ${JSON.stringify(pathToFileURL(join(packageRoot, 'scripts/context-proof-process.mts')).href)};
const processes = new ProofProcesses();
const scratch = ${JSON.stringify(join(root, 'scratch'))};
mkdirSync(scratch);
await assert.rejects(processes.run('cargo', []));
assert.ok(calls.length > 0);
if (${fail}) await assert.rejects(processes.cleanup(scratch), /scratch retained/);
else await processes.cleanup(scratch);
assert.equal(existsSync(scratch), ${fail});
process.exitCode = 0;
`);
      const result = spawnSync(process.execPath, [script], { encoding: 'utf8', timeout: 10_000 });
      assert.equal(result.status, 0, result.stderr);
      rmSync(join(root, 'scratch'), { force: true, recursive: true });
    }
  } finally { rmSync(root, { recursive: true, force: true }); }
});

/** Darwin may report EPERM for an already-dead group; live or unreadable groups stay protected. */
test('signal errors require positive termination evidence before restoration', () => {
  const root=mkdtempSync(join(tmpdir(),'pm-signal-evidence-'));
  try {
    for(const runner of [false,true]) for(const state of ['absent','zombie','live','unavailable']) {
      const script=join(root,'signal.mjs');
      writeFileSync(script, `import assert from 'node:assert/strict';import childProcess from 'node:child_process';import {EventEmitter} from 'node:events';import {PassThrough} from 'node:stream';import {syncBuiltinESMExports} from 'node:module';import {mkdirSync,writeFileSync} from 'node:fs';
let clock=0,killed=false,checks=0;Object.defineProperty(performance,'now',{value:()=>clock+=1100});
process.kill=(_pid,signal)=>{if(signal==='SIGKILL'){killed=true;throw Object.assign(new Error('kill EPERM'),{code:'EPERM'})}return true;};
childProcess.execFileSync=()=>{if(!killed)return '424242 S';checks++;if(checks===1&&${JSON.stringify(state)}==='unavailable')throw new Error('process table unavailable');if(checks===1&&${JSON.stringify(state)}==='live')return '424242 S';return ${JSON.stringify(state)}==='zombie'?'424242 Z':'';};
childProcess.spawn=()=>{const child=new EventEmitter();child.pid=424242;child.stdout=new PassThrough();child.stderr=new PassThrough();setImmediate(()=>{child.emit('exit',0,null);child.stdout.end();child.stderr.end();child.emit('close',0)});return child;};syncBuiltinESMExports();
const {ProofProcesses,verifyAsyncMutation}=await import(${JSON.stringify(pathToFileURL(join(packageRoot,'scripts/context-proof-process.mts')).href)});const owned=new ProofProcesses();
if(${runner})writeFileSync(owned.registry+'/424242.owner','424242');
const scratch=${JSON.stringify(join(root,'scratch'))};mkdirSync(scratch,{recursive:true});let input='fixed';
const run=()=>verifyAsyncMutation(async reverted=>{if(reverted)await owned.run('cargo',[])},()=>{input='mutated'},()=>{input='fixed'},owned);
if(${state==='absent'||state==='zombie'}){await run();assert.equal(input,'fixed');await owned.cleanup(scratch);}
else{await assert.rejects(run());assert.equal(input,'mutated');await assert.rejects(owned.cleanup(scratch),/scratch retained/);}
assert.ok(killed);`);
      const result=spawnSync(process.execPath,[script],{encoding:'utf8',timeout:10000,env:{...process.env,TMPDIR:root,TEMP:root,TMP:root}});
      assert.equal(result.status,0,`${runner} ${state}: ${result.stderr}`);
    }
  } finally {rmSync(root,{recursive:true,force:true});}
});

/** Only the draining owner escalates after leader exit, even when the event loop stalls. */
test('leader exit transfers escalation to the drain without a duplicate timer', () => {
  const root=mkdtempSync(join(tmpdir(),'pm-escalation-transfer-'));
  try {
    const script=join(root,'escalation.mjs');
    writeFileSync(script, `import assert from 'node:assert/strict';import childProcess from 'node:child_process';import {EventEmitter} from 'node:events';import {PassThrough} from 'node:stream';import {syncBuiltinESMExports} from 'node:module';
let clock=0,kills=0;Object.defineProperty(performance,'now',{value:()=>clock+=1100});const started=Date.now();
process.kill=(_pid,signal)=>{if(signal==='SIGKILL'){kills++;while(Date.now()-started<1200){}if(kills>1)throw Object.assign(new Error('kill EPERM'),{code:'EPERM'});}return true;};
childProcess.execFileSync=()=>kills?'':'424242 S';
childProcess.spawn=()=>{const child=new EventEmitter();child.pid=424242;child.stdout=new PassThrough();child.stderr=new PassThrough();setTimeout(()=>{child.emit('exit',0,null);child.stdout.end();child.stderr.end();child.emit('close',0)},20);return child;};syncBuiltinESMExports();
const {ProofProcesses}=await import(${JSON.stringify(pathToFileURL(join(packageRoot,'scripts/context-proof-process.mts')).href)});const owned=new ProofProcesses();await owned.run('cargo',[],{timeout:1});assert.equal(kills,1);await owned.cleanup(${JSON.stringify(join(root,'scratch'))});`);
    const result=spawnSync(process.execPath,[script],{encoding:'utf8',timeout:10000});
    assert.equal(result.status,0,result.stderr);
  } finally {rmSync(root,{recursive:true,force:true});}
});
