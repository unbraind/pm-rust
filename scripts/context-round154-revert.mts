/** Prove review regressions in one owned disposable source and build directory. */
import assert from 'node:assert/strict';
import { cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, symlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { ProofProcesses, verifyAsyncMutation } from './context-proof-process.mts';
import { assertNamedTests, checkedGit, copyWorkingTree, escapeTestName } from './context-revert-helpers.mts';

/** One surgical behavioral mutation, retaining the fixed tests. */
interface Edit { path: string; before: string; after: string; all?: boolean; }
/** A proof names both its assertion and the diagnostic specific to that invariant. */
interface Proof { name: string; test: string; failure: string; edits: Edit[]; target?: 'lib' | 'bin' | 'integration'; file?: string; windows?: boolean; withoutOracle?: boolean; }

const source = process.cwd();
const scratch = mkdtempSync(join(tmpdir(), 'pm-review154-revert-'));
const processes = new ProofProcesses();
const processPath = 'scripts/context-proof-process.mts';
const helpers = 'scripts/context-revert-helpers.mts';
/** Define a focused Node assertion without changing its setup or fixtures. */
function node(name: string, test: string, failure: string, edit: Edit): Proof { return { name, test: 'review154 ' + test, failure, edits: [edit] }; }
/** Define a real native regression with a mutation-specific assertion diagnostic. */
function rust(name: string, test: string, failure: string, edits: Edit[], target: Proof['target'] = 'lib', file?: string): Proof { return { name, test, failure, edits, target, file }; }

const partialSource = readFileSync('src/orientation_context.rs', 'utf8');
const partialBegin = partialSource.indexOf('        let mut documents = Vec::new();');
const partialEnd = partialSource.indexOf('        #[cfg(test)]', partialBegin);
assert.ok(partialBegin >= 0 && partialEnd > partialBegin);
const partialEdit: Edit = {
  path: 'src/orientation_context.rs', before: partialSource.slice(partialBegin, partialEnd),
  after: '        let mut documents = self.stream_items(if names.is_empty() { crate::read_projection::Projection::Context } else { crate::read_projection::Projection::ContextSections })?.collect::<Result<Vec<_>, _>>()?;\n        let item_warnings: Vec<String> = Vec::new();\n',
};

const diagnosticsSource = readFileSync('src/main.rs', 'utf8');
const diagnosticsBegin = diagnosticsSource.indexOf('    let mut spans = Vec::new();', diagnosticsSource.indexOf('fn orientation_diagnostic_text'));
const diagnosticsEnd = diagnosticsSource.indexOf('    text.char_indices()', diagnosticsBegin);
assert.ok(diagnosticsBegin >= 0 && diagnosticsEnd > diagnosticsBegin);
const wholeCommandSpans = `    let spans: Vec<_> = commands.filter(|command| command.starts_with("pm "))
        .flat_map(|command| text.match_indices(command).map(move |(start, _)| start..start + command.len()))
        .collect();
`;

const proofs: Proof[] = [
  node('same-line-fixture-diagnostic', 'legacy proofs require the selected assertion location', 'review154: same-line fixture assertions cannot certify selected regression', { path: 'scripts/context-proof-diagnostics.mts', before: "            const columns = Array.from({ length: lastColumn - firstColumn + 1 }, (_, index) => firstColumn + index).join('|');", after: "            const columns = '.*';" }),
  node('inline-fixture-diagnostic', 'legacy proofs require the selected assertion location', 'review154: inline fixture assertions cannot certify selected regression', { path: 'scripts/context-proof-diagnostics.mts', before: '          if (!markedArgument && !markedStatement) continue;', after: '          void markedArgument; void markedStatement;' }),
  rust('canonical-probe-cache', 'orientation_deps::packet::tests::review154_canonical_recovery_reuses_bounded_producers', 'review154: canonical probes must retain their bounded producer cache', [{ path: 'src/orientation_deps/packet.rs', before: '        &mut measurements.producers,', after: '        &mut RenderCache::default(),' }]),
  rust('partial-warning-estimate', 'orientation_context::tests::review154_external_schema_and_partial_context', 'review154: partial warning bytes must enter the selected intent estimate', [{ path: 'src/orientation_context.rs', before: '                crate::read_output::stabilize_intent(&mut delivery, original.toon);', after: '                let _ = &mut delivery;' }]),
  rust('path-only-diagnostics', 'tests::review154_lossless_retry_and_argument_roles', 'review154: only explicit path values retain diagnostic control characters', [{ path: 'src/main.rs', before: diagnosticsSource.slice(diagnosticsBegin, diagnosticsEnd), after: wholeCommandSpans }], 'bin'),
  rust('stored-full-depth', 'orientation_context::tests::context_stored_settings_fallback_precedes_overrides', 'review154: invalid stored settings must drop schema: {"default_depth":"full"}', [{ path: 'src/settings_shape.json', before: '"allowed":["brief","standard","deep","full"]', after: '"allowed":["brief","standard","deep"]' }]),
  node('legacy-mutation-diagnostic', 'legacy proofs require the selected assertion location', 'review154: fixture assertions cannot certify selected regression', { path: 'scripts/context-proof-diagnostics.mts', before: "  return new RegExp(alternatives.join('|'));", after: "  return new RegExp('');" }),
  node('registration', 'process ownership registration', 'review154: launched worker must be owned', { path: processPath, before: '      this.groups.add(pid);\n      if (process.platform', after: '      if (process.platform' }),
  node('preload-registration', 'process ownership preload-registration', 'Error: registration denied', { path: processPath, before: '        preloadFailures.set(child, failure as Error);', after: '        throw failure;' }),
  node('bounded-stop', 'process ownership windows-failed-stop', 'Detected unsettled top-level await', { path: processPath, before: '      abandon ??= globalThis.setTimeout(() => {', after: '      if (false) abandon ??= globalThis.setTimeout(() => {' }),
  node('runner-cause', 'process ownership timeout', 'Error: Proof interrupted', { path: processPath, before: '!runnerStopped && ', after: '' }),
  node('kernel-job', 'process ownership windows-job', 'review154: launched worker must be owned', { path: processPath, before: 'const stop = windowsJobStop(owned.marker);', after: 'const stop = { command: "taskkill", args: ["/pid", String(pid), "/T", "/F"] };' }),
  node('stale-windows-registration', 'process ownership windows-job', 'Cannot prove descendant termination; scratch retained', { path: processPath, before: "    if (process.platform === 'win32') return;", after: '' }),
  node('ps-search', 'Unix cleanup finds ps without PATH entries', 'spawnSync ps ENOENT', { path: processPath, before: "const executable = ['/bin/ps', '/usr/bin/ps'].find(path => existsSync(path)) ?? 'ps';", after: "const executable = 'ps';" }),
  node('version-snapshot', 'real-copy refuses version-write', 'review154: incomplete or mutating measurements must refuse', { path: 'scripts/context-real.mts', before: 'const before = authoritative(tracker);\nconst oracleVersion = verifiedOracleVersion(driver, root, environment);', after: 'const oracleVersion = verifiedOracleVersion(driver, root, environment);\nconst before = authoritative(tracker);' }),
  node('signal-status', 'real-copy refuses signal-intent', 'Benchmark parity failed: context --for orient --limit 10', { path: 'scripts/context-real.mts', before: 'oracle.status === null || native.status === null || ', after: '', all: true }),
  node('mutation-diagnostic', 'mutation proof rejects a named fixture failure', 'review154: unrelated fixture failures must not certify a mutation', { path: helpers, before: '  if (reverted) {', after: '  if (false) {' }),
  node('link-errors', 'proof copy rejects cycles and reports link resolution errors', 'review154: cyclic links are errors', { path: helpers, before: "['ENOENT', 'ENOTDIR'].includes((error as NodeJS.ErrnoException).code ?? '')", after: 'true' }),
  node('directory-cycle', 'proof copy rejects cycles and reports link resolution errors', 'review154: never descend into an ancestor alias', { path: helpers, before: 'if (entry?.isSymbolicLink() && statSync(path).isDirectory()) {', after: 'if (false) {' }),
  node('dangling-kind', 'dangling proof skips identify symlink coverage', 'review154: dangling proof kind', { path: helpers, before: '/symlink|dangling.link/i.test', after: '/symlink/i.test' }),
  node('interpreter', 'proof runner forwards the selected interpreter without node on PATH', 'review154 interpreter must reach Cargo controls', { path: 'scripts/context-review-revert.mts', before: ", 'PM_NODE_INTERPRETER'", after: '' }),
  node('oracle-preflight', 'safety proof requires its oracle before copying source', 'Cannot enumerate package source', { path: 'scripts/context-safety-revert.mts', before: "if (!existsSync(join(source, 'tests', oracle, 'node_modules', '@unbrained/pm-cli/dist/cli.js')))", after: "if (false && !existsSync(join(source, 'tests', oracle, 'node_modules', '@unbrained/pm-cli/dist/cli.js')))" }),
  node('cargo-configuration', 'Cargo options reach every independent proof runner', 'original configuration missing', { path: helpers, before: '  cargoOrigins.set(resolve(scratch), source);', after: '  void source; void scratch;' }),
  node('compiler-diagnostic', 'perf control failure preserves tool and compiler diagnostics', 'Error: Unexpected control result', { path: 'scripts/perf-revert-proof.mts', before: 'if (!expected) throw new Error(', after: 'if (!expected) throw new Error("Unexpected control result"); //' }),
  node('historical-receipt', 'dated benchmark explicitly limits its evidence scope', 'review154: historical benchmark cannot certify the final head', { path: 'docs/benchmarks/native-context-intents-2026-11-06.json', before: '"status": "historical"', after: '"status": "current"' }),
  rust('whole-settings-fallback', 'orientation_context::tests::context_stored_settings_fallback_precedes_overrides', 'review154: invalid stored settings must drop schema', [{ path: 'src/settings.rs', before: 'value: if fallback { json!({}) } else { value },', after: 'value: if fallback { json!({"schema":value["schema"]}) } else { value },' }]),
  rust('stored-validation', 'orientation_context::tests::review154_external_schema_and_partial_context', 'review154: merged schema must not invalidate stored settings', [{ path: 'src/orientation_context.rs', before: 'let settings_fallback = read.fallback;', after: 'let settings_fallback = read.fallback | sections::settings_fallback(&mut settings);' }]),
  rust('activity-whitespace', 'orientation_context::tests::review154_external_schema_and_partial_context', 'review154: whitespace-only activity limits must refuse', [{ path: 'src/orientation_context.rs', before: 'if trim_control(raw).is_empty() {', after: 'if false {' }]),
  rust('sdk-classification', 'orientation_context::tests::context_settings_warning_survives_refusals', 'review154: SDK refusal classification must remain CommandRefusal', [
    { path: 'src/error.rs', before: 'pub enum PmRustError {', after: 'pub enum PmRustError {\n    /// Legacy diagnostic wrapper.\n    #[error("{source}")]\n    ContextSettingsFallback {\n        /// Original typed failure.\n        source: Box<PmRustError>,\n    },' },
    { path: 'src/orientation_context.rs', before: '.map_err(|mut error| {', after: '.map_err(|mut error| {\n                if settings_fallback { return PmRustError::ContextSettingsFallback { source: Box::new(error) }; }' },
  ]),
  rust('lossless-retry', 'tests::review154_lossless_retry_and_argument_roles', 'review154: displayed retry must preserve exact path bytes', [{ path: 'src/main.rs', before: '!spans.iter().any(|span| span.contains(&index))', after: 'true', all: true }], 'bin'),
  rust('generous-metadata', 'read_output::tests::item_metadata_named_context_intent_compacts_under_budgets', 'review154: generous budget preserves item metadata', [{ path: 'src/read_output.rs', before: '    let amount = bound(options.output_limit.as_deref())?;', after: '    if options.output_budget.as_deref() == Some("5000") { compact_strings(&mut result.value); }\n    let amount = bound(options.output_limit.as_deref())?;' }]),
  rust('per-intent-receipt', 'orientation_context::tests::review154_each_intent_retains_receipt', 'each intent retains its own receipt: handoff', [{ path: 'src/orientation_context.rs', before: 'result = intent.attach(result.into(), original).value;', after: 'result = intent.attach(result.into(), original).value;\n            if original.intent.as_deref() == Some("handoff") { result.as_object_mut().map(|fields| fields.shift_remove("context_intent")); }' }]),
  rust('argument-selected-tracker', 'context_parser_refusals_preserve_settings_warnings', 'argument-selected tracker must warn', [{ path: 'src/main.rs', before: 'Cli::try_parse_from(prefix).map_or_else(|_| PathBuf::from("."), |cli| cli.workspace)', after: 'let _ = prefix; PathBuf::from(".")' }], 'integration', 'orientation_differential'),
  rust('partial-context', 'orientation_context::tests::review154_external_schema_and_partial_context', 'review154: unreadable item must preserve a partial packet', [partialEdit]),
  rust('list-wire', 'read_output::tests::review154_list_wire_receipt', 'review154: list receipt must measure the selected encoding', [{ path: 'src/read_output.rs', before: '        let measured = measure(result, toon);', after: '        let measured = if result["context_intent"]["command"] == "list" { estimate(result, true) } else { measure(result, toon) };' }]),
  rust('main-refusal-compaction', 'tests::current_orientation_diagnostics_preserve_other_commands', 'review154: main duplicate-detail compaction applies to every command', [{ path: 'src/main.rs', before: 'payload["detail"] == payload["title"]\n}', after: 'payload["detail"] == payload["title"] && current_orientation_refusal(payload)\n}' }], 'bin'),
  rust('generic-warning-redaction', 'tests::review154_generic_warning_refusals_redact_userinfo', 'review154: generic warning diagnostics retain main userinfo redaction', [{ path: 'src/main.rs', before: 'let detail = refusal::diagnostic(&error.to_string(), &raw_invocation());', after: 'let detail = error.to_string();' }], 'bin'),
  { name: 'managed-definitions', test: 'all managed mutation targets match the live package: context-review-revert.mts', failure: 'missing mutation src/orientation_context.rs', file: 'scripts/context-proof-definitions.test.mts', edits: [{ path: 'scripts/context-review-revert.mts', before: 'let settings_fallback = read.fallback;', after: 'let settings_fallback = sections::settings_fallback(&mut settings);', all: true }] },
  { ...rust('terminal-without-oracle', 'triage_omitted_intent_receipts_match_published', 'review154: terminal receipt survives without an oracle', [{ path: 'src/read_output.rs', before: '        result.value["read_output"] = receipt_for(options, default_budget, command);', after: '        if result["context_intent"].is_object() { return Ok(json!({"output_budget_exceeded":{"omitted_result":true}}).into()); }\n        result.value["read_output"] = receipt_for(options, default_budget, command);' }], 'integration', 'pagination_differential'), withoutOracle: true },

  { name: 'windows-argv', test: 'Windows proof job argv round-trips losslessly', failure: 'review154: kernel job argv must round-trip', windows: true, file: 'scripts/context-review-safety.test.mts', edits: [{ path: 'scripts/context-windows-job.mts', before: "const line = [command, ...args].map(quote).join(' ');", after: "const line = [quote(command), ...args].join(' ');" }] },

];

const filters = process.argv[2]?.split(',');
const chosen = proofs.filter(proof => filters === undefined || filters.some(filter => proof.name.includes(filter)));
assert.ok(chosen.length > 0, 'Unknown review proof filter');
try {
  copyWorkingTree(source, scratch);
  checkedGit(scratch, ['init', '--quiet'], 'Initialize disposable source enumeration');
  for (const oracle of ['oracle', 'orientation-oracle']) symlinkSync(join(source, 'tests', oracle, 'node_modules'), join(scratch, 'tests', oracle, 'node_modules'), 'junction');
  let nativeCopied = false;
  const previous = join(source, 'target/review154-revert.json');
  const retained: { proof: string; test: string; reverted: boolean; status: number | null }[] = existsSync(previous) ? (JSON.parse(readFileSync(previous, 'utf8')) as { receipts: { proof: string; test: string; reverted: boolean; status: number | null }[] }).receipts.filter(receipt => !chosen.some(proof => proof.name === receipt.proof)) : [];
  const receipts = retained;
  for (const proof of chosen) {
    if (proof.windows && process.platform !== 'win32') { console.log(JSON.stringify({ proof: proof.name, skipped: 'Windows host required' })); continue; }
    if (process.platform === 'win32' && ['link-errors', 'directory-cycle', 'ps-search', 'interpreter'].includes(proof.name)) {
      console.log(JSON.stringify({ proof: proof.name, skipped: ['link-errors', 'directory-cycle'].includes(proof.name) ? 'symlink proof requires a Unix host' : 'Unix host required' }));
      continue;
    }
    if (proof.target && !nativeCopied) {
      assert.ok(existsSync(join(source, 'target/debug')), 'Build targeted native tests before running their behavioral reverts');
      mkdirSync(join(scratch, 'target'), { recursive: true });
      cpSync(join(source, 'target/debug'), join(scratch, 'target/debug'), { recursive: true });
      nativeCopied = true;
    }
    const originals = new Map(proof.edits.map(edit => [edit.path, readFileSync(join(scratch, edit.path), 'utf8')]));
    for (const edit of proof.edits) {
      const count = originals.get(edit.path)!.split(edit.before).length - 1;
      assert.ok(count > 0 && (edit.all || count === 1), 'Stale review mutation: ' + proof.name + ': ' + edit.path);
    }
    await verifyAsyncMutation(async reverted => {
      const args = proof.target
        ? ['test', '-j', '2', '--locked', ...(proof.target === 'lib' ? ['--lib'] : proof.target === 'bin' ? ['--bin', 'pm-rust'] : ['--test', proof.file!]), proof.test, '--', '--exact', '--test-threads=2']
        : ['--test', '--test-reporter=tap', '--test-concurrency=1', '--test-name-pattern=^' + escapeTestName(proof.test) + '$', proof.file ?? 'scripts/context-round154.test.mts'];
      const childEnv: NodeJS.ProcessEnv = { ...process.env, CARGO_BUILD_JOBS: '2', RUST_TEST_THREADS: '2', CARGO_TARGET_DIR: join(scratch, 'target'), PM_NODE_INTERPRETER: process.execPath, PM_NODE_CLI: join(scratch, 'tests/oracle/node_modules/@unbrained/pm-cli') };
      if (proof.withoutOracle) { delete childEnv.PM_RUST_REQUIRE_PUBLISHED_CLI; childEnv.PM_NODE_CLI = join(scratch, 'unavailable-oracle'); }
      const result = await processes.run(proof.target ? 'cargo' : process.execPath, args, {
        cwd: scratch, timeout: 300_000,
        env: childEnv,
      });
      assertNamedTests(result, [proof.test], reverted, !proof.target, proof.failure);
      const receipt = { proof: proof.name, test: proof.test, reverted, status: result.status };
      receipts.push(receipt); console.log(JSON.stringify(receipt));
    }, () => {
      for (const edit of proof.edits) writeFileSync(join(scratch, edit.path), originals.get(edit.path)!.replaceAll(edit.before, edit.after));
    }, () => {
      for (const [path, bytes] of originals) writeFileSync(join(scratch, path), bytes);
    }, processes);
  }
  mkdirSync(join(source, 'target'), { recursive: true });
  writeFileSync(join(source, 'target/review154-revert.json'), JSON.stringify({ restored: true, receipts }, null, 2) + '\n');
} catch (error) { if (!processes.wasInterrupted) throw error; } finally { await processes.cleanup(scratch); }
