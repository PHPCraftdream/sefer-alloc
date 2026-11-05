// J7 step 3: call counts of the functions matching each pattern from a callgrind out file written with
// `--dump-line=yes --compress-strings=no --compress-pos=no` (sum of `calls=` over every call site of the callee).
// usage: node calls.mjs OUT PATTERN...
import { readFileSync } from 'node:fs';
const [out, ...pats] = process.argv.slice(2);
const lines = readFileSync(out, 'utf8').split(/\r?\n/);
const calls = Object.fromEntries(pats.map((p) => [p, 0]));
let callee = '';
for (const l of lines) {
  if (l.startsWith('cfn=')) { callee = l.slice(4); continue; }
  if (l.startsWith('calls=')) {
    const n = Number(l.slice(6).split(' ')[0]);
    for (const p of pats) if (callee.includes(p)) calls[p] += n;
  }
}
for (const p of pats) console.log(`${String(calls[p]).padStart(10)}  ${p}`);
