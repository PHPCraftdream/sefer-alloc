import assert from 'node:assert/strict';
import { readFileSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
const names = ['dealloc_prealloc_only_16b', 'dealloc_free_only_16b_n8', 'dealloc_free_only_16b_n9', 'dealloc_free_only_16b_n16', 'small_churn_16b', 'small_churn_16b_2n', 'mimalloc_small_churn_16b', 'mimalloc_small_churn_16b_2n', 'large_alloc_free_cycle', 'alloc_magazine_hit_only_16b', 'alloc_magazine_prefill_only_16b'];
function parse(text) {
  const rows = {}; let current;
  for (const line of text.split(/\r?\n/)) {
    const header = /^(?:\w+::)+(\w+)\b/.exec(line);
    if (header) current = header[1];
    const metric = /^\s*Instructions:\s*([\d,]+)\s*\|/.exec(line);
    if (metric && current) { assert(!(current in rows), 'duplicate benchmark'); rows[current] = Number(metric[1].replaceAll(',', '')); }
  }
  for (const name of names) assert(Number.isSafeInteger(rows[name]), `missing ${name}`);
  return rows;
}
function aa(first, second) { assert.deepEqual(first, second, 'A/A mismatch'); }
function metrics(r) {
  return { push: [r.dealloc_free_only_16b_n9-r.dealloc_free_only_16b_n8, 1], free_average: [r.dealloc_free_only_16b_n16-r.dealloc_prealloc_only_16b, 16], churn: [r.small_churn_16b_2n-r.small_churn_16b, 64], magazine: [r.alloc_magazine_hit_only_16b-r.alloc_magazine_prefill_only_16b, 16] };
}
function derive(root, oracleRoot) {
  const rows = [], failures = []; const data = {};
  try {
    for (const arm of ['A','B','C']) {
      const text = readFileSync(join(oracleRoot, `_raw_r19_prf02_${arm}.oracle.log`), 'utf8');
      const o = JSON.parse(text.split(/\r?\n/).find(line => line.startsWith('{"arm"')));
      assert.equal(o.arm, arm); assert.equal(o.expected, arm === 'A' ? 32 : 16);
      assert.equal(o.hits+o.misses,o.expected); assert.equal(o.tcache,16); assert.equal(o.n,16); assert.equal(o.preallocated,64); assert.equal(o.routed_scans,0);
      for(const n of ['hits','misses','tcache','routed_scans']) rows.push(['oracle',arm,n,o[n],1,o[n]]);
    }
    for (const axis of ['instrumented','pure']) {
      data[axis] = {};
      for (const arm of ['A','B','C']) {
        const runs = [1,2].map(n => parse(readFileSync(join(root, `_raw_r19_prf02_${arm}_${axis}_${n}.log`),'utf8')));
        if(axis==='instrumented') for(const r of runs) for(const n of ['dealloc_own_thread_body_only_16b','dealloc_segment_base_of_ptr_probe_only_16b']) assert(Number.isSafeInteger(r[n]), `missing ${n}`);
        aa(...runs); data[axis][arm] = runs[0];
        for(const name of names) rows.push([axis,arm,`raw_${name}`,runs[0][name],1,runs[0][name]]);
        if(axis==='instrumented') rows.push([axis,arm,'body_diagnostic',runs[0].dealloc_own_thread_body_only_16b-runs[0].dealloc_segment_base_of_ptr_probe_only_16b,64,(runs[0].dealloc_own_thread_body_only_16b-runs[0].dealloc_segment_base_of_ptr_probe_only_16b)/64]);
        for (const [metric,[numerator,denominator]] of Object.entries(metrics(runs[0]))) rows.push([axis,arm,metric,numerator,denominator,numerator/denominator]);
      }
      const a = data[axis].A,b = data[axis].B,c = data[axis].C;
      const ma=metrics(a),mb=metrics(b),mc=metrics(c);
      const check=(condition,label)=>{if(!condition) failures.push(`${axis}: ${label}`);};
      check(mb.push[0]-ma.push[0] <= (axis==='pure' ? -6 : -8),'B-A push');
      if(axis==='instrumented') {check((mb.churn[0]-ma.churn[0])/64<=-8,'B-A churn'); check(mc.push[0]-mb.push[0]>=-3 && mc.push[0]-mb.push[0]<=1,'C-B push');}
      for(const [left,right,label] of [[a,b,'B-A'],[b,c,'C-B']]) {
        for(const name of ['small_churn_16b','small_churn_16b_2n']) check(right[name]-left[name]<=10,`${label} churn kill`);
        for(const name of ['mimalloc_small_churn_16b','mimalloc_small_churn_16b_2n']) check(right[name]===left[name],`${label} mimalloc`);
        check(Math.abs(right.large_alloc_free_cycle-left.large_alloc_free_cycle)<=10,`${label} Large`);
        check(Math.abs(metrics(right).magazine[0]-metrics(left).magazine[0])<=10,`${label} magazine`);
        for(const name of ['mimalloc_small_churn_16b','mimalloc_small_churn_16b_2n','large_alloc_free_cycle','small_churn_16b','small_churn_16b_2n']) rows.push([axis,label,`raw_${name}`,right[name]-left[name],1,right[name]-left[name]]);
        for(const metric of ['push','free_average','churn','magazine']) {const [ln,d]=metrics(left)[metric], [rn]=metrics(right)[metric]; rows.push([axis,label,metric,rn-ln,d,(rn-ln)/d]);}
      }
    }
  } catch(error) { return {verdict:'NULL', B:'NULL', C:'NULL', reason:error.message, rows }; }
  return {verdict:failures.length?'NO-GO':'GO', B:failures.some(f=>/B-A/.test(f))?'NO-GO':'GO', C:failures.some(f=>/C-B/.test(f))?'NO-GO':'GO',reason:failures.join('; '),rows};
}
const [mode,rootArg,oracleArg] = process.argv.slice(2);
if(mode==='--self-test') {
  const text=names.map((name,i)=>`fixture::group::${name}\n Instructions: ${100+i}|999`).join('\n');
  const parsed=parse(text); assert.equal(parsed[names[0]],100);
  aa(parsed,{...parsed}); assert.throws(()=>aa(parsed,{...parsed,[names[0]]:101}));
  const m=metrics(parsed); assert.equal(m.push[0],1); assert.equal(m.free_average[1],16); assert.equal(m.churn[1],64);
  assert.throws(()=>parse('')); console.log('PASS: absolute first column, completeness, unequal A/A rejection');
} else if(mode==='--derive') {
  const root=resolve(rootArg); const result=derive(root,resolve(oracleArg));
  const metadata=JSON.parse(readFileSync(join(root,'R19_PRF02_OWN_FREE_SINGLE_RESOLVE_GATE_identity.json'),'utf8'));
  const prefix=[metadata.base,metadata.b_patch_sha256,metadata.c_patch_sha256,JSON.stringify(metadata.harness),metadata.environment?.trim(),2];
  const csv=v=>'"'+String(v).replaceAll('"','""')+'"';
  writeFileSync(join(root,'R19_PRF02_OWN_FREE_SINGLE_RESOLVE_GATE_summary.csv'),'base,b_patch,c_patch,harness,compiler_target_cpu_os,samples,features,axis,arm_or_delta,metric,numerator,denominator,value\n'+result.rows.map(r=>[...prefix,r[0]==='pure'?'production internals':'production bench-internals internals',...r].map(csv).join(',')).join('\n')+'\n');
  writeFileSync(join(root,'R19_PRF02_OWN_FREE_SINGLE_RESOLVE_GATE_results.md'),`# perf(runtime) candidate B: ${result.B}; fix(perf) C: ${result.C}; overall ${result.verdict}\n\n${result.reason}\n\nHistorical arm: BLOCKED, no verdict.\n\n| Axis | Arm/delta | Metric | Numerator | Denominator | Value |\n|---|---|---|---:|---:|---:|\n`+result.rows.map(r=>'| '+r.join(' | ')+' |').join('\n')+'\n');
  console.log(result.verdict, result.reason);
} else throw new Error('Usage: --self-test | --derive raw-log-directory oracle-directory');
