import assert from 'node:assert/strict';
import {readFileSync,writeFileSync,existsSync} from 'node:fs';
import {createHash} from 'node:crypto';
import {resolve,join} from 'node:path';
const names=['dealloc_prealloc_only_16b','dealloc_free_only_16b_n8','dealloc_free_only_16b_n9','dealloc_free_only_16b_n16','small_churn_16b','small_churn_16b_2n','mimalloc_small_churn_16b','mimalloc_small_churn_16b_2n','large_alloc_free_cycle','alloc_magazine_hit_only_16b','alloc_magazine_prefill_only_16b'];
const sha=b=>createHash('sha256').update(b).digest('hex');
const features={pure:'production internals',instrumented:'production internals bench-internals'};
function parse(text,axis) {
 const rows={}; let current;
 for(const line of text.split(/\r?\n/)) {const h=/^(?:\w+::)+(\w+)\b/.exec(line); if(h)current=h[1]; const m=/^\s*Instructions:\s*([\d,]+)\s*\|/.exec(line); if(m&&current) {assert(!(current in rows),'duplicate'); rows[current]=Number(m[1].replaceAll(',',''));}}
 for(const n of [...names,...(axis==='instrumented'?['dealloc_own_thread_body_only_16b','dealloc_segment_base_of_ptr_probe_only_16b']:[])]) assert(Number.isSafeInteger(rows[n]),`missing ${n}`);
 return rows;
}
const push=r=>r.dealloc_free_only_16b_n9-r.dealloc_free_only_16b_n8;
const mag=r=>r.alloc_magazine_hit_only_16b-r.alloc_magazine_prefill_only_16b;
function judge(b,c) {
 const f=[],check=(ok,n)=>{if(!ok)f.push(n);};
 const d=push(c)-push(b); check(d>=-3&&d<=1,'push [-3,+1]');
 for(const n of ['small_churn_16b','small_churn_16b_2n']) check(c[n]-b[n]<=10,`raw churn ${n}`);
 for(const n of ['mimalloc_small_churn_16b','mimalloc_small_churn_16b_2n']) check(c[n]===b[n],`mimalloc ${n}`);
 check(c.large_alloc_free_cycle-b.large_alloc_free_cycle<=10,'Large one-sided'); check(Math.abs(mag(c)-mag(b))<=10,'magazine numerator'); return f;
}
function derive(root) {
 const out=join(root,'R19_PRF02C_RESULT.json'); assert(!existsSync(out),'immutable result exists');
 const failures=[],rows=[],ih=sha(readFileSync(join(root,'R19_PRF02C_identity.json'))),ph=sha(readFileSync(join(root,'R19_PRF02C_PREREGISTRATION.md')));
 assert.equal(readFileSync(join(root,'R19_PRF02C_prereg.sha256'),'utf8').trim(),ph);
 const metric=(axis,arm,repeat,metric,numerator,denominator,raw)=>{assert(Number.isSafeInteger(numerator)); assert(Number.isSafeInteger(denominator)&&denominator>0); const value=numerator/denominator; assert.equal(value*denominator,numerator); rows.push({axis,arm,repeat,metric,numerator,denominator,value,raw});};
 for(const axis of ['pure','instrumented']) try {
  const data={};
  for(const arm of ['B','Cprime']) {
   const runs=[];
   for(const repeat of [1,2]) {
    const raw=`_raw_r19_prf02c_${arm}_${axis}_${repeat}.log`,text=readFileSync(join(root,raw),'utf8');
    const line=text.split(/\r?\n/).find(l=>l.startsWith('PRF02C_METADATA ')); assert(line,'metadata absent'); const m=JSON.parse(line.slice('PRF02C_METADATA '.length));
    assert.deepEqual(m,{arm,axis,repeat,identity_sha256:ih,prereg_sha256:ph,features:features[axis],env:{RUSTC_WRAPPER:'',CARGO_BUILD_JOBS:'3',CARGO_TARGET_DIR:'<repo>/target'}});
    assert(text.endsWith('\nPRF02C_EXIT 0\n'),'failed receipt'); const v=parse(text,axis); runs.push(v);
    for(const [n,count] of Object.entries(v)) metric(axis,arm,repeat,n,count,1,raw);
    metric(axis,arm,repeat,'push',push(v),1,raw);
    metric(axis,arm,repeat,'churn/pair',v.small_churn_16b_2n-v.small_churn_16b,64,raw);
    metric(axis,arm,repeat,'free-average',v.dealloc_free_only_16b_n16-v.dealloc_prealloc_only_16b,16,raw);
    metric(axis,arm,repeat,'magazine numerator',mag(v),1,raw);
    if(axis==='instrumented') metric(axis,arm,repeat,'body diagnostic',v.dealloc_own_thread_body_only_16b-v.dealloc_segment_base_of_ptr_probe_only_16b,64,raw);
   }
   assert.deepEqual(runs[0],runs[1],`${axis}/${arm} repeat mismatch`); data[arm]=runs[0];
  }
  failures.push(...judge(data.B,data.Cprime).map(f=>`${axis}: ${f}`));
  for(const b of rows.filter(r=>r.axis===axis&&r.arm==='B')) {const c=rows.find(r=>r.axis===axis&&r.arm==='Cprime'&&r.repeat===b.repeat&&r.metric===b.metric); assert.equal(b.denominator,c.denominator); metric(axis,'Cprime-B',b.repeat,b.metric,c.numerator-b.numerator,b.denominator,`${b.raw};${c.raw}`);}
 } catch(e) {failures.push(`${axis}: ${e.message}`);}
 const result={verdict:failures.length?'NO-GO':'GO',identity_sha256:ih,prereg_sha256:ph,failures,rows,rule3:'On NO-GO remove all Cprime hook/inventory/contract/test/source changes; preserve external snapshots; tree remains B.'};
 writeFileSync(out,JSON.stringify(result,null,2)+'\n',{flag:'wx'});
 // Render both reports from the persisted judge JSON, never a separate table.
 const persisted=JSON.parse(readFileSync(out,'utf8'));
 const columns=['axis','arm','repeat','metric','numerator','denominator','value','raw'];
 const csv=v=>'"'+String(v).replaceAll('"','""')+'"';
 writeFileSync(join(root,'R19_PRF02C_RESULT_summary.csv'),columns.join(',')+'\n'+persisted.rows.map(r=>columns.map(k=>csv(r[k])).join(',')).join('\n')+'\n',{flag:'wx'});
 const md='# R19 PRF02C derived result\n\nExact verdict: **'+persisted.verdict+'**\n\n'+persisted.failures.map(f=>'- '+f).join('\n')+'\n\n| '+columns.join(' | ')+' |\n| '+columns.map(()=>'---').join(' | ')+' |\n'+persisted.rows.map(r=>'| '+columns.map(k=>String(r[k]).replaceAll('|','\\|')).join(' | ')+' |').join('\n')+'\n';
 writeFileSync(join(root,'R19_PRF02C_RESULT.md'),md,{flag:'wx'});
 const attempt=join(root,'R19_PRF02C_ATTEMPT.md');
 writeFileSync(attempt,readFileSync(attempt,'utf8')+'\n## Measured judge verdict\n\nExact verdict: **'+persisted.verdict+'**. Numeric rows and all ratio numerators/denominators are derived from R19_PRF02C_RESULT.json in R19_PRF02C_RESULT.md and R19_PRF02C_RESULT_summary.csv.\n');
 return persisted;
}
const [mode,rootArg]=process.argv.slice(2);
if(mode==='--self-test') {
 const b=Object.fromEntries(names.map(n=>[n,1000]));
 for(const delta of [-3,0,1]) assert.deepEqual(judge(b,{...b,dealloc_free_only_16b_n9:1000+delta}),[]);
 for(const delta of [-4,2]) assert(judge(b,{...b,dealloc_free_only_16b_n9:1000+delta}).includes('push [-3,+1]'));
 assert(judge(b,{...b,small_churn_16b:1011}).length); assert(judge(b,{...b,small_churn_16b_2n:1011}).length);
 assert(judge(b,{...b,mimalloc_small_churn_16b:999}).length); assert(judge(b,{...b,large_alloc_free_cycle:1011}).length);
 assert.deepEqual(judge(b,{...b,large_alloc_free_cycle:800}),[]); assert(judge(b,{...b,alloc_magazine_hit_only_16b:1011}).length);
 assert.throws(()=>parse('','pure')); assert.equal(parse(names.map(n=>`x::group::${n}\n Instructions: 1,000|999`).join('\n'),'pure').large_alloc_free_cycle,1000);
 console.log('PASS: BOTH-axis thresholds [-3,+1], raw N/2N kills, original controls, first absolute column; no cargo');
} else if(mode==='--derive') {const r=derive(resolve(rootArg)); console.log(r.verdict,r.failures.join('; ')); if(r.failures.length)process.exitCode=1;}
else throw new Error('Usage: --self-test | --derive raw-doc-directory');
