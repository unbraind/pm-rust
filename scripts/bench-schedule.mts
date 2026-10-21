/** One measured launch, including its zero-based round and executed runtime order. */
export type RecordedSample<Sample> = Sample & { round: number; runtime_order: string[] };

/** Alternate real launches, excluding warmups while recording their measured successors. */
export function measureRuntimes<Runtime extends { name: string }, Sample>(
  runtimes: Runtime[], warmups: number, samples: number,
  execute: (runtime: Runtime, measured: boolean) => Sample,
): Map<string, RecordedSample<Sample>[]> {
  const results = new Map<string, RecordedSample<Sample>[]>();
  for (let round = -warmups; round < samples; round++) {
    const order = round % 2 === 0 ? runtimes : [...runtimes].reverse();
    for (const runtime of order) {
      const sample = execute(runtime, round >= 0);
      if (round < 0) continue;
      const recorded = { ...sample, round, runtime_order: order.map(entry => entry.name) };
      const measured = results.get(runtime.name) ?? [];
      measured.push(recorded);
      results.set(runtime.name, measured);
    }
  }
  return results;
}
