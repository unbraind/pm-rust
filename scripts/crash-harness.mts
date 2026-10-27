import { createHash } from 'node:crypto';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, realpathSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { basename, dirname, isAbsolute, join, relative, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';
import { CRASH_CLOCK_IMPORT, CRASH_SYSCALLS, boundaryCount, execute, killedAtBoundary } from './crash-process.mts';
import type { Boundary, Implementation, ProcessReceipt } from './crash-process.mts';
import { fingerprint, journals, materialize, preservesHistory, sameBytes, snapshot } from './crash-storage.mts';
import type { Snapshot } from './crash-storage.mts';
import { exactState, phasePairs, transactionPhase, transactionState } from './crash-oracle.mts';
import type { TransactionPhase } from './crash-oracle.mts';

/** Public clock used by both real implementations. */
export const CRASH_CLOCK = '2026-10-21T12:00:00.000Z';
/** Supported independent corpus mutations, each following an acknowledged seed write. */
export type CrashScenario = 'update' | 'comment';
/** Exact single-case selection for independently replaying a retained synthetic failure. */
export interface CrashSelection { scenario: CrashScenario; implementation: 'native' | 'published'; syscall: Boundary['syscall']; ordinal: number }
/** Machine-readable refusals deliberately omit paths and process output. */
export type RefusalCode = 'unsupported_platform' | 'invalid_options' | 'process_unavailable' |
  'timeout' | 'oracle_version' | 'unsafe_source' | 'baseline_failed' | 'boundary_unreached' | 'io_error';
/** A refusal is not a successful conformance receipt. */
export class CrashRefusal extends Error {
  readonly code: RefusalCode;
  readonly remedy: string;
  /** Construct an actionable, privacy-safe refusal. */
  constructor(code: RefusalCode, remedy: string) { super(code); this.code = code; this.remedy = remedy; }
}
/** SDK configuration. Input trackers are read only; every child works in a fresh non-git clone. */
export interface CrashOptions {
  native: string;
  oracle?: string;
  source?: string;
  artifacts: string;
  scenarios?: readonly CrashScenario[];
  /** Maximum per-task ordinal for each syscall. Default 64; truncation refuses. */
  maxBoundaries?: number;
  timeoutMs?: number;
  strace?: string;
  only?: CrashSelection;
}
/** One individually measured kill/restart result, with no raw tracker or process contents. */
export interface CrashCase {
  scenario: CrashScenario;
  implementation: 'native' | 'published';
  syscall: Boundary['syscall'];
  boundary: number;
  phase: TransactionPhase;
  restartExact: boolean;
  returnExact: boolean;
  killed: boolean;
  acknowledged: boolean;
  historyPreserved: boolean;
  restartStatus: number | null;
  state: 'before' | 'committed' | 'diverged';
  durableSha256: string;
  receiptSha256: string;
  receiptMatches: boolean;
  returnStatus: number | null;
  journalsAtKill: number;
  journalsAfterRestart: number;
  journalsAfterReturn: number;
  journalSha256: string;
  failure?: { code: 'restart_refused' | 'history_rewritten' | 'durable_divergence' | 'receipt_mismatch' | 'pending_journal' | 'return_refused'; remedy: string };
  ok: boolean;
}
/** Compact versioned agent receipt; every declared boundary has a corresponding row. */
export interface CrashReport {
  schema: 'pm-rust/crash-v1';
  ok: boolean;
  oracleVersion: string;
  nativeSha256: string;
  sourceFiles: number;
  sourceItems: number;
  sourceSha256: string;
  sourceUnchanged: boolean;
  acknowledgedComparisons: number;
  killed: number;
  boundaries: { scenario: CrashScenario; implementation: string; syscall: Boundary['syscall']; count: number }[];
  cases: CrashCase[];
  failuresRetained: number;
  parity: { scenario: CrashScenario; bytes: boolean; acknowledgement: boolean }[];
  recoveryParity: RecoveryComparison[];
  phaseCoverage?: { scenario: CrashScenario; phase: TransactionPhase; native: number; published: number; paired: boolean }[];
  clock: typeof CLOCK_POLICY;
  reduction?: { status: 'completed' | 'refused'; code?: RefusalCode; remedy?: string };
}

/** The metadata clock also freezes Date.now-based elapsed time; external process deadlines remain real. */
export const CLOCK_POLICY = { metadata: CRASH_CLOCK, dateNow: 'fixed', performanceNow: 'real',
  limitation: 'Published Date.now lock TTL/wait ages are frozen; only the real external timeout bounds those waits. This corpus does not certify elapsed-time or stale-gate parity.' } as const;

/** A paired recovery comparison never treats an unpaired syscall as cross-runtime evidence. */
export interface RecoveryComparison { scenario: CrashScenario; syscall: Boundary['syscall']; boundary: number; phase: TransactionPhase; publishedSyscall: Boundary['syscall']; publishedBoundary: number; bytes: boolean; receipt: boolean; state: boolean; journals: boolean; ok: boolean }

/** Compare recovered bytes and shared acknowledgement projections at a common observed cut. */
export function compareRecovery(native: Snapshot, published: Snapshot, nativeReceipt: string, publishedReceipt: string,
  nativeState: CrashCase['state'], publishedState: CrashCase['state'], nativeJournals: Snapshot, publishedJournals: Snapshot): Omit<RecoveryComparison, 'scenario' | 'syscall' | 'boundary' | 'phase' | 'publishedSyscall' | 'publishedBoundary'> {
  const result = { bytes: sameBytes(native, published), receipt: nativeReceipt === publishedReceipt,
    state: nativeState === publishedState, journals: sameBytes(nativeJournals, publishedJournals) };
  return { ...result, ok: Object.values(result).every(Boolean) };
}

/** Preserve an already measured verdict even if its independent synthetic reduction refuses. */
export function reduceFailure(report: CrashReport, reduce: () => CrashReport): void {
  try { report.failuresRetained = reduce().failuresRetained; report.reduction = { status: 'completed' }; }
  catch (error) {
    if (!(error instanceof CrashRefusal)) throw error;
    report.reduction = { status: 'refused', code: error.code, remedy: error.remedy };
  }
}

/** Resolve symlinks in an existing ancestor before checking a future output directory. */
function physicalPath(path: string): string {
  return existsSync(path) ? realpathSync(path) : join(physicalPath(dirname(path)), basename(path));
}

/** Hash a receipt without leaking contents to agent output. */
function receiptHash(receipt: ProcessReceipt): string {
  return createHash('sha256').update(JSON.stringify([receipt.status, receipt.signal]))
    .update(receipt.stdout).update('\0').update(receipt.stderr).digest('hex');
}

/** Build native or published command adapters with clocks pinned before CLI evaluation. */
function adapters(options: CrashOptions): Record<'native' | 'published', Implementation> {
  const oracle = resolve(options.oracle ?? fileURLToPath(new URL('../tests/oracle/node_modules/@unbrained/pm-cli', import.meta.url)));
  let metadata: unknown;
  try { metadata = JSON.parse(readFileSync(join(oracle, 'package.json'), 'utf8')); } catch { metadata = undefined; }
  if (typeof metadata !== 'object' || metadata === null || !('version' in metadata) || metadata.version !== '2026.10.23') {
    throw new CrashRefusal('oracle_version', 'Run just oracle-install to install the locked pm-cli 2026.10.23 oracle.');
  }
  return {
    native: { executable: resolve(options.native), args: [], trailing: ['--timestamp', CRASH_CLOCK] },
    published: { executable: process.execPath, args: ['--import', CRASH_CLOCK_IMPORT, join(oracle, 'dist/cli.js')], trailing: ['--json', '--no-extensions'] },
  };
}

/** Require a successful, acknowledged real CLI command. */
function acknowledged(receipt: ProcessReceipt): ProcessReceipt {
  if (receipt.status !== 0 || receipt.stdout.length === 0) {
    const refusal = new CrashRefusal('baseline_failed', 'Inspect the isolated baseline with the locked oracle and a compatible native executable.');
    refusal.cause = receipt;
    throw refusal;
  }
  return receipt;
}

/** Select only the probe item and stream, preserving all other source bytes separately. */
function probe(files: Snapshot, id: string): Map<string, Buffer> {
  return new Map([...files].filter(([name]) => name === `tasks/${id}.toon` || name === `history/${id}.jsonl`));
}

/** Require all pre-existing non-probe files, including original history, to remain exact. */
function originalsUnchanged(original: Snapshot, current: Snapshot, id: string): boolean {
  return [...original].every(([name, bytes]) => name.startsWith('runtime/') && !name.startsWith('runtime/transactions/') || current.get(name)?.equals(bytes)) &&
    [...current.keys()].every(name => original.has(name) || name.startsWith('runtime/') || name === `tasks/${id}.toon` || name === `history/${id}.jsonl` || !/\.(toon|jsonl)$/.test(name));
}

/** Validate options before reading source data or launching processes. */
function validate(options: CrashOptions): { timeout: number; maximum: number; scenarios: readonly CrashScenario[] } {
  const timeout = options.timeoutMs ?? 15000;
  const maximum = options.maxBoundaries ?? 64;
  const scenarios = options.scenarios ?? ['update', 'comment'];
  if (!options.native || !options.artifacts || !Number.isSafeInteger(timeout) || timeout < 100 || timeout > 120000 ||
    !Number.isSafeInteger(maximum) || maximum < 1 || maximum > 64 || scenarios.length === 0 ||
    new Set(scenarios).size !== scenarios.length || scenarios.some(scenario => !['update', 'comment'].includes(scenario))) {
    throw new CrashRefusal('invalid_options', 'Supply native and artifacts paths, distinct update/comment scenarios, timeout 100..120000 ms and maxBoundaries 1..64.');
  }
  if (options.only && (!scenarios.includes(options.only.scenario) || !['native', 'published'].includes(options.only.implementation) ||
    ![...CRASH_SYSCALLS, 'openat'].includes(options.only.syscall) || !Number.isSafeInteger(options.only.ordinal) || options.only.ordinal < 1 || options.only.ordinal > maximum || options.only.syscall === 'openat' && options.only.ordinal !== 1)) {
    throw new CrashRefusal('invalid_options', 'Select one declared scenario, implementation, syscall and positive ordinal within maxBoundaries.');
  }
  if (process.platform !== 'linux') throw new CrashRefusal('unsupported_platform', 'Run the live rename/fsync SIGKILL corpus on Linux with strace syscall injection enabled.');
  return { timeout, maximum, scenarios };
}

/** Run the real kill/restart corpus. It never mutates its input tracker or invokes Git. */
export function runCrashCorpus(options: CrashOptions): CrashReport {
  const { timeout, maximum, scenarios } = validate(options);
  const root = mkdtempSync(join(tmpdir(), 'pm-rust-crash-'));
  try {
    return run(options, root, timeout, maximum, scenarios);
  } catch (error) {
    if (error instanceof CrashRefusal) throw error;
    const message = error instanceof Error ? error.message : '';
    if (message === 'timeout' || message === 'process_unavailable') {
      throw new CrashRefusal(message, 'Install executable native/Node/strace tools, allow ptrace, and increase timeoutMs within its documented bound if needed.');
    }
    if (message === 'symlink' || message === 'special_file') throw new CrashRefusal('unsafe_source', 'Use a stable regular-file public tracker copy without symlinks or special files.');
    throw new CrashRefusal('io_error', 'Check readable input files and writable scratch/artifact directories; retry with a stable public tracker copy.');
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
}

/**
 * Replay a retained failure on its minimal synthetic fixture, never stored commands.
 * Boundary failures use their whitelisted scenario/implementation/syscall/ordinal.
 * A baseline_parity failure replays the scenario across both implementations and
 * all their observed boundaries, rather than selecting a single case.
 * A recovery_parity failure likewise replays its complete paired scenario.
 */
export function replayCrashFailure(options: Omit<CrashOptions, 'source' | 'only' | 'scenarios'>, evidence: string): CrashReport {
  let manifest: unknown;
  try { manifest = JSON.parse(readFileSync(evidence, 'utf8')); } catch {
    throw new CrashRefusal('invalid_options', 'Supply a readable retained case.json from this harness.');
  }
  const record = typeof manifest === 'object' && manifest !== null ? manifest as Record<string, unknown> : {};
  const selection = typeof record.only === 'object' && record.only !== null ? record.only as Record<string, unknown> : {};
  if (record.schema === 'pm-rust/crash-v1' && (record.kind === 'baseline_parity' || record.kind === 'recovery_parity') && (record.scenario === 'update' || record.scenario === 'comment')) {
    return runCrashCorpus({ ...options, scenarios: [record.scenario], source: join(dirname(resolve(evidence)), 'fixture/.agents/pm') });
  }
  if (record.schema !== 'pm-rust/crash-v1' || !['update', 'comment'].includes(String(selection.scenario)) ||
    !['native', 'published'].includes(String(selection.implementation)) || !['rename', 'fsync', 'openat'].includes(String(selection.syscall)) ||
    typeof selection.ordinal !== 'number') throw new CrashRefusal('invalid_options', 'Use a crash-v1 case with a declared synthetic scenario and syscall selection.');
  const only = selection as unknown as CrashSelection;
  return runCrashCorpus({ ...options, only, scenarios: [only.scenario], source: join(dirname(resolve(evidence)), 'fixture/.agents/pm') });
}

/** Execute measured cases from a private baseline and retain only synthetic failure fixtures. */
function run(options: CrashOptions, root: string, timeout: number, maximum: number, scenarios: readonly CrashScenario[]): CrashReport {
  const implementations = adapters(options);
  const base = join(root, 'base');
  mkdirSync(base);
  let source: Snapshot | undefined;
  const sourceRoot = options.source ? resolve(options.source) : undefined;
  if (sourceRoot) {
    const inside = relative(physicalPath(sourceRoot), physicalPath(resolve(options.artifacts)));
    if (inside !== '..' && !inside.startsWith(`..${sep}`) && !isAbsolute(inside)) throw new CrashRefusal('unsafe_source', 'Keep the artifact output directory outside the read-only source tracker.');
    if (!existsSync(join(sourceRoot, 'settings.json'))) throw new CrashRefusal('unsafe_source', 'Point source at the .agents/pm directory of a stable public tracker copy.');
    source = snapshot(sourceRoot);
    materialize(source, join(base, '.agents/pm'));
  } else {
    acknowledged(execute(implementations.published, ['init', '--defaults', '--prefix', 'crash-', '--agent-guidance', 'skip'], base, timeout));
  }
  const tracker = join(base, '.agents/pm');
  const original = snapshot(tracker);
  let settings: unknown;
  try { settings = JSON.parse(readFileSync(join(tracker, 'settings.json'), 'utf8')); } catch { settings = undefined; }
  const prefix = typeof settings === 'object' && settings !== null && 'id_prefix' in settings ? settings.id_prefix : undefined;
  if (typeof prefix !== 'string' || !/^[a-z0-9-]+$/.test(prefix)) throw new CrashRefusal('unsafe_source', 'Use a tracker with a lowercase ASCII id_prefix.');
  const id = `${prefix}crash-probe`;
  if ([...original.keys()].some(name => name.endsWith(`/${id}.toon`) || name === `history/${id}.jsonl`)) {
    throw new CrashRefusal('unsafe_source', 'Use a public tracker copy without the reserved crash-probe item.');
  }
  const common = ['--author', 'conformance-agent'];
  acknowledged(execute(implementations.published, ['create', 'task', 'Synthetic crash probe', '--id', id,
    '--description', 'Acknowledged synthetic write', '--priority', '1', '--create-mode', 'progressive', ...common], base, timeout));
  const seed = snapshot(tracker);
  const report: CrashReport = {
    schema: 'pm-rust/crash-v1', ok: true, oracleVersion: '2026.10.23',
    nativeSha256: createHash('sha256').update(readFileSync(resolve(options.native))).digest('hex'),
    sourceFiles: original.size, sourceItems: [...original.keys()].filter(name => name.endsWith('.toon')).length, sourceSha256: fingerprint(original),
    sourceUnchanged: true, acknowledgedComparisons: 0, killed: 0, boundaries: [], cases: [], failuresRetained: 0, parity: [], recoveryParity: [], phaseCoverage: [], clock: CLOCK_POLICY,
  };
  let sequence = 0;
  /** Make a unique workspace from an immutable snapshot. */
  function clone(files: Snapshot): string {
    const directory = join(root, `case-${sequence++}`);
    materialize(files, join(directory, '.agents/pm'));
    return directory;
  }
  for (const scenario of scenarios) {
    const mutation = scenario === 'update' ? ['update', id, '--description', 'Interrupted synthetic write'] : ['comment', id, 'Interrupted synthetic comment'];
    // A different operation must observe/recover every pending journal before it writes.
    const restart = scenario === 'update' ? ['comment', id, 'Restart synthetic sentinel', ...common] :
      ['update', id, '--description', 'Restart synthetic description', ...common];
    const returnCommand = scenario === 'update' ? ['update', id, '--priority', '2', ...common] :
      ['comment', id, 'Return synthetic sentinel', ...common];
    const baselines = new Map<string, Snapshot>();
    const receipts = new Map<string, string>();
    const recoveries: { row: CrashCase; phase: TransactionPhase; implementation: 'native' | 'published'; syscall: Boundary['syscall']; boundary: number; bytes: Snapshot; receipt: string; journals: Snapshot }[] = [];
    for (const [label, implementation] of Object.entries(implementations)) {
      const directory = clone(seed);
      const trace = join(directory, 'baseline.trace');
      const receipt = acknowledged(execute(implementation, [...mutation, ...common], directory, timeout, trace, undefined, options.strace));
      baselines.set(label, snapshot(join(directory, '.agents/pm')));
      // Native update/comment expose legacy full JSON; published mutations expose flat JSON.
      // Compare the shared update id/status or comment id/comments acknowledgement projection.
      let decoded: unknown;
      try { decoded = JSON.parse(receipt.stdout.toString()); } catch { decoded = undefined; }
      const record = typeof decoded === 'object' && decoded !== null ? decoded as Record<string, unknown> : {};
      const identity = typeof record.item === 'object' && record.item !== null ? record.item as Record<string, unknown> : record;
      if (identity.id !== id || (scenario === 'update' ? typeof identity.status !== 'string' : !Array.isArray(identity.comments))) {
        const refusal = new CrashRefusal('baseline_failed', 'Use implementations that emit an update id/status or comment id/comments acknowledgement.');
        refusal.cause = { label, scenario, decoded };
        throw refusal;
      }
      receipts.set(label, JSON.stringify([identity.id, scenario === 'update' ? identity.status : identity.comments]));
      const historyDirectory = clone(seed);
      const historyReceipt = acknowledged(execute(implementation, [...mutation, ...common], historyDirectory, timeout,
        join(historyDirectory, 'history.trace'), undefined, options.strace, join(historyDirectory, `.agents/pm/history/${id}.jsonl`)));
      rmSync(historyDirectory, { recursive: true, force: true });
      const counts: { syscall: Boundary['syscall']; count: number }[] = CRASH_SYSCALLS.map(syscall => ({ syscall, count: boundaryCount(receipt.trace, syscall) }));
      if (boundaryCount(historyReceipt.trace, 'openat') < 1) throw new CrashRefusal('boundary_unreached', 'Enable history-path openat tracing; the post-publication history boundary must be observed.');
      counts.push({ syscall: 'openat', count: 1 });
      report.boundaries.push(...counts.map(entry => ({ scenario, implementation: label, ...entry })));
      if (counts.every(entry => entry.count === 0) || counts.some(entry => entry.count > maximum)) {
        const refusal = new CrashRefusal('boundary_unreached', 'Allow ptrace/syscall tracing and set maxBoundaries high enough for every measured per-task rename/fsync ordinal (maximum 64).');
        refusal.cause = { label, counts, receipt };
        throw refusal;
      }
      // The reference restart uses precisely the same invocation as interrupted cases.
      const committedReceipt = acknowledged(execute(implementation, restart, directory, timeout));
      const committed = snapshot(join(directory, '.agents/pm'));
      acknowledged(execute(implementation, returnCommand, directory, timeout));
      const committedReturn = snapshot(join(directory, '.agents/pm'));
      rmSync(directory, { recursive: true, force: true });
      const beforeDirectory = clone(seed);
      const beforeReceipt = acknowledged(execute(implementation, restart, beforeDirectory, timeout));
      const before = snapshot(join(beforeDirectory, '.agents/pm'));
      acknowledged(execute(implementation, returnCommand, beforeDirectory, timeout));
      const beforeReturn = snapshot(join(beforeDirectory, '.agents/pm'));
      rmSync(beforeDirectory, { recursive: true, force: true });
      for (const { syscall, count } of counts) {
        for (let ordinal = 1; ordinal <= count; ordinal++) {
          if (options.only && (options.only.scenario !== scenario || options.only.implementation !== label || options.only.syscall !== syscall || options.only.ordinal !== ordinal)) continue;
          const killedDirectory = clone(seed);
          const crashed = execute(implementation, [...mutation, ...common], killedDirectory, timeout,
            join(killedDirectory, 'kill.trace'), { syscall, ordinal }, options.strace,
            syscall === 'openat' ? join(killedDirectory, `.agents/pm/history/${id}.jsonl`) : undefined);
          if (!killedAtBoundary(crashed, { syscall, ordinal })) {
            const refusal = new CrashRefusal('boundary_unreached', 'Use strace with SIGKILL injection support; a requested boundary must be observed, never skipped.');
            refusal.cause = { label, syscall, ordinal, receipt: crashed };
            throw refusal;
          }
          const atKill = snapshot(join(killedDirectory, '.agents/pm'));
          const phase = transactionPhase(crashed.trace, label as 'native' | 'published', id);
          const expected = phase === 'before-commit' ? before : committed;
          const expectedReturn = phase === 'before-commit' ? beforeReturn : committedReturn;
          const recovered = execute(implementation, restart, killedDirectory, timeout);
          const after = snapshot(join(killedDirectory, '.agents/pm'));
          const returned = recovered.status === 0 ? execute(implementation, returnCommand, killedDirectory, timeout) : undefined;
          const afterReturn = returned ? snapshot(join(killedDirectory, '.agents/pm')) : after;
          const pending = journals(after);
          const pendingAfterReturn = journals(afterReturn);
          const restartExact = exactState(expected, after);
          const returnExact = exactState(expectedReturn, afterReturn);
          const state: CrashCase['state'] = restartExact ? phase === 'before-commit' ? 'before' : 'committed' : 'diverged';
          const historyPreserved = preservesHistory(seed, after);
          const itemName = `tasks/${id}.toon`;
          const itemPreserved = [seed, before, committed, baselines.get(label)!].some(reference => after.get(itemName)?.equals(reference.get(itemName)!));
          const acknowledgedWrite = originalsUnchanged(original, after, id) && historyPreserved && itemPreserved;
          const receiptMatches = state === 'before' ? receiptHash(recovered) === receiptHash(beforeReceipt) :
            state === 'committed' && receiptHash(recovered) === receiptHash(committedReceipt);
          const row: CrashCase = {
            scenario, implementation: label as 'native' | 'published', syscall, boundary: ordinal, phase, restartExact, returnExact, killed: true,
            acknowledged: acknowledgedWrite, historyPreserved, restartStatus: recovered.status, state,
            durableSha256: fingerprint(probe(after, id)), receiptSha256: receiptHash(recovered),
            receiptMatches, returnStatus: returned?.status ?? null, journalsAtKill: journals(atKill).size,
            journalsAfterRestart: pending.size, journalsAfterReturn: pendingAfterReturn.size, journalSha256: fingerprint(pending),
            ok: recovered.status === 0 && returned?.status === 0 && acknowledgedWrite && state !== 'diverged' && receiptMatches && pending.size === 0 && pendingAfterReturn.size === 0 && restartExact && returnExact,
          };
          if (!row.ok) row.failure = {
            code: recovered.status !== 0 ? 'restart_refused' : pending.size > 0 ? 'pending_journal' : returned?.status !== 0 ? 'return_refused' : !historyPreserved ? 'history_rewritten' : !restartExact || !returnExact ? 'durable_divergence' : 'receipt_mismatch',
            remedy: 'Replay the retained synthetic case and inspect its lock, journal, bytes and receipt; preserve all original history. Runtime repairs need separate implementation work.',
          };
          report.cases.push(row);
          let projected: unknown;
          try { projected = JSON.parse(recovered.stdout.toString()); } catch { projected = { status: recovered.status }; }
          const outer = projected !== null && typeof projected === 'object' ? projected as Record<string, unknown> : {};
          const entity = outer.item !== null && typeof outer.item === 'object' ? outer.item as Record<string, unknown> : outer;
          const projectedReceipt = JSON.stringify([recovered.status, entity.id, scenario === 'update' ? entity.comments : entity.status, returned?.status]);
          recoveries.push({ row, phase, implementation: row.implementation, syscall, boundary: ordinal, bytes: transactionState(after), receipt: projectedReceipt, journals: pending });
          report.killed++;
          if (!row.ok) {
            report.ok = false;
            // Never retain source data: the independent one-item synthetic run is the reduction corpus.
            if (!source) {
              mkdirSync(resolve(options.artifacts), { recursive: true });
              const retained = mkdtempSync(join(resolve(options.artifacts), 'failure-'));
              const minimal = new Map([...original].filter(([name]) => !name.endsWith('.toon') && !name.endsWith('.jsonl')));
              materialize(minimal, join(retained, 'fixture/.agents/pm'));
              materialize(probe(seed, id), join(retained, 'acknowledged'));
              materialize(transactionState(after), join(retained, 'recovered'));
              materialize(transactionState(expected), join(retained, 'expected-restart'));
              materialize(transactionState(expectedReturn), join(retained, 'expected-return'));
              materialize(transactionState(afterReturn), join(retained, 'returned'));
              materialize(new Map([...atKill].filter(([name]) => name.startsWith('runtime/'))), join(retained, 'at-kill'));
              materialize(new Map([...afterReturn].filter(([name]) => name.startsWith('runtime/'))), join(retained, 'after-return'));
              const only: CrashSelection = { scenario, implementation: row.implementation, syscall, ordinal };
              writeFileSync(join(retained, 'case.json'), JSON.stringify({ schema: report.schema, only, row, mutation, restart, returnCommand, clock: CLOCK_POLICY }, null, 2));
              writeFileSync(join(retained, 'kill.trace'), crashed.trace);
              writeFileSync(join(retained, 'receipts.json'), JSON.stringify({ recovered: { status: recovered.status, stdout: recovered.stdout.toString(), stderr: recovered.stderr.toString() },
                before: { status: beforeReceipt.status, stdout: beforeReceipt.stdout.toString(), stderr: beforeReceipt.stderr.toString() },
                committed: { status: committedReceipt.status, stdout: committedReceipt.stdout.toString(), stderr: committedReceipt.stderr.toString() } }, null, 2));
              report.failuresRetained++;
            }
          }
          rmSync(killedDirectory, { recursive: true, force: true });
        }
      }
    }
    for (const phase of ['before-commit', 'after-commit', 'during-cleanup'] as const) {
      const native = recoveries.filter(cut => cut.implementation === 'native' && cut.phase === phase).length;
      const published = recoveries.filter(cut => cut.implementation === 'published' && cut.phase === phase).length;
      report.phaseCoverage!.push({ scenario, phase, native, published, paired: native > 0 && published > 0 });
    }
    for (const [nativeRecovery, publishedRecovery] of phasePairs(recoveries)) {
      const candidate = nativeRecovery.row;
      const comparison = compareRecovery(nativeRecovery.bytes, publishedRecovery.bytes, nativeRecovery.receipt, publishedRecovery.receipt,
        candidate.state, publishedRecovery.row.state, nativeRecovery.journals, publishedRecovery.journals);
      report.recoveryParity.push({ scenario, syscall: candidate.syscall, boundary: candidate.boundary, phase: candidate.phase, publishedSyscall: publishedRecovery.row.syscall, publishedBoundary: publishedRecovery.row.boundary, ...comparison });
      if (!comparison.ok) {
        report.ok = false;
        if (!source && candidate.ok && publishedRecovery.row.ok) {
          mkdirSync(resolve(options.artifacts), { recursive: true });
          const retained = mkdtempSync(join(resolve(options.artifacts), 'failure-'));
          materialize(new Map([...original].filter(([name]) => !name.endsWith('.toon') && !name.endsWith('.jsonl') && !name.startsWith('runtime/'))), join(retained, 'fixture/.agents/pm'));
          materialize(probe(seed, id), join(retained, 'acknowledged'));
          materialize(nativeRecovery.bytes, join(retained, 'native'));
          materialize(publishedRecovery.bytes, join(retained, 'published'));
          writeFileSync(join(retained, 'case.json'), JSON.stringify({ schema: report.schema, kind: 'recovery_parity', scenario, syscall: candidate.syscall, ordinal: candidate.boundary, comparison }, null, 2));
          report.failuresRetained++;
        }
      }
    }
    const native = baselines.get('native')!;
    const published = baselines.get('published')!;
    const bytes = exactState(native, published);
    const acknowledgement = receipts.get('native') === receipts.get('published');
    const parity = bytes && acknowledgement;
    report.parity.push({ scenario, bytes, acknowledgement });
    report.acknowledgedComparisons++;
    if (!parity) {
      report.ok = false;
      if (!source) {
        mkdirSync(resolve(options.artifacts), { recursive: true });
        const retained = mkdtempSync(join(resolve(options.artifacts), 'failure-'));
        materialize(new Map([...original].filter(([name]) => !name.endsWith('.toon') && !name.endsWith('.jsonl'))), join(retained, 'fixture/.agents/pm'));
        materialize(probe(seed, id), join(retained, 'acknowledged'));
        materialize(probe(native, id), join(retained, 'native'));
        materialize(probe(published, id), join(retained, 'published'));
        writeFileSync(join(retained, 'case.json'), JSON.stringify({ schema: report.schema, kind: 'baseline_parity', scenario, bytes, acknowledgement, clock: CRASH_CLOCK }, null, 2));
        report.failuresRetained++;
      }
    }
  }
  if (options.only && report.cases.length !== 1) throw new CrashRefusal('boundary_unreached', 'The retained boundary was not observed by these executable versions; use the original pinned implementations.');
  if (sourceRoot && source) {
    report.sourceUnchanged = sameBytes(source, snapshot(sourceRoot));
    report.ok &&= report.sourceUnchanged;
    if (!report.ok) {
      // Independently replay the same corpus on a minimal synthetic tracker, never copy public histories into evidence.
      reduceFailure(report, () => runCrashCorpus({ ...options, source: undefined }));
    }
  }
  return report;
}
