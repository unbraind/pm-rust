/** Measure warmed fresh processes in three fixed windows, retaining every sample. */
export function measure(sample: () => number): { windows_ms: number[][]; p50_ms: number; p95_ms: number } {
  for (let index = 0; index < 5; index++) sample();
  const windows_ms = Array.from({ length: 3 }, () => Array.from({ length: 30 }, sample));
  const sorted = windows_ms.map(values => [...values].sort((a, b) => a - b));
  /** Return the middle statistic across the three fixed windows. */
  const median = (values: number[]): number => [...values].sort((a, b) => a - b)[1];
  return {
    windows_ms,
    p50_ms: median(sorted.map(values => (values[14] + values[15]) / 2)),
    p95_ms: median(sorted.map(values => values[Math.ceil(0.95 * values.length) - 1])),
  };
}
