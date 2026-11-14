/** Review regressions use disposable public fixtures and actual harness entry points. */
import assert from 'node:assert/strict';
import { assertionFailureDiagnostic } from './context-proof-diagnostics.mts';
import { spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { syncSpawnBridge } from './context-test-process-shim.mts';
import { absentLink, assertNamedTests, cargoConfigurationArgs, copyWorkingTree, supportedProofs } from './context-revert-helpers.mts';

const packageRoot = new URL('..', import.meta.url);

test('review154 mutation proof rejects a named fixture failure', () => {
  const outcome = { status: 101, stdout: 'test selected ... FAILED\nfixture creation failed', stderr: '' };
  assert.throws(() => assertNamedTests(outcome, ['selected'], true, false, 'revision-specific invariant'), /Missing mutation diagnostic/, 'review154: unrelated fixture failures must not certify a mutation');
  assertNamedTests({ ...outcome, stdout: outcome.stdout + '\nrevision-specific invariant' }, ['selected'], true, false, 'revision-specific invariant');
});

test('review154 proof copy rejects cycles and reports link resolution errors', { skip: process.platform === 'win32' }, () => {
  const root = mkdtempSync(join(tmpdir(), 'pm-review154-links-'));
  try {
    const source = join(root, 'source'); mkdirSync(source);
    assert.equal(spawnSync('git', ['init', '--quiet'], { cwd: source }).status, 0);
    writeFileSync(join(source, 'kept'), 'kept');
    symlinkSync('.', join(source, 'alias'));
    assert.throws(() => copyWorkingTree(source, join(root, 'copy')), /ancestor directory symlink cycle/, 'review154: never descend into an ancestor alias');
    rmSync(join(source, 'alias'));
    symlinkSync('cycle', join(source, 'cycle'));
    assert.throws(() => absentLink(join(source, 'cycle')), /ELOOP/, 'review154: cyclic links are errors rather than absent fixtures');
    rmSync(join(source, 'cycle'));
    symlinkSync('missing', join(source, 'dangling'));
    assert.equal(absentLink(join(source, 'dangling')), true);
    copyWorkingTree(source, join(root, 'copy-good'));
    assert.equal(readFileSync(join(root, 'copy-good/kept'), 'utf8'), 'kept');
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('review154 dangling proof skips identify symlink coverage', () => {
  const receipts: string[] = [];
  supportedProofs([{ name: 'dangling links: runner', test: 'copy contract', unixOnly: true }], undefined, 'win32', message => receipts.push(message));
  assert.match(receipts[0], /symlink proof requires a Unix host/, 'review154: dangling proof kind');
});

test('review154 Cargo configuration preserves source-relative references', () => {
  const root = mkdtempSync(join(tmpdir(), 'pm-review154-cargo-'));
  try {
    mkdirSync(join(root, 'source/.cargo'), { recursive: true });
    const config = join(root, 'source/.cargo/config.toml');
    writeFileSync(config, '[env]\nREVIEW154_LOCATION = { value = "toolchain-assets", relative = true }\n');
    assert.deepEqual(cargoConfigurationArgs(join(root, 'source')).slice(-2), ['--config', config], 'review154: preserve the original Cargo configuration location');
  } finally { rmSync(root, { recursive: true, force: true }); }
});

for (const mode of ['version-write', 'signal-intent']) {
  test('review154 real-copy refuses ' + mode, () => {
    const root = mkdtempSync(join(tmpdir(), 'pm-review154-receipt-'));
    try {
      mkdirSync(join(root, '.agents/pm'), { recursive: true });
      writeFileSync(join(root, '.agents/pm/settings.json'), '{}');
      writeFileSync(join(root, '.pm-rust-read-copy'), 'synthetic');
      const driver = join(root, 'driver.mjs'); writeFileSync(driver, 'synthetic driver');
      const shim = join(root, 'shim.mjs');
      writeFileSync(shim, [
        "import cp from 'node:child_process';import{writeFileSync}from'node:fs';import{syncBuiltinESMExports}from'node:module';",
        'cp.spawnSync=(command,args)=>{const argv=command===process.execPath?args.slice(1):args;',
        "if(argv[0]==='--version') {",
        mode === 'version-write' ? 'writeFileSync(' + JSON.stringify(join(root, '.agents/pm/settings.json')) + ',\'{"changed":true}\');' : '',
        "return{status:0,stdout:'2026.11.11\\n',stderr:''};}",
        "const stdout=argv[0]==='list'?JSON.stringify({items:[{id:'a',parent:'b'},{id:'b'}]}):JSON.stringify({context:{meta:{}}});",
        "return{status:" + (mode === 'signal-intent' ? "argv.includes('--for')?null:0" : '0') + ",stdout:Buffer.from(stdout),stderr:Buffer.from('')};};syncBuiltinESMExports();",
      ].join('\n'));
      const result = spawnSync(process.execPath, ['--import', pathToFileURL(shim).href, fileURLToPath(new URL('scripts/context-real.mts', packageRoot)), '--copy', root, '--driver', driver, '--binary', driver, '--samples', '1', '--output', join(root, 'receipt.json')], { encoding: 'utf8', timeout: 30_000 });
      assert.ifError(result.error);
      assert.notEqual(result.status, null, 'the measurement process must exit normally');
      assert.equal(result.status, 1, 'review154: incomplete or mutating measurements must refuse');
      assert.match(result.stderr, mode === 'version-write' ? /Authoritative copy changed/ : /Intent parity failed/, 'review154: specific receipt failure');
    } finally { rmSync(root, { recursive: true, force: true }); }
  });
}

test('review154 safety proof requires its oracle before copying source', () => {
  const root = mkdtempSync(join(tmpdir(), 'pm-review154-preflight-'));
  try {
    const result = spawnSync(process.execPath, [fileURLToPath(new URL('scripts/context-safety-revert.mts', packageRoot)), 'oracle provenance'], { cwd: root, encoding: 'utf8', timeout: 10_000 });
    assert.equal(result.status, 1);
    assert.match(result.stderr, /Missing oracle dependencies; run just oracle-install/, 'review154: actionable oracle preflight');
    assert.doesNotMatch(result.stderr, /Cannot enumerate package source/);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

for (const mode of ['registration', 'preload-registration', 'timeout', 'windows-job', 'windows-failed-stop']) {
  test('review154 process ownership ' + mode, () => {
    const root = mkdtempSync(join(tmpdir(), 'pm-review154-process-'));
    try {
      const windows = mode.startsWith('windows');
      const script = join(root, 'simulation.mjs');
      writeFileSync(script, [
        "import assert from 'node:assert/strict';import cp from 'node:child_process';import fs from 'node:fs';import{EventEmitter}from'node:events';import{PassThrough}from'node:stream';import{syncBuiltinESMExports}from'node:module';",
        "Object.defineProperty(process,'platform',{value:" + JSON.stringify(windows ? 'win32' : 'darwin') + "});",
        "const realTimer=globalThis.setTimeout;globalThis.setTimeout=(fn,ms,...args)=>realTimer(fn,ms===12000?100:ms,...args);",
        "let child,alive=true,kills=0,unref=false,marker='';",
        'const scratch=' + JSON.stringify(join(root, 'scratch')) + ';fs.mkdirSync(scratch);',
        "const originalWrite=fs.writeFileSync;fs.writeFileSync=(path,...args)=>{if(" + String(mode.endsWith('registration')) + "&&String(path).endsWith('.pid'))throw new Error('registration denied');return originalWrite(path,...args);};",
        "cp.spawn=(command,args)=>{child=new EventEmitter();child.pid=424242;child.stdout=new PassThrough();child.stderr=new PassThrough();child.unref=()=>{unref=true;};child.kill=()=>{kills++;alive=false;setImmediate(()=>{child.stderr.write(" + JSON.stringify(mode === 'windows-job' ? '' : undefined) + " ?? marker);child.emit('exit',1,'SIGTERM');child.emit('close',1);});return true;};",
        windows ? "const payload=Buffer.from(args.at(-1),'base64').toString('utf16le').match(/FromBase64String\\('([^']+)'\\)/)[1];marker=JSON.parse(Buffer.from(payload,'base64')).marker;" : '',
        'return child;};',
        "process.kill=(pid)=>{assert.equal(pid,-424242);assert.ok(fs.existsSync(scratch));child.kill();};",
        "cp.execFileSync=(command,args)=>{if(" + String(windows) + "){assert.equal(command,'powershell.exe','review154: termination must use named kernel ownership');const decoded=Buffer.from(args.at(-1),'base64').toString('utf16le');assert.ok(decoded.includes('OpenJobObject'));assert.ok(!decoded.includes('888888'));if(" + String(mode === 'windows-failed-stop') + ")throw new Error('job termination denied');return '';}return alive?'424242 S\\n':'';};",
        'syncBuiltinESMExports();',
        'const{ProofProcesses}=await import(' + JSON.stringify(new URL('scripts/context-proof-process.mts', packageRoot).href) + ');',
        'const owned=new ProofProcesses();',
        windows ? 'const registry=fs.readdirSync(' + JSON.stringify(root) + ').find(name=>name.startsWith("pm-proof-owned-"));originalWrite(' + JSON.stringify(root) + ' + "/" + registry + "/888888.pid","888888");' : '',
        'const result=await owned.run("cargo",[],{timeout:' + (mode.endsWith('registration') ? '5000' : '30') + '});',
        "assert.ok(result.error,'review154: runner cause must be retained');assert.match(result.error.message," + (mode.endsWith('registration') ? '/registration denied/' : '/timeout/') + ");",
        "assert.equal(owned.wasInterrupted,false,'review154: runner termination is not an external interrupt');",
        mode === 'windows-failed-stop'
          ? "assert.ok(unref,'review154: failed stop must detach handles');await assert.rejects(owned.cleanup(scratch),/scratch retained/);assert.ok(fs.existsSync(scratch));"
          : "assert.ok(kills>0,'review154: launched worker must be owned');await owned.cleanup(scratch);assert.equal(fs.existsSync(scratch),false);",
      ].join('\n'));
      const environment: NodeJS.ProcessEnv = { ...process.env };
      const preload = environment.PM_PROOF_PRELOAD_OPTION;
      if (preload) environment.NODE_OPTIONS = (environment.NODE_OPTIONS ?? '').replace(preload, '');
      delete environment.PM_PROOF_PRELOAD_OPTION;
      environment.TMPDIR = root; environment.TEMP = root; environment.TMP = root;
      if (windows || mode === 'preload-registration') environment.PM_PROOF_PROCESS_REGISTRY = root; else delete environment.PM_PROOF_PROCESS_REGISTRY;
      const result = spawnSync(process.execPath, [script], { encoding: 'utf8', timeout: 20_000, env: environment });
      assert.equal(result.status, 0, 'review154 ' + mode + ': ' + result.stderr);
    } finally { rmSync(root, { recursive: true, force: true }); }
  });
}

test('review154 proof runner forwards the selected interpreter without node on PATH', { skip: process.platform === 'win32' }, () => {
  const root = mkdtempSync(join(tmpdir(), 'pm-review154-interpreter-'));
  try {
    mkdirSync(join(root, 'bin'));
    const git = spawnSync('sh', ['-c', 'command -v git'], { encoding: 'utf8' }).stdout.trim();
    symlinkSync(git, join(root, 'bin/git'));
    const shim = join(root, 'shim.mjs');
    writeFileSync(shim, [
      "import assert from 'node:assert/strict';import childProcess from 'node:child_process';import{syncBuiltinESMExports}from'node:module';",
      'const original=childProcess.spawnSync;',
      "childProcess.spawnSync=(command,args,options)=>{if(command!=='cargo')return original(command,args,options);assert.equal(options.env.PM_NODE_INTERPRETER,process.execPath,'review154 interpreter must reach Cargo controls');throw new Error('review154 interpreter forwarded');};",
      syncSpawnBridge, 'syncBuiltinESMExports();',
    ].join('\n'));
    const result = spawnSync(process.execPath, ['--import', pathToFileURL(shim).href, fileURLToPath(new URL('scripts/context-review-revert.mts', packageRoot)), 'current_compact_tables_round_trip'], { cwd: fileURLToPath(packageRoot), encoding: 'utf8', timeout: 30_000, env: { ...process.env, PM_NODE_INTERPRETER: process.execPath, PATH: join(root, 'bin') } });
    assert.equal(result.status, 1);
    assert.match(result.stderr, /review154 interpreter forwarded/, 'review154: configured interpreter reaches proof controls');
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('review154 perf control failure preserves tool and compiler diagnostics', () => {
  const root = mkdtempSync(join(tmpdir(), 'pm-review154-diagnostic-'));
  try {
    const shim = join(root, 'shim.mjs');
    writeFileSync(shim, [
      "import childProcess from 'node:child_process';import{syncBuiltinESMExports}from'node:module';",
      "childProcess.spawnSync=()=>({status:1,stdout:'',stderr:'review154 compiler diagnostic',error:new Error('review154 tool unavailable')});",
      syncSpawnBridge, 'syncBuiltinESMExports();',
    ].join('\n'));
    const result = spawnSync(process.execPath, ['--import', pathToFileURL(shim).href, fileURLToPath(new URL('scripts/perf-revert-proof.mts', packageRoot))], { encoding: 'utf8', timeout: 30_000 });
    assert.equal(result.status, 1);
    assert.match(result.stderr, /review154 compiler diagnostic/, 'review154: compiler stderr survives control failure');
    assert.match(result.stderr, /review154 tool unavailable/, 'review154: spawn cause survives control failure');
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('review154 Unix cleanup finds ps without PATH entries', { skip: process.platform === 'win32' }, async () => {
  const root = mkdtempSync(join(tmpdir(), 'pm-review154-ps-'));
  const { ProofProcesses } = await import('./context-proof-process.mts');
  const owned = new ProofProcesses();
  const previousPath = process.env.PATH;
  process.env.PATH = root;
  try {
    const result = await owned.run(process.execPath, ['-e', 'process.stdout.write("completed")'], { env: { ...process.env, PATH: root }, timeout: 10_000 });
    assert.ifError(result.error);
    assert.equal(result.stdout, 'completed', 'review154: cleanup must inspect the owned group without PATH ps');
  } finally {
    try { await owned.cleanup(root); } finally {
      if (previousPath === undefined) delete process.env.PATH; else process.env.PATH = previousPath;
    }
  }
});

test('review154 Cargo options reach every independent proof runner', () => {
  const root = mkdtempSync(join(tmpdir(), 'pm-review154-cargo-runners-'));
  try {
    const source = join(root, 'source');
    copyWorkingTree(fileURLToPath(packageRoot), source);
    mkdirSync(join(source, '.cargo'), { recursive: true });
    const config = join(source, '.cargo/config.toml');
    writeFileSync(config, '[env]\nREVIEW154_LOCATION = { value = "toolchain-assets", relative = true }\n');
    for (const oracle of ['oracle', 'orientation-oracle']) symlinkSync(fileURLToPath(new URL('tests/' + oracle + '/node_modules', packageRoot)), join(source, 'tests', oracle, 'node_modules'), 'junction');
    const shim = join(root, 'shim.mjs');
    writeFileSync(shim, [
      "import assert from 'node:assert/strict';import childProcess from 'node:child_process';import{syncBuiltinESMExports}from'node:module';",
      'const original=childProcess.spawnSync;',
      "childProcess.spawnSync=(command,args,options)=>{if(command!=='cargo')return original(command,args,options);const index=args.indexOf('--config');assert.ok(index>=0,'original configuration missing');assert.equal(args[index+1]," + JSON.stringify(config) + ");throw new Error('review154 Cargo configuration forwarded');};",
      syncSpawnBridge, 'syncBuiltinESMExports();',
    ].join('\n'));
    for (const runner of ['native-adapter-revert.mts', 'perf-revert-proof.mts']) {
      const result = spawnSync(process.execPath, ['--import', pathToFileURL(shim).href, join(source, 'scripts', runner)], { cwd: source, encoding: 'utf8', timeout: 30_000 });
      assert.equal(result.status, 1);
      assert.match(result.stderr, /review154 Cargo configuration forwarded/, 'review154: original Cargo references reach ' + runner);
    }
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('review154 dated benchmark explicitly limits its evidence scope', () => {
  const receipt = JSON.parse(readFileSync(new URL('docs/benchmarks/native-context-intents-2026-11-06.json', packageRoot), 'utf8')) as { evidence_scope: { status: string; measured_revision: null; receipt_revision: string; runner_semantics: string } };
  assert.equal(receipt.evidence_scope.status, 'historical', 'review154: historical benchmark cannot certify the final head');
  assert.equal(receipt.evidence_scope.measured_revision, null, 'historical source revision was not retained and must not be inferred');
  assert.match(receipt.evidence_scope.receipt_revision, /^[a-f0-9]{40}$/);
  assert.match(receipt.evidence_scope.runner_semantics, /predates/);
});


test('review154 legacy proofs require the selected assertion location', () => {
  const root = mkdtempSync(join(tmpdir(), 'pm-review154-diagnostic-'));
  try {
    mkdirSync(join(root, 'tests'));
    writeFileSync(join(root, 'tests/corpus.rs'), 'fn fixture() { assert!(false); }\nfn selected() {\n    assert!(true); fixture();\n    assert_eq!(/* proof-behavior */ 1, 2);\n}\n');
    const diagnostic = assertionFailureDiagnostic(root, ['module::selected']);
    assert.equal(diagnostic.test("thread selected panicked at tests/corpus.rs:1:16:\nassertion failed"), false, 'review154: fixture assertions cannot certify selected regression');
    assert.equal(diagnostic.test("thread selected panicked at tests/corpus.rs:3:5:\nassertion failed"), false, 'review154: inline fixture assertions cannot certify selected regression');
    assert.equal(diagnostic.test("thread selected panicked at tests/corpus.rs:4:5:\nassertion failed"), true, 'review154: direct selected assertion is required');
    writeFileSync(join(root, 'tests/projected.rs'), 'fn compare() { assert_eq!(/* proof-behavior */ 1, 2); }\nfn projected() { compare(); }\nfn unrelated() { assert!(/* proof-behavior */ true); }\n');
    assert.equal(assertionFailureDiagnostic(root, ['projected']).test('thread projected panicked at tests/projected.rs:1:16:'), true);
    assert.equal(assertionFailureDiagnostic(root, ['unrelated']).test('thread unrelated panicked at tests/projected.rs:1:16:'), false);
    const sameLine = 'fn same_line() { assert!(true); assert_eq!(/* proof-behavior */ 1, 2); }';
    writeFileSync(join(root, 'tests/same.rs'), sameLine);
    const same = assertionFailureDiagnostic(root, ['same_line']);
    assert.equal(same.test('thread same_line panicked at tests/same.rs:1:' + (sameLine.indexOf('assert!') + 1) + ':'), false, 'review154: same-line fixture assertions cannot certify selected regression');
    assert.equal(same.test('thread same_line panicked at tests/same.rs:1:' + (sameLine.indexOf('assert_eq!') + 1) + ':'), true);
    assert.equal(same.test('thread same_line panicked at tests/same.rs:1:' + ((sameLine.indexOf('assert_eq!') + 1) * 10) + ':'), false, 'review154: column prefixes cannot stand in for declared assertion spans');
    writeFileSync(join(root, 'tests/corpus.test.mts'), "test('selected behavior', { timeout: 1000 }, () => {\n fixture(); assert.ok(true);\n assert.equal(/* proof-behavior */ 1, 2);\n});\n");
    const node = assertionFailureDiagnostic(root, ['selected behavior'], true);
    assert.equal(node.test('stack: |-\n    fixture (file:///package/tests/corpus.test.mts:1:1)\n    TestContext (file:///package/tests/corpus.test.mts:3:2)'), false, 'review154: first failing frame must be the selected assertion');
    assert.equal(node.test('stack: |-\n    TestContext (file:///package/tests/corpus.test.mts:3:2)'), true);
  } finally { rmSync(root, { recursive: true, force: true }); }
});
