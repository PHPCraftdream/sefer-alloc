// G1 rhythm report (docs/perf/g1_scan/{SC,PC,RP}_PREREG.md): validates the
// probe's JSON lines against the pre-registered grids, then derives the
// tables, the summary CSV and the verdicts.
//   node scripts/g1_scan_report.mjs <raw dir> [--csv FILE]
// A family (SC / PC / RP) that has any row must have its whole grid; every
// printed ratio is recomputed from the rows.
import assert from 'node:assert/strict';
import { readdirSync, readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

const argv = process.argv.slice(2);
const dir = argv[0];
const csvAt = argv.includes('--csv') ? argv[argv.indexOf('--csv') + 1] : null;
assert.ok(dir, 'usage: g1_scan_report.mjs <raw dir> [--csv FILE]');

const rows = readdirSync(dir)
  .filter((f) => f.endsWith('.jsonl'))
  .flatMap((f) => readFileSync(join(dir, f), 'utf8').split('\n').filter(Boolean).map((l) => JSON.parse(l)));

// ---- schema -----------------------------------------------------------------
const COUNTER_FIELDS = ['find_segment', 'scans', 'scan_words', 'foreign_publish_ok', 'directory_lookups', 'discovery_scans', 'deferred_scans', 'routed_miss_scans', 'full_scan_slots', 'pin_hits', 'pin_misses', 'pin_inserts', 'pin_evictions'];
const FIELDS = {
  growth: ['size', 'n', 'segments', 'phase_ns'],
  trickle: ['size', 'n', 'k', 'gifts_carved', 'gifts', 'reissued', 'reclaim_alloc_refills_p50', 'reclaim_alloc_refills_max', 'segments', 'phase_ns'],
  bigheap: ['n_fill', 'm', 'fill_segments', 'segments', 'phase_ns'],
  pincycle: ['m', 'n', 'segments', 'free_ns'],
  reclaim: ['n_fill', 'n_free', 'calls', 'segments', 'call_ns_max', 'call_ns_p999', 'call_ns_p99', 'call_ns_p50', 'calls_ns_total'],
};
const isCount = (x) => Number.isSafeInteger(x) && x >= 0;
for (const r of rows) {
  assert.ok(FIELDS[r.mode], `unknown mode ${r.mode}`);
  assert.ok(r.alloc === 'sefer' || r.alloc === 'mimalloc', `${r.mode}: alloc`);
  assert.equal(typeof r.counters, 'boolean', `${r.mode}: counters flag`);
  const need = [...FIELDS[r.mode], ...(r.counters ? COUNTER_FIELDS : [])];
  if (r.mode === 'reclaim' && r.counters) need.push(...COUNTER_FIELDS.map((c) => `backlog_${c}`));
  for (const f of need) assert.ok(isCount(r[f]), `${r.mode}: field ${f} = ${r[f]} is not a non-negative safe integer`);
}

// ---- deterministic points: exactly two agreeing runs --------------------------
const keyOf = (r) => JSON.stringify([r.mode, r.alloc, r.size, r.n, r.k, r.control, r.n_fill, r.m, r.order, r.n_free]);
const groups = new Map();
for (const r of rows.filter((x) => x.counters)) {
  const k = keyOf(r);
  if (!groups.has(k)) groups.set(k, []);
  groups.get(k).push(r);
}
const point = new Map();
for (const [k, runs] of groups) {
  const want = runs[0].mode === 'reclaim' ? 1 : 2;
  assert.equal(runs.length, want, `point ${k}: ${runs.length} runs, prereg requires ${want}`);
  for (const r of runs.slice(1)) for (const c of [...COUNTER_FIELDS, 'segments']) assert.equal(r[c], runs[0][c], `point ${k}: runs disagree on ${c}`);
  point.set(k, runs[0]);
}
const get = (o) => {
  const r = point.get(keyOf(o));
  assert.ok(r, `missing pre-registered point ${keyOf(o)}`);
  return r;
};
const ratio = (a, b) => {
  assert.ok(isCount(a) && isCount(b) && b > 0, `ratio ${a}/${b}`);
  return a / b;
};
const fmt = (x) => x.toFixed(2);
const has = (mode) => rows.some((r) => r.mode === mode);
const out = [];
const csv = ['family,mode,size,n,k,control,m,order,segments,find_segment,routed_miss_scans,full_scan_slots,scan_words,discovery_scans,deferred_scans,foreign_publish_ok,pin_hits,pin_misses'];
const emit = (fam, r) => csv.push([fam, r.mode, r.size ?? '', r.n ?? r.n_fill, r.k ?? '', r.control ?? '', r.m ?? '', r.order ?? '', r.segments, r.find_segment, r.routed_miss_scans, r.full_scan_slots, r.scan_words, r.discovery_scans, r.deferred_scans, r.foreign_publish_ok, r.pin_hits, r.pin_misses].join(','));

// ---- SC ---------------------------------------------------------------------
const S_GRID = [8, 16, 32, 64];
const PER_SEG = { 64: 65536, 1024: 4096 };
if (has('growth') || has('trickle') || has('bigheap')) {
  const verdicts = { slots: [], words: [] };
  for (const size of [64, 1024]) {
    const g = S_GRID.map((s) => get({ mode: 'growth', alloc: 'sefer', size, n: s * PER_SEG[size] }));
    out.push(`\n### growth ${size} B\n\n| S заказано | S факт | find_segment | routed_miss_scans | full_scan_slots | ×slots | scan_words | ×words | slots/find |\n|---|---|---|---|---|---|---|---|---|`);
    g.forEach((r, i) => {
      assert.ok(r.segments >= S_GRID[i], `growth ${size} S=${S_GRID[i]}: only ${r.segments} segments`);
      assert.ok(r.find_segment >= 0.9 * (r.n / 16), `growth ${size} S=${S_GRID[i]}: routed refill path not active`);
      assert.equal(r.foreign_publish_ok, 0, `growth ${size}: foreign publications in pure growth`);
      const p = g[i - 1];
      if (p) assert.equal(r.n, 2 * p.n, 'growth grid must double n');
      const rs = p ? ratio(r.full_scan_slots, p.full_scan_slots) : null;
      const rw = p ? ratio(r.scan_words, p.scan_words) : null;
      if (p && S_GRID[i - 1] >= 16) {
        verdicts.slots.push(rs);
        verdicts.words.push(rw);
      }
      out.push(`| ${S_GRID[i]} | ${r.segments} | ${r.find_segment} | ${r.routed_miss_scans} | ${r.full_scan_slots} | ${rs === null ? '—' : fmt(rs)} | ${r.scan_words} | ${rw === null ? '—' : fmt(rw)} | ${fmt(ratio(r.full_scan_slots, r.find_segment))} |`);
      emit('SC', r);
    });
  }
  assert.equal(verdicts.slots.length, 4, 'SC-2 needs four doublings (16→32, 32→64 for both sizes)');
  const rule = (xs) => (xs.every((x) => x >= 3.5) ? 'квадрат подтверждён' : xs.some((x) => x <= 2.5) ? 'квадрата нет' : 'не решено');
  out.push(`\nSC-2: full_scan_slots — ${rule(verdicts.slots)} (${verdicts.slots.map(fmt).join(', ')}); scan_words — ${rule(verdicts.words)} (${verdicts.words.map(fmt).join(', ')})`);

  out.push('\n### trickle 64 B (k — чужой free каждые k×16 выделений; контроль — те же подарки без передачи)\n\n| S | k | deferred/find T | deferred/find контроль | окно U31 (T) | words T/контроль | gifts | reissued | задержка p50 / max (пополнений-эквивалентов) |\n|---|---|---|---|---|---|---|---|---|');
  for (const s of [8, 32]) {
    for (const k of [1, 2, 3, 8]) {
      const n = s * PER_SEG[64];
      const t = get({ mode: 'trickle', alloc: 'sefer', size: 64, n, k, control: false });
      const c = get({ mode: 'trickle', alloc: 'sefer', size: 64, n, k, control: true });
      assert.equal(t.foreign_publish_ok, t.gifts, `trickle S=${s} k=${k}: publications ≠ gifts`);
      assert.equal(c.foreign_publish_ok, 0, `trickle control S=${s} k=${k}: publications`);
      assert.equal(c.gifts_carved, t.gifts_carved, `trickle S=${s} k=${k}: control carved a different prefix`);
      for (const x of [t, c]) {
        assert.ok(x.segments >= s, `trickle S=${s} k=${k}: only ${x.segments} segments`);
        assert.ok(x.find_segment >= 0.45 * (x.n / 16), `trickle S=${s} k=${k}: discovery path not active`);
      }
      const st = ratio(t.deferred_scans, t.find_segment);
      const sc = ratio(c.deferred_scans, c.find_segment);
      out.push(`| ${s} | ${k} | ${fmt(st)} | ${fmt(sc)} | ${st >= 0.5 ? 'открывается' : 'не открывается'} | ${fmt(ratio(t.scan_words, c.scan_words))} | ${t.gifts} | ${t.reissued} | ${t.reclaim_alloc_refills_p50} / ${t.reclaim_alloc_refills_max} |`);
      emit('SC', t);
      emit('SC', c);
    }
  }

  out.push('\n### bigheap (заполнение 64 Б, замер 65 536 × 1 КиБ)\n\n| S заказано | S заполнения | routed_miss_scans | full_scan_slots | slots/find |\n|---|---|---|---|---|');
  for (const s of [8, 32, 64]) {
    const r = get({ mode: 'bigheap', alloc: 'sefer', n_fill: s * PER_SEG[64], m: 65536 });
    assert.ok(r.fill_segments >= s, `bigheap S=${s}: only ${r.fill_segments} fill segments`);
    assert.equal(r.foreign_publish_ok, 0, `bigheap S=${s}: foreign publications`);
    assert.ok(r.find_segment >= 0.9 * (r.m / 16), `bigheap S=${s}: routed refill path not active`);
    out.push(`| ${s} | ${r.fill_segments} | ${r.routed_miss_scans} | ${r.full_scan_slots} | ${fmt(ratio(r.full_scan_slots, r.find_segment))} |`);
    emit('SC', r);
  }
}

// ---- PC ---------------------------------------------------------------------
if (has('pincycle')) {
  out.push('\n### pincycle (2048 × 1 КиБ из каждого из m сегментов)\n\n| m | порядок | hits | misses | доля попаданий | lookups |\n|---|---|---|---|---|---|');
  for (const m of [4, 5, 8, 16, 64]) {
    for (const order of ['cyclic', 'seq', 'random']) {
      const r = get({ mode: 'pincycle', alloc: 'sefer', n: m * 2048, m, order });
      assert.equal(r.segments, m, `pincycle m=${m}: ${r.segments} segments`);
      assert.equal(r.n, m * 2048, `pincycle m=${m}: n`);
      assert.equal(r.foreign_publish_ok, r.n, `pincycle m=${m} ${order}: publications ≠ n`);
      assert.equal(r.pin_hits + r.pin_misses, r.n, `pincycle m=${m} ${order}: hits + misses ≠ n`);
      out.push(`| ${m} | ${order} | ${r.pin_hits} | ${r.pin_misses} | ${fmt(ratio(r.pin_hits, r.n))} | ${r.directory_lookups} |`);
      emit('PC', r);
    }
  }
}

// ---- RP ---------------------------------------------------------------------
if (has('reclaim')) {
  const med = (xs) => {
    const v = [...xs].sort((a, b) => a - b);
    return v.length % 2 ? v[(v.length - 1) / 2] : (v[v.length / 2 - 1] + v[v.length / 2]) / 2;
  };
  out.push('\n### reclaim 64 Б (медианы 10 процессов на сторону, нс; тайминги — сборка без счётчиков)\n\n| S | n_free | max s | max m | max s/m | p999 s | p999 m | p999 s/m | p99 s | p99 m | p50 s | p50 m | сумма s/m | scan_words s | discovery_scans s | find_segment s | backlog публикаций |\n|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|');
  for (const s of [8, 32, 64]) {
    const fill = s * PER_SEG[64];
    for (const frac of [16, 4, 1]) {
      const nFree = fill / frac;
      const c = get({ mode: 'reclaim', alloc: 'sefer', n_fill: fill, n_free: nFree });
      assert.equal(c.backlog_foreign_publish_ok, nFree, `reclaim S=${s} n_free=${nFree}: backlog publications`);
      assert.equal(c.foreign_publish_ok, 0, `reclaim S=${s} n_free=${nFree}: owner phase published foreign frees`);
      const side = (alloc) => rows.filter((r) => r.mode === 'reclaim' && !r.counters && r.alloc === alloc && r.n_fill === fill && r.n_free === nFree);
      const sf = side('sefer');
      const mi = side('mimalloc');
      assert.equal(sf.length, 10, `reclaim S=${s} n_free=${nFree}: ${sf.length} sefer timing runs`);
      assert.equal(mi.length, 10, `reclaim S=${s} n_free=${nFree}: ${mi.length} mimalloc timing runs`);
      const m = (v, f) => med(v.map((x) => x[f]));
      const cells = ['call_ns_max', 'call_ns_p999', 'call_ns_p99', 'call_ns_p50'].flatMap((f) => [m(sf, f), m(mi, f)]);
      out.push(`| ${s} | ${nFree} | ${cells[0]} | ${cells[1]} | ${fmt(cells[0] / cells[1])} | ${cells[2]} | ${cells[3]} | ${fmt(cells[2] / cells[3])} | ${cells[4]} | ${cells[5]} | ${cells[6]} | ${cells[7]} | ${fmt(m(sf, 'calls_ns_total') / m(mi, 'calls_ns_total'))} | ${c.scan_words} | ${c.discovery_scans} | ${c.find_segment} | ${c.backlog_foreign_publish_ok} |`);
    }
  }
}

console.log(out.join('\n'));
if (csvAt) writeFileSync(csvAt, csv.join('\n') + '\n');
