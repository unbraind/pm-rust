// Preload a fixed clock in the existing oracle launcher and its final CLI child.
// External wall time and GNU time resource measurements remain real.
const fixed = Date.parse(process.env.FIXED_CLOCK ?? '2026-10-21T12:00:00.000Z');
globalThis.Date = new Proxy(Date, {
  construct(target, args, newTarget) {
    return Reflect.construct(target, args.length ? args : [fixed], newTarget);
  },
  get(target, key, receiver) {
    return key === 'now' ? () => fixed : Reflect.get(target, key, receiver);
  },
});
