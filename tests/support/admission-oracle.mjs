/** Published npm SDK differential for canonical native admission; no product runtime dependency. */
import assert from 'node:assert/strict';
import { mkdtemp, readFile, writeFile, rename, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { createRequire } from 'node:module';
import { pathToFileURL } from 'node:url';
process.once('uncaughtException', error => { console.error(JSON.stringify({error:error.message,context:error.context,stack:error.stack?.split('\n').slice(0,5)})); process.exitCode=1; });
process.env.PM_SENTRY_DISABLED = '1';
process.env.PM_TELEMETRY_SOURCE_CONTEXT = 'test';
const oracle = createRequire(new URL('../oracle/package.json', import.meta.url));
const sdk = await import(pathToFileURL(oracle.resolve('@unbrained/pm-cli/sdk')).href);
const directory = await mkdtemp(path.join(tmpdir(), 'native-admission-oracle-'));
const pmRoot = path.join(directory, '.agents/pm');
const create = (id, options = {}) => ({op:'create',id,options:{title:'Admission differential fixture',type:'Task',status:'open',priority:2,createMode:'progressive',parent:'none',assignee:'none',allowDuplicate:true,...options}});
let cases = 0;
try {
  await sdk.init('sample', {defaults:true,agentGuidance:'skip'}, {pmRoot,noExtensions:true});
  await sdk.create({...create('sample-existing').options,id:'sample-existing',author:'fixture-agent'}, {pmRoot,noExtensions:true});
  await sdk.focus('sample-existing',{}, {pmRoot,noExtensions:true});
  const corpus = [
    ...['Epic','Feature','Task','Chore','Issue','Decision','Event','Reminder','Milestone','Meeting','Plan'].map((type,i)=>[create(`sample-type-${i}`,{type})]),
    ...['',' , ','A,,B','__proto__,constructor'].map(tags=>[{op:'update',id:'sample-existing',options:{tags}}]),
    [{op:'update',id:'sample-existing',options:{status:'in_progress'}}],
    [create('sample-new')], [create('sample-new',{title:undefined})], [create('sample-new',{title:' \r\n '})],
    [create('sample-new',{priority:5})], [create('sample-new',{type:'Unknown'})],
    [{op:'update',id:'sample-existing',options:{title:'  New title  ',description:'\r\n text \r\n',body:' \r\n body  ',priority:0,tags:' Mixed, MIXED,other '}}],
    [{op:'update',id:'sample-existing',options:{title:' \r\n '}}],
    [{op:'update',id:'sample-existing',options:{priority:255}}],
    [{op:'update',id:'sample-missing',options:{title:'New'}}],
    [create('sample-new'),{op:'update',id:'sample-new',options:{title:'Second stage'}}],
    [create('sample-existing')], [create('sample-new'),create('sample-new')],
    [create('sample-bad',{title:''}),{op:'update',id:'sample-bad',options:{body:'dependent'}},create('sample-independent')],
  ];
  const statuses=[];
  for (let i=0;i<20;i++) {
    const token=`stage_${i}`;
    const statusFile=path.join(pmRoot,'schema/statuses.json');
    statuses.push({id:token,roles:[i%2 ? 'active':'draft']});
    await writeFile(statusFile,JSON.stringify({statuses}));
    corpus.push([create(`sample-custom-${i}`,{status:token})]);
  }
  const sourceBefore = await sdk.get('sample-existing', {}, {pmRoot,noExtensions:true});
  async function check(mutations,author='fixture-agent') {
    const batch={author,mutations};
    const native=spawnSync(process.env.PM_RUST_ADMISSION_BINARY,['--workspace',directory,'preview-item-mutations','--json'],{input:JSON.stringify(batch),encoding:'utf8',env:{...process.env,GIT_CEILING_DIRECTORIES:path.dirname(directory)}});
    assert.equal(native.status,0,native.stderr);
    const preview=JSON.parse(native.stdout);
    let admitted=true, refusedIndex;
    try {await sdk.previewItemMutations({pmRoot,transactionId:`admission-oracle-${cases}`,author:batch.author,mutations});}
    catch(error) {admitted=false;refusedIndex=error.context?.transaction_operation?.index;}
    assert.equal(preview.ok,admitted,JSON.stringify({mutations,preview}));
    if (refusedIndex !== undefined) assert.equal(preview.diagnostics.find(row=>row.state==='refused')?.index,refusedIndex);
    // Both native renderers carry the same typed decision in a non-git tracker.
    const toon=spawnSync(process.env.PM_RUST_ADMISSION_BINARY,['--workspace',directory,'preview-item-mutations'],{input:JSON.stringify(batch),encoding:'utf8',env:{...process.env,PATH:'',GIT_CEILING_DIRECTORIES:path.dirname(directory)}});
    assert.equal(toon.status,0,toon.stderr);
    assert.match(toon.stdout,new RegExp(`ok: ${admitted}`));
    cases++;
  }
  for (const mutations of corpus) await check(mutations);
  assert.deepEqual(await sdk.get('sample-existing', {}, {pmRoot,noExtensions:true}),sourceBefore);
  assert.equal((await readFile(path.join(pmRoot,'tasks/sample-existing.toon'),'utf8')).includes('Admission differential fixture'),true);
  const settingsFile=path.join(pmRoot,'settings.json');
  const initialSettings=JSON.parse(await readFile(settingsFile,'utf8'));
  const fields=[{title:'Changed'},{description:'Changed'},{body:'Changed'},{priority:0},{tags:'one,two'},{status:'in_progress'}];
  for (const preset of ['strict','minimal','custom','default']) {
    for (const owner of ['fixture-agent','another-agent']) {
      await sdk.claim('sample-existing',{force:true,author:owner},{pmRoot,noExtensions:true});
      const settings=structuredClone(initialSettings);
      settings.governance={...settings.governance,preset,ownership_enforcement:'strict',require_close_reason:true};
      await writeFile(settingsFile,JSON.stringify(settings));
      const before=await sdk.get('sample-existing',{}, {pmRoot,noExtensions:true});
      await check([{op:'update',id:'sample-existing',options:{title:'Admission differential fixture'}}],' fixture-agent ');
      for (const options of fields) await check([{op:'update',id:'sample-existing',options}]);
      assert.deepEqual(await sdk.get('sample-existing',{}, {pmRoot,noExtensions:true}),before);
    }
  }
  await sdk.release('sample-existing',{force:true,author:'fixture-agent'},{pmRoot,noExtensions:true});
  for (let i=0;i<24;i++) {
    const settings=structuredClone(initialSettings);
    const close=i===0?'closed':`finish_${i}`;
    settings.schema.workflow.close_status=close;
    settings.governance.require_close_reason=true;
    await writeFile(settingsFile,JSON.stringify(settings));
    await writeFile(path.join(pmRoot,'schema/statuses.json'),JSON.stringify({statuses:[{id:close,roles:[i%2?'draft':'active']}]}));
    const workflowFile=path.join(pmRoot,'schema/workflows.json');
    await writeFile(workflowFile,JSON.stringify({workflow:settings.schema.workflow}));
    await check([create(`sample-close-${i}`,{status:close})]);
    // Configured closure updates are outside the declared native scalar slice.
    // The native apply/property corpus checks their refusal independently: the
    // pinned transaction preview currently admits active-role closure updates.
    const closingBatch={author:'fixture-agent',mutations:[{op:'update',id:'sample-existing',options:{status:close}}]};
    for (const output of [[],['--json']]) {
      const native=spawnSync(process.env.PM_RUST_ADMISSION_BINARY,['--workspace',directory,'preview-item-mutations',...output],{input:JSON.stringify(closingBatch),encoding:'utf8',env:{...process.env,PATH:'',GIT_CEILING_DIRECTORIES:path.dirname(directory)}});
      assert.equal(native.status,0,native.stderr);
      assert.match(native.stdout,/close_reason_required/);
    }
  }
  // Missing schema files use the published default registry, even with embedded overrides.
  const settings=structuredClone(initialSettings);
  settings.schema.workflow.close_status='inline_finish';
  settings.schema.statuses.push({id:'inline_finish',roles:['active']});
  await writeFile(settingsFile,JSON.stringify(settings));
  await rm(path.join(pmRoot,'schema/workflows.json'),{force:true});
  await rm(path.join(pmRoot,'schema/statuses.json'),{force:true});
  await check([create('sample-inline',{status:'inline_finish'})]);
  const identityTarget='sample-identity';
  await sdk.create({...create(identityTarget).options,id:identityTarget,author:'fixture-agent'}, {pmRoot,noExtensions:true});
  const canonicalFile=path.join(pmRoot,'tasks',`${identityTarget}.toon`);
  const identityBytes=await readFile(canonicalFile);
  for (let i=0;i<12;i++) {
    const aliasFile=path.join(pmRoot,'tasks',`sample-alias-${i}.toon`);
    await rename(canonicalFile,aliasFile);
    try {
      await assert.rejects(sdk.get(identityTarget,{}, {pmRoot,noExtensions:true}));
      const mutations=[{op:'update',id:identityTarget,options:{title:'Must refuse'}}];
      await assert.rejects(sdk.previewItemMutations({pmRoot,transactionId:`identity-${i}`,author:'fixture-agent',mutations}));
      for (const output of [[],['--json']]) {
        const native=spawnSync(process.env.PM_RUST_ADMISSION_BINARY,['--workspace',directory,'preview-item-mutations',...output],{input:JSON.stringify({author:'fixture-agent',mutations}),encoding:'utf8',env:{...process.env,PATH:'',GIT_CEILING_DIRECTORIES:path.dirname(directory)}});
        assert.equal(native.status,2,native.stderr);
        const refusal=JSON.parse(native.stderr);assert.equal(refusal.code,'admission_source');assert.match(refusal.required,/filename/);assert.ok(refusal.limits.source_rows>0);
        assert.deepEqual(await readFile(aliasFile),identityBytes);
      }
    } finally {await rename(aliasFile,canonicalFile);}
    assert.equal((await sdk.get(identityTarget,{}, {pmRoot,noExtensions:true})).item.id,identityTarget);
  }
  // Every schema role can reuse one frozen JSON object without changing stable decisions.
  const shared={statuses:[{id:'shared_active',roles:['active']}],workflow:initialSettings.schema.workflow,definitions:[],fields:[],type_workflows:[]};
  const sharedFile=path.join(pmRoot,'schema/shared.json');await writeFile(sharedFile,JSON.stringify(shared));
  for (const roles of [['statuses','workflows'],['types','fields','statuses','workflows']]) {
    const aliased=structuredClone(initialSettings);
    for(const role of roles) aliased.schema.files[role]='schema/shared.json';
    await writeFile(settingsFile,JSON.stringify(aliased));
    await check([create('sample-shared',{status:'shared_active'})]);
  }
  console.log('identity fixtures=12; native JSON/TOON and published SDK refuse unresolvable targets; restored canonical files resolve');
  console.log(`oracle cases=${cases}; admitted/refused and first failing row agree; source SDK state preserved`);
} finally { await rm(directory,{recursive:true,force:true}); }
