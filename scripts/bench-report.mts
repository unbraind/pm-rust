import { readFileSync, writeFileSync } from 'node:fs';

type Row = {
  items: number; command: string; runtime: string; p50_ms: number; p95_ms: number;
  peak_rss_kib: number; stdout_bytes: number; stderr_bytes: number; status: number;
  oracle_byte_identical: boolean; oracle_storage_identical: boolean | null;
};
type Receipt = { complete: boolean; samples: number; warmups: number; rows: Row[] };

/** Render all measurements, keeping unsupported exits out of successful speed claims. */
export function report(receipt: Receipt): string {
  if (!receipt.complete) throw new Error('Cannot publish an incomplete benchmark receipt');
  const lines = [
    `Samples per runtime/case: ${receipt.samples}; warmups: ${receipt.warmups}. Wall times are milliseconds; RSS is KiB.`, '',
    '| Items | Command | Before p50 / p95 | Native p50 / p95 | Oracle p50 / p95 | Native / oracle exit |',
    '| ---: | --- | ---: | ---: | ---: | ---: |',
  ];
  /** Retain one decimal place consistently across the complete table. */
  const timing = (row: Row | undefined): string => row ? `${row.p50_ms.toFixed(1)} / ${row.p95_ms.toFixed(1)}` : '—';
  const native = receipt.rows.filter(row => row.runtime === 'native');
  for (const row of native) {
    const before = receipt.rows.find(other => other.items === row.items && other.command === row.command && other.runtime === 'before');
    const oracle = receipt.rows.find(other => other.items === row.items && other.command === row.command && other.runtime === 'oracle');
    if (!oracle) throw new Error('Missing oracle row');
    lines.push(`| ${row.items} | ${row.command} | ${timing(before)} | ${timing(row)} | ${timing(oracle)} | ${row.status} / ${oracle.status} |`);
  }
  lines.push('', '| Items | Command | Before / native / oracle peak RSS | Native stdout + stderr bytes | Oracle stdout + stderr bytes | Exact oracle output / storage |',
    '| ---: | --- | ---: | ---: | ---: | --- |');
  for (const row of native) {
    const before = receipt.rows.find(other => other.items === row.items && other.command === row.command && other.runtime === 'before');
    const oracle = receipt.rows.find(other => other.items === row.items && other.command === row.command && other.runtime === 'oracle');
    if (!oracle) throw new Error('Missing oracle row');
    lines.push(`| ${row.items} | ${row.command} | ${before?.peak_rss_kib ?? '—'} / ${row.peak_rss_kib} / ${oracle.peak_rss_kib} | ${row.stdout_bytes} + ${row.stderr_bytes} | ${oracle.stdout_bytes} + ${oracle.stderr_bytes} | ${row.oracle_byte_identical ? 'yes' : 'no'} / ${row.oracle_storage_identical === null ? 'n/a' : row.oracle_storage_identical ? 'yes' : 'no'} |`);
  }
  return `${lines.join('\n')}\n`;
}

if (process.argv[1]?.endsWith('bench-report.mts')) {
  if (!process.argv[2] || !process.argv[3]) throw new Error('Usage: bench-report.mts receipt.json report.md');
  const receipt: Receipt = JSON.parse(readFileSync(process.argv[2], 'utf8'));
  writeFileSync(process.argv[3], report(receipt));
}
