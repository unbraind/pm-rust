import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';

/** Both missing oracle installations fail before source copying or compilation. */
test('revert preflight names the missing oracle installation', () => {
  const root = mkdtempSync(join(tmpdir(), 'pm-rust-preflight-'));
  try {
    for (const oracle of ['oracle', 'orientation-oracle']) {
      const result = spawnSync(process.execPath, [fileURLToPath(new URL('./context-intents-revert.mts', import.meta.url))], {
        cwd: root, encoding: 'utf8', timeout: 10_000,
      });
      assert.ifError(result.error);
      assert.equal(result.status, 1);
      assert.ok(result.stderr.includes(`Missing ${oracle} dependencies; run just oracle-install`));
      assert.ok(!result.stderr.includes('Cannot enumerate package source'));
      mkdirSync(join(root, 'tests', oracle, 'node_modules'), { recursive: true });
    }
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
