import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { decodePmTableRows, encodePmTableRows, type PmEncodedTableRows } from '../tests/orientation-oracle/node_modules/@unbrained/pm-cli/dist/sdk/output.js';
import { decode } from '../tests/orientation-oracle/node_modules/@unbrained/pm-cli/node_modules/@toon-format/toon/dist/index.mjs';

// Consume only the synthetic fixture passed by the Rust integration test.
const input = JSON.parse(readFileSync(0, 'utf8')) as { wire: string; source: Parameters<typeof encodePmTableRows>[0] };
const decoded = decode(input.wire, { strict: true }) as unknown as { rows: PmEncodedTableRows['rows']; rows_encoding: PmEncodedTableRows['encoding'] };
assert.ok(decoded.rows_encoding.list_columns, 'must exercise compact list columns');
assert.ok((decoded.rows_encoding.null_columns?.length ?? 0) >= 2, 'must exercise multiple null columns');
assert.ok(decoded.rows_encoding.absent.length > 0, 'must exercise absent cells');
const restored = decodePmTableRows({ rows: decoded.rows, encoding: decoded.rows_encoding });
assert.deepEqual(restored, input.source);
assert.deepEqual(restored.map(Object.keys), decodePmTableRows(encodePmTableRows(input.source, { compact: true })).map(Object.keys));
