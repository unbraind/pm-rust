// Capture only path-shape diagnostics; never write fixture paths to receipts.
import crypto from 'node:crypto';
import {syncBuiltinESMExports} from 'node:module';
import {writeFileSync, realpathSync} from 'node:fs';
import {pathToFileURL} from 'node:url';

const originalHash = crypto.createHash;
const ordinary = root => root.replace(/^\\\\\?\\UNC\\/, '\\\\').replace(/^\\\\\?\\/, '');
crypto.createHash = function (...args) {
    const hash = originalHash(...args);
    const update = hash.update;
    hash.update = function (value, ...rest) {
        if (typeof value === 'string' && value.startsWith('{')) {
            const contract = JSON.parse(value);
            if (typeof contract.pmRoot === 'string') {
                const native = ordinary(process.env.PM_RUST_QUERY_ROOT);
                const node = contract.pmRoot;
                const shape = root => ({length: root.length, forward: root.split('/').length - 1, backward: root.split('\\').length - 1, shortName: root.includes('~')});
                const index = [...native].findIndex((char, i) => char !== node[i]);
                writeFileSync('query-shape.json', JSON.stringify({
                    native: shape(native), node: shape(node),
                    equal: native === node, equalIgnoringCase: native.toLowerCase() === node.toLowerCase(),
                    canonicalEqual: native === ordinary(realpathSync(node)),
                    firstDifference: index, nativeCode: native.charCodeAt(index), nodeCode: node.charCodeAt(index),
                }));
            }
        }
        return update.call(this, value, ...rest);
    };
    return hash;
};
syncBuiltinESMExports();
const fixed = Date.parse(process.env.PM_RUST_FIXED_CLOCK);
const OriginalDate = Date;
globalThis.Date = class extends OriginalDate {
    constructor(...args) { args.length ? super(...args) : super(fixed); }
    static now() { return fixed; }
};
process.argv = [process.argv[0], 'pm', ...process.argv.slice(2)];
await import(pathToFileURL(process.env.PM_RUST_PUBLISHED_ENTRY));
