import { spawnSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

/** Audit every reachable and unreachable commit object without compiling the Rust suite. */
export function auditIdentities(directory = '.'): number {
  const allowed = new Set(readFileSync(resolve(directory, '.github/approved-git-identities.txt'), 'utf8')
    .split(/\r?\n/).map(line => line.trim()).filter(line => line && !line.startsWith('#')));
  const objects = spawnSync('git', ['cat-file', '--batch-all-objects', '--batch-check=%(objectname) %(objecttype)'],
    { cwd: directory, encoding: 'utf8', maxBuffer: 128 * 1024 * 1024 });
  if (objects.status !== 0) throw new Error('Cannot enumerate commit identities');
  const commits = objects.stdout.trim().split('\n').filter(line => line.endsWith(' commit')).map(line => line.split(' ')[0]);
  if (commits.length === 0 || allowed.size === 0) throw new Error('Identity audit requires commits and a nonempty allowlist');
  const bodies = spawnSync('git', ['cat-file', '--batch'], { cwd: directory,
    input: commits.join('\n') + '\n', maxBuffer: 128 * 1024 * 1024 });
  if (bodies.status !== 0) throw new Error('Cannot read commit identities');
  let offset = 0;
  for (const oid of commits) {
    const end = bodies.stdout.indexOf(10, offset);
    const [actual, type, size] = bodies.stdout.subarray(offset, end).toString().split(' ');
    if (actual !== oid || type !== 'commit' || !/^\d+$/.test(size)) throw new Error('Malformed commit audit response');
    offset = end + 1;
    const body = bodies.stdout.subarray(offset, offset + Number(size)).toString();
    const headers = body.split('\n\n', 1)[0].split('\n');
    for (const role of ['author', 'committer']) {
      const header = headers.find(line => line.startsWith(`${role} `));
      const email = header?.match(/<([^<>]+)> \d+ [+-]\d{4}$/)?.[1];
      if (!email || !allowed.has(email)) throw new Error(`Unapproved ${role} in commit ${oid}`);
    }
    offset += Number(size) + 1;
  }
  return commits.length;
}
if (process.argv[1] && pathToFileURL(resolve(process.argv[1])).href === import.meta.url) {
  console.log(`Audited ${auditIdentities()} commit objects`);
}
