/** Check every managed mutation definition before spending time on compilation. */
import assert from 'node:assert/strict';
import { assertionFailureDiagnostic } from './context-proof-diagnostics.mts';
import { readFileSync } from 'node:fs';
import { stripTypeScriptTypes } from 'node:module';
import { runInNewContext } from 'node:vm';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';

/** Source-only mutations have one exact anchor unless explicitly declared multi-site. */
interface Mutation { path: string; before: string; after: string; multiple?: boolean; test: string; target?: string[]; rust?: string[]; }

for (const runner of ['context-review-revert.mts', 'context-safety-revert.mts']) {
  test(`all managed mutation targets match the live package: ${runner}`, () => {
    const source = readFileSync(new URL(runner, import.meta.url), 'utf8');
    const start = source.indexOf('const cases');
    assert.ok(start >= 0);
    const body = source.slice(source.indexOf('= [', start) + 2);
    const end = body.indexOf('\n  ];');
    assert.ok(end >= 0);
    // Evaluate only this trusted package's data declaration, without running tools.
    const declaration = stripTypeScriptTypes(`const cases = ${body.slice(0, end + 5)}; cases;`);
    const mutations = runInNewContext(declaration, Object.create(null)) as Mutation[];
    assert.ok(mutations.length > 0);
    for (const mutation of mutations) {
      assert.ok(assertionFailureDiagnostic(fileURLToPath(new URL('..', import.meta.url)), [mutation.test], mutation.target?.[0] === 'node' || !mutation.target && !mutation.rust));
      const live = readFileSync(new URL(`../${mutation.path}`, import.meta.url), 'utf8');
      const matches = live.split(mutation.before).length - 1;
      assert.ok(/* proof-behavior */ matches > 0, `${runner}: missing mutation ${mutation.path}: ${mutation.before}`);
      if (!mutation.multiple) assert.equal(/* proof-behavior */ matches, 1, `${runner}: ambiguous mutation ${mutation.path}: ${mutation.before}`);
      assert.notEqual(/* proof-behavior */ mutation.before, mutation.after);
    }
  });
}
