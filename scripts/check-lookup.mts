import { spawnSync } from 'node:child_process';
import { homedir, tmpdir } from 'node:os';

/** Keep compiler checkout and home paths out of public linked-test receipts. */
function sanitize(text: string): string {
  const roots = [process.cwd(), homedir(), tmpdir(), process.env.CARGO_HOME, process.env.RUSTUP_HOME]
    .filter((value): value is string => Boolean(value))
    .sort((a, b) => b.length - a.length);
  let result = text;
  for (const root of roots) {
    for (const spelling of [root, root.replaceAll('\\', '/'), root.replaceAll('/', '\\')]) {
      result = result.replaceAll(spelling, '<local>');
    }
  }
  return result;
}

const result = spawnSync('cargo', ['test', '--quiet', '--locked', '--lib', process.argv[2] ?? 'workspace::tests::single_item_get'],
  { encoding: 'utf8', env: process.env });
process.stdout.write(sanitize(result.stdout ?? ''));
process.stderr.write(sanitize(result.error?.message ?? result.stderr ?? ''));
process.exitCode = result.status ?? 1;
