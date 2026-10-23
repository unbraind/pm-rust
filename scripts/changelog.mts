/** Run the pinned changelog recipe without requiring just on npm workflow runners. */
import { spawnSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('../', import.meta.url));
const justfile = readFileSync(new URL('../justfile', import.meta.url), 'utf8');

/** Read a required literal from the justfile shared with release automation. */
function constant(name: string): string {
  const value = justfile.match(new RegExp(`^${name} := "([^"\\r\\n]+)"$`, 'm'))?.[1];
  if (!value) throw new Error(`Missing changelog constant: ${name}`);
  return value;
}

/** Read the package version without accepting a dependency version as the release. */
function crateVersion(): string {
  const value = readFileSync(new URL('../Cargo.toml', import.meta.url), 'utf8')
    .split(/^\[package\]\s*$/m)[1]?.split(/^\[/m)[0]?.match(/^version\s*=\s*"([^"]+)"/m)?.[1];
  if (!value) throw new Error('Missing package version in Cargo.toml');
  return value;
}

const [mode, ...extra] = process.argv.slice(2);
const flags = mode === 'full' ? [
  '--mode', 'replace', '--output', 'CHANGELOG.md', '--all-release-tags',
  '--release-version', crateVersion(),
  '--date', constant('CHANGELOG_DATE'), ...extra,
] : process.argv.slice(2);
if (flags.some(flag => ['--all-release-tags', '--since-previous-tag', '--until-release-tag'].includes(flag))
    && spawnSync('git', ['rev-parse', '--show-toplevel'], { cwd: root, stdio: 'ignore' }).status !== 0) {
  console.error('Changelog generation from release tags needs a git repository and an available Git executable; .agents/pm cannot resolve one. Tracker reads and writes still work without git.');
  process.exit(1);
}
const windows = process.platform === 'win32';
const result = spawnSync(windows ? 'npx.cmd' : 'npx', [
  '--yes', `--package=${constant('PM_CHANGELOG_PKG')}`, `--package=${constant('PM_CLI_PKG')}`,
  '--', 'pm-changelog', '--pm-root', '.agents/pm',
  '--pm-arg=--output-budget', '--pm-arg=unbounded',
  '--pm-arg=--output-limit', '--pm-arg=unbounded',
  '--item-url-base', constant('ITEM_URL_BASE'), '--respect-item-release',
  '--conventional', '--include-links', '--item-ref-style', 'toon', '--include-metadata',
  ...flags,
], { cwd: root, stdio: 'inherit', shell: windows });
if (result.error) throw result.error;
process.exitCode = result.status ?? 1;
