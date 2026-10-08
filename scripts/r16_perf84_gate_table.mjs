// R16 item84 checked judge. Default generates CSV/prints tables from docs only.
// --check is read-only; --update-report explicitly embeds generated tables.
// --capture imports selected actual logs from ignored snapshots (explicit operation).
// --emit writes generated tables to stdout and CSV to stdout after a separator.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFileSync, writeFileSync, existsSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const P = join(ROOT, 'docs', 'perf');
const BENCHES = ['dealloc_free_only_16b_n16','dealloc_free_only_16b_n17','dealloc_free_only_16b_n32','dealloc_prealloc_only_16b','dealloc_free_only_16b_n8','dealloc_free_only_16b_n9','dealloc_flush_class_only_16b_prefix','dealloc_flush_class_only_16b','alloc_clear_magazine_only_16b_prefix','alloc_clear_magazine_only_16b','alloc_magazine_hit_only_16b','small_churn_16b','small_churn_16b_2n','dealloc_flush_all_tcache_16b_prefix','dealloc_flush_all_tcache_16b'];
const LABELS = ['A1','A2','C0','B1','B2'];
const sha = (s) => createHash('sha256').update(s).digest('hex');
if (process.argv.includes('--capture')) {
  for (const label of LABELS) {
    const source = readFileSync(join(ROOT,'target','r16-perf84',`${label}.log`),'utf8');
    const kept=[]; let selected=false;
    for(const line of source.split(/\r?\n/)) {
      const header=/^perf_gate_iai::perf_gate::(\w+)\b/.exec(line);
      if(header) selected=BENCHES.includes(header[1]);
      if(header&&selected || selected&&/^\s+(Instructions|L1 Hits|L2 Hits|RAM Hits|Total read\+write|Estimated Cycles):/.test(line)) kept.push(line);
      if(line.startsWith('[iai]') || /^  bench\s+Ir/.test(line) || /^  -{4}/.test(line)) kept.push(line);
      const row=/^  (\w+)\s+[\d,]+\s+[\d,]+\s+[\d,]+\s+[\d,]+\s+[\d,]+\s+\S+$/.exec(line);
      if(row&&BENCHES.includes(row[1])) kept.push(line);
    }
    const out=`# TRUNCATED — omitted build progress, unselected benchmark blocks and elapsed-time/derived comparison output; selected current-run metrics retained verbatim.\n# Full output reproduction: node scripts/r16_perf84_iai.mjs --run ${label} (fresh receipt location required; driver refuses overwrites).\n# Sanitized full-output SHA-256: ${sha(source)}\n`+kept.join('\n')+'\n';
    assert(Buffer.byteLength(out)<200*1024);
    assert(!/[A-Za-z]:[\\/]|\/mnt\/[a-z]\//.test(out));
    const file=join(P,`_raw_r16_perf84_${label}.log`);
    if(existsSync(file)) assert.equal(readFileSync(file,'utf8'),out,'different evidence');
    else writeFileSync(file,out);
  }
  console.log('Captured five sanitized selected excerpts');
  process.exit(0);
}
const keys=['ir','l1','l2','ram','total','est'];
const metric={'Instructions':'ir','L1 Hits':'l1','L2 Hits':'l2','RAM Hits':'ram','Total read+write':'total','Estimated Cycles':'est'};
function parse(label) {
  const text=readFileSync(join(P,`_raw_r16_perf84_${label}.log`),'utf8');
  const raw=new Map(), tables=new Map(); let current=null;
  for(const line of text.split(/\r?\n/)) {
    const h=/^perf_gate_iai::perf_gate::(\w+)\b/.exec(line);
    if(h) {current=BENCHES.includes(h[1])?h[1]:null;if(current){assert(!raw.has(current));raw.set(current,{});}continue;}
    const m=/^\s+(Instructions|L1 Hits|L2 Hits|RAM Hits|Total read\+write|Estimated Cycles):\s*([\d,]+)/.exec(line);
    if(m&&current){const row=raw.get(current),key=metric[m[1]];assert(!(key in row));row[key]=Number(m[2].replaceAll(',',''));}
    const t=/^  (\w+)\s+([\d,]+)\s+([\d,]+)\s+([\d,]+)\s+([\d,]+)\s+([\d,]+)\s+\S+$/.exec(line);
    if(t&&BENCHES.includes(t[1])) {assert(!tables.has(t[1]));tables.set(t[1],t.slice(2).map(s=>Number(s.replaceAll(',',''))));}
  }
  for(const name of BENCHES){const r=raw.get(name);assert(r&&tables.has(name),`${label}: missing ${name}`);for(const key of keys)assert(Number.isSafeInteger(r[key]),`${label}: missing/invalid ${key}`);assert.deepEqual([r.ir,r.l1,r.l2,r.ram,r.est],tables.get(name),'raw/table mismatch');assert.equal(r.total,r.l1+r.l2+r.ram);assert.equal(r.est,r.l1+5*r.l2+35*r.ram);}
  return raw;
}
const arms=Object.fromEntries(LABELS.map(l=>[l,parse(l)]));
for(const [a,b] of [['A1','A2'],['B1','B2']])for(const name of BENCHES)assert.deepEqual(arms[a].get(name),arms[b].get(name),`${a}/${b} repeat mismatch ${name}`);
const ratio=(num,den)=>{assert(Number.isFinite(num)&&Number.isFinite(den)&&den>0);const r=num/den;assert(Math.abs(r*den-num)<=1e-8*Math.max(1,Math.abs(num)));return `${r.toFixed(6)} (${num}/${den})`;};
const saving=n=>arms.A1.get(n).ir-arms.B1.get(n).ir;
const maxCtl=Math.max(...BENCHES.map(n=>Math.abs(arms.C0.get(n).ir-arms.A1.get(n).ir)));
const T=Math.max(10,maxCtl);
const n17='dealloc_free_only_16b_n17',n32='dealloc_free_only_16b_n32',flush='dealloc_flush_all_tcache_16b',prefix=flush+'_prefix';
const dose=Math.abs(saving(n32)-2*saving(n17)),doseTolerance=Math.max(10,(5/100)*2*saving(n17));
const paired=saving(flush)-saving(prefix);
const controls=BENCHES.filter(n=>![n17,n32,flush,'small_churn_16b','small_churn_16b_2n'].includes(n));
const failures=[];
for(const n of [n17,n32,flush])if(!(saving(n)>T))failures.push(`${n} raw saving <= T`);
if(!(paired>2*T))failures.push('flush-all paired saving <= 2T');
if(!(saving(n17)>0&&dose<=doseTolerance))failures.push('dose');
for(const n of controls)if(Math.abs(saving(n))>T)failures.push(`control ${n} outside T`);
for(const [x,y] of [['dealloc_free_only_16b_n9','dealloc_free_only_16b_n8'],['dealloc_flush_class_only_16b','dealloc_flush_class_only_16b_prefix'],['alloc_clear_magazine_only_16b','alloc_clear_magazine_only_16b_prefix']])if(Math.abs(saving(x)-saving(y))>2*T)failures.push(`paired control ${x}`);
for(const n of ['small_churn_16b','small_churn_16b_2n'])if(saving(n)<-T)failures.push(`churn regression ${n}`);
const metadata=JSON.parse(readFileSync(join(P,'R16_PERF84_FLUSH_ROOT_MASK_GATE_identity.json'),'utf8'));
const receipt=metadata.correctness_receipt;
assert(receipt && receipt.source && receipt.focused_tests_passed===20 && receipt.broad_targets===34 && receipt.broad_executed_binaries_passed && receipt.no_panic_tests_passed===4 && ['exact_object_proto_narrow_exit','clippy_exit','rustfmt_exit','diff_check_exit'].every(k=>receipt[k]===0),'missing parent correctness receipt');
const verdict=failures.length?'NO-GO':receipt.accepted===true?'GO':'PERF-PASS; correctness acceptance required';
const table=['| bench | A Ir | B Ir | B−A Ir | B/A Ir (num/den) | C0−A Ir | A EstCycles | B EstCycles | B/A EstCycles (num/den) |','|---|---:|---:|---:|---|---:|---:|---:|---|'];
for (const label of LABELS) assert.equal(sha(readFileSync(join(P,`_raw_r16_perf84_${label}.log`))),metadata.raw_sha256[label],`${label} evidence hash`);
const csv=['bench,base_sha,tree_sha,a_input_sha256,b_input_sha256,candidate_patch_sha256,bench_patch_sha256,features,cpu,os,rustc,target,runner,valgrind,a_samples,b_samples,c0_samples,a1_ir,a2_ir,c0_ir,b1_ir,b2_ir,delta_ir,a1_l1,a1_l2,a1_ram,a1_total,a1_est,b1_l1,b1_l2,b1_ram,b1_total,b1_est,c0_l1,c0_l2,c0_ram,c0_total,c0_est,threshold_ir,verdict'];
for(const n of BENCHES){const a=arms.A1.get(n),b=arms.B1.get(n),c=arms.C0.get(n);table.push(`| ${n} | ${a.ir} | ${b.ir} | ${b.ir-a.ir} | ${ratio(b.ir,a.ir)} | ${c.ir-a.ir} | ${a.est} | ${b.est} | ${ratio(b.est,a.est)} |`);csv.push([n,metadata.base_sha,metadata.tree_sha,metadata.a_input_sha256,metadata.b_input_sha256,metadata.candidate_patch_sha256,metadata.bench_patch_sha256,metadata.features,metadata.cpu,metadata.os,metadata.rustc,metadata.target,metadata.runner,metadata.valgrind,2,2,1,a.ir,arms.A2.get(n).ir,c.ir,b.ir,arms.B2.get(n).ir,b.ir-a.ir,...['l1','l2','ram','total','est'].map(k=>a[k]),...['l1','l2','ram','total','est'].map(k=>b[k]),...['l1','l2','ram','total','est'].map(k=>c[k]),T,verdict].join(','));}
table.push('',`Repeat equality: A1=A2 and B1=B2 for Ir/L1/L2/RAM/Total read+write/Estimated Cycles. C0 max absolute Ir delta=${maxCtl}; T=max(10; ${maxCtl})=${T} Ir.`,`Raw savings: n17=${saving(n17)} Ir; n32=${saving(n32)} Ir; flush-all=${saving(flush)} Ir. Flush-all paired saving=${paired} Ir; required >${2*T} Ir.`,`Dose residual=${dose} Ir; tolerance=max(10; (5/100)*2*${saving(n17)})=${doseTolerance.toFixed(3)} Ir. Dose residual fraction=${ratio(dose,Math.max(1,2*saving(n17)))}.`,`Computed gate: ${verdict}. Failures: ${failures.join('; ')||'none'}.`);
const cacheEnvelope=Math.max(...BENCHES.map(n=>Math.abs(arms.C0.get(n).est-arms.A1.get(n).est)));
table.push('',`Cache-model signal: max absolute C0 Estimated Cycles delta=${cacheEnvelope}; adverse B control deltas: ${controls.filter(n=>arms.B1.get(n).est>arms.A1.get(n).est).map(n=>`${n} +${arms.B1.get(n).est-arms.A1.get(n).est}`).join('; ')}. Secondary model only; no wall-clock/RSS claim.`);
const tableText=table.join('\n'),csvText=csv.join('\n')+'\n';
const reportPath=join(P,'R16_PERF84_FLUSH_ROOT_MASK_GATE.md');
const csvPath=join(P,'R16_PERF84_FLUSH_ROOT_MASK_GATE_summary.csv');
if(process.argv.includes('--emit'))console.log(tableText+'\n---CSV---\n'+csvText);
else if(process.argv.includes('--check')) {
  const report=readFileSync(reportPath,'utf8');
  const begin='<!-- r16_perf84:tables:begin -->',end='<!-- r16_perf84:tables:end -->';
  const i=report.indexOf(begin),j=report.indexOf(end);assert(i>=0&&j>i);
  assert.equal(report.slice(i+begin.length,j).trim(),tableText.trim(),'report table drift');
  assert.equal(readFileSync(csvPath,'utf8'),csvText,'CSV drift');
  console.log(`r16_perf84 gate table/CSV check: OK; ${verdict}`);
} else {
  writeFileSync(csvPath,csvText);
  if(process.argv.includes('--update-report')) {
    const report=readFileSync(reportPath,'utf8');
    const begin='<!-- r16_perf84:tables:begin -->',end='<!-- r16_perf84:tables:end -->';
    const i=report.indexOf(begin),j=report.indexOf(end);assert(i>=0&&j>i);
    writeFileSync(reportPath,report.slice(0,i+begin.length)+'\n'+tableText+'\n'+report.slice(j));
  }
  console.log(tableText);
}
