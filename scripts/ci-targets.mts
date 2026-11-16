import { spawnSync } from 'node:child_process';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

/** Partition every integration target exactly once; balance the measured slow differential targets. */
export function targetShards(names: readonly string[]): string[][] {
  const shards: string[][] = [[], []];
  const owners: Record<string, number> = { claim_release_differential: 0, pagination_differential: 0,
    orientation_differential: 1, search_collections_differential: 1, close_differential: 1 };
  [...names].sort().forEach((name, index) => shards[Object.hasOwn(owners, name) ? owners[name] : index % 2].push(name));
  return shards;
}

/** Invoke Cargo with disjoint target scopes, preserving examples and both unit-test binaries. */
function main(): void {
  const shard = Number(process.argv[2]);
  if (![1, 2].includes(shard)) throw new Error('Pass shard 1 or 2');
  const metadata = spawnSync('cargo', ['metadata', '--locked', '--no-deps', '--format-version=1'], { encoding: 'utf8' });
  if (metadata.status !== 0) throw new Error(metadata.stderr || 'Cargo metadata failed');
  const workspace: { packages: { name: string; targets: { name: string; kind: string[] }[] }[] } = JSON.parse(metadata.stdout);
  const targetsInCrate = workspace.packages.find(pkg => pkg.name === 'pm-rust')?.targets;
  if (!targetsInCrate) throw new Error('Cargo metadata omitted pm-rust');
  const names = targetsInCrate.filter(target => target.kind.includes('test')).map(target => target.name);
  const targets = targetShards(names)[shard - 1];
  const coverage = process.argv[3] === '--coverage';
  const args = [...(coverage ? ['+nightly-2026-08-06', 'llvm-cov', '--no-report', '--branch'] : ['test']), '--locked', '--all-features', ...(shard === 1 ? ['--lib', '--bins', '--examples', '--benches'] : []),
    ...targets.flatMap(name => ['--test', name])];
  console.log(`cargo ${args.join(' ')}`);
  const result = spawnSync('cargo', args, { stdio: 'inherit', env: process.env });
  if (result.error) throw result.error;
  process.exitCode = result.status ?? 1;
}
if (process.argv[1] && pathToFileURL(resolve(process.argv[1])).href === import.meta.url) main();
