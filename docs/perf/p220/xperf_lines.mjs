// #220: per-source-line attribution of judge samples. Usage: node p220-lines.mjs <dump.txt> <judge.exe> <outdir>
// Streams the xperf dumper text, histograms judge-module IPs per PID, symbolizes them with llvm-symbolizer --inlines.
import { createReadStream, writeFileSync } from 'node:fs';
import { createInterface } from 'node:readline';
import { spawnSync } from 'node:child_process';
const [dump, exe, out] = process.argv.slice(2);
const load = new Map(), first = new Map(), hist = new Map(), mods = new Map();
const rl = createInterface({ input: createReadStream(dump), crlfDelay: Infinity });
for await (const line of rl) {
  const t = line.trimStart();
  if (/^(I-)?DC(Start|End)|^ImageLoad|^I-Start/.test(t) && /g1_wild_judge\.exe/i.test(t) && /\\g1_wild_judge\.exe/i.test(t)) {
    const c = t.split(/,\s*/);
    const pid = (c[2].match(/\((\d+)\)/) || [])[1];
    if (pid && !load.has(pid)) load.set(pid, { base: BigInt(c[3]), defbase: BigInt(c[7]) });
    continue;
  }
  if (!t.startsWith('SampledProfile,')) continue;
  const c = t.split(/,\s*/);
  const pm = c[2].match(/^g1_wild_judge\.exe \((\d+)\)$/);
  if (!pm) continue;
  const pid = pm[1];
  if (!first.has(pid)) first.set(pid, Number(c[1]));
  const img = c[7].split('!')[0];
  const mk = `${pid}|${img}`;
  mods.set(mk, (mods.get(mk) ?? 0) + 1);
  if (img.toLowerCase() !== 'g1_wild_judge.exe') continue;
  const h = hist.get(pid) ?? new Map();
  h.set(c[4], (h.get(c[4]) ?? 0) + 1);
  hist.set(pid, h);
}
const pids = [...first.keys()].sort((a, b) => first.get(a) - first.get(b));
const report = [];
for (const pid of pids) {
  const L = load.get(pid);
  if (!L) throw new Error(`no image load for ${pid}`);
  const h = hist.get(pid) ?? new Map();
  const addrs = [...h.keys()];
  const va = addrs.map((a) => '0x' + (BigInt(a) - L.base).toString(16));
  const r = spawnSync('llvm-symbolizer', [`--obj=${exe}`, '--inlines', '--relative-address', '--output-style=JSON', '--relativenames'], { input: va.join('\n'), encoding: 'utf8', maxBuffer: 1 << 30 });
  if (r.status !== 0) throw new Error(r.stderr);
  const sym = r.stdout.trim().split(/\r?\n/).map((l) => JSON.parse(l));
  const leafFn = new Map(), leafLine = new Map(), outer = new Map();
  let total = 0;
  sym.forEach((s, i) => {
    const n = h.get(addrs[i]); total += n;
    const fr = s.Symbol ?? [];
    const leaf = fr[0] ?? { FunctionName: '?', FileName: '?', Line: 0 };
    const add = (m, k) => m.set(k, (m.get(k) ?? 0) + n);
    add(leafFn, leaf.FunctionName);
    add(leafLine, `${leaf.FileName}:${leaf.Line}`);
    // outermost sefer frame (entry into the allocator from the judge)
    const sf = [...fr].reverse().find((f) => /sefer|mimalloc|mi_/.test(f.FunctionName + f.FileName));
    add(outer, sf ? sf.FunctionName : '(judge)');
  });
  const top = (m, k) => [...m].sort((a, b) => b[1] - a[1]).slice(0, k).map(([x, n]) => `${String(n).padStart(8)} ${(100 * n / total).toFixed(1).padStart(5)}%  ${x}`);
  const mk = [...mods].filter(([k]) => k.startsWith(pid + '|')).sort((a, b) => b[1] - a[1]).slice(0, 5).map(([k, n]) => `${k.split('|')[1]} ${n}`);
  report.push(`== pid ${pid} (start ${first.get(pid)}), judge-module samples ${total}; modules: ${mk.join(', ')}`,
    '-- leaf function', ...top(leafFn, 30), '-- leaf line', ...top(leafLine, 40), '-- outermost allocator frame', ...top(outer, 20), '');
}
writeFileSync(`${out}/p220-lines.txt`, report.join('\n'));
console.log(report.join('\n'));
