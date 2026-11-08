/** Owned proof subprocesses: drain descendants before restoring or deleting scratch inputs. */
import { execFileSync, spawn, type SpawnOptions } from 'node:child_process';
import { rmSync } from 'node:fs';
import { setTimeout } from 'node:timers/promises';
import type { TestOutcome } from './context-revert-helpers.mts';

/** Completed output always owns both captured streams. */
interface CapturedOutcome extends TestOutcome { stdout: string; stderr: string; }

/** Async mutation controls always finish cleanup before the next control starts. */
export async function verifyAsyncMutation(check: (reverted: boolean) => Promise<void>, mutate: () => void, restore: () => void): Promise<void> {
  await check(false);
  try { mutate(); await check(true); } finally { restore(); }
  await check(false);
}

/** Own process groups across normal completion, timeouts, output limits and interruption. */
export class ProofProcesses {
  private groups = new Set<number>();
  private interrupted: NodeJS.Signals | undefined;
  private handlers = new Map<NodeJS.Signals, () => void>();
  private cleanupSafe = true;

  /** Reports whether the runner must stop without certifying further controls. */
  get wasInterrupted(): boolean { return this.interrupted !== undefined; }

  /** Capture interrupts until the owned scratch workspace has been cleaned. */
  constructor() {
    for (const signal of ['SIGINT', 'SIGTERM'] as const) {
      const handler = (): void => {
        this.interrupted ??= signal;
        process.exitCode = signal === 'SIGINT' ? 130 : 143;
        for (const pid of this.groups) this.stop(pid, signal);
      };
      this.handlers.set(signal, handler);
      process.on(signal, handler);
    }
  }

  /** Signal an entire owned process tree; failed Windows tree termination retains inputs. */
  private stop(pid: number, signal: NodeJS.Signals): void {
    if (!this.groups.has(pid)) return;
    if (process.platform === 'win32') {
      try {
        execFileSync('taskkill', ['/pid', String(pid), '/T', '/F'], { stdio: 'ignore' });
        this.groups.delete(pid);
      }
      catch { this.cleanupSafe = false; }
    } else {
      try { process.kill(-pid, signal); } catch (error) {
        if ((error as NodeJS.ErrnoException).code !== 'ESRCH') { this.cleanupSafe = false; throw error; }
      }
    }
  }

  /** Reap executable group members, escalating stubborn workers while inputs still exist. */
  private async drain(): Promise<void> {
    for (const pid of this.groups) {
      this.stop(pid, 'SIGTERM');
      if (process.platform !== 'win32') {
        const started = performance.now();
        for (;;) {
          const rows = execFileSync('ps', ['-eo', 'pgid=,stat='], { encoding: 'utf8' });
          const live = rows.split('\n').some(row => {
            const [group, state = ''] = row.trim().split(/\s+/);
            return Number(group) === pid && !state.startsWith('Z');
          });
          if (!live) break;
          if (performance.now() - started >= 1000) this.stop(pid, 'SIGKILL');
          if (performance.now() - started >= 5000) { this.cleanupSafe = false; throw new Error('Owned proof workers did not stop; scratch retained'); }
          await setTimeout(10);
        }
      }
      this.groups.delete(pid);
    }
  }

  /** Run one bounded tool asynchronously, preserving diagnostics and direct-child signals. */
  async run(command: string, args: string[], options: SpawnOptions & { timeout?: number; maxBuffer?: number } = {}): Promise<CapturedOutcome> {
    if (this.interrupted) throw new Error('Proof interrupted before the next invocation');
    const { timeout = 900_000, maxBuffer = 16 * 1024 * 1024, ...spawnOptions } = options;
    let escalation: ReturnType<typeof globalThis.setTimeout> | undefined;
    const child = spawn(command, args, { ...spawnOptions, detached: process.platform !== 'win32', stdio: ['ignore', 'pipe', 'pipe'] });
    const pid = child.pid;
    if (pid !== undefined) this.groups.add(pid);
    let stdout = ''; let stderr = ''; let bytes = 0; let error: Error | undefined;
    let drained: Promise<void> = Promise.resolve();
    /** Terminate the tree immediately, then escalate independently of inherited pipe closure. */
    const stop = (): void => {
      if (pid === undefined) return;
      this.stop(pid, 'SIGTERM');
      escalation ??= globalThis.setTimeout(() => this.stop(pid, 'SIGKILL'), 1000);
    };
    const timer = globalThis.setTimeout(() => { error = new Error('Proof subprocess timeout'); stop(); }, timeout);
    /** Bound diagnostics while still draining both output streams. */
    const receive = (chunk: Buffer, output: boolean): void => {
      bytes += chunk.length;
      if (bytes > maxBuffer) { error ??= new Error('Proof diagnostic byte ceiling'); stop(); }
      else if (output) stdout += chunk.toString(); else stderr += chunk.toString();
    };
    child.stdout!.on('data', (chunk: Buffer) => receive(chunk, true));
    child.stderr!.on('data', (chunk: Buffer) => receive(chunk, false));
    const onInterrupt = (): void => stop();
    process.on('SIGINT', onInterrupt); process.on('SIGTERM', onInterrupt);
    try {
      return await new Promise<CapturedOutcome>(resolve => {
        child.once('error', failure => { error = failure; });
        child.once('exit', (status, signal) => {
          if (signal === 'SIGINT' || signal === 'SIGTERM') {
            this.interrupted ??= signal;
            stop();
          }
          // Exit can precede close indefinitely when an orphan inherits the output pipes.
          if (process.platform === 'win32' && !signal && !this.interrupted && pid !== undefined) this.groups.delete(pid);
          drained = this.drain();
          void drained.catch(failure => { error = failure; child.stdout!.destroy(); child.stderr!.destroy(); });
        });
        child.once('close', status => resolve({ status, stdout, stderr, error }));
      }).then(async result => {
        await drained;
        if (this.interrupted) { process.exitCode = this.interrupted === 'SIGINT' ? 130 : 143; throw new Error('Proof interrupted'); }
        return result;
      });
    } finally {
      clearTimeout(timer); if (escalation) clearTimeout(escalation);
      process.off('SIGINT', onInterrupt); process.off('SIGTERM', onInterrupt);
    }
  }

  /** Remove scratch only after owned workers have stopped; always detach signal handlers. */
  async cleanup(scratch: string): Promise<void> {
    try {
      await this.drain();
      if (!this.cleanupSafe) throw new Error('Cannot prove descendant termination; scratch retained');
      rmSync(scratch, { recursive: true, force: true });
    } finally {
      for (const [signal, handler] of this.handlers) process.off(signal, handler);
    }
  }
}
