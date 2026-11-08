/** Existing boundary tests supply synchronous outcomes; expose those outcomes as Node streams. */
export const syncSpawnBridge = `
import { EventEmitter } from 'node:events';
import { PassThrough } from 'node:stream';
childProcess.spawn = (command, args, options) => {
  const outcome = childProcess.spawnSync(command, args, options);
  const child = new EventEmitter();
  child.stdout = new PassThrough(); child.stderr = new PassThrough();
  setImmediate(() => {
    if (outcome.stdout) child.stdout.write(outcome.stdout);
    if (outcome.stderr) child.stderr.write(outcome.stderr);
    if (outcome.error) child.emit('error', outcome.error);
    child.emit('exit', outcome.status, null);
    child.stdout.end(); child.stderr.end(); child.emit('close', outcome.status, null);
  });
  return child;
};
`;
