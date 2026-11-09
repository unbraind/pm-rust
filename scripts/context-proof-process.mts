/** Owned proof subprocesses: drain descendants before restoring or deleting scratch inputs. */
import { execFileSync, spawn, type SpawnOptions } from 'node:child_process';
import { mkdtempSync, readdirSync, rmSync, writeFileSync, type Dirent } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { randomUUID } from 'node:crypto';
import { windowsJob } from './context-windows-job.mts';
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
  private registry = mkdtempSync(join(process.env.PM_PROOF_PROCESS_REGISTRY ?? tmpdir(), 'pm-proof-owned-'));
  private registrations = new Map<number, string>();
  private runners = new Map<number, string>();
  private interrupted: NodeJS.Signals | undefined;
  private handlers = new Map<NodeJS.Signals, () => void>();
  private cleanupSafe = true;

  /** Reports whether the runner must stop without certifying further controls. */
  get wasInterrupted(): boolean { return this.interrupted !== undefined; }

  /** Capture interrupts until the owned scratch workspace has been cleaned. */
  constructor() {
    if (process.platform !== 'win32') writeFileSync(join(this.registry, `${process.pid}.owner`), String(process.pid));
    for (const signal of ['SIGINT', 'SIGTERM'] as const) {
      const handler = (): void => {
        this.interrupted ??= signal;
        process.exitCode = signal === 'SIGINT' ? 130 : 143;
        this.collectNested();
        for (const pid of this.runners.keys()) this.stopRunner(pid, signal);
        for (const pid of this.groups) this.stop(pid, signal);
      };
      this.handlers.set(signal, handler);
      process.on(signal, handler);
    }
  }

  /** Adopt every nested runner group registered below this ownership scope. */
  private collectNested(directory = this.registry): void {
    let entries: Dirent[];
    try { entries = readdirSync(directory, { withFileTypes: true }); }
    catch (error) { if ((error as NodeJS.ErrnoException).code === 'ENOENT') return; throw error; }
    for (const entry of entries) {
      const path = join(directory, entry.name);
      try {
        if (entry.isDirectory()) this.collectNested(path);
        else if (/^\d+\.owner$/.test(entry.name) && process.platform !== 'win32') {
          const pid = Number(entry.name.split('.')[0]);
          if (pid !== process.pid) this.runners.set(pid, path);
        } else if (/^\d+\.pid$/.test(entry.name)) {
          const pid = Number(entry.name.split('.')[0]);
          if (Number.isSafeInteger(pid) && pid > 0) { this.groups.add(pid); this.registrations.set(pid, path); }
        }
      } catch (error) { if ((error as NodeJS.ErrnoException).code !== 'ENOENT') throw error; }
    }
  }

  /** Release registration only after the process tree is known to have stopped. */
  private forget(pid: number): void {
    this.groups.delete(pid);
    const path = this.registrations.get(pid);
    if (path) rmSync(path, { force: true });
    this.registrations.delete(pid);
  }

  /** Signal an entire owned process tree; failed Windows tree termination retains inputs. */
  private stop(pid: number, signal: NodeJS.Signals): void {
    if (!this.groups.has(pid)) return;
    if (process.platform === 'win32') {
      try {
        execFileSync('taskkill', ['/pid', String(pid), '/T', '/F'], { stdio: 'ignore' });
        this.forget(pid);
      }
      catch { this.cleanupSafe = false; }
    } else {
      try { process.kill(-pid, signal); } catch (error) {
        if ((error as NodeJS.ErrnoException).code !== 'ESRCH') { this.cleanupSafe = false; throw error; }
      }
    }
  }

  /** Stop a nested runner itself, allowing its own restoration finally block to finish. */
  private stopRunner(pid: number, signal: NodeJS.Signals): void {
    try { process.kill(pid, signal); }
    catch (error) { if ((error as NodeJS.ErrnoException).code !== 'ESRCH') { this.cleanupSafe = false; throw error; } }
  }

  /** Wait for executable members, keeping inputs until both workers and restoring runners stop. */
  private async reap(pid: number, group: boolean): Promise<void> {
    const started = performance.now();
    for (;;) {
      const rows = execFileSync('ps', ['-eo', group ? 'pgid=,stat=' : 'pid=,stat='], { encoding: 'utf8' });
      const live = rows.split('\n').some(row => {
        const [identity, state = ''] = row.trim().split(/\s+/);
        return Number(identity) === pid && !state.startsWith('Z');
      });
      if (!live) return;
      if (performance.now() - started >= (group ? 1000 : 5000)) {
        if (group) this.stop(pid, 'SIGKILL'); else this.stopRunner(pid, 'SIGKILL');
      }
      if (performance.now() - started >= (group ? 5000 : 8000)) { this.cleanupSafe = false; throw new Error('Owned proof workers did not stop; scratch retained'); }
      await setTimeout(10);
    }
  }

  /** Reap executable group members, escalating stubborn workers while inputs still exist. */
  private async drain(): Promise<void> {
    for (;;) {
      this.collectNested();
      if (this.groups.size === 0 && this.runners.size === 0) break;
      for (const pid of this.runners.keys()) this.stopRunner(pid, 'SIGTERM');
      for (const pid of this.groups) {
        this.stop(pid, 'SIGTERM');
        if (process.platform !== 'win32') await this.reap(pid, true);
        this.forget(pid);
      }
      for (const [pid, path] of this.runners) {
        await this.reap(pid, false);
        rmSync(path, { force: true });
        this.runners.delete(pid);
      }
    }
  }

  /** Run one bounded tool asynchronously, preserving diagnostics and direct-child signals. */
  async run(command: string, args: string[], options: SpawnOptions & { timeout?: number; maxBuffer?: number } = {}): Promise<CapturedOutcome> {
    if (this.interrupted) throw new Error('Proof interrupted before the next invocation');
    const { timeout = 900_000, maxBuffer = 16 * 1024 * 1024, ...spawnOptions } = options;
    let escalation: ReturnType<typeof globalThis.setTimeout> | undefined;
    const marker = `proof-job-drained-${randomUUID()}`;
    const invocation = process.platform === 'win32' ? windowsJob(command, args, marker) : { command, args };
    const child = spawn(invocation.command, invocation.args, { ...spawnOptions,
      env: { ...(spawnOptions.env ?? process.env), PM_PROOF_PROCESS_REGISTRY: this.registry },
      detached: process.platform !== 'win32', stdio: ['ignore', 'pipe', 'pipe'] });
    const pid = child.pid;
    if (pid !== undefined) {
      const path = join(this.registry, `${pid}.pid`);
      writeFileSync(path, String(pid));
      this.registrations.set(pid, path);
      this.groups.add(pid);
    }
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
          // Unix groups can outlive the leader. Windows retains ownership until the
          // job supervisor attests that the kernel job is empty; the timeout stays armed.
          if (process.platform !== 'win32') drained = this.drain();
          void drained.catch(failure => { error = failure; child.stdout!.destroy(); child.stderr!.destroy(); });
        });
        child.once('close', status => {
          if (process.platform === 'win32') {
            if (stderr.includes(marker)) {
              stderr = stderr.replace(marker, '');
              // Nested jobs inherit the outer job: its empty-job receipt covers them too.
              this.collectNested();
              for (const member of this.groups) this.forget(member);
            }
            drained = this.drain();
            void drained.catch(failure => { error = failure; });
          }
          resolve({ status, stdout, stderr, error });
        });
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
      rmSync(this.registry, { recursive: true, force: true });
    } finally {
      for (const [signal, handler] of this.handlers) process.off(signal, handler);
    }
  }
}
