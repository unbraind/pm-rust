import { isDeepStrictEqual } from 'node:util';
import { resolveHistoryHashAlgorithm } from '../tests/oracle/node_modules/@unbrained/pm-cli/dist/core/history/digest.js';
import { resealHistoryRewrite, verifyHistoryRecordHash } from '../tests/oracle/node_modules/@unbrained/pm-cli/dist/core/history/history.js';
import { applyHistoryPatch } from '../tests/oracle/node_modules/@unbrained/pm-cli/dist/core/history/projection.js';
import { cloneEmptyReplayDocument, historyEntriesToRaw, replayHash, verifyHistoryChain } from '../tests/oracle/node_modules/@unbrained/pm-cli/dist/core/history/replay.js';
import type { HistoryEntry } from '../tests/oracle/node_modules/@unbrained/pm-cli/dist/types.js';

export interface RedactionApproval {
  item: string;
  literal: string;
  replacement: string;
  reason: string;
  approved: string;
  author: string;
  marker_hash: string;
}

/** PM substitutes literal text in string values, including nested patch values. Keys are immutable. */
function substitute(value: unknown, rule: RedactionApproval): unknown {
  if (typeof value === 'string') return value.split(rule.literal).join(rule.replacement);
  if (Array.isArray(value)) return value.map(element => substitute(element, rule));
  if (value && typeof value === 'object') {
    return Object.fromEntries(Object.entries(value).map(([key, child]) => [key, substitute(child, rule)]));
  }
  return value;
}

/** Reproduce the approved subset of history-redact from the base alone, including PM's rewrite evidence. */
export function matchesRedaction(before: HistoryEntry[], after: HistoryEntry[], rule: RedactionApproval,
  original: Buffer, current: Buffer): boolean {
  // A JSON decoder can silently discard duplicate keys and replace invalid UTF-8. PM emits compact LF JSONL.
  if (!original.equals(Buffer.from(historyEntriesToRaw(before))) || !current.equals(Buffer.from(historyEntriesToRaw(after)))) return false;
  const marker = after[before.length];
  if (marker?.op !== 'history_redact' || marker.event_class !== 'maintenance'
      || marker.author !== rule.author || marker.message !== rule.reason || marker.record_hash !== rule.marker_hash
      || marker.record_hash_version !== 1 || !verifyHistoryRecordHash(marker).ok
      || !isDeepStrictEqual(marker.patch, []) || marker.before_hash !== marker.after_hash) return false;
  if (!verifyHistoryChain(before).ok) return false;

  let replay = cloneEmptyReplayDocument();
  let used = false;
  for (const [index, entry] of before.entries()) {
    // Reducing earlier retained evidence to a digest would discard more than the approved substitution.
    if (!isDeepStrictEqual(entry.reanchor_evidence, substitute(entry.reanchor_evidence, rule))) return false;
    const rewritten = { ...entry, patch: entry.patch.map(operation => Object.hasOwn(operation, 'value')
      ? { ...operation, value: substitute(operation.value, rule) } : operation) };
    if (typeof entry.message === 'string') rewritten.message = entry.message.split(rule.literal).join(rule.replacement);
    const changed = !isDeepStrictEqual(rewritten, entry);
    used ||= changed;
    const algorithm = resolveHistoryHashAlgorithm(entry.hash_algorithm);
    rewritten.before_hash = replayHash(replay, undefined, algorithm);
    replay = applyHistoryPatch(replay, rewritten.patch, index + 1, entry.op);
    rewritten.after_hash = replayHash(replay, undefined, algorithm);
    const expected = resealHistoryRewrite(entry, rewritten, { retainPriorRecord: changed, retainPriorRecordHash: !changed });
    if (!isDeepStrictEqual(expected, after[index])) return false;
  }
  return used;
}
