import { readFileSync } from 'node:fs';
import { join } from 'node:path';

/** Read the installed benchmark oracle version and enforce its manifest-owned pin. */
export function benchmarkOracleVersion(repo: string): string {
  const installed: string = JSON.parse(readFileSync(join(repo, 'tests/bench-oracle/node_modules/@unbrained/pm-cli/package.json'), 'utf8')).version;
  const expected: string = JSON.parse(readFileSync(join(repo, 'tests/bench-oracle/package.json'), 'utf8')).dependencies['@unbrained/pm-cli'];
  if (installed !== expected) throw new Error('Benchmark oracle does not match its declared dependency pin');
  return installed;
}
