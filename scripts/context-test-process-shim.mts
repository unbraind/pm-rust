/** Existing boundary tests supply synchronous outcomes; expose those outcomes as Node streams. */
export const syncSpawnBridge = `
import { EventEmitter } from 'node:events';
import { PassThrough } from 'node:stream';
childProcess.spawn = (command, args, options) => {
  let marker;
  if (command === 'powershell.exe' && args.includes('-EncodedCommand')) {
    const script = Buffer.from(args.at(-1), 'base64').toString('utf16le');
    const encoded = script.match(/FromBase64String\\('([^']+)'\\)/)?.[1];
    if (!encoded) throw new Error('Missing Windows job invocation');
    const original = JSON.parse(Buffer.from(encoded, 'base64').toString('utf8'));
    command = original.command; args = original.args; marker = original.marker;
  }
  const outcome = childProcess.spawnSync(command, args, options);
  const child = new EventEmitter();
  child.stdout = new PassThrough(); child.stderr = new PassThrough();
  setImmediate(() => {
    if (outcome.stdout) child.stdout.write(outcome.stdout);
    if (outcome.stderr) child.stderr.write(outcome.stderr);
    if (marker) child.stderr.write(marker);
    if (outcome.error) child.emit('error', outcome.error);
    child.emit('exit', outcome.status, null);
    child.stdout.end(); child.stderr.end(); child.emit('close', outcome.status, null);
  });
  return child;
};
`;
