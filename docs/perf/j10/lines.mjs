// J10 (copy of docs/perf/j7/lines.mjs, reads a gzipped OUT and maps any jNsrc root):  self Ir per source line of the functions matching FN (default find_segment_with_free_impl) from a callgrind out file
// written with `--dump-line=yes --compress-strings=no --compress-pos=no`. Inlined callees appear under their own file (`fi=`/`fe=`),
// so each (file, line) is attributed to the real source line. Prints the top N lines with their source text.
// usage: node lines.mjs OUT SRC_ROOT [FN] [N]
import { readFileSync } from 'node:fs';
import { gunzipSync } from 'node:zlib';
const [out, root, fnPat = 'find_segment_with_free_impl', n = '30'] = process.argv.slice(2);
const raw = readFileSync(out);
const lines = (out.endsWith('.gz') ? gunzipSync(raw) : raw).toString('utf8').split(/\r?\n/);
let file = '', fn = '', inFn = false, inCall = false;
const cost = new Map();
for (const l of lines) {
  if (l.startsWith('fl=') || l.startsWith('fi=') || l.startsWith('fe=')) { file = l.slice(3); continue; }
  if (l.startsWith('fn=')) { fn = l.slice(3); inFn = fn.includes(fnPat); inCall = false; continue; }
  if (l.startsWith('calls=')) { inCall = true; continue; }
  if (/^\d+ \d+/.test(l)) {
    if (inCall) { inCall = false; continue; } // the line after calls= is the inclusive cost of the call, not self cost
    if (!inFn) continue;
    const [ln, c] = l.split(' ').map(Number);
    const k = `${file}:${ln}`;
    cost.set(k, (cost.get(k) ?? 0) + c);
  }
}
const total = [...cost.values()].reduce((a, b) => a + b, 0);
const top = [...cost.entries()].sort((a, b) => b[1] - a[1]).slice(0, Number(n));
console.log(`self Ir of functions matching "${fnPat}": ${total}`);
const cache = new Map();
const src = (f, ln) => {
  const p = f.startsWith('/home/') ? f : f;
  if (!cache.has(p)) { try { cache.set(p, readFileSync(p.replace(/^.*\/j7src\//, `${root}/`), 'utf8').split(/\r?\n/)); } catch { cache.set(p, null); } }
  return cache.get(p)?.[ln - 1]?.trim().slice(0, 100) ?? '';
};
for (const [k, c] of top) {
  const i = k.lastIndexOf(':');
  const f = k.slice(0, i), ln = Number(k.slice(i + 1));
  console.log(`${String(c).padStart(11)} ${(100 * c / total).toFixed(1).padStart(5)}%  ${f.replace(/^.*\/(src|crates|examples)\//, '$1/')}:${ln}  ${src(f, ln)}`);
}
