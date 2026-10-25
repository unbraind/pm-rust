import type { CrashReport } from './crash-harness.mts';

/** Format bounded crash evidence as strict TOON with unique keys and tabular failures. */
export function formatCrashToon(report: CrashReport): string {
  const failures = report.cases.filter(row => row.failure);
  const recoveryFailures = report.recoveryParity.filter(row => !row.ok);
  const lines = [
    `schema: ${JSON.stringify(report.schema)}`,
    `ok: ${report.ok}`,
    `oracle_version: ${JSON.stringify(report.oracleVersion)}`,
    `source_items: ${report.sourceItems}`,
    `source_unchanged: ${report.sourceUnchanged}`,
    `acknowledged_comparisons: ${report.acknowledgedComparisons}`,
    `killed: ${report.killed}`,
    `failures_retained: ${report.failuresRetained}`,
    `clock_date_now: ${report.clock.dateNow}`,
    `clock_limitation: ${JSON.stringify(report.clock.limitation)}`,
    `recovery_comparisons: ${report.recoveryParity.length}`,
    `recovery_mismatches: ${recoveryFailures.length}`,
    `reduction: ${report.reduction?.status ?? 'none'}`,
    `phases[${report.phaseCoverage?.length ?? 0}]{scenario,phase,native,published,paired}:`,
    ...(report.phaseCoverage ?? []).map(row => `  ${row.scenario},${row.phase},${row.native},${row.published},${row.paired}`),
    `parity[${report.parity.length}]{scenario,bytes,acknowledgement}:`,
    ...report.parity.map(row => `  ${row.scenario},${row.bytes},${row.acknowledgement}`),
    `failures[${failures.length}]{scenario,implementation,syscall,boundary,code,remedy}:`,
    ...failures.map(row => `  ${row.scenario},${row.implementation},${row.syscall},${row.boundary},${row.failure!.code},${JSON.stringify(row.failure!.remedy)}`),
    `recovery_failures[${recoveryFailures.length}]{scenario,phase,syscall,boundary,published_syscall,published_boundary,bytes,receipt,state,journals,ok}:`,
    ...recoveryFailures.map(row => `  ${row.scenario},${row.phase ?? "legacy-unclassified"},${row.syscall},${row.boundary},${row.publishedSyscall ?? row.syscall},${row.publishedBoundary ?? row.boundary},${row.bytes},${row.receipt},${row.state},${row.journals},${row.ok}`),
    `cases[${report.cases.length}]{scenario,implementation,syscall,boundary,phase,restart_exact,return_exact,state,history_preserved,return_status,pending_journals,failure,ok}:`,
    ...report.cases.map(row => `  ${row.scenario},${row.implementation},${row.syscall},${row.boundary},${row.phase ?? "legacy-unclassified"},${row.restartExact ?? false},${row.returnExact ?? false},${row.state},${row.historyPreserved},${row.returnStatus},${row.journalsAfterRestart},${row.failure?.code ?? 'none'},${row.ok}`),
  ];
  if (report.reduction?.remedy) lines.push(`reduction_code: ${JSON.stringify(report.reduction.code)}`, `reduction_remedy: ${JSON.stringify(report.reduction.remedy)}`);
  return lines.join('\n');
}
