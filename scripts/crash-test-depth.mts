/** Scale only harness watchdogs, validating before any child can launch. */
const scale = Number(process.env.PM_TEST_TIME_SCALE || '1');
if (!Number.isFinite(scale) || scale <= 0) throw new Error('PM_TEST_TIME_SCALE must be finite and positive');

/** Return integer milliseconds suitable for Node timers and child-process watchdogs. */
export function testTime(milliseconds: number): number {
  const scaled = Math.ceil(milliseconds * scale);
  if (!Number.isSafeInteger(scaled) || scaled < 1 || scaled > 2_147_483_647) throw new Error('Invalid scaled harness timeout');
  return scaled;
}
