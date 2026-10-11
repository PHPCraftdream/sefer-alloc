// Measurement-only gate. --run is explicit opt-in; self-tests never measure.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFileSync, writeFileSync, existsSync, mkdirSync, statSync } from 'node:fs';
import { resolve, dirname, relative } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';
import os from 'node:os';
const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const digest = b => createHash('sha256').update(b).digest('hex');
const natural = x => assert(Number.isSafeInteger(x) && x >= 0);
export function median(xs) {
  assert(xs.length); assert(xs.every(Number.isFinite));
  const s = [...xs].sort((a,b) => a-b), m = Math.floor(s.length/2);
  return s.length % 2 ? s[m] : (s[m-1]+s[m])/2;
}
export function nearestRank(xs, q) {
  assert(xs.length && q > 0 && q <= 1); assert(xs.every(Number.isFinite));
  return [...xs].sort((a,b) => a-b)[Math.ceil(q*xs.length)-1];
}
// Two-sided Student t survival by regularized incomplete beta, continued fraction.
function logGamma(z) {
  const c = [676.5203681218851,-1259.1392167224028,771.32342877765313,-176.61502916214059,12.507343278686905,-0.13857109526572012,9.984369578019572e-6,1.5056327351493116e-7];
  if (z < .5) return Math.log(Math.PI)-Math.log(Math.sin(Math.PI*z))-logGamma(1-z);
  z--; let x=.99999999999980993; c.forEach((v,i) => x+=v/(z+i+1));
  const t=z+7.5; return .5*Math.log(2*Math.PI)+(z+.5)*Math.log(t)-t+Math.log(x);
}
function betaFraction(a,b,x) {
  const tiny=1e-300; let c=1,d=1-(a+b)*x/(a+1); if(Math.abs(d)<tiny)d=tiny; d=1/d; let h=d;
  for(let m=1;m<=500;m++) {
    let aa=m*(b-m)*x/((a+2*m-1)*(a+2*m));
    d=1+aa*d; if(Math.abs(d)<tiny)d=tiny; c=1+aa/c; if(Math.abs(c)<tiny)c=tiny; d=1/d; h*=d*c;
    aa=-(a+m)*(a+b+m)*x/((a+2*m)*(a+2*m+1));
    d=1+aa*d; if(Math.abs(d)<tiny)d=tiny; c=1+aa/c; if(Math.abs(c)<tiny)c=tiny; d=1/d;
    const step=d*c; h*=step; if(Math.abs(step-1)<3e-14)return h;
  }
  throw Error('beta did not converge');
}
function beta(a,b,x) {
  if(x===0 || x===1)return x;
  const f=Math.exp(logGamma(a+b)-logGamma(a)-logGamma(b)+a*Math.log(x)+b*Math.log1p(-x));
  return x<(a+1)/(a+b+2) ? f*betaFraction(a,b,x)/a : 1-f*betaFraction(b,a,1-x)/b;
}
export function pairedT(a,b) {
  assert.equal(a.length,b.length); assert(a.length>=2);
  const ds=a.map((v,i) => { assert(Number.isFinite(v)&&Number.isFinite(b[i])); return b[i]-v; });
  const mean=ds.reduce((x,y)=>x+y,0)/ds.length;
  const variance=ds.reduce((x,y)=>x+(y-mean)**2,0)/(ds.length-1);
  const t=variance===0 ? (mean===0 ? 0 : Math.sign(mean)*Infinity) : mean/Math.sqrt(variance/ds.length);
  return {mean,t,p:t===0 ? 1 : beta((ds.length-1)/2,.5,(ds.length-1)/(ds.length-1+t*t)),n:ds.length};
}
export function validate(r) {
  assert.equal(r.schema,2); assert(['A','B'].includes(r.arm));
  natural(r.k); natural(r.w); assert(r.k>0&&r.w>0);
  assert(['wave','live','churn'].includes(r.mode));
  assert.equal(typeof r.diagnostic,'boolean');
  if(r.mode==='wave')assert(([2,8,32].includes(r.k)&&[16,128].includes(r.w))||(r.k===2&&r.w===2));
  if(r.mode==='live')assert([4,64].includes(r.k)&&r.w===1);
  if(r.mode==='churn')assert(r.k===1&&[20,2000].includes(r.w));
  assert.equal(r.maintenance_running,false); assert.equal(r.requested_config,'LargeCacheConfig::new');
  assert.equal(r.config_conflicts_before,0); assert.equal(r.config_conflicts_after,0);
  assert.equal(r.count_before,1); natural(r.count_delta);
  assert.equal(r.count_after-r.count_before,r.count_delta);
  if(r.mode==='wave') {
    if(r.arm==='A')assert.equal(r.count_delta,1+r.w*(r.k-1));
    else assert(r.count_delta<=r.k+1);
  } else if(r.mode==='live')assert.equal(r.count_delta,r.k);
  else assert(r.count_delta<=2);
  for(const f of ['rss_before','rss_idle','rss_peak_sampled','segments_reserved_delta'])natural(r[f]);
  assert(r.rss_idle>0&&r.rss_peak_sampled>=r.rss_idle);
  for(const f of ['commit_before','commit_idle','commit_peak_sampled'])if(r[f]!==null)natural(r[f]);
  assert.equal(typeof r.observer,'boolean');
  assert.equal(r.observer_status,r.observer?'OBSERVED':'NONE');
  if(r.observer) {
    assert.equal(r.first_call_metric,'first allocating call, includes TLS bind/claim');
    assert(Array.isArray(r.pre_first_alloc_deallocs));
    assert.equal(r.first_call_samples.length,r.k*r.w);
    r.first_call_samples.forEach(v=>{assert.equal(v.length,5);v.forEach(natural);assert.equal(v[0],1);assert(v[2]>0);});
    assert.deepEqual(r.pre_first_alloc_deallocs,r.first_call_samples.map(v=>v[3]),'pre-first dealloc counter mismatch');
    const valid=r.mode!=='live'||r.first_call_samples.every(v=>v[1]>v[4]);
    assert.equal(r.live_valid,valid);
  } else { assert.equal(r.first_call_metric,null); assert.equal(r.pre_first_alloc_deallocs,null); assert.equal(r.first_call_samples,null); assert.equal(r.live_valid,true); }

  assert(!('first_bind_ns' in r)&&!('first_workload_alloc_ns' in r),'no surrogate latency');
  if(r.diagnostic) {
    assert.equal(r.wall_ns,null); assert.equal(r.claim_sources_len,r.claim_sources.length);
    assert.equal(r.claim_sources_len,r.arm==='A'?4:5);
    assert.equal(r.missing_fifth_explicit_zero,r.claim_sources_len===4);
    if(r.observer)assert.equal(r.first_call_samples.reduce((sum,v)=>sum+v[0],0),r.claim_sources.reduce((a,b)=>a+b,0));
    r.claim_sources.forEach(natural); assert.equal(r.claim_sources.reduce((a,b)=>a+b,0),r.k*r.w);
    natural(r.free_set); assert.equal(r.free_set,r.claim_sources[4]??0);
    const free=r.free_set;
    if(r.arm==='B')assert.equal(r.claim_sources_len,5);
    if(r.mode==='wave'&&r.arm==='B') {
      assert(free+r.claim_sources[0]>=(r.w-1)*r.k-1); assert(r.claim_sources[1]<=r.k+1);
    }
    if(r.mode==='live')assert.equal(free,0);
    assert.deepEqual(r.resolved_config_results,[r.k*r.w,0]); natural(r.routed_scans);
  } else {
    natural(r.wall_ns); assert(r.wall_ns>0);
    for(const f of ['claim_sources','claim_sources_len','free_set','missing_fifth_explicit_zero','resolved_config_results','routed_scans'])assert.equal(r[f],null);
  }
}
function identities(identity, rows) {
  assert(/^[a-f0-9]{64}$/.test(identity.prereg_sha256),'prereg SHA256 format');
  assert.equal(identity.schema,2); natural(identity.captured_at_ms);
  for(const arm of ['A','B']) {
    const id=identity.arms[arm]; assert.equal(id.label,arm);
    assert(/^[a-f0-9]{40}$/.test(id.commit)); assert(/^[a-f0-9]{40}$/.test(id.tree));
    for(const mode of ['production','diagnostic','observer_production','observer_diagnostic'])assert(/^[a-f0-9]{64}$/.test(id.binaries[mode]));
    assert(/^[a-f0-9]{64}$/.test(id.patch_sha256));
    for(const file of harnessFiles)assert(/^[a-f0-9]{64}$/.test(id.harness[file]));
    assert.equal(id.profile,'release'); assert.equal(id.locked,true);
    assert.equal(id.production_features,'production');
    assert.equal(id.diagnostic_features,'production bench-internals alloc-stats internals');
    assert(typeof id.rustc==='string'&&typeof id.target==='string');
  }
  assert.notEqual(identity.arms.A.commit,identity.arms.B.commit);
  for(const key of ['rustc','target','cargo'])assert.equal(identity.arms.A[key],identity.arms.B[key],`${key} mismatch between arms`);
  assert.deepEqual(identity.arms.A.metadata,identity.arms.B.metadata,'resolved feature mismatch between arms');
  assert.deepEqual(identity.arms.A.harness,identity.arms.B.harness,'wrapper/workload mismatch');
  rows.forEach(r=> {
    const id=identity.arms[r.arm]; assert.equal(r.commit,id.commit); assert.equal(r.tree,id.tree);
    assert.equal(r.binary_sha256,id.binaries[(r.observer?'observer_':'')+(r.diagnostic?'diagnostic':'production')]);
    assert.equal(r.patch_sha256,id.patch_sha256);
    natural(r.sample_started_at_ms); assert(r.sample_started_at_ms>identity.captured_at_ms);
  });
}
export function intendedWaveP99(runs) {
  assert.equal(runs.length,5);
  return median(runs.map(xs=>nearestRank(xs,.99)));
}
export function intendedBenefit(a,b) {
  if(a===null||b===null)return {missing:true,benefit:false};
  natural(a); natural(b); assert(a>0&&b>0);
  const numerator=a-b,denominator=a;
  assert.equal(numerator+b,a);
  return {missing:false,benefit:b<=a-Math.max(67108864,.25*a),numerator,denominator};
}
export function intendedSchedule(pairs=20) {
  assert(Number.isInteger(pairs)&&pairs>=20);
  return Array.from({length:pairs},(_,i)=>({pair:i+1,ab:['A','B','B','A'],aa:['A','A','A','A']}));
}
export function medianNoise(pairs) {
  assert(pairs.length&&pairs.every(([x,y])=>Number.isFinite(x)&&Number.isFinite(y)&&x>0&&y>0));
  const first=median(pairs.map(p=>p[0])),second=median(pairs.map(p=>p[1]));
  const numerator=second-first,value=numerator/first; assert(Math.abs(value*first-numerator)<1e-8*Math.max(1,Math.abs(numerator)));
  return Math.abs(value);
}
// `widen` is KILL-2 only: the bound is max(limit, A/A noise) and noise never makes the axis inconclusive.
export function intendedAxis(a,b,aa,limit,absolute=false,widen=false) {
  assert(a.length>=20&&a.length===b.length&&aa.length>=20);
  assert(a.every(x=>Number.isFinite(x)&&x>0)&&b.every(x=>Number.isFinite(x)&&x>0));
  const noise=medianNoise(aa), numerator=median(b)-median(a), denominator=median(a);
  const ratio=numerator/denominator; assert(Math.abs(ratio*denominator-numerator)<1e-8*Math.max(1,Math.abs(numerator)));
  const t=pairedT(a,b), bound=widen?Math.max(limit,noise):limit;
  return {numerator,denominator,ratio,noise,bound,t,
    inconclusive:!widen&&noise>limit,
    kill:(widen||noise<=limit)&&((absolute?Math.abs(ratio):ratio)>bound||(absolute&&t.mean>0&&t.p<.05))};
}
export function decide({kill=false,missing=false,noise=false,observer=false,oracles=false,benefit=false}) {
  if(kill)return 'NO-GO';
  if(missing||noise||!observer||!oracles)return 'INCONCLUSIVE';
  return benefit?'GO':'NULL';
}
function safeText(s) { assert(!/[A-Za-z]:[\\/](?:Users|dev|home)[\\/]|\\\\[A-Za-z0-9_.-]+\\[A-Za-z0-9_.-]+|\/(?:Users|home|dev)\/[A-Za-z0-9_.-]+\//i.test(s),'private path'); return s; }
function selfTest() {
  let checks=0; const check=f=>{f();checks++;};
  check(()=>assert.equal(median([5,1,4,2,3]),3));
  check(()=>assert.equal(median([1,4,2,3]),2.5));
  check(()=>assert.equal(nearestRank(Array.from({length:100},(_,i)=>i+1),.99),99));
  check(()=>{const t=pairedT([0,0,0,0,0],[1,2,3,4,5]);assert(Math.abs(t.t-4.242640687)<1e-8);assert(Math.abs(t.p-.01323559956)<1e-8);});
  check(()=>assert.equal(pairedT([1,2],[1,2]).p,1));
  const r={schema:2,arm:'A',mode:'wave',k:2,w:2,diagnostic:true,maintenance_running:false,requested_config:'LargeCacheConfig::new',config_conflicts_before:0,config_conflicts_after:0,count_before:1,count_after:4,count_delta:3,rss_before:1,rss_idle:2,rss_peak_sampled:2,segments_reserved_delta:3,commit_before:1,commit_idle:2,commit_peak_sampled:2,observer_status:'NONE',observer:false,first_call_metric:null,pre_first_alloc_deallocs:null,first_call_samples:null,live_valid:true,wall_ns:null,claim_sources:[1,3,0,0],claim_sources_len:4,free_set:0,missing_fifth_explicit_zero:true,resolved_config_results:[4,0],routed_scans:0};
  check(()=>validate(r));
  const b={...r,arm:'B',claim_sources:[1,2,0,0,1],claim_sources_len:5,free_set:1,missing_fifth_explicit_zero:false};
  check(()=>validate(b));
  const production={...r,diagnostic:false,wall_ns:1,claim_sources:null,claim_sources_len:null,free_set:null,missing_fifth_explicit_zero:null,resolved_config_results:null,routed_scans:null};
  check(()=>validate(production));
  const observed={...production,observer:true,observer_status:'OBSERVED',first_call_metric:'first allocating call, includes TLS bind/claim',pre_first_alloc_deallocs:[0,1,2,3],first_call_samples:[0,1,2,3].map(n=>[1,2,100,n,0])};
  check(()=>validate(observed));
  for(const mutate of [r=>r.first_call_metric='first bind',r=>delete r.first_call_metric,r=>delete r.pre_first_alloc_deallocs,r=>r.pre_first_alloc_deallocs[0]++,r=>r.first_call_samples[0][3]++,r=>r.pre_first_alloc_deallocs.pop()])check(()=>{const bad=structuredClone(observed);mutate(bad);assert.throws(()=>validate(bad));});
  for(const fixture of [r,production])for(const field of ['first_call_metric','pre_first_alloc_deallocs'])for(const value of [undefined,'',[]])check(()=>assert.throws(()=>validate({...fixture,[field]:value})));
  for(const fixture of [r,b,production])for(const value of [undefined,null,0,1,-1,.5]) {
    if(value===fixture.free_set)continue;
    check(()=>assert.throws(()=>validate({...fixture,free_set:value})));
  }
  check(()=>{const bad={...r};delete bad.free_set;assert.throws(()=>validate(bad));});
  check(()=>{const bad={...r,claim_sources_length:4};delete bad.claim_sources_len;assert.throws(()=>validate(bad));});
  for(const mutate of [r=>r.count_delta++,r=>r.config_conflicts_after++,r=>r.maintenance_running=true,r=>r.claim_sources[0]++,r=>r.resolved_config_results[1]++,r=>r.wall_ns=1,r=>r.missing_fifth_explicit_zero=false,r=>r.first_bind_ns=[1]])check(()=>{const bad=structuredClone(r);mutate(bad);assert.throws(()=>validate(bad));});
  check(()=>assert.equal(decide({observer:true,oracles:true,missing:true}),'INCONCLUSIVE'));
  check(()=>assert.equal(decide({observer:true,oracles:true,benefit:true,noise:true}),'INCONCLUSIVE'));
  check(()=>assert.equal(decide({kill:true,missing:true}),'NO-GO'));
  check(()=>assert.equal(decide({observer:true,oracles:true}),'NULL'));
  check(()=>assert.equal(decide({observer:true,oracles:true,benefit:true}),'GO'));
  check(()=>assert.equal(decide({benefit:true,oracles:true}),'INCONCLUSIVE'));
  check(()=>assert.throws(()=>identities({schema:2,captured_at_ms:0,arms:{}},[])));
  check(()=>assert.throws(()=>safeText(['','home','synthetic-user','artifact'].join('/'))));
  check(()=>assert.equal(digest('abc'),'ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad'));
  check(()=>assert.equal(intendedWaveP99([[1],[2],[3],[4],[100]]),3));
  check(()=>assert(intendedBenefit(null,1).missing));
  check(()=>assert(intendedBenefit(268435456,201326592).benefit));
  check(()=>assert(!intendedBenefit(268435456,201326593).benefit));
  check(()=>assert.equal(intendedSchedule().length,20));
  check(()=>assert.deepEqual(intendedSchedule()[0].ab,['A','B','B','A']));
  check(()=>assert.throws(()=>intendedSchedule(19)));
  check(()=>assert(intendedAxis(Array(20).fill(100),Array(20).fill(101),Array(20).fill([100,100]),.05,true).kill));
  check(()=>assert(intendedAxis(Array(20).fill(100),Array(20).fill(100),Array(20).fill([100,106]),.05,true).inconclusive));
  check(()=>assert(!intendedAxis(Array(20).fill(100),Array(20).fill(100),[...Array(19).fill([100,100]),[100,300]],.05,true).inconclusive)); // one outlier pair does not move the median
  check(()=>assert.equal(medianNoise([[100,110],[100,90],[100,100]]),0));
  check(()=>assert(Math.abs(medianNoise(Array(5).fill([100,106]))-.06)<1e-12));
  check(()=>assert(!intendedAxis(Array(20).fill(100),Array(20).fill(103),Array(20).fill([100,104]),.02,false,true).kill)); // churn: bound widens to the A/A noise
  check(()=>assert(intendedAxis(Array(20).fill(100),Array(20).fill(105),Array(20).fill([100,104]),.02,false,true).kill));
  check(()=>assert(!intendedAxis(Array(20).fill(100),Array(20).fill(100),Array(20).fill([100,104]),.02,false,true).inconclusive));
  check(()=>assert(!intendedAxis(Array(20).fill(100),Array(20).fill(101),Array(20).fill([100,100]),.02).kill));
  check(()=>assert(intendedAxis(Array(20).fill(100),Array(20).fill(103),Array(20).fill([100,100]),.02).kill));
  const id={schema:2,captured_at_ms:1,prereg_sha256:digest('synthetic prereg'),arms:Object.fromEntries(['A','B'].map((arm,i)=>[arm,{label:arm,commit:String(i+1).repeat(40),tree:'3'.repeat(40),patch_sha256:'6'.repeat(64),harness:Object.fromEntries(harnessFiles.map(f=>[f,'7'.repeat(64)])),binaries:{production:'4'.repeat(64),diagnostic:'5'.repeat(64),observer_production:'8'.repeat(64),observer_diagnostic:'9'.repeat(64)},profile:'release',locked:true,production_features:'production',diagnostic_features:'production bench-internals alloc-stats internals',rustc:'synthetic',target:'synthetic'}]))};
  const identityRow={...r,patch_sha256:id.arms.A.patch_sha256,commit:id.arms.A.commit,tree:id.arms.A.tree,binary_sha256:id.arms.A.binaries.diagnostic,sample_started_at_ms:2};
  check(()=>identities(id,[identityRow]));
  for(const mutate of [r=>r.commit='0'.repeat(40),r=>r.tree='0'.repeat(40),r=>r.binary_sha256='0'.repeat(64),r=>r.sample_started_at_ms=0])check(()=>{const bad=structuredClone(identityRow);mutate(bad);assert.throws(()=>identities(id,[bad]));});
  checks+=extendedTests();
  console.log(`PRF04 gate2b self-test PASS: ${checks} synthetic checks; zero measurements`);
}

const harnessFiles=['examples/r19_prf04_gate2b.rs','examples/r19_prf04_gate2b_observer.rs','examples/r19_prf04_gate2b_shared/harness.rs','scripts/r19_prf04_gate2b.mjs'];
export function schedule() {
  const jobs=[];
  const add=(arm,mode,k,w,observer,diagnostic,series,repetition,position=0)=>jobs.push({arm,mode,k,w,observer,diagnostic,series,repetition,position,index:jobs.length});
  for(let rep=1;rep<=5;rep++)for(const k of [2,8,32])for(const w of [16,128])for(const arm of rep%2?['A','B']:['B','A']) {
    add(arm,'wave',k,w,false,false,'wave',rep);
    add(arm,'wave',k,w,true,false,'wave',rep);
  }
  // Independent same-A controls for retention and observer p99, five pairs each.
  for(let rep=1;rep<=5;rep++)for(const observer of [false,true])for(let pos=0;pos<2;pos++)add('A','wave',32,128,observer,false,'aa_wave',rep,pos);
  for(const [mode,k,w,observer] of [['live',64,1,true],['churn',1,2000,false]])for(let rep=1;rep<=20;rep++) {
    for(const [series,arms] of [['ab',['A','B','B','A']],['aa',['A','A','A','A']]])arms.forEach((arm,pos)=>add(arm,mode,k,w,observer,false,series,rep,pos));
  }
  for(const [mode,k,w] of [...[2,8,32].flatMap(k=>[16,128].map(w=>['wave',k,w])),['live',64,1],['churn',1,2000]])for(const arm of ['A','B'])for(const observer of [false,true])add(arm,mode,k,w,observer,true,'diagnostic',1);
  assert.equal(jobs.length,492);
  return jobs;
}
const ratio=(a,b)=>{assert(a>0&&Number.isFinite(a)&&Number.isFinite(b));const numerator=b-a,denominator=a,value=numerator/denominator;assert(Math.abs(value*denominator-numerator)<1e-8*Math.max(1,Math.abs(numerator)));return {numerator,denominator,ratio:value};};
export function aaNoise(pairs) { assert(pairs.length);return Math.max(...pairs.map(([a,b])=>Math.abs(ratio(a,b).ratio))); }
export function aggregateRows(identity,rows) {
  const expected=schedule();assert.equal(rows.length,expected.length,'exact schedule cardinality');
  rows.forEach((r,i)=>{validate(r);for(const [key,value] of Object.entries(expected[i]))assert.equal(r[key],value,`schedule ${i} ${key}`);});
  identities(identity,rows);
  const select=criteria=>rows.filter(r=>Object.entries(criteria).every(([k,v])=>r[k]===v));
  const primary=(arm,observer)=>select({arm,mode:'wave',k:32,w:128,observer,diagnostic:false,series:'wave'});
  const commits=arm=>{const rs=primary(arm,false);assert.equal(rs.length,5);return rs.some(r=>r.commit_idle===null)?null:median(rs.map(r=>r.commit_idle));};
  const a=commits('A'),b=commits('B'),benefit=intendedBenefit(a,b);
  const control=observer=>{const rs=select({series:'aa_wave',observer});assert.equal(rs.length,10);return Array.from({length:5},(_,i)=>[rs[i*2],rs[i*2+1]]);};
  const commitPairs=control(false).map(pair=>pair.map(r=>r.commit_idle));
  const commitMissing=commitPairs.some(pair=>pair.includes(null));
  const commitNoise=commitMissing?null:aaNoise(commitPairs);
  if(!benefit.missing){benefit.ratio=benefit.numerator/benefit.denominator;assert(Math.abs(benefit.ratio*benefit.denominator-benefit.numerator)<1e-8*Math.max(1,Math.abs(benefit.numerator)));}
  // Each AA numerator A2-A1 and denominator A1; compare byte noise to its
  // own preregistered max(64MiB,25%*A1), never to RSS or an AB denominator.
  const commitNoisy=!commitMissing&&commitPairs.some(([x,y])=>Math.abs(y-x)>Math.max(67108864,.25*x));
  const p99=r=>nearestRank(r.first_call_samples.map(v=>v[2]),.99);
  const waveA=intendedWaveP99(primary('A',true).map(r=>r.first_call_samples.map(v=>v[2])));
  const waveB=intendedWaveP99(primary('B',true).map(r=>r.first_call_samples.map(v=>v[2])));
  const waveNoise=medianNoise(control(true).map(pair=>pair.map(p99)));
  const wave={...ratio(waveA,waveB),noise:waveNoise,inconclusive:waveNoise>.10,kill:waveNoise<=.10&&waveB>1.10*waveA};
  const paired=(mode,metric,limit,absolute,widen=false)=>{
    const ab=select({mode,series:'ab'}),aa=select({mode,series:'aa'});assert.equal(ab.length,80);assert.equal(aa.length,80);
    const av=[],bv=[],controls=[];
    for(let i=0;i<20;i++) {
      const group=ab.slice(i*4,i*4+4),same=aa.slice(i*4,i*4+4);
      // Pair outer A0 with B1 and inner A3 with B2. AAAA uses 0/1 and 3/2.
      for(const [x,y] of [[0,1],[3,2]]){av.push(metric(group[x]));bv.push(metric(group[y]));controls.push([metric(same[x]),metric(same[y])]);}
    }
    const invalid=select({mode}).some(r=>r.observer&&!r.live_valid);
    const axis=intendedAxis(av,bv,controls,limit,absolute,widen);
    return {...axis,inconclusive:axis.inconclusive||invalid,kill:axis.kill&&!invalid,invalid,pairs:av.length,t_stat:axis.t.t,p_value:axis.t.p,mean_delta:axis.t.mean};
  };
  const live=paired('live',r=>nearestRank(r.first_call_samples.map(v=>v[2]),.5),.05,true);
  const churn=paired('churn',r=>r.wall_ns,.02,false,true);
  const missing=benefit.missing||commitMissing;
  const noise=commitNoisy||wave.inconclusive||live.inconclusive||churn.inconclusive;
  const verdict=decide({kill:wave.kill||live.kill||churn.kill,missing,noise,observer:true,oracles:true,benefit:benefit.benefit});
  const axes=[{axis:'commit',a,b,...benefit,noise:commitNoise,inconclusive:missing||commitNoisy,kill:false}, {axis:'live',...live}, {axis:'churn',...churn}, {axis:'wave_p99',...wave}];
  const cells=[];
  for(const arm of ['A','B'])for(const k of [2,8,32])for(const w of [16,128]) {
    const rs=select({arm,k,w,mode:'wave',observer:false,diagnostic:false,series:'wave'});
    const obs=select({arm,k,w,mode:'wave',observer:true,diagnostic:false,series:'wave'});
    assert.equal(rs.length,5);assert.equal(obs.length,5);
    cells.push({arm,k,w,n:5,commit_idle_median:rs.some(r=>r.commit_idle===null)?null:median(rs.map(r=>r.commit_idle)),rss_idle_median:median(rs.map(r=>r.rss_idle)),wall_ns_median:median(rs.map(r=>r.wall_ns)),first_call_p99_median:median(obs.map(p99))});
  }
  const cellColumns=Object.keys(cells[0]);
  const cellMd='\n| '+cellColumns.join(' | ')+' |\n| '+cellColumns.map(()=>'---').join(' | ')+' |\n'+cells.map(r=>'| '+cellColumns.map(k=>r[k]??'unavailable').join(' | ')+' |').join('\n')+'\n';
  const columns=['axis','arm','k','w','n','commit_idle_median','rss_idle_median','wall_ns_median','first_call_p99_median','numerator','denominator','ratio','noise','bound','pairs','mean_delta','t_stat','p_value','inconclusive','kill'];
  const tableRows=[...axes,...cells.map(c=>({axis:c.k===32&&c.w===128?'primary_cell':'secondary_cell',...c}))];
  const csv=columns.join(',')+'\n'+tableRows.map(r=>columns.map(k=>r[k]??'unavailable').join(',')).join('\n')+'\n';
  const md='# PRF04 gate2b\n\n'+verdict+'\n\n| '+columns.join(' | ')+' |\n| '+columns.map(()=>'---').join(' | ')+' |\n'+axes.map(r=>'| '+columns.map(k=>r[k]??'unavailable').join(' | ')+' |').join('\n')+'\n\nRatios: (B-A)/A; commit savings (A-B)/A. AA noise: |median(A2)-median(A1)|/median(A1) (commit: per-pair bytes vs max(64MiB,25%*A1)). Diagnostic timings excluded. Secondary cells excluded from verdict.\n';
  return {csv,markdown:md+cellMd,result:{verdict,benefit:benefit.benefit,missing_commit:missing,axes,cells,samples:rows.length}};
}
function put(dir,name,text) { writeFileSync(resolve(dir,name),safeText(text),{flag:'wx'}); }
function verifyPrereg(identity,bytes) {
  assert(/^[a-f0-9]{64}$/.test(identity.prereg_sha256),'prereg SHA256 format');
  assert.equal(digest(bytes),identity.prereg_sha256,'prereg artifact mismatch');
}
function aggregate(dir) {
  for(const name of ['summary.csv','report.md','verdict.json'])assert(!existsSync(resolve(dir,name)),'no overwrite');
  const identity=JSON.parse(safeText(readFileSync(resolve(dir,'identity.json'),'utf8')));
  verifyPrereg(identity,readFileSync(resolve(dir,'prereg.md')));
  for(const arm of ['A','B'])assert.equal(digest(readFileSync(resolve(dir,`${arm}.source.patch`))),identity.arms[arm].patch_sha256);
  const rows=JSON.parse(safeText(readFileSync(resolve(dir,'samples.json'),'utf8')));
  const result=aggregateRows(identity,rows);
  put(dir,'summary.csv',result.csv);put(dir,'report.md',result.markdown);put(dir,'verdict.json',JSON.stringify(result.result,null,2)+'\n');
}
function command(cwd,exe,args,env=process.env) {
  const r=spawnSync(exe,args,{cwd,env,encoding:'utf8',timeout:1800000,maxBuffer:64*1024*1024});
  assert.equal(r.status,0,`child command failed (${exe}); private stderr not saved`);return r.stdout;
}
function sourceSnapshot(cwd) {
  const commit=command(cwd,'git',['rev-parse','HEAD']).trim();
  const tree=command(cwd,'git',['rev-parse','HEAD^{tree}']).trim();
  let patch=command(cwd,'git',['diff','--binary','HEAD','--','.']);
  const untracked=command(cwd,'git',['ls-files','--others','--exclude-standard','-z']).split('\0').filter(Boolean).filter(p=>/\.(rs|mjs|toml)$/.test(p)||/^docs\/perf\/R19_PRF04_GATE2B_PREREG\.(md|sha256)$/.test(p));
  for(const file of untracked.sort()) {
    assert(/^[\w./-]+$/.test(file));const text=readFileSync(resolve(cwd,file),'utf8');assert(!text.includes('\0'));
    const lines=text.split('\n'),newline=lines.at(-1)==='';if(newline)lines.pop();
    patch+=`diff --git a/${file} b/${file}\nnew file mode 100644\n--- /dev/null\n+++ b/${file}\n@@ -0,0 +1,${lines.length} @@\n`+lines.map(l=>'+'+l+'\n').join('')+(newline?'':'\\ No newline at end of file\n');
  }
  safeText(patch);
  const harness=Object.fromEntries(harnessFiles.map(file=>[file,digest(readFileSync(resolve(cwd,file)))]));
  return {commit,tree,patch_sha256:digest(patch),harness,patch};
}
function harnessHashes(cwd) { return Object.fromEntries(harnessFiles.map(file=>[file,digest(readFileSync(resolve(cwd,file)))])); }
function verifySource(expected,actual) {for(const key of ['commit','tree','patch_sha256','harness'])assert.deepEqual(actual[key],expected[key],`source drift ${key}`);}
function run(out,armA,armB) {
  assert(!existsSync(out),'output path exists');assert(statSync(dirname(out)).isDirectory());
  const arms={A:resolve(armA),B:resolve(armB)};assert.notEqual(arms.A,arms.B,'distinct explicit arms required');
  assert(!process.env.RUSTFLAGS&&!process.env.CARGO_ENCODED_RUSTFLAGS&&!process.env.CARGO_BUILD_TARGET,'reject flag/target substitutions');
  const snapshots=Object.fromEntries(Object.entries(arms).map(([arm,cwd])=>{assert(statSync(cwd).isDirectory());const snap=sourceSnapshot(cwd);assert.equal(snap.patch,'',`arm ${arm} worktree must be clean: identity is the commit`);return [arm,snap];}));
  assert.deepEqual(snapshots.A.harness,snapshots.B.harness,'wrapper/workload byte mismatch');
  // B = A + V4: A must be an ancestor of B.
  command(arms.B,'git',['merge-base','--is-ancestor',snapshots.A.commit,snapshots.B.commit]);
  const current=harnessHashes(root);assert.deepEqual(current,snapshots.A.harness,'runner/harness substitution');
  const prereg='docs/perf/R19_PRF04_GATE2B_PREREG.md';const preregBytes=readFileSync(resolve(root,prereg));const preregHash=digest(preregBytes);
  assert.equal(readFileSync(resolve(root,prereg.replace('.md','.sha256')),'utf8').trim().split(/\s+/)[0],preregHash);
  for(const cwd of Object.values(arms))assert.equal(digest(readFileSync(resolve(cwd,prereg))),preregHash);
  mkdirSync(out);
  const identity={schema:2,captured_at_ms:0,prereg_sha256:preregHash,arms:{},os:{platform:os.platform(),release:os.release(),arch:os.arch(),cpu:os.cpus()[0]?.model}};
  const executables={};
  for(const [arm,cwd] of Object.entries(arms)) {
    const env={...process.env,RUSTC_WRAPPER:'',CARGO_BUILD_JOBS:'3',CARGO_TARGET_DIR:resolve(cwd,'target')};
    const copyDir=resolve(cwd,'target',`r19_prf04_gate2b_${digest(relative(root,out)).slice(0,16)}_${arm}`);assert(!existsSync(copyDir),'binary output exists');mkdirSync(copyDir);
    const binaries={},metadata={};executables[arm]={};
    for(const [diagnostic,features] of [[false,'production'],[true,'production bench-internals alloc-stats internals']]) {
      command(cwd,'cargo',['build','--locked','--release','--jobs','3','--example','r19_prf04_gate2b','--example','r19_prf04_gate2b_observer','--features',features],env);
      const meta=JSON.parse(command(cwd,'cargo',['metadata','--locked','--format-version','1','--features',features],env));
      metadata[diagnostic?'diagnostic':'production']=meta.resolve.nodes.map(n=>({name:meta.packages.find(p=>p.id===n.id).name,features:n.features}));
      for(const observer of [false,true]) {
        const key=(observer?'observer_':'')+(diagnostic?'diagnostic':'production');
        const bytes=readFileSync(resolve(cwd,'target/release/examples/r19_prf04_gate2b'+(observer?'_observer':'')+(process.platform==='win32'?'.exe':'')));
        const exe=resolve(copyDir,key+(process.platform==='win32'?'.exe':''));writeFileSync(exe,bytes,{flag:'wx',mode:0o755});executables[arm][key]=exe;binaries[key]=digest(bytes);
      }
    }
    verifySource(snapshots[arm],sourceSnapshot(cwd));
    const rustc=command(cwd,'rustc',['-vV']);const target=rustc.match(/^host: (.+)$/m)?.[1];assert(target);
    const {patch,...snapshot}=snapshots[arm];put(out,`${arm}.source.patch`,patch);
    identity.arms[arm]={...snapshot,label:arm,binaries,metadata,rustc,target,cargo:command(cwd,'cargo',['--version']).trim(),profile:'release',locked:true,default_features:true,production_features:'production',diagnostic_features:'production bench-internals alloc-stats internals'};
  }
  writeFileSync(resolve(out,'prereg.md'),preregBytes,{flag:'wx'});
  verifyPrereg(identity,readFileSync(resolve(out,'prereg.md')));
  identity.captured_at_ms=Date.now();identities(identity,[]);put(out,'identity.json',JSON.stringify(identity,null,2)+'\n');
  put(out,'schedule.json',JSON.stringify(schedule(),null,2)+'\n');
  const drift=()=>{assert.deepEqual(harnessHashes(root),current,'root runner/harness drift');verifyPrereg(identity,readFileSync(resolve(out,'prereg.md')));for(const [arm,cwd] of Object.entries(arms)){verifySource(snapshots[arm],sourceSnapshot(cwd));for(const [key,exe] of Object.entries(executables[arm]))assert.equal(digest(readFileSync(exe)),identity.arms[arm].binaries[key],'binary drift');}assert.equal(digest(readFileSync(resolve(root,prereg))),preregHash);};
  const rows=[];
  for(const job of schedule()) {
    drift();const key=(job.observer?'observer_':'')+(job.diagnostic?'diagnostic':'production');
    const sample_started_at_ms=Math.max(Date.now(),identity.captured_at_ms+1);
    const r=JSON.parse(command(arms[job.arm],executables[job.arm][key],[job.mode,job.arm,String(job.k),String(job.w)]));
    for(const field of ['arm','mode','k','w','observer','diagnostic'])assert.equal(r[field],job[field],'sample substitution');
    validate(r);
    Object.assign(r,job,{commit:identity.arms[job.arm].commit,tree:identity.arms[job.arm].tree,patch_sha256:identity.arms[job.arm].patch_sha256,binary_sha256:identity.arms[job.arm].binaries[key],sample_started_at_ms});
    validate(r);identities(identity,[r]);drift();rows.push(r);
    // Early bootstrap allocation is saved as invalidation, never replaced.
    put(out,`${String(job.index).padStart(4,'0')}_${r.live_valid?'sample':'invalidation'}.json`,JSON.stringify(r)+'\n');
  }
  drift();put(out,'samples.json',JSON.stringify(rows)+'\n');aggregate(out);
}
function extendedTests() {
  let checks=0;const check=f=>{f();checks++;};
  check(()=>assert.equal(schedule().length,492));
  const identity={schema:2,captured_at_ms:1,prereg_sha256:digest('synthetic prereg'),arms:Object.fromEntries(['A','B'].map((arm,i)=>[arm,{label:arm,commit:String(i+1).repeat(40),tree:'3'.repeat(40),patch_sha256:'4'.repeat(64),harness:Object.fromEntries(harnessFiles.map(f=>[f,'5'.repeat(64)])),binaries:Object.fromEntries(['production','diagnostic','observer_production','observer_diagnostic'].map(k=>[k,'6'.repeat(64)])),profile:'release',locked:true,production_features:'production',diagnostic_features:'production bench-internals alloc-stats internals',rustc:'synthetic',target:'synthetic'}]))};
  const rows=schedule().map(job=>{
    const {arm,k,w,mode,observer,diagnostic}=job;const count=mode==='wave'?(arm==='A'?1+w*(k-1):k):mode==='live'?k:1;
    const sources=arm==='A'?[k*w-count,count,0,0]:[0,k,0,0,k*w-k];if(mode==='live')sources.splice(0,sources.length,...(arm==='A'?[0,k,0,0]:[0,k,0,0,0]));if(mode==='churn')sources.splice(0,sources.length,...(arm==='A'?[w-1,1,0,0]:[w-1,1,0,0,0]));
    return {...job,schema:2,maintenance_running:false,requested_config:'LargeCacheConfig::new',config_conflicts_before:0,config_conflicts_after:0,count_before:1,count_after:count+1,count_delta:count,rss_before:1,rss_idle:2,rss_peak_sampled:2,segments_reserved_delta:count,commit_before:1,commit_idle:arm==='A'?268435456:134217728,commit_peak_sampled:268435456,observer_status:observer?'OBSERVED':'NONE',first_call_metric:observer?'first allocating call, includes TLS bind/claim':null,pre_first_alloc_deallocs:observer?Array(k*w).fill(1):null,first_call_samples:observer?Array.from({length:k*w},()=>[1,2,100,1,mode==='live'?1:0]):null,live_valid:true,wall_ns:diagnostic?null:100,claim_sources:diagnostic?sources:null,claim_sources_len:diagnostic?sources.length:null,free_set:diagnostic?(sources[4]??0):null,missing_fifth_explicit_zero:diagnostic?sources.length===4:null,resolved_config_results:diagnostic?[k*w,0]:null,routed_scans:diagnostic?0:null,commit:identity.arms[arm].commit,tree:identity.arms[arm].tree,patch_sha256:identity.arms[arm].patch_sha256,binary_sha256:'6'.repeat(64),sample_started_at_ms:2};
  });
  check(()=>{const r=aggregateRows(identity,rows);assert.equal(r.result.verdict,'GO');assert(r.csv.includes('commit'));assert(r.markdown.includes('GO'));});
  const mutate=f=>{const rs=structuredClone(rows);f(rs);return aggregateRows(identity,rs);};
  for(const f of [rs=>rs[1].first_call_metric='first bind',rs=>rs[1].pre_first_alloc_deallocs[0]++,rs=>rs[1].first_call_samples[0][3]++,rs=>delete rs[1].first_call_metric,rs=>delete rs[1].pre_first_alloc_deallocs,rs=>rs[0].first_call_metric='first allocating call, includes TLS bind/claim',rs=>rs[0].pre_first_alloc_deallocs=[],rs=>rs.pop(),rs=>rs[0].index++,rs=>rs[1].first_call_samples[0][0]=0,rs=>rs[1].first_call_samples.pop(),rs=>rs[0].count_delta++,rs=>rs[0].maintenance_running=true,rs=>rs[0].config_conflicts_after++,rs=>rs[0].commit='0'.repeat(40),rs=>rs[0].patch_sha256='0'.repeat(64),rs=>rs.find(r=>r.diagnostic).claim_sources[0]++])check(()=>assert.throws(()=>mutate(f)));
  check(()=>{const id=structuredClone(identity);id.arms.B.harness[harnessFiles[1]]='0'.repeat(64);assert.throws(()=>aggregateRows(id,rows));});
  check(()=>assert.throws(()=>verifySource(identity.arms.A,{...identity.arms.A,patch_sha256:'0'.repeat(64)})));
  check(()=>assert.throws(()=>mutate(rs=>rs.find(r=>r.mode==='live').first_call_samples[0][1]=0)));
  check(()=>assert.equal(mutate(rs=>{const r=rs.find(r=>r.mode==='live');r.first_call_samples[0][1]=0;r.live_valid=false;}).result.verdict,'INCONCLUSIVE'));
  check(()=>assert.equal(mutate(rs=>rs[0].commit_idle=null).result.verdict,'GO')); // secondary cell is excluded
  check(()=>assert.equal(mutate(rs=>rs.find(r=>r.series==='wave'&&r.arm==='A'&&r.k===32&&r.w===128&&!r.observer).commit_idle=null).result.verdict,'INCONCLUSIVE'));
  for(const axis of ['commit','wave','live'])check(()=>assert.equal(mutate(rs=>{
    if(axis==='commit')rs.find(r=>r.series==='aa_wave'&&!r.observer&&r.position===1).commit_idle=400000000;
    if(axis==='wave')rs.filter(r=>r.series==='aa_wave'&&r.observer&&r.position===1).forEach(r=>r.first_call_samples.forEach(v=>v[2]=120));
    if(axis==='live')rs.filter(r=>r.series==='aa'&&r.mode==='live'&&[1,2].includes(r.position)).forEach(r=>r.first_call_samples.forEach(v=>v[2]=106));
    if(axis==='churn')rs.filter(r=>r.series==='aa'&&r.mode==='churn'&&[1,2].includes(r.position)).forEach(r=>r.wall_ns=103);
  }).result.verdict,'INCONCLUSIVE'));
  for(const axis of ['wave','live','churn'])check(()=>assert.equal(mutate(rs=>{
    rs.filter(r=>r.arm==='B'&&!r.diagnostic&&(axis==='wave'?r.series==='wave'&&r.observer&&r.k===32&&r.w===128:r.mode===axis&&r.series==='ab')).forEach(r=>{if(axis==='churn')r.wall_ns=103;else r.first_call_samples.forEach(v=>v[2]=axis==='live'?101:111);});
  }).result.verdict,'NO-GO'));
  // KILL-2 widening: A/A noise 3% lets a 3% churn regression pass, a 5% one still kills.
  check(()=>assert.equal(mutate(rs=>{rs.filter(r=>r.series==='aa'&&r.mode==='churn'&&[1,2].includes(r.position)).forEach(r=>r.wall_ns=103);rs.filter(r=>r.arm==='B'&&!r.diagnostic&&r.mode==='churn'&&r.series==='ab').forEach(r=>r.wall_ns=103);}).result.verdict,'GO'));
  check(()=>assert.equal(mutate(rs=>{rs.filter(r=>r.series==='aa'&&r.mode==='churn'&&[1,2].includes(r.position)).forEach(r=>r.wall_ns=103);rs.filter(r=>r.arm==='B'&&!r.diagnostic&&r.mode==='churn'&&r.series==='ab').forEach(r=>r.wall_ns=105);}).result.verdict,'NO-GO'));
  check(()=>{const id=structuredClone(identity);id.arms.B.rustc='other';assert.throws(()=>aggregateRows(id,rows));});
  check(()=>{const id=structuredClone(identity);id.arms.B.target='other';assert.throws(()=>aggregateRows(id,rows));});
  check(()=>{const id=structuredClone(identity);id.arms.B.metadata={x:1};assert.throws(()=>aggregateRows(id,rows));});
  check(()=>verifyPrereg(identity,Buffer.from('synthetic prereg')));
  check(()=>assert.throws(()=>verifyPrereg(identity,Buffer.from('changed prereg'))));
  for(const sha of [undefined,null,'','A'.repeat(64),'a'.repeat(63),'g'.repeat(64)])check(()=>assert.throws(()=>verifyPrereg({...identity,prereg_sha256:sha},Buffer.from('synthetic prereg'))));
  check(()=>assert.equal(aaNoise([[100,110],[200,180]]),.1));
  return checks;
}
const args=process.argv.slice(2);
if(args.length===1&&args[0]==='--self-test')selfTest();
else if(args.length===6&&args[0]==='--run'&&args[2]==='--arm-a'&&args[4]==='--arm-b')run(resolve(root,args[1]),args[3],args[5]);
else if(args.length===2&&args[0]==='--aggregate')aggregate(resolve(root,args[1]));
else throw Error('usage: --self-test | --run <outdir> --arm-a <wt> --arm-b <wt> | --aggregate <existing outdir>');
