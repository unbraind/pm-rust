import { spawnSync } from 'node:child_process';

export const MUTATIONS = ['delete-item', 'truncate-event', 'drop-field', 'reorder-history'] as const;
export const REAL_CHECKS = ['update', 'comment'].flatMap(scenario => ['post', 'restart', 'return'].flatMap(stage =>
  MUTATIONS.map(mutation => `real exact oracle kills ${scenario} ${stage} ${mutation}`)));
export const LIVE_CHECKS = ['restart', 'return'].flatMap(stage => MUTATIONS.map(mutation => `live exact oracle kills ${stage} ${mutation} despite exit zero`));
export const PHASE_CHECK = 'phase parity pairs equivalent cuts with different ordinals and rejects ordinal aliases';
export const EXPECTED_CHECKS = [...REAL_CHECKS, ...LIVE_CHECKS, PHASE_CHECK];
export const PROOF_PATTERN = 'exact state oracle kills all|live exact oracle kills|phase parity';

/** A named terminal result; skipped/todo tests provide no executed evidence. */
export interface CheckResult { name: string; status: 'passed' | 'failed' | 'not_run' }
/** Preserve runner outcomes separately from the expected-check inventory. */
export interface TestRun { command: string[]; exit: number | null; observed: CheckResult[]; checks: CheckResult[] }

/** Decode only Node terminal events, rejecting malformed or duplicated observations. */
export function parseResults(output: string): CheckResult[] {
  const observed: CheckResult[] = [];
  for (const line of output.trim().split('\n').filter(Boolean)) {
    const event: unknown = JSON.parse(line);
    if (!event || typeof event !== 'object' || !('type' in event) || !('data' in event)) throw new Error('Invalid test reporter event');
    if (event.type !== 'test:pass' && event.type !== 'test:fail') throw new Error('Unexpected test reporter event');
    const data = event.data;
    if (!data || typeof data !== 'object' || !('name' in data) || typeof data.name !== 'string') throw new Error('Missing test name');
    if (observed.some(row => row.name === data.name)) throw new Error(`Duplicate test result: ${data.name}`);
    observed.push({ name: data.name, status: ('skip' in data && data.skip || 'todo' in data && data.todo) ? 'not_run' : event.type === 'test:pass' ? 'passed' : 'failed' });
  }
  return observed;
}

/** Join expectations to observations; absence never becomes a passing result. */
export function reconcileResults(observed: CheckResult[], expected: readonly string[]): CheckResult[] {
  return expected.map(name => ({ name, status: observed.find(row => row.name === name)?.status ?? 'not_run' }));
}

/** Count only observed outcomes, retaining the expected inventory and missing checks. */
export function counts(checks: CheckResult[]): { expected: number; executed: number; passed: number; failed: number; not_run: number } {
  return { expected: checks.length, executed: checks.filter(row => row.status !== 'not_run').length,
    passed: checks.filter(row => row.status === 'passed').length, failed: checks.filter(row => row.status === 'failed').length,
    not_run: checks.filter(row => row.status === 'not_run').length };
}

/** Execute the real Node test runner with a bounded JSON-lines reporter. */
export function runChecks(pattern: string, expected: readonly string[] = EXPECTED_CHECKS, file = 'scripts/crash-oracle.test.mts'): TestRun {
  const args = ['--test', '--test-concurrency=1', '--test-reporter=./scripts/crash-test-reporter.mts', `--test-name-pattern=${pattern}`, file];
  const env = { ...process.env };
  // Nested runners must own their test process rather than inherit the parent's IPC context.
  delete env.NODE_TEST_CONTEXT;
  const result = spawnSync(process.execPath, args, { env, encoding: 'utf8', timeout: 300000, maxBuffer: 8 * 1024 * 1024 });
  if (result.error) throw result.error;
  const observed = parseResults(result.stdout);
  return { command: ['node', ...args], exit: result.status, observed, checks: reconcileResults(observed, expected) };
}
