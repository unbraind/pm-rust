import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { test } from 'node:test';
import { proseReceiptCommits, receiptCommits, verifyEvidenceCommits } from './crash-evidence-commits.mts';

test('fresh clone rejects branch-only and tag-only receipt sources, accepts preserved evidence and main', () => {
  const root = mkdtempSync(join(tmpdir(), 'pm-evidence-gate-'));
  try {
    /** Build real commits with the same generic identity used by the repository. */
    function git(...args: string[]): string {
      const result = spawnSync('git', ['-c', 'user.name=SteveBot', '-c', 'user.email=1153461+unbraind@users.noreply.github.com', ...args], { cwd: root, encoding: 'utf8' });
      assert.equal(result.status, 0, result.stderr);
      return result.stdout.trim();
    }
    git('init', '-b', 'main');
    writeFileSync(join(root, 'source.txt'), 'main'); git('add', 'source.txt'); git('commit', '-m', 'Main source');
    const main = git('rev-parse', 'HEAD');
    git('checkout', '-b', 'feature');
    writeFileSync(join(root, 'source.txt'), 'branch'); git('add', 'source.txt'); git('commit', '-m', 'Branch source');
    const branch = git('rev-parse', 'HEAD');
    git('checkout', 'main'); git('remote', 'add', 'origin', root);
    mkdirSync(join(root, 'docs'));
    const receipt = join(root, 'docs', 'receipt.json');
    writeFileSync(receipt, JSON.stringify({ provenance: { sourceCommit: branch }, publicTrackerCommit: main }));
    mkdirSync(join(root, 'docs', 'evidence'));
    const prose = join(root, 'docs', 'evidence', 'proof.md');
    writeFileSync(prose, `Source: ${main}\n`);
    git('add', 'docs/receipt.json', 'docs/evidence/proof.md');
    assert.throws(() => verifyEvidenceCommits(root), /evidence_commit_unretained: docs\/receipt.json provenance.sourceCommit/);
    git('branch', 'evidence/test', branch);
    writeFileSync(prose, `Source: ${branch}\n`);
    assert.deepEqual(verifyEvidenceCommits(root), { receipts: 2, commits: 3, retainedBy: { [branch]: 'evidence/test', [main]: 'main' } });
    git('tag', 'test-evidence-tag', branch);
    git('branch', '-D', 'feature', 'evidence/test');
    assert.throws(() => verifyEvidenceCommits(root), /evidence_commit_unresolvable: docs\/evidence\/proof.md line:1/);
    writeFileSync(receipt, JSON.stringify({ sourceCommit: main }));
    assert.throws(() => verifyEvidenceCommits(root), /evidence_commit_unresolvable: docs\/evidence\/proof.md line:1/);
    writeFileSync(prose, `Source: ${main}\n`);
    assert.equal(verifyEvidenceCommits(root).commits, 2);
    writeFileSync(receipt, JSON.stringify({ sourceCommit: 'bad' }));
    assert.throws(() => verifyEvidenceCommits(root), /evidence_commit_invalid: docs\/receipt.json sourceCommit/);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('receipt scanner includes nested and array commit fields without confusing hashes', () => {
  const commit = 'a'.repeat(40);
  assert.deepEqual(receiptCommits({ hashes: { file: 'b'.repeat(64) }, rows: [{ sourceCommit: commit }] }, 'docs/nested.json'),
    [{ receipt: 'docs/nested.json', field: 'rows.0.sourceCommit', commit }]);
  assert.deepEqual(proseReceiptCommits(`Hash: ${'b'.repeat(64)}\nSource: ${commit}`, 'docs/evidence/proof.md'),
    [{ receipt: 'docs/evidence/proof.md', field: 'line:2', commit }]);
});

test('repository-only evidence gate names missing git and preserves plain trackers', () => {
  const root = mkdtempSync(join(tmpdir(), 'pm-evidence-nogit-'));
  const previous = process.env.GIT_CEILING_DIRECTORIES;
  process.env.GIT_CEILING_DIRECTORIES = dirname(root);
  try {
    const initialized = spawnSync(process.execPath, [resolve('tests/oracle/node_modules/@unbrained/pm-cli/dist/cli.js'), 'init', '--defaults', '--agent-guidance', 'skip', '--no-extensions'], {
      cwd: root, encoding: 'utf8', env: { ...process.env, PM_SENTRY_DISABLED: '1', PM_TELEMETRY_SOURCE_CONTEXT: 'test' },
    });
    assert.equal(initialized.status, 0, initialized.stderr);
    assert.throws(() => verifyEvidenceCommits(root), /needs a git repository.*plain tracker operations still work/);
    const result = spawnSync(process.execPath, [resolve('scripts/crash-evidence-commits.mts')], {
      cwd: root, encoding: 'utf8', env: { ...process.env, PATH: join(root, 'no-tools') },
    });
    assert.equal(result.status, 1);
    assert.match(result.stderr, /needs a git repository.*plain tracker operations still work/);
  } finally { if (previous === undefined) delete process.env.GIT_CEILING_DIRECTORIES; else process.env.GIT_CEILING_DIRECTORIES = previous; rmSync(root, { recursive: true, force: true }); }

});
