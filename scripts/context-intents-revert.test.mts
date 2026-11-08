import { checkedGit } from './context-revert-helpers.mts';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
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

/** Real Git failures retain the caller context and the failed path or repository diagnostic. */
test('revert Git failures preserve diagnostics for source scans and patch checks', () => {
  const root = mkdtempSync(join(tmpdir(), 'pm-rust-git-diagnostic-'));
  try {
    assert.throws(() => checkedGit(root, ['ls-files'], 'source scan failed'), /source scan failed:.*not a git repository/s);
    writeFileSync(join(root, 'packet.rs'), 'different packet contents\n');
    const patch = join(root, 'packet.patch');
    writeFileSync(patch, 'diff --git a/packet.rs b/packet.rs\n--- a/packet.rs\n+++ b/packet.rs\n@@ -1 +1 @@\n-original\n+expected\n');
    for (const check of [[], ['--check']]) {
      assert.throws(() => checkedGit(root, ['apply', '--reverse', ...check, patch], 'packet revert failed'), /packet revert failed:.*patch failed: packet.rs:1/s);
    }
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
