/** Pin wall-clock metadata in fresh published CLI processes, as in the Rust differentials. */
const instant = Date.parse('2026-10-21T12:00:00.000Z');
const OriginalDate = Date;
const PinnedDate = class extends OriginalDate {
  /** Construct a deterministic clock while preserving explicitly supplied instants. */
  constructor(value?: string | number) { super(value ?? instant); }
  /** Return the corpus clock. */
  static now(): number { return instant; }
};
// Date's callable legacy signature is outside CLI metadata generation.
globalThis.Date = PinnedDate as unknown as DateConstructor;
