import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';
import { auditIdentities } from './ci-identity.mts';
import { targetShards } from './ci-targets.mts';

test('target partition retains every Cargo integration target once, including new targets', () => {
  const names = ['claim_release_differential', 'orientation_differential', 'pagination_differential',
    'search_collections_differential', 'close_differential', 'future_contract', 'read_contract'];
  const shards = targetShards(names);
  assert.deepEqual(shards.flat().sort(), [...names].sort());
  assert.deepEqual(shards, targetShards([...names].reverse()));
  assert.equal(new Set(shards.flat()).size, names.length);
  assert.ok(shards.every(shard => shard.length > 0));
});

test('docs identity gate inspects unreachable authors and committers', () => {
  const root = mkdtempSync(join(tmpdir(), 'ci-identity-'));
  const env = { ...process.env, GIT_CONFIG_NOSYSTEM: '1', GIT_CONFIG_GLOBAL: process.platform === 'win32' ? 'NUL' : '/dev/null' };
  const git = (args: string[], input?: string) => execFileSync('git', args, { cwd: root, env, input, encoding: 'utf8' }).trim();
  try {
    git(['init', '--quiet']);
    mkdirSync(join(root, '.github'));
    writeFileSync(join(root, '.github/approved-git-identities.txt'), 'fixture@example.invalid\n');
    const tree = git(['hash-object', '-t', 'tree', '-w', '--stdin'], '');
    const allowed = 'Fixture <fixture@example.invalid> 1 +0000';
    const foreign = 'Fixture <foreign@example.invalid> 1 +0000';
    const commit = (author: string, committer: string) => git(['hash-object', '-t', 'commit', '-w', '--stdin'],
      `tree ${tree}\nauthor ${author}\ncommitter ${committer}\n\nSynthetic identity fixture\n`);
    commit(allowed, allowed);
    assert.equal(auditIdentities(root), 1);
    commit(allowed, foreign);
    assert.throws(() => auditIdentities(root), /Unapproved committer/);
    commit(foreign, allowed);
    assert.throws(() => auditIdentities(root), /Unapproved (author|committer)/);
  } finally { rmSync(root, { recursive: true, force: true }); }
});
