// R16 item83 judge: measured artifacts only. --self-check uses memory-only test
// fixtures, never evidence. --check/--emit are read-only; capture never measures.
// Durable identity schema v1:
// {schema:1, base, features, artifacts:{relativeName:{sha256, text}},
//  raw_sha256:{A1,A2,C0,B1,B2,rss}, full_run_sha256:{A1,A2,C0,B1,B2},
//  correctness_receipt:null|{accepted,source_input_sha256,correctness,
//    counterfactual,clippy,narrowchecks,rustfmt,diffcheck}}
// artifacts contains actual driver identities + SHA sidecars, patches, compile
// receipts/build logs/results, run pre/post/tools/results, and activation
// verified/binary receipts/logs/results. No synthesized successful receipt.
// RSS producer contract (future script): target/r16-perf83/rss.log NDJSON plus
// A.rss.compile.json/B.rss.compile.json with .sha256 sidecars. RSS compile JSON:
// {features,input_sha256,binary:{path,sha256},tools,compiling_sefer_alloc:true,
//  build_result:{code:0,sanitized_log_sha256},build_log:<sanitized actual log>}.
// RSS path is relative to arm snapshot; build_log must contain Compiling sefer-alloc.
// Every RSS sample carries the complete user-specified schema validated below.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { existsSync, readFileSync, writeFileSync, lstatSync } from 'node:fs';
import { gzipSync, gunzipSync } from 'node:zlib';
import { dirname, join, resolve, isAbsolute } from 'node:path';
import { fileURLToPath } from 'node:url';
const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const DOC = join(ROOT, 'docs', 'perf'), STORE = join(ROOT, 'target', 'r16-perf83');
const BASE = '549d07b648ad48ee8e9d929bfec40157626a80b8';
const FEATURES = 'production bench-internals internals';
const TITLE = 'R16_PERF83_LARGE_SHRINK_INPLACE_GATE';
const RUNS = ['A1','A2','C0','B1','B2'], ARMS = ['A','C0','B'];
const WORK = ['realloc_large_shrink_8_to_6mib','realloc_large_shrink_8_to_4p5mib',
  'realloc_large_shrink_8_to_3mib','realloc_large_grow_6_to_8mib','realloc_large_equal_8mib'];
const ROWS = [...WORK.flatMap(n => [n, n+'_prefix']), 'large_alloc_free_cycle',
  'large_cache_prefill_only_4mib','large_cache_hit_only_4mib',
  'large_cache_free_slot_search_prefill_only','large_cache_free_slot_search_cycle_only',
  'realloc_grow','dealloc_realloc_burst_1088_16b_n17'];
const RSS_SCENARIOS = [...WORK, 'cycles8to6'];
const KEYS = ['ir','l1','l2','ram','total','est'];
const METRICS = {Instructions:'ir','L1 Hits':'l1','L2 Hits':'l2','RAM Hits':'ram',
  'Total read+write':'total','Estimated Cycles':'est'};
const sha = bytes => createHash('sha256').update(bytes).digest('hex');
const armFor = run => run.startsWith('B') ? 'B' : run === 'C0' ? 'C0' : 'A';
const hex = value => assert(typeof value === 'string' && /^[a-f0-9]{64}$/.test(value), 'invalid SHA256');
const integer = (value, name, positive = false) => assert(Number.isSafeInteger(value) && value >= (positive ? 1 : 0), `invalid ${name}`);
function safeRead(root, name, binary = false) {
  assert(!isAbsolute(name) && !name.split(/[\\/]/).includes('..'), 'relative artifact path required');
  let cursor = root;
  assert(!lstatSync(cursor).isSymbolicLink(), 'symlink root');
  for (const part of name.split('/')) {
    cursor = join(cursor, part);
    assert(existsSync(cursor), `missing data: ${name}`);
    assert(!lstatSync(cursor).isSymbolicLink(), `symlink artifact: ${name}`);
  }
  assert(lstatSync(cursor).isFile(), `regular artifact required: ${name}`);
  return binary ? readFileSync(cursor) : readFileSync(cursor, 'utf8');
}
function sanitized(text) {
  assert(!/[A-Za-z]:[\\/]|\/mnt\/[a-z]\/|\/(?:home|Users|tmp|var|opt|usr|root)\//.test(text), 'unsanitized machine path');
  return text;
}
function readDoc(name) {
  assert(existsSync(join(DOC, name)), `missing data: docs/perf/${name}`);
  return sanitized(safeRead(DOC, name));
}
function rawName(run) { return `_raw_r16_perf83_${run}.log`; }
function parseIr(text, label) {
  const raw = new Map(), tables = new Map(); let current = null;
  for (const line of text.split(/\r?\n/)) {
    const h = /^perf_gate_iai::perf_gate::(\w+)\b/.exec(line);
    if (h) {
      current = ROWS.includes(h[1]) ? h[1] : null;
      if (current) { assert(!raw.has(current), `${label}: duplicate raw ${current}`); raw.set(current, {}); }
      continue;
    }
    const m = /^\s+(Instructions|L1 Hits|L2 Hits|RAM Hits|Total read\+write|Estimated Cycles):\s*([\d,]+)/.exec(line);
    if (m && current) {
      const row = raw.get(current), key = METRICS[m[1]];
      assert(!(key in row), `${label}: duplicate metric`);
      row[key] = Number(m[2].replaceAll(',', ''));
    }
    const t = /^  (\w+)\s+([\d,]+)\s+([\d,]+)\s+([\d,]+)\s+([\d,]+)\s+([\d,]+)\s+\S+$/.exec(line);
    if (t && ROWS.includes(t[1])) {
      assert(!tables.has(t[1]), `${label}: duplicate table row`);
      tables.set(t[1], t.slice(2).map(s => Number(s.replaceAll(',', ''))));
    }
  }
  for (const name of ROWS) {
    const row = raw.get(name);
    assert(row && tables.has(name), `${label}: missing row ${name}`);
    for (const key of KEYS) integer(row[key], `${label}/${name}/${key}`, key === 'ir');
    assert.deepEqual([row.ir,row.l1,row.l2,row.ram,row.est], tables.get(name), 'raw/table mismatch');
    assert.equal(row.total, row.l1 + row.l2 + row.ram, 'total arithmetic');
    assert.equal(row.est, row.l1 + 5*row.l2 + 35*row.ram, 'est arithmetic');
  }
  return raw;
}
function parseRss(text) {
  const expectedKeys = ['arm','scenario','sample','rss_bytes','baseline_rss_bytes','peak_bytes',
    'final_free_rss_bytes','teardown_rss_bytes','pid','statistic','source_input_sha256','binary_sha256'].sort();
  const samples = new Map(), pids = new Set();
  for (const line of text.split(/\r?\n/).filter(s => s.trim())) {
    const row = JSON.parse(line);
    assert.deepEqual(Object.keys(row).sort(), expectedKeys, 'RSS schema must be complete/exact');
    assert(['A','B'].includes(row.arm) && RSS_SCENARIOS.includes(row.scenario), 'RSS arm/scenario');
    integer(row.sample, 'sample', true); assert(row.sample <= 9, 'RSS sample1..9');
    for (const key of ['rss_bytes','baseline_rss_bytes','peak_bytes','final_free_rss_bytes','teardown_rss_bytes','pid']) integer(row[key], key, true);
    assert.equal(row.statistic, 'per-process absolute RSS');
    assert(row.peak_bytes >= row.rss_bytes && row.peak_bytes >= row.baseline_rss_bytes, 'lifetime peak below resident bytes');
    hex(row.source_input_sha256); hex(row.binary_sha256);
    assert(!pids.has(row.pid), 'RSS processes must be fresh and distinct'); pids.add(row.pid);
    const key = `${row.arm}/${row.scenario}/${row.sample}`;
    assert(!samples.has(key), 'duplicate RSS sample'); samples.set(key, row);
  }
  for (const arm of ['A','B']) for (const scenario of RSS_SCENARIOS) for (let sample=1; sample<=9; sample++) {
    assert(samples.has(`${arm}/${scenario}/${sample}`), `missing RSS sample ${arm}/${scenario}/${sample}`);
  }
  assert.equal(samples.size, 108);
  return samples;
}
function ratio(num, den) {
  assert(Number.isFinite(num) && Number.isFinite(den) && den > 0, 'ratio operands');
  const value = num/den;
  assert(Math.abs(value*den-num) <= 1e-9*Math.max(1,Math.abs(num)), 'ratio arithmetic');
  return `${value.toFixed(6)} (${num}/${den})`;
}
function median9(values) {
  assert.equal(values.length, 9);
  values.forEach(v => integer(v, 'median operand', true));
  return [...values].sort((a,b) => a-b)[4];
}
function artifactNames() {
  const names = ['bench.patch','baseline.patch','candidate.patch'];
  for (const arm of ARMS) {
    names.push(`${arm}.identity.json`,`${arm}.identity.sha256`,`${arm}.compile.json`,`${arm}.compile.sha256`,
      `${arm}.compile-build.log`,`${arm}.compile-build.result.json`,`${arm}.compile-tools.log`,
      `${arm}.activation-binary.json`,`${arm}.activation-binary.sha256`,`${arm}.activation-build.log`,
      `${arm}.activation-build.result.json`);
    for (const scenario of WORK) names.push(`${arm}.activation-${scenario}.verified.json`,
      `${arm}.activation-${scenario}.verified.sha256`,`${arm}.activation-${scenario}.log`,`${arm}.activation-${scenario}.result.json`);
  }
  for (const run of RUNS) names.push(`${run}.pre-run.json`,`${run}.pre-run.sha256`,`${run}.post-run.json`,`${run}.post-run.sha256`,
    `${run}.run.result.json`,`${run}.run-tools.log`,`${run}.post-run-tools.log`);
  for (const arm of ['A','B']) names.push(`${arm}.rss.compile.json`,`${arm}.rss.compile.sha256`);
  return names.sort();
}
function validateIdentity(bundle, irTexts, rssText) {
  assert.equal(bundle.schema, 1); assert.equal(bundle.base, BASE); assert.equal(bundle.features, FEATURES);
  assert.deepEqual(Object.keys(bundle.artifacts).sort(), artifactNames(), 'artifact inventory');
  const text = name => {
    const a = bundle.artifacts[name]; assert(a, `missing identity artifact ${name}`); hex(a.sha256);
    assert.equal(sha(a.text), a.sha256, `artifact hash ${name}`); return sanitized(a.text);
  };
  const json = name => JSON.parse(text(name));
  const signed = stem => { const bytes = text(stem+'.json'); assert.equal(sha(bytes), text(stem+'.sha256').trim()); return JSON.parse(bytes); };
  const ids = {}, builds = {}, stats = {};
  for (const arm of ARMS) {
    const id = signed(`${arm}.identity`); ids[arm] = id;
    assert.equal(id.base, BASE); assert(/^[a-f0-9]{40}$/.test(id.tree)); assert.equal(id.features, FEATURES);
    assert(Array.isArray(id.inputs) && id.inputs.length > 0); hex(id.input_sha256);
    assert.deepEqual(id.inputs.map(([p])=>p), [...new Set(id.inputs.map(([p])=>p))].sort());
    for (const [p,h] of id.inputs) { assert(!isAbsolute(p) && !p.split('/').includes('..')); hex(h); }
    assert.equal(sha(JSON.stringify(id.inputs)), id.input_sha256);
    assert.equal(sha(text('bench.patch')), id.bench_patch_sha256);
    assert.equal(sha(text(arm==='B'?'candidate.patch':'baseline.patch')), id.src_patch_sha256);
    assert.equal(id.harness_sha256, id.inputs.find(([p])=>p==='scripts/iai.mjs')?.[1]);
    const build = signed(`${arm}.compile`); builds[arm] = build;
    assert.equal(build.input_sha256, id.input_sha256);
    assert.equal(build.identity_sha256, sha(text(`${arm}.identity.json`)));
    assert.equal(build.base, BASE); assert.equal(build.tree, id.tree); assert.equal(build.features, FEATURES);
    assert.equal(build.compiling_sefer_alloc, true); hex(build.binary.sha256);
    assert(build.binary.path.startsWith('target/') && !build.binary.path.split('/').includes('..'));
    assert.equal(build.tools, text(`${arm}.compile-tools.log`));
    assert(build.tools.includes('rustc ') && build.tools.includes('cargo ') && build.tools.includes('valgrind-') && /0\.14\.2/.test(build.tools), 'tool versions missing');
    assert(/CARGO_BUILD_JOBS=3/.test(build.command) && /cargo bench --locked --no-run/.test(build.command));
    const result = json(`${arm}.compile-build.result.json`);
    assert.equal(result.code, 0); assert.equal(result.sanitized_log_sha256, sha(text(`${arm}.compile-build.log`)));
    assert.deepEqual(build.build_result, result); assert(/Compiling sefer-alloc\b/.test(text(`${arm}.compile-build.log`)));
    const stat = signed(`${arm}.activation-binary`); stats[arm]=stat;
    assert.equal(stat.input_sha256, id.input_sha256); assert.equal(stat.features, FEATURES+' alloc-stats'); hex(stat.binary.sha256);
    assert(stat.binary.path.startsWith('target/activation/'), 'separate activation target');
    const statResult=json(`${arm}.activation-build.result.json`);
    assert.equal(statResult.code,0); assert.equal(statResult.sanitized_log_sha256,sha(text(`${arm}.activation-build.log`)));
    assert(/Compiling sefer-alloc\b/.test(text(`${arm}.activation-build.log`)));
    for (const scenario of WORK) {
      const verified = signed(`${arm}.activation-${scenario}.verified`), row=verified.outcome;
      assert.equal(verified.input_sha256,id.input_sha256); assert.deepEqual(verified.binary,stat.binary);
      const inplace=scenario===WORK[4] || (arm==='B' && WORK.slice(0,2).includes(scenario));
      assert.equal(row.arm,arm); assert.equal(row.scenario,scenario); integer(row.pid,'activation pid',true);
      assert.equal(row.moved,!inplace); assert.equal(row.inplace_large_delta,Number(inplace));
      assert.equal(row.inplace_small_delta,0); assert.equal(row.decline_delta,Number(!inplace));
      assert.equal(row.preserved_prefix,true); assert.equal(row.new_layout_dealloc,true);
      const log=text(`${arm}.activation-${scenario}.log`), result=json(`${arm}.activation-${scenario}.result.json`);
      assert.equal(result.code,0); assert.equal(result.sanitized_log_sha256,sha(log));
      assert(/test result: ok\. 1 passed; 0 failed/.test(log));
      const raw=log.split(/\r?\n/).map(l=>l.slice(l.indexOf('{'))).filter(l=>l.startsWith('{')).map(l=>JSON.parse(l));
      assert.deepEqual(raw,[row], 'activation raw/receipt mismatch');
    }
  }
  assert.deepEqual(ids.C0,ids.A,'C0 same-source different-path identity');
  assert.equal(ids.B.tree,ids.A.tree);
  assert.deepEqual(ids.B.inputs.map(([p])=>p),ids.A.inputs.map(([p])=>p));
  assert.deepEqual(ids.B.inputs.filter(([p,h],i)=>h!==ids.A.inputs[i][1]).map(([p])=>p),['src/alloc_core/alloc_core/mem/realloc_fastpath.rs']);
  assert.equal(text('baseline.patch'),''); assert(text('candidate.patch').length>0);
  for (const run of RUNS) {
    const arm=armFor(run), pre=signed(`${run}.pre-run`), post=signed(`${run}.post-run`);
    assert.equal(pre.input_sha256,ids[arm].input_sha256); assert.equal(post.input_sha256,pre.input_sha256);
    assert.equal(pre.features,FEATURES); assert.deepEqual(pre.benches,ROWS);
    assert.deepEqual(pre.binary,builds[arm].binary); assert.deepEqual(post.binary,pre.binary);
    assert.equal(pre.compile_receipt_sha256,sha(text(`${arm}.compile.json`)));
    assert.equal(pre.tools,builds[arm].tools); assert.equal(text(`${run}.run-tools.log`),pre.tools);
    assert.equal(text(`${run}.post-run-tools.log`),pre.tools);
    const result=json(`${run}.run.result.json`); assert.equal(result.code,0); assert.deepEqual(post.result,result);
    assert.equal(result.sanitized_log_sha256,bundle.full_run_sha256[run]);
    assert.equal(sha(irTexts[run]),bundle.raw_sha256[run]);
    assert(irTexts[run].includes(`# Sanitized full-output SHA-256: ${result.sanitized_log_sha256}`), 'excerpt/full linkage');
  }
  assert.equal(sha(rssText),bundle.raw_sha256.rss);
  const rssBuilds={};
  for (const arm of ['A','B']) {
    const b=signed(`${arm}.rss.compile`); rssBuilds[arm]=b;
    assert.equal(b.features,FEATURES); assert.equal(b.input_sha256,ids[arm].input_sha256); hex(b.binary.sha256);
    assert(b.binary.path.startsWith('target/') && !b.binary.path.split('/').includes('..'));
    assert.equal(b.compiling_sefer_alloc,true); assert.equal(b.build_result.code,0);
    assert.equal(sha(b.build_log),b.build_result.sanitized_log_sha256); assert(/Compiling sefer-alloc\b/.test(b.build_log));
    assert(typeof b.tools==='string' && b.tools.includes('rustc'), 'RSS compiler identity');
  }
  const samples=parseRss(rssText);
  for (const row of samples.values()) {
    assert.equal(row.source_input_sha256,ids[row.arm].input_sha256);
    assert.equal(row.binary_sha256,rssBuilds[row.arm].binary.sha256);
  }
  const receipt=bundle.correctness_receipt;
  if (receipt !== null) {
    assert(receipt && typeof receipt.accepted==='boolean', 'parent receipt schema');
    assert.equal(receipt.source_input_sha256,ids.B.input_sha256,'parent source identity');
    for (const key of ['correctness','counterfactual','clippy','narrowchecks','rustfmt','diffcheck']) assert(typeof receipt[key]==='boolean', `parent flag ${key}`);
  }
  return {ids,builds,samples,accepted:receipt?.accepted===true && ['correctness','counterfactual','clippy','narrowchecks','rustfmt','diffcheck'].every(k=>receipt[k]===true)};
}
function derive(bundle, texts, rssText) {
  const validated=validateIdentity(bundle,texts,rssText), arms=Object.fromEntries(RUNS.map(r=>[r,parseIr(texts[r],r)]));
  const failures=[];
  if (bundle.correctness_receipt) {
    for (const key of ['correctness','counterfactual','clippy','narrowchecks','rustfmt','diffcheck']) {
      if (bundle.correctness_receipt[key] === false) failures.push(`parent check failed ${key}`);
    }
  }
  for (const [a,b] of [['A1','A2'],['B1','B2']]) for (const name of ROWS) {
    if (KEYS.some(k=>arms[a].get(name)[k]!==arms[b].get(name)[k])) failures.push(`repeat mismatch ${a}/${b}/${name}`);
  }
  const T=Math.max(10,...ROWS.map(n=>Math.abs(arms.C0.get(n).ir-arms.A1.get(n).ir)));
  for (const n of ROWS) {
    const delta=arms.B1.get(n).ir-arms.A1.get(n).ir;
    if (WORK.slice(0,2).includes(n) ? !(-delta>T) : !(Math.abs(delta)<=T)) failures.push(`Ir gate ${n}`);
  }
  const rssSummaries=[];
  for (const scenario of RSS_SCENARIOS) {
    const values=arm=>Array.from({length:9},(_,i)=>validated.samples.get(`${arm}/${scenario}/${i+1}`));
    const a=values('A'),b=values('B'),cycle=scenario==='cycles8to6';
    const stat=cycle?'max lifetime VmHWM including setup (9 processes, 20 cycles)':'median absolute post-realloc RSS (9 processes)';
    const av=cycle?Math.max(...a.map(x=>x.peak_bytes)):median9(a.map(x=>x.rss_bytes));
    const bv=cycle?Math.max(...b.map(x=>x.peak_bytes)):median9(b.map(x=>x.rss_bytes));
    if (!(bv<=av+0.01*av)) failures.push(`RSS gate ${scenario}`);
    rssSummaries.push({scenario,stat,a:av,b:bv});
  }
  const verdict=failures.length?'NO-GO':validated.accepted?'GO':'PERF-PASS; correctness acceptance required';
  const table=['| bench | A1 Ir | A2 Ir | C0 Ir | B1 Ir | B2 Ir | B−A Ir | B/A Ir (num/den) | A/B/C0 L1 | A/B/C0 L2 | A/B/C0 RAM | A/B/C0 Total | A/B/C0 EstCycles | B/A EstCycles (num/den) |',
    '|---|---:|---:|---:|---:|---:|---:|---|---|---|---|---|---|---|'];
  const csv=[['kind','name','arm','sample','statistic','value','numerator','denominator','threshold_ir','verdict','base_sha','features','detail']];
  const add=(kind,name,arm,sample,stat,value,num='',den='',detail='')=>csv.push([kind,name,arm,sample,stat,value,num,den,T,verdict,BASE,FEATURES,detail]);
  for (const n of ROWS) {
    const a=arms.A1.get(n),b=arms.B1.get(n),c=arms.C0.get(n);
    table.push(`| ${n} | ${a.ir} | ${arms.A2.get(n).ir} | ${c.ir} | ${b.ir} | ${arms.B2.get(n).ir} | ${b.ir-a.ir} | ${ratio(b.ir,a.ir)} | ${['l1','l2','ram','total','est'].map(k=>`${a[k]}/${b[k]}/${c[k]}`).join(' | ')} | ${ratio(b.est,a.est)} |`);
    for (const run of ['A1','C0','B1']) for (const k of KEYS) add('iai',n,run,'',k,arms[run].get(n)[k],'','',validated.builds[armFor(run)].binary.sha256);
  }
  table.push('',`T=max(10 Ir,max absolute C0−A1 across all 17 rows)=${T} Ir. All six numeric columns must repeat exactly.`,
    '', '| work | A work−prefix (operands) | B work−prefix (operands) | paired B−A (operands) |', '|---|---|---|---|');
  for (const n of WORK) {
    const aw=arms.A1.get(n).ir,ap=arms.A1.get(n+'_prefix').ir,bw=arms.B1.get(n).ir,bp=arms.B1.get(n+'_prefix').ir;
    const a=aw-ap,b=bw-bp;
    assert.equal(b-a,(bw-aw)-(bp-ap));
    table.push(`| ${n} | ${a} (${aw}−${ap}) | ${b} (${bw}−${bp}) | ${b-a} (${b}−${a}) |`);
    add('paired',n,'B−A','', 'paired Ir delta',b-a,b,a,`A=${aw}-${ap}; B=${bw}-${bp}`);
  }
  table.push('', '| RSS scenario | statistic | A bytes | B bytes | B/A (num/den) | allowed B bytes (A+1% A) |', '|---|---|---:|---:|---|---:|');
  for (const row of rssSummaries) {
    table.push(`| ${row.scenario} | ${row.stat} | ${row.a} | ${row.b} | ${ratio(row.b,row.a)} | ${row.a+0.01*row.a} |`);
    add('rss_summary',row.scenario,'B/A','',row.stat,row.b,row.b,row.a,`limit=${row.a}+0.01*${row.a}`);
  }
  for (const row of validated.samples.values()) for (const k of ['rss_bytes','peak_bytes']) {
    add('rss_sample',row.scenario,row.arm,row.sample,`per-process absolute ${k}`,row[k],'','',`pid=${row.pid};binary=${row.binary_sha256};source=${row.source_input_sha256}`);
  }
  table.push('',`Computed gate: ${verdict}. Failures: ${failures.join('; ')||'none'}.`,
    'Paired values are explanatory, not replacement gates. Cache-model columns are secondary, not native cycles/wall-clock. RSS uses absolute A denominators, not baseline-subtracted deltas.');
  const escape=v=>'"'+String(v).replaceAll('"','""')+'"';
  return {table:table.join('\n'),csv:csv.map(r=>r.map(escape).join(',')).join('\n')+'\n',verdict};
}
function excerpt(text, run) {
  const lines=[]; let selected=false;
  for (const line of text.split(/\r?\n/)) {
    const h=/^perf_gate_iai::perf_gate::(\w+)\b/.exec(line);
    if(h)selected=ROWS.includes(h[1]);
    if(h&&selected || selected&&/^\s+(Instructions|L1 Hits|L2 Hits|RAM Hits|Total read\+write|Estimated Cycles):/.test(line))lines.push(line);
    const row=/^  (\w+)\s+[\d,]+\s+[\d,]+\s+[\d,]+\s+[\d,]+\s+[\d,]+\s+\S+$/.exec(line);
    if(row&&ROWS.includes(row[1]))lines.push(line);
  }
  const out=`# TRUNCATED — omitted build progress/unselected benches; selected current-run metrics/table rows retained verbatim.\n# Reproduce: node scripts/r16_perf83_iai.mjs --run ${run} (fresh no-overwrite receipt location required).\n# Sanitized full-output SHA-256: ${sha(text)}\n`+lines.join('\n')+'\n';
  assert(Buffer.byteLength(out)<200*1024);parseIr(out,run);return sanitized(out);
}
function capture() {
  assert(existsSync(STORE),'missing data: frozen target receipts');
  const artifacts={};
  for(const name of artifactNames()){const text=sanitized(safeRead(STORE,name));artifacts[name]={text,sha256:sha(text)};}
  const texts={},raw_sha256={},full_run_sha256={};
  for(const run of RUNS){const full=sanitized(safeRead(STORE,`${run}.run.log`));{const ran=[...full.matchAll(/Running benches\/perf_gate_iai\.rs \((target\/release\/deps\/perf_gate_iai-[0-9a-f]+)\)/g)].map(m=>m[1]);assert.deepEqual(ran,[JSON.parse(artifacts[armFor(run)+'.compile.json'].text).binary.path],'measured binary differs from compile receipt');}texts[run]=excerpt(full,run);raw_sha256[run]=sha(texts[run]);full_run_sha256[run]=sha(full);}
  const rssText=sanitized(safeRead(STORE,'rss.log'));raw_sha256.rss=sha(rssText);
  const parentPath='parent.correctness.json';
  const bundle={schema:1,base:BASE,features:FEATURES,artifacts,raw_sha256,full_run_sha256,
    correctness_receipt:existsSync(join(STORE,parentPath))?JSON.parse(sanitized(safeRead(STORE,parentPath))):null};
  validateIdentity(bundle,texts,rssText);
  // Verify actual frozen source/binary bytes in target before importing evidence.
  for(const arm of ARMS){const id=JSON.parse(artifacts[`${arm}.identity.json`].text);for(const[p,h]of id.inputs)assert.equal(sha(safeRead(join(STORE,arm),p,true)),h,`frozen input changed ${arm}/${p}`);
    for(const stem of [`${arm}.compile`,`${arm}.activation-binary`,...(arm==='C0'?[]:[`${arm}.rss.compile`])]){const receipt=JSON.parse(artifacts[stem+'.json'].text);assert.equal(sha(safeRead(join(STORE,arm),receipt.binary.path,true)),receipt.binary.sha256,`binary changed ${stem}`);}}
  const files=new Map(RUNS.map(r=>[rawName(r),texts[r]]));files.set('_raw_r16_perf83_rss.log',rssText);
  files.set(TITLE+'_identity.json.gz',gzipSync(Buffer.from(JSON.stringify(bundle)+'\n'),{level:9}));
  for(const[name,text]of files){if(name.endsWith('.gz'))assert(gunzipSync(text).length<2*1024*1024,'CLAUDE.md tier-2 gzip: uncompressed < 2 MiB '+name);else assert(Buffer.byteLength(text)<200*1024,'200 KiB ceiling '+name);assert(!existsSync(join(DOC,name)),`refuse overwrite ${name}`);}
  // Only complete validated capture writes; missing inputs never create partial fake evidence.
  for(const[name,text]of files)writeFileSync(join(DOC,name),text,{flag:'wx'});
  console.log('Captured actual complete sanitized receipts/logs; no measurement or GO asserted.');
}
function selfCheck() {
  // In-memory parser/math tests only: these values are NOT measurement evidence.
  const text=ROWS.map(n=>`perf_gate_iai::perf_gate::${n}\n  Instructions: 100\n  L1 Hits: 10\n  L2 Hits: 2\n  RAM Hits: 1\n  Total read+write: 13\n  Estimated Cycles: 55\n  ${n} 100 10 2 1 55 -`).join('\n');
  assert.equal(parseIr(text,'memory-only').size,17);
  assert.throws(()=>parseIr(text.replace('Total read+write: 13','Total read+write: 14'),'bad total'));
  assert.throws(()=>parseIr(text.replace('Estimated Cycles: 55','Estimated Cycles: 56'),'bad est'));
  assert.equal(median9([9,1,8,2,7,3,6,4,5]),5); assert.equal(ratio(2,4),'0.500000 (2/4)');
  assert.equal(Math.max(10,Math.abs(112-100)),12);assert(!(100-88>12));assert(100-87>12);
  const rows=[];let pid=1;
  for(const arm of ['A','B'])for(const scenario of RSS_SCENARIOS)for(let sample=1;sample<=9;sample++)rows.push({arm,scenario,sample,rss_bytes:100,baseline_rss_bytes:90,peak_bytes:120,final_free_rss_bytes:95,teardown_rss_bytes:90,pid:pid++,statistic:'per-process absolute RSS',source_input_sha256:'a'.repeat(64),binary_sha256:'b'.repeat(64)});
  const ndjson=rows.map(r=>JSON.stringify(r)).join('\n');assert.equal(parseRss(ndjson).size,108);
  assert.throws(()=>parseRss(rows.slice(1).map(r=>JSON.stringify(r)).join('\n')));
  const invalid={...rows[0],rss_bytes:0};assert.throws(()=>parseRss([invalid,...rows.slice(1)].map(r=>JSON.stringify(r)).join('\n')));
  assert(101<=100+0.01*100);assert(!(102<=100+0.01*100));
  console.log('Memory-only parser/schema/math self-check passed; no measurement artifacts created.');
}
try {
  const args=process.argv.slice(2);assert(args.length<=1,'one mode only');const mode=args[0]??'--generate';
  assert(['--self-check','--capture','--check','--emit','--update-report','--generate'].includes(mode),'invalid mode');
  if(mode==='--self-check')selfCheck();
  else if(mode==='--capture')capture();
  else {
    const texts=Object.fromEntries(RUNS.map(r=>[r,readDoc(rawName(r))]));
    const rssText=readDoc('_raw_r16_perf83_rss.log'),bundle=JSON.parse(gunzipSync(safeRead(DOC,TITLE+'_identity.json.gz',true)).toString('utf8'));
    const result=derive(bundle,texts,rssText),reportPath=join(DOC,TITLE+'.md'),csvPath=join(DOC,TITLE+'_summary.csv');
    const begin='<!-- r16_perf83:tables:begin -->',end='<!-- r16_perf83:tables:end -->';
    if(mode==='--emit')console.log(result.table+'\n---CSV---\n'+result.csv);
    else if(mode==='--check') {
      const report=readDoc(TITLE+'.md'),i=report.indexOf(begin),j=report.indexOf(end);
      assert(i>=0&&j>i,'missing report table markers');assert.equal(report.slice(i+begin.length,j).trim(),result.table.trim(),'table drift');
      assert.equal(readDoc(TITLE+'_summary.csv'),result.csv,'CSV drift');console.log(`r16_perf83 checked: ${result.verdict}`);
    } else {
      if(mode==='--update-report') {
        const report=readDoc(TITLE+'.md'),i=report.indexOf(begin),j=report.indexOf(end);
        assert((i===-1&&j===-1)||(i>=0&&j>i),'malformed table markers');
        const replacement=begin+'\n'+result.table+'\n'+end;
        writeFileSync(reportPath,i===-1?report+'\n\n'+replacement+'\n':report.slice(0,i)+replacement+report.slice(j+end.length));
      }
      writeFileSync(csvPath,result.csv);console.log(result.table);
    }
  }
} catch(error){console.error(error.message);process.exitCode=1;}
