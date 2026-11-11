import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { CRASH_ORACLE_VERSION } from './crash-harness.mts';

/** Both oracle consumers must certify the same exact published version. */
test('crash harness and differential oracle pins never diverge', () => {
  const manifest = JSON.parse(readFileSync(new URL('../tests/oracle/package.json', import.meta.url), 'utf8')) as { dependencies: Record<string, string> };
  assert.equal(CRASH_ORACLE_VERSION, manifest.dependencies['@unbrained/pm-cli']);
});
