// J10: self Ir of the functions matching FN grouped by source file (inlined callees under their own file) from a gzipped callgrind out.
// usage: node files.mjs OUT.gz FN [N]
import { readFileSync } from 'node:fs';
import { gunzipSync } from 'node:zlib';
const [out, fnPat, n = '12'] = process.argv.slice(2);
const lines = gunzipSync(readFileSync(out)).toString('utf8').split(/\r?\n/);
let file = '', inFn = false, inCall = false;
const by = new Map();
for (const l of lines) {
  if (l.startsWith('fl=') || l.startsWith('fi=') || l.startsWith('fe=')) { file = l.slice(3); continue; }
  if (l.startsWith('fn=')) { inFn = l.slice(3).includes(fnPat); inCall = false; continue; }
  if (l.startsWith('calls=')) { inCall = true; continue; }
  if (/^\d+ \d+/.test(l)) {
    if (inCall) { inCall = false; continue; }
    if (!inFn) continue;
    by.set(file, (by.get(file) ?? 0) + Number(l.split(' ')[1]));
  }
}
const total = [...by.values()].reduce((a, b) => a + b, 0);
console.log(`self Ir of "${fnPat}": ${total}`);
for (const [f, c] of [...by.entries()].sort((a, b) => b[1] - a[1]).slice(0, Number(n))) console.log(`${String(c).padStart(10)} ${(100 * c / total).toFixed(1).padStart(5)}%  ${f.replace(/^.*\/(src|crates|examples)\//, '$1/')}`);
