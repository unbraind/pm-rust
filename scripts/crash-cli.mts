import { parseArgs } from 'node:util';
import { CrashRefusal, replayCrashFailure, runCrashCorpus } from './crash-harness.mts';
import { formatCrashToon } from './crash-output.mts';

/** Execute the standalone CLI and emit bounded, privacy-safe TOON or JSON. */
function main(): void {
  let json = process.argv.includes('--json');
  try {
    const { values } = parseArgs({ options: {
      native: { type: 'string' }, oracle: { type: 'string' }, source: { type: 'string' },
      artifacts: { type: 'string' }, 'max-boundaries': { type: 'string' }, timeout: { type: 'string' },
      json: { type: 'boolean' }, full: { type: 'boolean' }, replay: { type: 'string' }, strace: { type: 'string' }, help: { type: 'boolean' },
    } });
    json = values.json ?? false;
    if (values.help) {
      console.log('Usage: node scripts/crash-cli.mts --native <binary> --artifacts <failures-dir> [--source <public-tracker-copy> | --replay <case.json>] [--oracle <package>] [--max-boundaries <1..64>] [--timeout <ms>] [--json] [--full]');
      return;
    }
    if (!values.native || !values.artifacts) throw new CrashRefusal('invalid_options', 'Supply --native and --artifacts; use --help for the full command.');
    if (values.source !== undefined && values.replay !== undefined) throw new CrashRefusal('invalid_options', 'Supply only one of --source and --replay.');
    const options = { native: values.native, artifacts: values.artifacts, oracle: values.oracle,
      source: values.source, strace: values.strace, maxBoundaries: values['max-boundaries'] === undefined ? undefined : Number(values['max-boundaries']),
      timeoutMs: values.timeout === undefined ? undefined : Number(values.timeout) };
    const report = values.replay !== undefined ? replayCrashFailure(options, values.replay) : runCrashCorpus(options);
    if (json) console.log(JSON.stringify(values.full ? report : { ...report,
      cases: report.cases.map(row => ({ scenario: row.scenario, implementation: row.implementation,
        syscall: row.syscall, boundary: row.boundary, state: row.state, acknowledged: row.acknowledged,
        historyPreserved: row.historyPreserved, restartStatus: row.restartStatus, returnStatus: row.returnStatus,
        journalsAtKill: row.journalsAtKill, journalsAfterRestart: row.journalsAfterRestart, journalsAfterReturn: row.journalsAfterReturn,
        receiptMatches: row.receiptMatches, failure: row.failure, ok: row.ok })) }));
    else console.log(formatCrashToon(report));
    process.exitCode = report.ok ? 0 : 1;
  } catch (error) {
    const refusal = error instanceof CrashRefusal ? error : new CrashRefusal('invalid_options', 'Check option spelling and values; run --help.');
    const payload = { ok: false, code: refusal.code, remedy: refusal.remedy };
    console.log(json ? JSON.stringify(payload) : `ok: false\ncode: ${JSON.stringify(payload.code)}\nremedy: ${JSON.stringify(payload.remedy)}`);
    process.exitCode = 2;
  }
}
main();
