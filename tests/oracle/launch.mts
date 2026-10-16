import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

// Apply the test boundary in the final child, even after env_clear or hostile
// inherited overrides. Drivers and the published CLI share this one launcher.
const args = process.argv.slice(2);
const tool = args[0] === '--command';
const entry = tool || args[0] === '--entry'
  ? args.splice(0, 2)[1]
  : fileURLToPath(new URL('./node_modules/@unbrained/pm-cli/dist/cli.js', import.meta.url));
if (!entry) throw new Error('--entry requires an oracle entry script');
const result = spawnSync(tool ? entry : process.execPath, tool ? args : [entry, ...args], {
  env: {
    ...process.env,
    PM_SENTRY_DISABLED: '1',
    PM_TELEMETRY_SOURCE_CONTEXT: 'test',
  },
  stdio: 'inherit',
});
if (result.error) throw result.error;
process.exit(result.status ?? 1);
