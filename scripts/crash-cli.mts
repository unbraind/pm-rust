import { parseArgs } from 'node:util';
import { CrashRefusal, replayCrashFailure, runCrashCorpus } from './crash-harness.mts';

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
    else {
      console.log(`schema: ${JSON.stringify(report.schema)}\nok: ${report.ok}\noracle_version: ${JSON.stringify(report.oracleVersion)}\nsource_items: ${report.sourceItems}\nsource_unchanged: ${report.sourceUnchanged}\nacknowledged_comparisons: ${report.acknowledgedComparisons}\nkilled: ${report.killed}\nfailures_retained: ${report.failuresRetained}`);
      console.log(`clock_date_now: ${report.clock.dateNow}\nclock_limitation: ${JSON.stringify(report.clock.limitation)}\nrecovery_comparisons: ${report.recoveryParity.length}\nrecovery_mismatches: ${report.recoveryParity.filter(row => !row.ok).length}\nreduction: ${report.reduction?.status ?? 'none'}`);
      console.log(`parity[${report.parity.length}]{scenario,bytes,acknowledgement}:`);
      for (const row of report.parity) console.log(`  ${row.scenario},${row.bytes},${row.acknowledgement}`);
      for (const row of report.cases.filter(row => row.failure)) console.log(`failure: ${JSON.stringify({ scenario: row.scenario, implementation: row.implementation, syscall: row.syscall, boundary: row.boundary, ...row.failure })}`);
      for (const row of report.recoveryParity.filter(row => !row.ok)) console.log(`recovery_failure: ${JSON.stringify(row)}`);
      console.log(`cases[${report.cases.length}]{scenario,implementation,syscall,boundary,state,history_preserved,return_status,pending_journals,failure,ok}:`);
      for (const row of report.cases) console.log(`  ${row.scenario},${row.implementation},${row.syscall},${row.boundary},${row.state},${row.historyPreserved},${row.returnStatus},${row.journalsAfterRestart},${row.failure?.code ?? 'none'},${row.ok}`);
    }
    process.exitCode = report.ok ? 0 : 1;
  } catch (error) {
    const refusal = error instanceof CrashRefusal ? error : new CrashRefusal('invalid_options', 'Check option spelling and values; run --help.');
    const payload = { ok: false, code: refusal.code, remedy: refusal.remedy };
    console.log(json ? JSON.stringify(payload) : `ok: false\ncode: ${JSON.stringify(payload.code)}\nremedy: ${JSON.stringify(payload.remedy)}`);
    process.exitCode = 2;
  }
}
main();
