import { createHash } from 'node:crypto';
import { copyFileSync, mkdirSync, readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { basename, join, resolve } from 'node:path';
import { execFileSync } from 'node:child_process';
import { pathToFileURL } from 'node:url';

interface Receipt { shard: string; identity: string; files: { name: string; sha256: string }[] }
/** Keep each original process profile; never combine percentages or average file summaries. */
export function collectProfiles(rawRoot: string, artifact: string, shard: string, identity: string): void {
  const files = readdirSync(rawRoot).filter(name => name.endsWith('.profraw')).sort();
  if (files.length === 0) throw new Error('Coverage shard produced no raw profiles');
  mkdirSync(join(artifact, 'profiles'), { recursive: true });
  const receipt: Receipt = { shard, identity, files: files.map((file, index) => {
    const name = `${index}.profraw`;
    const bytes = readFileSync(join(rawRoot, file));
    copyFileSync(join(rawRoot, file), join(artifact, 'profiles', name));
    return { name, sha256: createHash('sha256').update(bytes).digest('hex') };
  }) };
  writeFileSync(join(artifact, 'manifest.json'), JSON.stringify(receipt));
}

/** Restore the exact profile multiset, verifying every shard, file digest and build identity. */
export function restoreProfiles(artifacts: readonly string[], rawRoot: string, identity: string): number {
  mkdirSync(rawRoot, { recursive: true });
  if (readdirSync(rawRoot).some(name => name.endsWith('.profraw'))) throw new Error('Merge destination contains non-shard profiles');
  let count = 0;
  for (const [index, artifact] of artifacts.entries()) {
    const receipt: Receipt = JSON.parse(readFileSync(join(artifact, 'manifest.json'), 'utf8'));
    if (receipt.identity !== identity || receipt.shard !== String(index + 1) || receipt.files.length === 0) throw new Error('Coverage shard identity/count mismatch');
    const actual = readdirSync(join(artifact, 'profiles')).sort();
    const declared = receipt.files.map(file => file.name).sort();
    if (actual.length !== declared.length || new Set(declared).size !== declared.length || actual.some((name, i) => name !== declared[i])) throw new Error('Coverage profile inventory mismatch');
    for (const file of receipt.files) {
      if (basename(file.name) !== file.name || !file.name.endsWith('.profraw')) throw new Error('Invalid coverage profile name');
      const source = join(artifact, 'profiles', file.name);
      if (createHash('sha256').update(readFileSync(source)).digest('hex') !== file.sha256) throw new Error('Coverage profile digest mismatch');
      copyFileSync(source, join(rawRoot, `shard-${index + 1}-${file.name}`));
      count++;
    }
  }
  return count;
}

/** Native LLVM merges the counters after this transport restores their exact union. */
function main(): void {
  const identity = execFileSync('git', ['rev-parse', 'HEAD'], { encoding: 'utf8' }).trim() + '\n' +
    execFileSync('rustc', ['+nightly-2026-08-06', '-Vv'], { encoding: 'utf8' }).trim();
  const rawRoot = resolve('target/llvm-cov-target');
  if (process.argv[2] === 'collect' && /^[12]$/.test(process.argv[3] ?? '')) {
    collectProfiles(rawRoot, `target/ci-coverage/${process.argv[3]}`, process.argv[3], identity);
  } else if (process.argv[2] === 'restore') {
    console.log(`Restored ${restoreProfiles([1, 2].map(shard => `target/coverage-download/rust-coverage-${shard}`), rawRoot, identity)} original profiles`);
  } else throw new Error('Use collect 1|2 or restore');
}
if (process.argv[1] && pathToFileURL(resolve(process.argv[1])).href === import.meta.url) main();
