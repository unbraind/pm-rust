/** Match explicitly marked behavior assertions, excluding inline and helper fixture failures. */
import { existsSync, readFileSync, readdirSync } from 'node:fs';
import { join, relative } from 'node:path';
import { escapeTestName } from './context-revert-helpers.mts';

/** Hide comments and literal contents while retaining offsets and line boundaries. */
function codeMask(source: string, node: boolean): string {
  if (node) return source.replace(/\/\*[\s\S]*?\*\/|\/\/[^\n]*|"(?:\\[\s\S]|[^"\\])*"|'(?:\\[\s\S]|[^'\\])*'|`(?:\\[\s\S]|[^`\\])*`/g, value => value.replace(/[^\n]/g, ' '));
  return source.replace(/\/\*[\s\S]*?\*\/|\/\/[^\n]*|r(#+)?"[\s\S]*?"\1|"(?:\\[\s\S]|[^"\\])*"|'(?:\\.|[^'\\\n])'|`(?:\\[\s\S]|[^`\\])*`/g,
    value => value.replace(/[^\n]/g, ' '));
}

/** Locate a selected test body after its declaration, preserving nested blocks. */
function bodyRange(mask: string, start: number): [number, number] | undefined {
  const open = mask.indexOf('{', start);
  if (open < 0) return undefined;
  let depth = 1;
  for (let index = open + 1; index < mask.length; index++) {
    if (mask[index] === '{') depth++;
    if (mask[index] === '}' && --depth === 0) return [open, index];
  }
  return undefined;
}

/** Enumerate only package source/tests, never dependency trees or the host filesystem. */
function sources(root: string): string[] {
  const files: string[] = [];
  /** Visit the bounded package test corpus. */
  function visit(directory: string, depth: number): void {
    if (!existsSync(directory) || depth > 4) return;
    for (const entry of readdirSync(directory, { withFileTypes: true })) {
      if (entry.name === 'node_modules' || entry.isSymbolicLink()) continue;
      const path = join(directory, entry.name);
      if (entry.isDirectory()) visit(path, depth + 1);
      else if (/\.(rs|test\.mts)$/.test(path)) files.push(path);
    }
  }
  for (const directory of ['src', 'tests', 'scripts']) visit(join(root, directory), 0);
  return files;
}

/** Require a panic or first TAP stack frame at a selected assertion or its named oracle comparator. */
export function assertionFailureDiagnostic(root: string, names: string[], node = false): RegExp {
  const alternatives: string[] = [];
  for (const path of sources(root)) {
    if (node !== path.endsWith('.mts')) continue;
    const source = readFileSync(path, 'utf8');
    if (!node && !names.some(name => source.includes(name.split('::').at(-1)!))) continue;
    const mask = codeMask(source, node);
    for (const name of names) {
      const short = name.split('::').at(-1)!;
      let start = mask.search(new RegExp('\\bfn\\s+' + escapeTestName(short) + '\\s*\\('));
      if (node) {
        start = -1;
        let matched = 0;
        for (const declaration of source.matchAll(/\btest(?:\.skip)?\s*\(\s*(['"`])([\s\S]*?)\1/g)) {
          const prefix = declaration[2].split('${')[0];
          if (prefix.length > Math.max(5, matched) && name.startsWith(prefix)) { start = declaration.index; matched = prefix.length; }
        }
        if (start >= 0) start = mask.indexOf('=>', start);
      }
      if (start < 0) continue;
      const range = bodyRange(mask, start);
      if (!range) continue;
      const ranges = [range];
      // Wrapper tests may delegate their behavior to these declared comparison helpers.
      if (!node) {
        for (let index = 0; index < ranges.length; index++) {
          const body = mask.slice(ranges[index][0], ranges[index][1]);
          for (const helper of ['compare', 'context_intents_fixture', 'strict_frame_verdict', 'compare_controls', 'compare_outputs', 'assert_bytes']) {
            if (!new RegExp('\\b' + helper + '\\s*\\(').test(body)) continue;
            const declaration = mask.search(new RegExp('\\bfn\\s+' + helper + '\\s*\\('));
            const nested = declaration >= 0 ? bodyRange(mask, declaration) : undefined;
            if (nested && !ranges.some(range => range[0] === nested[0])) ranges.push(nested);
          }
        }
      }
      for (const range of ranges) {
        const body = mask.slice(range[0], range[1]);
        const assertions = node ? /\bassert(?:\.\w+)?\s*\(/g : /\bassert(?:_eq|_ne)?\s*!\s*\(/g;
        for (const assertion of body.matchAll(assertions)) {
          const offset = range[0] + assertion.index;
          let end = mask.indexOf('(', offset) + 1;
          let depth = 1;
          for (; end < range[1] && depth > 0; end++) {
            if (mask[end] === '(') depth++;
            if (mask[end] === ')') depth--;
          }
          const open = mask.indexOf('(', offset) + 1;
          const markedArgument = /^\s*\/\* proof-behavior \*\//.test(source.slice(open, end));
          const markedStatement = /(?:^|\n)[ \t]*\/\/ proof-behavior[ \t]*\r?\n[ \t]*$/.test(source.slice(0, offset));
          if (!markedArgument && !markedStatement) continue;
          const prefix = source.slice(0, offset).split('\n');
          const first = prefix.length;
          const lines = source.slice(0, end).split('\n');
          for (let line = first; line <= lines.length; line++) {
            const before = line === first ? prefix.at(-1)! : '';
            const firstColumn = (node ? before.length : [...before].length) + 1;
            const lastColumn = node ? lines[line - 1].length : [...lines[line - 1]].length;
            if (lastColumn < firstColumn) continue;
            const columns = Array.from({ length: lastColumn - firstColumn + 1 }, (_, index) => firstColumn + index).join('|');
            const location = escapeTestName(relative(root, path)).replaceAll('\\\\', '[\\\\/]') + ':' + line + ':(?:' + columns + ')(?=[:)]|$)';
            alternatives.push(node
              ? 'stack: \\|-\\r?\\n[^\\n]*' + location
              : 'panicked at [^\\n]*' + location);
          }
        }
      }
    }
  }
  if (alternatives.length === 0) throw new Error('No marked behavior assertion diagnostic declared for: ' + names.join(', '));
  return new RegExp(alternatives.join('|'));
}
