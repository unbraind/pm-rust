import { sameBytes } from './crash-storage.mts';
import type { Snapshot } from './crash-storage.mts';

/** Recovery obligation determined by observed publication, independent of injection ordinals. */
export type TransactionPhase = 'before-commit' | 'after-commit' | 'during-cleanup';
/** All canonical tracker bytes plus pending recovery intent; only documented derived/transient data is excluded. */
export function transactionState(files: Snapshot): Snapshot {
  return new Map([...files].filter(([name]) => {
    if (name === 'runtime/context-usage.jsonl' || name === 'runtime/metadata-cache-delta.json') return false;
    // Private atomic-write staging names are not published item/history files.
    return !name.split('/').at(-1)!.startsWith('.') || !name.endsWith('.tmp');
  }));
}
/** One exact state oracle for clean returns, restart operations and final returns. No prefix acceptance. */
export function exactState(expected: Snapshot, actual: Snapshot): boolean {
  return sameBytes(transactionState(expected), transactionState(actual));
}
/** Join strace's per-task resumed lines before inspecting successful commit events. */
function completedCalls(trace: string): string[] {
  const pending = new Map<string, string>();
  const calls: string[] = [];
  for (const line of trace.split('\n')) {
    const task = /^\s*(\d+)\s+(.*)$/.exec(line);
    if (!task) continue;
    if (task[2].endsWith('<unfinished ...>')) pending.set(task[1], task[2].replace('<unfinished ...>', ''));
    else if (/^<\.\.\. \w+ resumed>/.test(task[2])) {
      calls.push((pending.get(task[1]) ?? '') + task[2].replace(/^<\.\.\. \w+ resumed>/, ''));
      pending.delete(task[1]);
    } else calls.push(task[2]);
  }
  return calls;
}
/** Read the commit point: native durable journal publication, published canonical item publication.
 * A path-filtered history-write open occurs after publication; a read open does not.
 * No recovered bytes or syscall ordinals participate in this decision.
 */
export function transactionPhase(trace: string, implementation: 'native' | 'published', id: string): TransactionPhase {
  let phase: TransactionPhase = 'before-commit';
  for (const call of completedCalls(trace)) {
    const paths = [...call.matchAll(/"(?:[^"\\]|\\.)*"/g)].map(match => {
      try { return String(JSON.parse(match[0])).replaceAll('\\', '/'); } catch { return ''; }
    });
    const destination = paths[1] ?? '';
    const journal = destination.endsWith(`/runtime/transactions/update-${id}.json`) || destination.endsWith(`/runtime/transactions/comment-${id}.json`);
    const item = destination.endsWith(`/tasks/${id}.toon`);
    if (/^(?:link|linkat|rename)\(/.test(call) && /= 0$/.test(call) && (implementation === 'native' ? journal : item)) phase = 'after-commit';
    // Injection at the history write-open is itself independent commit evidence.
    if (call.startsWith('openat(') && paths.some(path => path.endsWith(`/history/${id}.jsonl`)) && /O_(?:WRONLY|RDWR)/.test(call)) phase = 'after-commit';
    if (/^write(?:v)?\(/.test(call) && call.includes(`/history/${id}.jsonl>`) && /= [1-9]\d*$/.test(call)) phase = 'during-cleanup';
  }
  return phase;
}
/** Phase-aware pairing covers every observed cut, including unequal per-runtime cut counts. */
export function phasePairs<T extends { phase: TransactionPhase; implementation: 'native' | 'published'; syscall: string; boundary: number }>(cuts: readonly T[]): [T, T][] {
  return cuts.filter(cut => cut.implementation === 'native').flatMap(native =>
    cuts.filter(published => published.implementation === 'published' && published.phase === native.phase).map(published => [native, published] as [T, T]));
}
