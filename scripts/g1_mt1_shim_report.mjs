#!/usr/bin/env node
// G1-MT1-S: derive the harness-shim report tables and summary CSV from the
// three raw logs. Usage: node scripts/g1_mt1_shim_report.mjs [--csv]
import fs from 'node:fs';

const dir = new URL('../docs/perf/', import.meta.url);
const read = (f) => fs.readFileSync(new URL(f, dir), 'utf8');
const ev = (l, name) => {
  const m = l.match(new RegExp(name + '=(\\d+)'));
  return m ? Number(m[1]) : NaN;
};
const push = (m, k, v) => (m[k] ??= []).push(v);
const min = (a) => Math.min(...a);
const max = (a) => Math.max(...a);

// Linux perf-stat logs: "<label> <workload> <alloc> T=<n> <x> Mops cycles:u=.. ..."
function parseLinux(text) {
  const m = {};
  for (const l of text.split('\n')) {
    const t = l.trim().split(/\s+/);
    if (t.length < 6 || !/^T=/.test(t[3])) continue;
    const [lab, w, a, T, mops] = t;
    const k = [w, T, lab, a].join('|');
    push(m, k + '|mops', Number(mops));
    push(m, k + '|cyc', ev(l, 'cycles:u'));
    push(m, k + '|sf', ev(l, 'ld_blocks.store_forward:u'));
  }
  return m;
}

// Counterfactual log: "<workload> <alloc> T=1 <x> Mops <label> cycles:u=.. ..."
function parseCounterfactual(text) {
  const m = {};
  for (const l of text.split('\n')) {
    const t = l.trim().split(/\s+/);
    if (t.length < 7 || t[2] !== 'T=1') continue;
    const k = [t[0], t[5], t[1]].join('|');
    push(m, k + '|cyc', ev(l, 'cycles:u'));
    push(m, k + '|ins', ev(l, 'instructions:u'));
    push(m, k + '|sf', ev(l, 'ld_blocks.store_forward:u'));
  }
  return m;
}

// Windows malloc_macro logs: "== <label> <round>" headers, then T rows.
function parseWindows(text) {
  const m = {};
  let lab;
  let w;
  for (const l of text.split('\n')) {
    let x;
    if ((x = l.match(/^== (g1-\S+) /))) lab = x[1];
    else if ((x = l.match(/workload: (\w+)/))) w = x[1];
    else if ((x = l.match(/^\s+(\d)\s+([\d.]+) M\s+([\d.]+) M\s+([\d.]+) M/))) {
      push(m, [w, 'T=' + x[1], lab, 'sefer'].join('|'), Number(x[2]));
      push(m, [w, 'T=' + x[1], lab, 'mi'].join('|'), Number(x[3]));
    }
  }
  return m;
}

const lin = parseLinux(read('_raw_g1_mt1_shim_linux.log'));
const cf = parseCounterfactual(read('_raw_g1_mt1_inline_counterfactual.log'));
const win = parseWindows(read('_raw_g1_mt1_shim_windows.log'));

const rows = [];
const out = [];
const f3 = (x) => x.toFixed(3);

out.push('### Counterfactual: `alloc_block` forced inline (Linux, T=1, min cycles of 5)');
out.push('');
out.push('| workload | build | sefer cycles | mi cycles | sefer/mi cycles | sefer instructions | sefer store-forward blocks |');
out.push('|---|---|--:|--:|--:|--:|--:|');
for (const w of ['larson', 'mstress']) {
  for (const b of ['as1h', 'as1i']) {
    const s = min(cf[[w, b, 'sefer'].join('|') + '|cyc']);
    const mi = min(cf[[w, b, 'mi'].join('|') + '|cyc']);
    const ins = min(cf[[w, b, 'sefer'].join('|') + '|ins']);
    const sf = min(cf[[w, b, 'sefer'].join('|') + '|sf']);
    const r = s / mi;
    if (Math.abs(r * mi - s) > 1) throw new Error('ratio check');
    out.push(`| ${w} | ${b === 'as1h' ? 'as is' : 'inline(always)'} | ${s} | ${mi} | ${f3(r)} | ${ins} | ${sf} |`);
    rows.push(['counterfactual', 'linux', w, 'T=1', b, 'cycles_min', s, mi, f3(r), sf]);
  }
}

out.push('');
out.push('### Harness shim A/B, Linux (WSL2, perf stat; best Mops and min cycles of 5 interleaved pairs)');
out.push('');
out.push('| workload | T | main: mi/sefer Mops | shim: mi/sefer Mops | main: sefer/mi cycles | shim: sefer/mi cycles | sefer store-forward blocks main → shim |');
out.push('|---|--:|--:|--:|--:|--:|--:|');
for (const w of ['larson', 'mstress']) {
  for (const T of ['T=1', 'T=2', 'T=4']) {
    const g = (lab, a, f) => lin[[w, T, lab, a].join('|') + '|' + f];
    const mr = (lab) => max(g(lab, 'mi', 'mops')) / max(g(lab, 'sefer', 'mops'));
    const cr = (lab) => min(g(lab, 'sefer', 'cyc')) / min(g(lab, 'mi', 'cyc'));
    const sf = (lab) => min(g(lab, 'sefer', 'sf'));
    out.push(`| ${w} | ${T.slice(2)} | ${f3(mr('mt1b'))} | ${f3(mr('mt1s'))} | ${f3(cr('mt1b'))} | ${f3(cr('mt1s'))} | ${sf('mt1b')} → ${sf('mt1s')} |`);
    for (const lab of ['mt1b', 'mt1s']) {
      rows.push(['ab', 'linux', w, T, lab, 'mi_over_sefer_mops', max(g(lab, 'sefer', 'mops')), max(g(lab, 'mi', 'mops')), f3(mr(lab)), sf(lab)]);
    }
  }
}

out.push('');
out.push('### Harness shim A/B, Windows (`malloc_macro`, ABBA ×3 = 6 runs per build; best Mops per arm)');
out.push('');
out.push('| workload | T | main: sefer | main: mi | main: mi/sefer | shim: sefer | shim: mi | shim: mi/sefer |');
out.push('|---|--:|--:|--:|--:|--:|--:|--:|');
for (const w of ['larson', 'mstress']) {
  for (const T of ['T=1', 'T=2', 'T=4']) {
    const g = (lab, a) => max(win[[w, T, lab, a].join('|')]);
    const cells = [];
    for (const lab of ['g1-base', 'g1-mt1s']) {
      const s = g(lab, 'sefer');
      const mi = g(lab, 'mi');
      cells.push(s.toFixed(1), mi.toFixed(1), f3(mi / s));
      rows.push(['ab', 'windows', w, T, lab, 'mi_over_sefer_mops', s, mi, f3(mi / s), '']);
    }
    out.push(`| ${w} | ${T.slice(2)} | ${cells.join(' | ')} |`);
  }
}

if (process.argv.includes('--csv')) {
  console.log('section,os,workload,threads,build,metric,sefer,mimalloc,ratio,sefer_store_forward_min');
  for (const r of rows) console.log(r.join(','));
} else {
  console.log(out.join('\n'));
}
