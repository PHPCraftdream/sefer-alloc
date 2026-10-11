import assert from 'node:assert/strict';
import { readFileSync, existsSync, writeFileSync } from 'node:fs';
import { resolve, join } from 'node:path';
import { createHash } from 'node:crypto';
import { mkdtempSync, rmSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
const names=['dealloc_prealloc_only_16b','dealloc_free_only_16b_n8','dealloc_free_only_16b_n9','dealloc_free_only_16b_n16','small_churn_16b','small_churn_16b_2n','mimalloc_small_churn_16b','mimalloc_small_churn_16b_2n','large_alloc_free_cycle','alloc_magazine_hit_only_16b','alloc_magazine_prefill_only_16b'];
function parse(text,axis) {
  const rows={}; let current;
  for(const line of text.split(/\r?\n/)) {
    const h=/^(?:\w+::)+(\w+)\b/.exec(line); if(h) current=h[1];
    const m=/^\s*Instructions:\s*([\d,]+)\s*\|/.exec(line);
    if(m&&current) {assert(!(current in rows),'duplicate'); rows[current]=Number(m[1].replaceAll(',',''));}
  }
  const required=[...names,...(axis==='instrumented'?['dealloc_own_thread_body_only_16b','dealloc_segment_base_of_ptr_probe_only_16b']:[])];
  for(const n of required) assert(Number.isSafeInteger(rows[n]),`missing ${n}`);
  return rows;
}
const push=r=>r.dealloc_free_only_16b_n9-r.dealloc_free_only_16b_n8;
const churn=r=>r.small_churn_16b_2n-r.small_churn_16b;
const mag=r=>r.alloc_magazine_hit_only_16b-r.alloc_magazine_prefill_only_16b;
function judge(a,b,axis) {
  const f=[]; const check=(ok,n)=>{if(!ok)f.push(n);};
  check(push(b)-push(a)<=(axis==='pure'?-6:-8),'push');
  if(axis==='instrumented') check(churn(b)-churn(a)<=-8*64,'churn/pair');
  for(const n of ['small_churn_16b','small_churn_16b_2n']) check(b[n]-a[n]<=10,`raw kill ${n}`);
  for(const n of ['mimalloc_small_churn_16b','mimalloc_small_churn_16b_2n']) check(b[n]===a[n],`mimalloc ${n}`);
  check(b.large_alloc_free_cycle-a.large_alloc_free_cycle<=10,'Large one-sided');
  check(Math.abs(mag(b)-mag(a))<=10,'magazine numerator');
  return f;
}
function activation(text,arm) {
  const lines=text.split(/\r?\n/);
  const o=JSON.parse(lines.find(l=>l.startsWith('{"arm"')));
  assert.equal(o.arm,arm); assert.equal(o.expected,arm==='A'?32:16);
  assert.equal(o.hits+o.misses,o.expected); assert.equal(o.tcache,16); assert.equal(o.n,16); assert.equal(o.preallocated,64); assert.equal(o.routed_scans,0);
  const tail=arm==='A'?1:0;
  assert(text.includes(`arm=${arm} large_free=${2+tail}`),'Large prediction mismatch');
  assert(text.includes(`arm=${arm} inplace=1 alloc=0 move=${1+tail} promote=${2+tail} tail=${tail}`),'tail prediction mismatch');
  assert(/test result: ok\. 1 passed; 0 failed/.test(text),'tail test absent/failed');
}
const sha=b=>createHash('sha256').update(b).digest('hex');
const features={pure:'production internals',instrumented:'production internals bench-internals',activation:'production internals bench-internals medium-classes'};
function derive(root) {
  const outputs=['R19_PRF02R_REGATE.md','R19_PRF02R_REGATE_summary.csv'].map(n=>join(root,n));
  for(const out of outputs) assert(!existsSync(out),'report exists');
  const failures=[], rows=[], activations=[];
  const idHash=sha(readFileSync(join(root,'R19_PRF02R_identity.json')));
  const preregHash=sha(readFileSync(join(root,'R19_PRF02R_PREREGISTRATION.md')));
  assert.equal(readFileSync(join(root,'R19_PRF02R_prereg.sha256'),'utf8').trim(),preregHash);
  const receipt=(arm,axis,repeat)=>{
    const name=`_raw_r19_prf02r_${arm}_${axis}_${repeat}.log`;
    const text=readFileSync(join(root,name),'utf8');
    const m=JSON.parse(text.split(/\r?\n/).find(l=>l.startsWith('PRF02R_METADATA '))?.slice(16));
    assert.equal(m.arm,arm); assert.equal(m.axis,axis); assert.equal(m.repeat,repeat);
    assert.equal(m.identity_sha256,idHash); assert.equal(m.prereg_sha256,preregHash);
    assert.equal(m.features,features[axis]);
    assert.deepEqual(m.env,{RUSTC_WRAPPER:'',CARGO_BUILD_JOBS:'3',CARGO_TARGET_DIR:'<repo>/target',R19_PRF02R_ARM:axis==='activation'?arm:null});
    assert(text.endsWith('\nPRF02R_EXIT 0\n'),'command failed');
    return {text,name,metadata:m};
  };
  for(const arm of ['A','B']) for(const repeat of [1,2]) {
    try {
      const r=receipt(arm,'activation',repeat); activation(r.text,arm);
      activations.push([arm,repeat,'PASS',`small=${arm==='A'?32:16}/16; Large=${arm==='A'?3:2}/1; inplace=1/1; alloc=0/1; move=${arm==='A'?2:1}/1; promotion=${arm==='A'?3:2}/1; tail=${arm==='A'?1:0}/1`]);
    } catch(e) {activations.push([arm,repeat,'FAIL',e.message]); failures.push(`activation ${arm}/${repeat}: ${e.message}`);}
  }
  const metric=(axis,arm,repeat,name,numerator,denominator,formula,raw)=>{
    assert(Number.isSafeInteger(numerator)); assert(Number.isSafeInteger(denominator)&&denominator>0);
    const value=numerator/denominator;
    assert.equal(value*denominator,numerator,'arithmetic reconstruction');
    rows.push({axis,arm,repeat,metric:name,numerator,denominator,value,formula,raw});
  };
  // A failed activation is terminal: never require or fabricate iai samples.
  if(!failures.length) try {
    for(const axis of ['pure','instrumented']) {
      const data={};
      for(const arm of ['A','B']) {
        const runs=[];
        for(const repeat of [1,2]) {
          const r=receipt(arm,axis,repeat), v=parse(r.text,axis); runs.push(v);
          for(const [n,count] of Object.entries(v)) metric(axis,arm,repeat,n,count,1,'first absolute Instructions',r.name);
          metric(axis,arm,repeat,'push',push(v),1,'n9-n8',r.name);
          metric(axis,arm,repeat,'churn/pair',churn(v),64,'(2N-N)/64',r.name);
          metric(axis,arm,repeat,'free-average',v.dealloc_free_only_16b_n16-v.dealloc_prealloc_only_16b,16,'(n16-prefix)/16',r.name);
          metric(axis,arm,repeat,'magazine numerator',mag(v),1,'hit-prefill',r.name);
          if(axis==='instrumented') metric(axis,arm,repeat,'body diagnostic',v.dealloc_own_thread_body_only_16b-v.dealloc_segment_base_of_ptr_probe_only_16b,64,'(body-base-probe)/64',r.name);
        }
        assert.deepEqual(runs[0],runs[1],`${axis} ${arm} repeat mismatch`); data[arm]=runs[0];
      }
      failures.push(...judge(data.A,data.B,axis).map(n=>`${axis}: ${n}`));
      const armRows=rows.filter(r=>r.axis===axis&&r.arm==='A');
      for(const a of armRows) {
        const b=rows.find(r=>r.axis===axis&&r.arm==='B'&&r.repeat===a.repeat&&r.metric===a.metric);
        assert.equal(a.denominator,b.denominator);
        metric(axis,'B-A',a.repeat,a.metric,b.numerator-a.numerator,a.denominator,`B-A: ${a.formula}`,`${a.raw};${b.raw}`);
      }
    }
  } catch(e) {failures.push(e.message);}
  const verdict=failures.length?'NO-GO':'GO';
  const escape=v=>`"${String(v).replaceAll('"','""')}"`;
  const columns=['axis','arm','repeat','metric','numerator','denominator','value','formula','raw','identity_sha256','prereg_sha256','features','env'];
  const env='RUSTC_WRAPPER=;CARGO_BUILD_JOBS=3;CARGO_TARGET_DIR=<repo>/target';
  const csv=[columns.join(','),...rows.map(r=>columns.map(k=>escape(({...r,identity_sha256:idHash,prereg_sha256:preregHash,features:features[r.axis],env})[k])).join(','))].join('\n')+'\n';
  const report=`# R19-PRF02-R REGATE — perf(runtime)-кандидат B: ${verdict}\n\nIdentity SHA256: ${idHash}\nPreregistration SHA256: ${preregHash}\nEnvironment: ${env}; activation R19_PRF02R_ARM explicitly A/B.\nFeatures: ${JSON.stringify(features)}\n\nLarge: переписана после наблюдения, доказательством не является.\nThresholds remain exactly as preregistered; no percentages.\n\n## Activation\n\n| Arm | Repeat | Status | Observed counts / denominator or failure |\n|---|---|---|---|\n${activations.map(r=>'| '+r.map(v=>String(v).replaceAll('|','/')).join(' | ')+' |').join('\n')}\n\n## Arithmetic and controls\n\nAssertions enforced when data is available: every numerator is a safe integer, denominator a positive safe integer, and value * denominator equals numerator. Both repeats must match every raw count, including diagnostics. Gate checks: pure push <= -6; instrumented push <= -8; instrumented churn numerator <= -8*64; raw churn N/2N <= +10 on both axes; mimalloc N/2N delta = 0; magazine numerator delta within +/-10; Large one-sided delta <= +10. Free-average and body diagnostic have no added threshold.\n\n${failures.length?'Failures (final NO-GO):\n'+failures.map(f=>'- '+f).join('\n'):'All arithmetic, repeat and gate assertions passed.'}\n\n${activations.some(r=>r[2]==='FAIL')?'IAI not evaluated: failed activation; no measurements invented.':'See CSV for raw counts, every derived numerator/denominator, B-A deltas and receipt provenance. Partial data on failure is not a GO.'}\n\n| Axis | Arm | Repeat | Metric | Numerator | Denominator | Ir |\n|---|---|---|---|---:|---:|---:|\n${rows.map(r=>`| ${r.axis} | ${r.arm} | ${r.repeat} | ${r.metric} | ${r.numerator} | ${r.denominator} | ${r.value} |`).join('\n')}\n`;
  writeFileSync(outputs[0],report,{flag:'wx'}); writeFileSync(outputs[1],csv,{flag:'wx'});
  return {verdict,failures,rows};
}
const [mode,rootArg]=process.argv.slice(2);
if(mode==='--self-test') {
  const a=Object.fromEntries(names.map(n=>[n,1000])); const b={...a,dealloc_free_only_16b_n9:992,small_churn_16b_2n:488};
  assert.deepEqual(judge(a,b,'instrumented'),[]);
  assert(judge(a,{...b,large_alloc_free_cycle:1011},'instrumented').includes('Large one-sided'));
  assert.deepEqual(judge(a,{...b,large_alloc_free_cycle:800},'instrumented'),[]);
  assert(judge(a,{...b,dealloc_free_only_16b_n9:993},'instrumented').includes('push'));
  const fixture=names.map(n=>`x::group::${n}\n Instructions: 1,000|999`).join('\n');
  assert.equal(parse(fixture,'pure').large_alloc_free_cycle,1000); assert.throws(()=>parse('','pure'));
  assert.throws(()=>assert.deepEqual(a,b));
  assert.throws(()=>activation('{"arm":"B","expected":16,"hits":16,"misses":0,"tcache":16,"n":16,"preallocated":64,"routed_scans":0}\narm=B inplace=1 alloc=0 move=2 promote=2 tail=0\ntest result: ok. 1 passed; 0 failed','B'));
  const dir=mkdtempSync(join(fileURLToPath(new URL('../target/r19-prf02r/',import.meta.url)),'prf02r-selftest-'));
  try {
    writeFileSync(join(dir,'R19_PRF02R_identity.json'),'{}');
    writeFileSync(join(dir,'R19_PRF02R_PREREGISTRATION.md'),'fixture');
    writeFileSync(join(dir,'R19_PRF02R_prereg.sha256'),sha('fixture'));
    const emit=(arm,axis,repeat,body,status=0)=>writeFileSync(join(dir,`_raw_r19_prf02r_${arm}_${axis}_${repeat}.log`),`PRF02R_METADATA ${JSON.stringify({arm,axis,repeat,identity_sha256:sha('{}'),prereg_sha256:sha('fixture'),features:features[axis],env:{RUSTC_WRAPPER:'',CARGO_BUILD_JOBS:'3',CARGO_TARGET_DIR:'<repo>/target',R19_PRF02R_ARM:axis==='activation'?arm:null}})}\n${body}\nPRF02R_EXIT ${status}\n`);
    emit('A','activation',1,'failed',1);
    const failed=derive(dir); assert.equal(failed.verdict,'NO-GO'); assert.equal(failed.rows.length,0);
    assert(readFileSync(join(dir,'R19_PRF02R_REGATE.md'),'utf8').includes('no measurements invented'));
    for(const n of ['R19_PRF02R_REGATE.md','R19_PRF02R_REGATE_summary.csv']) rmSync(join(dir,n));
    for(const arm of ['A','B']) for(const repeat of [1,2]) {
      const tail=arm==='A'?1:0, expected=arm==='A'?32:16;
      emit(arm,'activation',repeat,`${JSON.stringify({arm,expected,hits:expected,misses:0,tcache:16,n:16,preallocated:64,routed_scans:0})}\narm=${arm} large_free=${2+tail}\narm=${arm} inplace=1 alloc=0 move=${1+tail} promote=${2+tail} tail=${tail}\ntest result: ok. 1 passed; 0 failed`);
      for(const axis of ['pure','instrumented']) {
        const v={...(arm==='A'?a:b)};
        if(axis==='instrumented') Object.assign(v,{dealloc_own_thread_body_only_16b:3000,dealloc_segment_base_of_ptr_probe_only_16b:1000});
        emit(arm,axis,repeat,Object.entries(v).map(([n,v])=>`x::group::${n}\n Instructions: ${v}|999`).join('\n'));
      }
    }
    const passed=derive(dir); assert.equal(passed.verdict,'GO');
    assert(passed.rows.some(r=>r.arm==='A'&&r.metric==='body diagnostic'&&r.numerator===2000&&r.denominator===64));
    assert(passed.rows.some(r=>r.arm==='B-A'&&r.metric==='churn/pair'&&r.numerator===-512&&r.denominator===64));
    assert.throws(()=>derive(dir),'exclusive outputs');
    for(const n of ['R19_PRF02R_REGATE.md','R19_PRF02R_REGATE_summary.csv']) rmSync(join(dir,n));
    emit('B','pure',2,'bad data'); assert.equal(derive(dir).verdict,'NO-GO');
  } finally {rmSync(dir,{recursive:true,force:true});}
  console.log('PASS: thresholds, asymmetric Large, first absolute column, completeness, repeats, tail rejection, metadata, CSV arithmetic, GO and failed activation NO-GO');
} else if(mode==='--derive') {
  const result=derive(resolve(rootArg)); console.log(result.verdict,result.failures.join('; ')); if(result.failures.length)process.exitCode=1;
} else throw new Error('Usage: --self-test | --derive raw-directory');
