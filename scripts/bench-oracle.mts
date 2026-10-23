// Keep output clocks and the context packing watchdog deterministic. External
// wall time and GNU time's resource accounting still measure the real process.
const fixed = Date.parse(process.env.FIXED_CLOCK ?? '2026-10-21T12:00:00.000Z');
globalThis.Date = new Proxy(Date, {
  construct(target, args, newTarget) {
    return Reflect.construct(target, args.length ? args : [fixed], newTarget);
  },
  get(target, key, receiver) {
    return key === 'now' ? () => fixed : Reflect.get(target, key, receiver);
  },
});
Object.defineProperty(performance, 'now', { value: () => 0, configurable: true });
process.argv = [process.argv[0], 'pm', ...process.argv.slice(2)];
// Import only after pinning Date, so the CLI's own clock captures this clock.
// Avoid loading the entire SDK into measured CLI children just to fix time.
await import('../tests/bench-oracle/node_modules/@unbrained/pm-cli/dist/cli.js');
