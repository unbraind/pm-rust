import { createHash } from 'node:crypto';
import { lstatSync, mkdirSync, readdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';

/** Immutable, relative authoritative file inventory; buffers never enter public receipts. */
export type Snapshot = ReadonlyMap<string, Buffer>;

/** Inventory regular tracker files, refusing symlinks and ignoring only known root caches. */
export function snapshot(root: string, prefix = ''): Map<string, Buffer> {
  if (!prefix && lstatSync(root).isSymbolicLink()) throw new Error('symlink');
  const result = new Map<string, Buffer>();
  for (const entry of readdirSync(join(root, prefix), { withFileTypes: true })) {
    if (!prefix && ['runtime', 'locks', 'transactions', 'checkpoints', 'search'].includes(entry.name)) continue;
    if (entry.isSymbolicLink()) throw new Error('symlink');
    const key = prefix ? `${prefix}/${entry.name}` : entry.name;
    if (entry.isDirectory()) {
      for (const [name, bytes] of snapshot(root, key)) result.set(name, bytes);
    } else if (entry.isFile()) result.set(key, readFileSync(join(root, key)));
    else throw new Error('special_file');
  }
  return result;
}

/** Hash both filenames and exact bytes with length framing and stable byte ordering. */
export function fingerprint(files: Snapshot): string {
  const hash = createHash('sha256');
  for (const key of [...files.keys()].sort()) {
    const bytes = files.get(key)!;
    hash.update(`${Buffer.byteLength(key)}:${key}${bytes.length}:`).update(bytes);
  }
  return hash.digest('hex');
}

/** Materialize a snapshot into a new private tracker; callers own the fresh destination. */
export function materialize(files: Snapshot, destination: string): void {
  if ([...files.keys()].some(name => name.split('/').some(part => !part || part === '.' || part === '..') || name.includes('\\') || /^[A-Za-z]:/.test(name))) throw new Error('unsafe_filename');
  for (const [name, bytes] of files) {
    const target = join(destination, name);
    mkdirSync(dirname(target), { recursive: true });
    writeFileSync(target, bytes, { flag: 'wx', mode: 0o600 });
  }
}

/** Require each retained history prefix to remain exactly byte-for-byte intact. */
export function preservesHistory(before: Snapshot, after: Snapshot): boolean {
  for (const [name, bytes] of before) {
    if (name.startsWith('history/') && name.endsWith('.jsonl') &&
      !after.get(name)?.subarray(0, bytes.length).equals(bytes)) return false;
  }
  return true;
}

/** Compare the complete inventory, including deletions and newly created files. */
export function sameBytes(left: Snapshot, right: Snapshot): boolean {
  return left.size === right.size && [...left].every(([key, bytes]) => right.get(key)?.equals(bytes));
}

/** Accept seed bytes only after a refused restart; successful restarts must match a complete reference. */
export function recoveredState(seed: Snapshot, before: Snapshot, committed: Snapshot, after: Snapshot, status: number | null): 'before' | 'committed' | 'diverged' {
  if (sameBytes(before, after) || status !== 0 && sameBytes(seed, after)) return 'before';
  return sameBytes(committed, after) ? 'committed' : 'diverged';
}
