import { spawnSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';

/** Observable syscall classes shared by real native and published processes. */
export const CRASH_SYSCALLS = ['rename', 'fsync'] as const;
/** One syscall's per-task entry ordinal; ordinals are not cross-runtime semantic phases. */
export interface Boundary { syscall: typeof CRASH_SYSCALLS[number]; ordinal: number }
/** Complete child receipt, retained privately and reduced to hashes in public output. */
export interface ProcessReceipt { status: number | null; signal: string | null; stdout: Buffer; stderr: Buffer; trace: string }
/** Executable argument-vector adapter; never evaluated by a shell. */
export interface Implementation { executable: string; args: readonly string[]; trailing?: readonly string[] }

/** Spawn one bounded isolated process, optionally injecting SIGKILL at a per-task syscall entry. */
export function execute(implementation: Implementation, args: readonly string[], cwd: string,
  timeout: number, traceFile?: string, boundary?: Boundary, strace = 'strace'): ProcessReceipt {
  const argv = [...implementation.args, ...args, ...(implementation.trailing ?? [])];
  const traced = traceFile !== undefined;
  const result = spawnSync(traced ? strace : implementation.executable,
    traced ? ['-f', '--kill-on-exit', '-qq', '-o', traceFile, '-e', `trace=${CRASH_SYSCALLS.join(',')}`,
      ...(boundary ? ['-e', `inject=${boundary.syscall}:signal=SIGKILL:when=${boundary.ordinal}`] : []),
      '--', implementation.executable, ...argv] : argv, {
      cwd, timeout, killSignal: 'SIGKILL', maxBuffer: 4 * 1024 * 1024,
      env: {
        PATH: process.env.PATH, HOME: join(cwd, 'isolated-home'),
        SystemRoot: process.env.SystemRoot, SystemDrive: process.env.SystemDrive,
        COMSPEC: process.env.COMSPEC, PATHEXT: process.env.PATHEXT,
        TEMP: process.env.TEMP, TMP: process.env.TMP,
        PM_AUTHOR: 'conformance-agent', PM_SENTRY_DISABLED: '1', PM_TELEMETRY_SOURCE_CONTEXT: 'test',
        DO_NOT_TRACK: '1', NODE_DISABLE_COMPILE_CACHE: '1',
        GIT_CEILING_DIRECTORIES: cwd, GIT_CONFIG_GLOBAL: '/dev/null', GIT_CONFIG_NOSYSTEM: '1',
        LC_ALL: 'C', TZ: 'UTC',
      },
    });
  if (result.error) throw new Error('code' in result.error && result.error.code === 'ETIMEDOUT' ? 'timeout' : 'process_unavailable');
  return { status: result.status, signal: result.signal, stdout: result.stdout, stderr: result.stderr,
    trace: traceFile ? readFileSync(traceFile, 'utf8') : '' };
}

/** Determine the largest syscall ordinal reached by any individual traced task. */
export function boundaryCount(trace: string, syscall: Boundary['syscall']): number {
  const counts = new Map<string, number>();
  for (const line of trace.split('\n')) {
    const match = /^\s*(\d+)\s+(rename|fsync)\(/.exec(line);
    if (match?.[2] === syscall) counts.set(match[1], (counts.get(match[1]) ?? 0) + 1);
  }
  return Math.max(0, ...counts.values());
}

/** Require evidence of an injected fatal signal, rather than counting an ordinary exit as a crash. */
export function killedAtBoundary(receipt: ProcessReceipt, boundary: Boundary): boolean {
  const interrupted = new RegExp(`${boundary.syscall}\\([^\\n]*(?:= \\?|<unfinished)`);
  return receipt.status !== 0 && boundaryCount(receipt.trace, boundary.syscall) === boundary.ordinal && interrupted.test(receipt.trace) &&
    /killed by SIGKILL/.test(receipt.trace);
}
