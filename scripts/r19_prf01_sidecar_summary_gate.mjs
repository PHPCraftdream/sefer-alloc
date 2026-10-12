// Read-only version1 judge: no builds, subprocesses, measurements or writes.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
const root = new URL('../', import.meta.url);
const files = ['docs/perf/R19_PRF01_SIDECAR_SUMMARY_PREREG.md', 'scripts/r19_prf01_sidecar_summary_gate.mjs'];
const sha = b => createHash('sha256').update(b).digest('hex');
const hashes = () => files.map(path => ({path, sha256:sha(readFileSync(new URL(path, root)))}));
const hashText = () => hashes().map(r => `${r.sha256}  ${r.path}\n`).join('');
const arms = ['A1','A2','B1','B2','C0'];
const primary = ['bench-internals','internals','production'];
const benefit = ['multiseg_cold_256k','seg_cycle_decommit_256k','growth16_n','growth16_2n','growth16_4n'];
const hot = ['small_churn_16b','churn_256b','churn_write_256b','aligned_churn_640b_a128'];
const mi = ['mimalloc_small_churn_16b','mimalloc_small_churn_16b_2n','mimalloc_churn_256b','mimalloc_cold_alloc_free_256x16b','mimalloc_cold_alloc_free_256x16b_2n','mimalloc_cold_alloc_free_256x16b_4n','mimalloc_cold_alloc_only_256x16b','mimalloc_cold_alloc_only_256x16b_2n','mimalloc_cold_alloc_only_256x16b_4n','mimalloc_cold_alloc_free_256x64b','mimalloc_recycle_alloc_free_256x16b','mimalloc_recycle_alloc_free_256x64b','mimalloc_bootstrap_proxy'];
// Amendment A: inventory DECLARATIONS (workCount, blockBytes, Layout alignment) of the four hot controls.
const hotDecl = {small_churn_16b:[64,16,8],churn_256b:[64,256,8],churn_write_256b:[64,256,8],aligned_churn_640b_a128:[64,640,128]};
const publication = 'r19_prf01_publication';
const producer = 'r19_prf01_joined_producer_same_word';
const inventory = [...benefit,...hot,...mi,publication,producer];
const counters = ['pending_words_visited','pending_words_exchanged','nonempty_cuts','discovery_records_reclaimed','routed_scans','summary_words_visited','summary_rmw','summary_load','strict_trim_remaining'];
const text = x => assert.ok(typeof x === 'string' && x.trim().length > 0);
const hex = x => { text(x); assert.match(x,/^[0-9a-f]{64}$/); };
const commit = x => { text(x); assert.match(x,/^[0-9a-f]{40}$/); };
const nat = x => assert.ok(Number.isSafeInteger(x) && x >= 0);
const positive = x => assert.ok(Number.isFinite(x) && x > 0);
const exact = (a,b) => assert.deepEqual(a,b);
const features = (a,b) => { assert.ok(Array.isArray(a)); exact([...a].sort(),[...b].sort()); };
const time = x => { text(x); assert.match(x,/^\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d(?:\.\d{3})?Z$/); const t=Date.parse(x); assert.ok(Number.isFinite(t)); return t; };
const receipt = r => { text(r.path); assert.ok(!r.path.startsWith('/') && !r.path.includes('..') && !/[\\:]/.test(r.path)); hex(r.sha256); };
function source(s) { commit(s.baseHead); for(const k of ['patchSha256','untrackedSha256','identitySha256'])hex(s[k]); time(s.capturedAt); }
function config(c, mode = 'normal', ctx = null) {
  if (mode === 'mimalloc') {
    exact(c.requested, null); exact(c.resolved, null); exact(c.configConflictsDelta, null);
    text(c.notApplicableReason);
    assert.equal(c.processBoundary,'isolated-process'); text(c.runId); nat(c.pid); assert.ok(c.pid>0);
    return;
  }
  exact(c.requested,c.resolved); assert.ok(c.requested && typeof c.requested==='object' && !Array.isArray(c.requested));
  for(const k of ['maintenance','workCount','segmentCount','blockBytes','cachePolicy'])assert.ok(k in c.requested);
  assert.equal(c.requested.maintenance,false); nat(c.requested.workCount); assert.ok(c.requested.workCount>0);
  nat(c.requested.blockBytes); assert.ok(c.requested.blockBytes>0); text(c.requested.cachePolicy);
  if (mode === 'hot') {
    // Amendment A: the body runs only under Callgrind; geometry fields are declarations, segmentCount is unobserved (null).
    const [work,block,alignment] = hotDecl[ctx.name]; assert.ok(Array.isArray(hotDecl[ctx.name]));
    assert.strictEqual(c.requested.segmentCount,null); assert.strictEqual(c.resolved.segmentCount,null);
    assert.equal(c.requested.workCount,work); assert.equal(c.requested.blockBytes,block);
    assert.ok(c.unobservedReasons && typeof c.unobservedReasons==='object'); text(c.unobservedReasons.segmentCount);
    const d=c.declaredProvenance; assert.ok(d && typeof d==='object'); receipt(d.receipt); assert.equal(d.cell,ctx.name);
    exact(d.fields,['workCount','blockBytes','alignment']); assert.equal(d.alignment,alignment);
    const r=c.resolvedProvenance; assert.ok(r && typeof r==='object'); text(r.runId); nat(r.pid); assert.ok(r.pid>0);
    assert.equal(r.arm,ctx.arm); assert.equal(r.binarySha256,ctx.pins[ctx.arm].binaries.oracle); receipt(r.receipt);
    assert.equal(r.claimPath,'global'); exact([...r.copiedFields].sort(),['cachePolicy','configConflictsDelta','maintenance']);
  } else {
    nat(c.requested.segmentCount); assert.ok(c.requested.segmentCount>0);
  }
  assert.equal(c.configConflictsDelta,0); assert.equal(c.processBoundary,'isolated-process'); text(c.runId); nat(c.pid); assert.ok(c.pid>0);
}
function schedule() {
  const jobs=[];
  for(const n of [1,2,4,8])for(let repetition=1;repetition<=9;repetition++)
    (repetition%2 ? ['A','AA','B'] : ['B','AA','A']).forEach((arm,position)=>jobs.push({n,arm,repetition,position,index:jobs.length}));
  return jobs;
}
function ratio(numerator,denominator) {
  positive(denominator); assert.ok(Number.isFinite(numerator)); const value=numerator/denominator;
  assert.ok(Number.isFinite(value)); assert.ok(Math.abs(value*denominator-numerator)<=1e-10*Math.max(1,Math.abs(numerator)));
  return {numerator,denominator,value};
}
const median = xs => { assert.equal(xs.length,9); xs.forEach(positive); return [...xs].sort((a,b)=>a-b)[4]; };
function derive(d,frozen) {
  const tables={iai:[],slope:[],oracle:[],native:[],size:[]};
  try {
    assert.equal(d.version,1); exact(d.hashes,frozen); hex(d.environmentSha256);
    for(const k of ['inventoryReceipt','featureExactReceipt','identityReviewReceipt','environmentReceipt'])receipt(d[k]);
    exact(d.inventory,inventory); exact(d.strictTrim,{cell:null,reason:'No existing strict-trim iai cell in benches/perf_gate_iai.rs'});
    assert.equal(d.entry,'SeferAlloc::GlobalAlloc->HeapCore routed');
    const pins=d.identities;
    for(const arm of ['A','B']) {
      const p=pins[arm]; assert.equal(p.label,arm); source(p.source); assert.equal(p.measurementCommit,p.source.baseHead); commit(p.measurementCommit);
      assert.equal(p.environmentSha256,d.environmentSha256);
      for(const k of ['iai','oracle','native']) { hex(p.binaries[k]); }
      assert.ok(time(p.source.capturedAt)<time(p.buildStartedAt));
      features(p.features,primary); features(p.nativeFeatures,['production']);
      exact(p.oracleCfg,['r19_sidecar_summary_oracle']); exact(p.primaryCfg,[]);
      receipt(p.buildReceipt);
    }
    hex(pins.C0.binarySha256); exact(pins.C0.source,pins.A.source); receipt(pins.C0.buildReceipt);
    assert.ok(time(pins.C0.source.capturedAt)<time(pins.C0.buildStartedAt));
    assert.notEqual(pins.C0.binarySha256,pins.A.binaries.iai);
    assert.notEqual(pins.A.source.identitySha256,pins.B.source.identitySha256);
    assert.notEqual(pins.A.measurementCommit,pins.B.measurementCommit);
    const ancestry=d.ancestry; receipt(ancestry.receipt); assert.equal(ancestry.result,'A ancestor B');
    assert.equal(ancestry.chain[0],pins.A.measurementCommit); assert.equal(ancestry.chain.at(-1),pins.B.measurementCommit);
    assert.ok(ancestry.chain.length>=2); ancestry.chain.forEach(commit); assert.equal(new Set(ancestry.chain).size,ancestry.chain.length);
    exact(pins.A.toolchain,pins.B.toolchain); for(const k of ['rustc','target','profile','flags'])text(pins.A.toolchain[k]);
    const runs=new Set();
    function provenance(r,arm,binary,expectedFeatures) {
      const p=pins[arm]; exact(r.source,p.source); assert.equal(r.binarySha256,binary); assert.equal(r.environmentSha256,d.environmentSha256);
      assert.ok(time(r.startedAt)>time(p.buildStartedAt)); features(r.features,expectedFeatures); receipt(r.receipt); config(r.config,mi.includes(r.name)?'mimalloc':hot.includes(r.name)?'hot':'normal',{name:r.name,arm,pins});
      assert.ok(!runs.has(r.config.runId)); runs.add(r.config.runId);
    }
    function oracle(r,needed) {
      const o=r.oracle; assert.ok(o && typeof o==='object'); exact(Object.keys(o).sort(),[...counters,'unobservedReasons','linkage'].sort());
      if(!needed) {
        assert.equal(o.linkage,null);
        exact(Object.keys(o.unobservedReasons).sort(),[...counters].sort());
        for(const k of counters) { assert.equal(o[k],null); text(o.unobservedReasons[k]); }
        return;
      }
      exact(Object.keys(o.unobservedReasons), ['strict_trim_remaining'].filter(k=>o[k]===null));
      for(const k of counters) {
        if(k==='strict_trim_remaining' && r.name!==publication) { assert.equal(o[k],null); text(o.unobservedReasons[k]); }
        else nat(o[k]);
      }
      assert.ok(o.nonempty_cuts<=o.pending_words_exchanged && o.pending_words_exchanged<=o.pending_words_visited);
      const l=o.linkage,p=pins[r.arm.startsWith('B')?'B':'A'];
      assert.equal(l.binarySha256,p.binaries.oracle); exact(l.source,r.source); features(l.features,primary); exact(l.cfg,['r19_sidecar_summary_oracle']);
      assert.equal(l.environmentSha256,d.environmentSha256); receipt(l.receipt); receipt(l.featureExactReceipt);
      assert.equal(l.featureExactReceipt.sha256,d.featureExactReceipt.sha256);
      assert.ok(time(l.startedAt)>time(p.buildStartedAt));
      assert.equal(l.primaryBinarySha256,r.binarySha256);
      exact(l.config.requested,r.config.requested); config(l.config);
      assert.ok(!runs.has(l.config.runId)); runs.add(l.config.runId);
    }
    assert.equal(d.iai.length,inventory.length*5);
    let mode1Fail=false,mode2Fail=false,nativeFail=false,noisy=false;
    const cells=new Map();
    for(const name of inventory) {
      const rs=arms.map(arm=>{const found=d.iai.filter(r=>r.name===name && r.arm===arm); assert.equal(found.length,1); return found[0];}); cells.set(name,rs);
      const needed=benefit.includes(name)||name===publication||name===producer;
      for(const r of rs) {
        const a=r.arm.startsWith('B')?'B':'A';
        provenance(r,a,r.arm==='C0'?pins.C0.binarySha256:pins[a].binaries.iai,primary);
        if(r.arm==='C0')assert.ok(time(r.startedAt)>time(pins.C0.buildStartedAt));
        nat(r.Ir); assert.ok(r.Ir>0); exact(r.config.requested,rs[0].config.requested); oracle(r,needed);
        if(benefit.includes(name)) {
          assert.ok(r.oracle.routed_scans>0); assert.equal(r.oracle.routed_scans,rs[0].oracle.routed_scans);
          assert.equal(r.oracle.nonempty_cuts,0); assert.equal(r.oracle.discovery_records_reclaimed,0);
          if(a==='B') { assert.equal(r.oracle.pending_words_visited,0); assert.ok(r.oracle.summary_words_visited>0); }
          else {
            assert.ok(r.oracle.pending_words_visited>0); assert.equal(r.oracle.pending_words_exchanged,r.oracle.pending_words_visited);
            for(const k of ['summary_words_visited','summary_rmw','summary_load'])assert.equal(r.oracle[k],0);
            const expected=name===benefit[0]?319063:name===benefit[1]?1004503:null;
            if(expected!==null)assert.ok(Math.abs(r.oracle.pending_words_visited-expected)<=Math.floor(expected*.01),'frozen 1% pending count tolerance');
          }
          if(name.startsWith('growth16_')) { assert.equal(r.config.requested.workCount,[32768,65536,131072][benefit.indexOf(name)-2]); assert.equal(r.config.requested.blockBytes,16); }
        }
        if(name===producer) {
          const p=r.producerIrReceipt; assert.ok(p && typeof p==='object');
          assert.equal(p.binarySha256,r.binarySha256); exact(p.source,r.source); assert.equal(p.environmentSha256,r.environmentSha256);
          features(p.features,primary); exact(p.cfg,[]); receipt(p.receipt); receipt(p.symbolReceipt); receipt(p.profileReceipt); receipt(p.control.receipt);
          assert.equal(p.symbol,'<perf_gate_iai::prf01_support::R19Prf01>::producer_free_window');
          assert.equal(p.runnerVersion,'0.14.2'); text(p.valgrindVersion);
          assert.equal(p.collectAtStart,false); assert.equal(p.defaultToggle,false); assert.equal(p.entryPoint,'Custom'); assert.equal(p.separateThreads,true);
          assert.equal(p.toggleCollect,p.symbol); text(p.generatedArguments); assert.equal(p.aggregation,'sum of all thread profile Ir');
          assert.equal(p.childFreeCount,4096); nat(p.childIr); assert.ok(p.childIr>0);
          assert.ok(time(p.startedAt)>time(r.source.capturedAt));
          assert.equal(p.control.workload,'child remote frees removed'); assert.equal(p.control.environmentSha256,r.environmentSha256);
          source(p.control.source); hex(p.control.binarySha256); assert.notEqual(p.control.binarySha256,r.binarySha256);
          nat(p.control.childIr); nat(p.control.totalIr); nat(p.totalIr); assert.equal(p.totalIr,r.Ir);
          assert.ok(p.childIr<=p.totalIr,'child Ir is a part of the total and cannot exceed it');
          assert.notEqual(p.control.totalIr,p.totalIr); assert.equal(p.control.deltaIr,p.totalIr-p.control.totalIr);
          assert.ok(p.control.deltaIr>0);
          // Amendment B: the control keeps the exact boundary; its own overhead is the shared, bounded baseline.
          const c=p.control; nat(p.boundaryOverheadIr); assert.ok(p.boundaryOverheadIr>=1&&p.boundaryOverheadIr<=16,'boundary overhead outside the frozen 1..16 range');
          assert.equal(c.childIr,p.boundaryOverheadIr); nat(p.workloadIr); assert.equal(p.workloadIr,p.childIr-p.boundaryOverheadIr); assert.ok(p.workloadIr>0);
          assert.equal(c.workloadIr,0); assert.equal(c.childFreeCount,0); assert.equal(p.boundaryExecutions,1); assert.equal(c.boundaryExecutions,1);
          assert.ok(c.childIr<=c.totalIr,'control child Ir is a part of its total');
          receipt(c.symbolReceipt); receipt(c.profileReceipt); assert.equal(c.symbol,p.symbol); assert.equal(c.toggleCollect,p.symbol);
          exact(c.cfg,['r19_prf01_producer_control']); features(c.features,primary); exact(c.source,r.source); text(c.generatedArguments);
          assert.equal(c.runnerVersion,'0.14.2'); text(c.valgrindVersion); assert.equal(c.collectAtStart,false); assert.equal(c.defaultToggle,false);
          assert.equal(c.entryPoint,'Custom'); assert.equal(c.separateThreads,true); assert.equal(c.aggregation,'sum of all thread profile Ir');
          assert.equal(p.control.parentSetupUnchanged,true); assert.equal(p.control.collectionUnchanged,true);
          assert.equal(r.config.requested.workCount,4096); assert.equal(r.config.requested.blockBytes,16); assert.equal(r.config.requested.segmentCount,1);
          assert.equal(r.scenario,'joined remote Small frees; repeated frees in same pending word; no owner discovery during producer window');
          if(a==='B') { assert.ok(r.oracle.summary_rmw>0); assert.ok(r.oracle.summary_load>0); }
          else { assert.equal(r.oracle.summary_rmw,0); assert.equal(r.oracle.summary_load,0); }
        }
        if(name===publication) {
          assert.ok(r.oracle.discovery_records_reclaimed>0); assert.ok(r.oracle.routed_scans>0);
          for(const k of ['discovery_records_reclaimed','strict_trim_remaining'])assert.equal(r.oracle[k],rs[0].oracle[k]);
        }
      }
      const [a1,a2,,,c0]=rs.map(r=>r.Ir),T=Math.max(10,Math.abs(a1-a2),Math.abs(c0-a1));
      for(const a of rs.slice(0,2))for(const b of rs.slice(2,4)) {
        const delta=b.Ir-a.Ir; let arithmetic=ratio(delta,a.Ir),pass=true,mode='descriptive';
        if(benefit.slice(0,2).includes(name)) { arithmetic=ratio(b.Ir,a.Ir); pass=arithmetic.value<=.25; mode='mode1'; }
        if(hot.includes(name)) { pass=Math.abs(delta)<=T; mode='mode1'; }
        if(mi.includes(name)) { pass=delta===0; mode='mode1'; }
        if(name===producer) { arithmetic=ratio(delta,4096); pass=arithmetic.value<=6; mode='mode2'; }
        if(mode==='mode1')mode1Fail ||= !pass; if(mode==='mode2')mode2Fail ||= !pass;
        tables.iai.push({name,A:a.arm,B:b.arm,delta,T,mode,...arithmetic,pass});
      }
      if(needed)tables.oracle.push({name,arms:rs.map(r=>({arm:r.arm,...r.oracle}))});
    }
    for(const arm of ['B1','B2']) {
      const [n,n2,n4]=benefit.slice(2).map(name=>cells.get(name).find(r=>r.arm===arm).Ir);
      const denominator=n2-n,numerator=n4-n2;
      const arithmetic=denominator>0?ratio(numerator,denominator):{numerator,denominator,value:null};
      const pass=denominator>0 && arithmetic.value<=2.5; mode1Fail ||= !pass;
      tables.slope.push({arm,...arithmetic,pass});
    }
    nat(d.sizeOf.A); nat(d.sizeOf.B); assert.ok(d.sizeOf.A>0); receipt(d.sizeOf.receipt);
    const sizePass=d.sizeOf.B-d.sizeOf.A===512; mode1Fail ||= !sizePass;
    tables.size.push({A:d.sizeOf.A,B:d.sizeOf.B,numerator:d.sizeOf.B-d.sizeOf.A,expected:512,pass:sizePass});
    const jobs=schedule(); assert.equal(d.native.length,108); exact(d.schedule,jobs);
    d.native.forEach((r,i)=>{
      for(const k of ['n','arm','repetition','position','index'])assert.equal(r[k],jobs[i][k]);
      const a=r.arm==='B'?'B':'A'; provenance(r,a,pins[a].binaries.native,['production']);
      assert.equal(r.entry,'installed #[global_allocator] SeferAlloc');
      assert.equal(r.config.requested.segmentCount,1); assert.equal(r.config.requested.blockBytes,16); assert.equal(r.config.requested.workCount,4096);
      for(const k of ['p50Ns','p99Ns','nsPerOp'])positive(r[k]); assert.ok(r.p99Ns>=r.p50Ns);
      const same=d.native.find(x=>x.n===r.n); exact(r.config.requested,same.config.requested);
    });
    for(const n of [1,2,4,8])for(const [metric,limit] of [['p50Ns',.05],['p99Ns',.10],['nsPerOp',null]]) {
      const [a,aa,b]=['A','AA','B'].map(arm=>median(d.native.filter(r=>r.n===n && r.arm===arm).map(r=>r[metric])));
      const noise=ratio(Math.abs(aa-a),a),change=ratio(b-a,a);
      const wide=limit!==null && noise.value>limit,pass=limit===null || change.value<=limit;
      noisy ||= wide; nativeFail ||= !pass;
      tables.native.push({n,metric,statistic:'median of nine per-process metrics; no pooling',A:a,AA:aa,B:b,change,noise,limit,wide,pass});
    }
    const modes={mode1:mode1Fail?'NO-GO':'GO',mode2Iai:mode2Fail?'NO-GO':'GO',mode2Native:noisy?'INCONCLUSIVE':nativeFail?'NO-GO':'GO'};
    return {version:1,verdict:noisy?'INCONCLUSIVE':mode1Fail||mode2Fail||nativeFail?'NO-GO':'GO',modes,tables};
  } catch(e) { return {version:1,verdict:'INCOMPLETE',modes:{mode1:'INCOMPLETE',mode2Iai:'INCOMPLETE',mode2Native:'INCOMPLETE'},reason:e.message,tables}; }
}
function fixture(frozen) {
  const h=c=>c.repeat(64),rec={path:'synthetic/receipt.json',sha256:h('f')};
  const src=c=>({baseHead:c.repeat(40),patchSha256:h(c),untrackedSha256:h(c),identitySha256:h(c),capturedAt:'2026-10-11T00:00:00Z'});
  const p=(c,bs)=>({label:c==='a'?'A':'B',source:src(c),measurementCommit:c.repeat(40),environmentSha256:h('e'),binaries:{iai:h(bs[0]),oracle:h(bs[1]),native:h(bs[2])},buildStartedAt:'2026-10-11T01:00:00Z',features:primary,nativeFeatures:['production'],oracleCfg:['r19_sidecar_summary_oracle'],primaryCfg:[],buildReceipt:rec,toolchain:{rustc:'synthetic',target:'synthetic',profile:'bench',flags:'none'}});
  let id=0;
  const cfg=(workCount=1,blockBytes=16)=>{const requested={maintenance:false,workCount,segmentCount:1,blockBytes,cachePolicy:'default'}; return {requested,resolved:{...requested},configConflictsDelta:0,processBoundary:'isolated-process',runId:`synthetic-${++id}`,pid:id};};
  const hotCfg=(name,a)=>{const [w,b,al]=hotDecl[name]; const requested={maintenance:false,workCount:w,segmentCount:null,blockBytes:b,cachePolicy:'default'}; return {requested,resolved:{...requested},configConflictsDelta:0,processBoundary:'isolated-process',runId:`synthetic-${++id}`,pid:id,unobservedReasons:{segmentCount:'measured body has no paired geometry observer'},declaredProvenance:{receipt:rec,cell:name,fields:['workCount','blockBytes','alignment'],alignment:al},resolvedProvenance:{runId:'probe-'+a,pid:999,arm:a,binarySha256:d.identities[a].binaries.oracle,receipt:rec,claimPath:'global',copiedFields:['cachePolicy','configConflictsDelta','maintenance']}};};
  const d={version:1,hashes:frozen,entry:'SeferAlloc::GlobalAlloc->HeapCore routed',environmentSha256:h('e'),inventoryReceipt:rec,featureExactReceipt:rec,identityReviewReceipt:rec,environmentReceipt:rec,inventory,strictTrim:{cell:null,reason:'No existing strict-trim iai cell in benches/perf_gate_iai.rs'},identities:{A:p('a','123'),B:p('b','456'),C0:{source:src('a'),binarySha256:h('7'),buildStartedAt:'2026-10-11T01:00:00Z',buildReceipt:rec}},ancestry:{result:'A ancestor B',chain:['a'.repeat(40),'b'.repeat(40)],receipt:rec},sizeOf:{A:41984,B:42496,receipt:rec},iai:[],native:[],schedule:schedule()};
  function row(a,binary,features,configuration) {return {environmentSha256:d.environmentSha256,binarySha256:binary,source:d.identities[a].source,startedAt:'2026-10-11T02:00:00Z',features,receipt:rec,config:configuration};}
  for(const name of inventory)for(const arm of arms) {
    const a=arm.startsWith('B')?'B':'A',isB=a==='B',pin=d.identities[a],needed=benefit.includes(name)||name===publication||name===producer;
    const work=name===producer?4096:name.startsWith('growth16_')?[32768,65536,131072][benefit.indexOf(name)-2]:1;
    const r={...row(a,arm==='C0'?h('7'):pin.binaries.iai,primary,cfg(work)),name,arm,Ir:benefit.slice(0,2).includes(name)?isB?250:1000:name.startsWith('growth16_')?[100,200,450][benefit.indexOf(name)-2]:1000};
    r.oracle=Object.fromEntries(counters.map(k=>[k,needed?0:null])); r.oracle.unobservedReasons=needed?{}:Object.fromEntries(counters.map(k=>[k,'compiled out in primary; no paired observer for this control'])); r.oracle.linkage=null;
    if(needed) {
      r.oracle.linkage={binarySha256:pin.binaries.oracle,primaryBinarySha256:r.binarySha256,source:r.source,features:primary,cfg:['r19_sidecar_summary_oracle'],environmentSha256:d.environmentSha256,receipt:rec,featureExactReceipt:rec,startedAt:r.startedAt,config:cfg(work)};
      if(benefit.includes(name)) { r.oracle.routed_scans=99; if(isB)r.oracle.summary_words_visited=64; else {const v=name===benefit[0]?319063:name===benefit[1]?1004503:100; r.oracle.pending_words_visited=v; r.oracle.pending_words_exchanged=v;} }
      if(name===publication) {r.oracle.routed_scans=1;r.oracle.discovery_records_reclaimed=34;}
      if(name===producer) {r.scenario='joined remote Small frees; repeated frees in same pending word; no owner discovery during producer window';if(isB){r.oracle.summary_rmw=1;r.oracle.summary_load=4096;}}
    }
    if(hot.includes(name)) r.config=hotCfg(name,a);
    if(mi.includes(name)) r.config={requested:null,resolved:null,configConflictsDelta:null,notApplicableReason:'mimalloc does not resolve SeferAlloc configuration',processBoundary:'isolated-process',runId:r.config.runId,pid:r.config.pid};
    if(needed && name!==publication) {r.oracle.strict_trim_remaining=null;r.oracle.unobservedReasons={strict_trim_remaining:'No strict trim remainder observation in this workload window'};}
    if(name===producer) r.producerIrReceipt={binarySha256:r.binarySha256,source:r.source,environmentSha256:r.environmentSha256,features:primary,cfg:[],receipt:rec,symbolReceipt:rec,profileReceipt:rec,symbol:'<perf_gate_iai::prf01_support::R19Prf01>::producer_free_window',runnerVersion:'0.14.2',valgrindVersion:'synthetic',collectAtStart:false,defaultToggle:false,entryPoint:'Custom',separateThreads:true,toggleCollect:'<perf_gate_iai::prf01_support::R19Prf01>::producer_free_window',generatedArguments:'synthetic',aggregation:'sum of all thread profile Ir',childFreeCount:4096,childIr:900,boundaryOverheadIr:8,workloadIr:892,boundaryExecutions:1,totalIr:r.Ir,startedAt:r.startedAt,control:{receipt:rec,workload:'child remote frees removed',source:r.source,binarySha256:h('8'),environmentSha256:r.environmentSha256,childIr:8,workloadIr:0,childFreeCount:0,boundaryExecutions:1,symbolReceipt:rec,profileReceipt:rec,symbol:'<perf_gate_iai::prf01_support::R19Prf01>::producer_free_window',toggleCollect:'<perf_gate_iai::prf01_support::R19Prf01>::producer_free_window',cfg:['r19_prf01_producer_control'],features:primary,generatedArguments:'synthetic',runnerVersion:'0.14.2',valgrindVersion:'synthetic',collectAtStart:false,defaultToggle:false,entryPoint:'Custom',separateThreads:true,aggregation:'sum of all thread profile Ir',totalIr:100,deltaIr:r.Ir-100,parentSetupUnchanged:true,collectionUnchanged:true}};
    d.iai.push(r);
  }
  for(const job of d.schedule) {const a=job.arm==='B'?'B':'A'; d.native.push({...row(a,d.identities[a].binaries.native,['production'],cfg(4096)),...job,entry:'installed #[global_allocator] SeferAlloc',p50Ns:100,p99Ns:200,nsPerOp:100});}
  return d;
}
function selftest() {
  const frozen=[{path:'synthetic',sha256:'a'.repeat(64)}],d=fixture(frozen); let checks=0;
  const check=(f,expected='INCOMPLETE')=>{const x=structuredClone(d); f(x); for(const r of x.iai ?? [])if(r.name===producer && r.Ir!==1000 && r.producerIrReceipt?.totalIr===1000){r.producerIrReceipt.totalIr=r.Ir;r.producerIrReceipt.control.deltaIr=r.Ir-r.producerIrReceipt.control.totalIr;} const result=derive(x,frozen); assert.equal(result.verdict,expected,`check ${checks+1}: ${result.reason ?? ''}`); checks++;};
  check(()=>{},'GO'); check(x=>x.version=2); check(x=>x.hashes=[]); check(x=>x.inventory.pop()); check(x=>x.iai.pop()); check(x=>x.native.pop()); check(x=>x.schedule.reverse());
  check(x=>x.entry='AllocCore'); check(x=>x.strictTrim.cell='invented'); check(x=>x.ancestry.chain.reverse()); check(x=>delete x.ancestry.receipt); check(x=>delete x.featureExactReceipt);
  for(const key of ['inventoryReceipt','environmentReceipt','identityReviewReceipt'])check(x=>delete x[key]);
  // Amendment A: hot-control geometry is declared, segmentCount is null, allocator config is copied from a same-arm probe.
  const hotRows=(x,name)=>x.iai.filter(r=>r.name===name), h0='small_churn_16b';
  for(const arm of ['A1','B2']) {
    check(x=>x.iai.find(r=>r.name===h0&&r.arm===arm).config.requested.segmentCount=1);
    check(x=>x.iai.find(r=>r.name===h0&&r.arm===arm).config.resolved.segmentCount=1);
    check(x=>delete x.iai.find(r=>r.name===h0&&r.arm===arm).config.requested.segmentCount);
    check(x=>x.iai.find(r=>r.name===h0&&r.arm===arm).config.unobservedReasons.segmentCount='');
    check(x=>delete x.iai.find(r=>r.name===h0&&r.arm===arm).config.unobservedReasons);
    check(x=>delete x.iai.find(r=>r.name===h0&&r.arm===arm).config.declaredProvenance);
    check(x=>delete x.iai.find(r=>r.name===h0&&r.arm===arm).config.resolvedProvenance);
    check(x=>x.iai.find(r=>r.name===h0&&r.arm===arm).config.declaredProvenance.cell='churn_256b');
    check(x=>x.iai.find(r=>r.name===h0&&r.arm===arm).config.declaredProvenance.alignment=16);
    check(x=>x.iai.find(r=>r.name===h0&&r.arm===arm).config.resolvedProvenance.arm=arm.startsWith('B')?'A':'B');
    check(x=>x.iai.find(r=>r.name===h0&&r.arm===arm).config.resolvedProvenance.claimPath='lease');
    check(x=>x.iai.find(r=>r.name===h0&&r.arm===arm).config.resolvedProvenance.copiedFields.push('segmentCount'));
    check(x=>x.iai.find(r=>r.name===h0&&r.arm===arm).config.resolvedProvenance.copiedFields.pop());
    check(x=>x.iai.find(r=>r.name===h0&&r.arm===arm).config.resolvedProvenance.binarySha256='9'.repeat(64));
  }
  // wrong declared geometry, even identical in all five arms, is rejected
  check(x=>hotRows(x,'churn_256b').forEach(r=>{r.config.requested.workCount=65;r.config.resolved.workCount=65;}));
  check(x=>hotRows(x,'aligned_churn_640b_a128').forEach(r=>{r.config.requested.blockBytes=512;r.config.resolved.blockBytes=512;}));
  // segmentCount stays a positive observed number everywhere else
  for(const name of [...benefit,publication,producer]) check(x=>{const r=x.iai.find(q=>q.name===name&&q.arm==='A1'); r.config.requested.segmentCount=null; r.config.resolved.segmentCount=null;});
  check(x=>{const r=x.native[0]; r.config.requested.segmentCount=null; r.config.resolved.segmentCount=null;});
  for(const key of ['source','binarySha256','environmentSha256','features','startedAt','receipt','config','oracle','Ir'])check(x=>delete x.iai[0][key]);
  for(const key of counters) {    if(key!=='strict_trim_remaining')check(x=>x.iai[0].oracle[key]=null); check(x=>delete x.iai[0].oracle[key]);}
  check(x=>x.iai[0].oracle.routed_scans=0); check(x=>x.iai[2].oracle.routed_scans++); check(x=>x.iai[2].oracle.pending_words_visited=1); check(x=>x.iai[2].oracle.summary_words_visited=0);
  check(x=>x.iai[0].oracle.pending_words_visited+=3191); check(x=>x.iai[0].oracle.pending_words_exchanged--);
  check(x=>x.iai.find(r=>r.name===publication).oracle.discovery_records_reclaimed=0);
  check(x=>x.iai.find(r=>r.name===publication&&r.arm==='B2').oracle.strict_trim_remaining=1);
  for(const k of ['summary_rmw','summary_load']) {check(x=>x.iai.find(r=>r.name===producer&&r.arm==='B2').oracle[k]=0);check(x=>x.iai.find(r=>r.name===producer&&r.arm==='A1').oracle[k]=1);}
  check(x=>x.iai.find(r=>r.name===producer).scenario='not repeated');
  for(const arm of ['B1','B2']) {
    check(x=>x.iai.find(r=>r.name==='growth16_4n'&&r.arm===arm).Ir=451,'NO-GO');
    check(x=>x.iai.find(r=>r.name==='growth16_2n'&&r.arm===arm).Ir=100,'NO-GO');
    check(x=>x.iai.find(r=>r.name==='growth16_2n'&&r.arm===arm).Ir=99,'NO-GO');
  }
  for(const name of benefit.slice(0,2))for(const arm of ['B1','B2'])check(x=>x.iai.find(r=>r.name===name&&r.arm===arm).Ir=251,'NO-GO');
  // Isolate the A2 cross comparisons: B passes A1 but fails only A2.
  for(const name of benefit.slice(0,2))check(x=>x.iai.find(r=>r.name===name&&r.arm==='A2').Ir=999,'NO-GO');
  for(const name of hot)for(const sign of [-1,1]) {check(x=>x.iai.find(r=>r.name===name&&r.arm==='B2').Ir+=sign*10,'GO');check(x=>x.iai.find(r=>r.name===name&&r.arm==='B2').Ir+=sign*11,'NO-GO');}
  for(const name of mi)check(x=>x.iai.find(r=>r.name===name&&r.arm==='B2').Ir++,'NO-GO');
  for(const arm of ['B1','B2']) {check(x=>x.iai.find(r=>r.name===producer&&r.arm===arm).Ir+=6*4096,'GO');check(x=>x.iai.find(r=>r.name===producer&&r.arm===arm).Ir+=6*4096+1,'NO-GO');}
  check(x=>x.sizeOf.B++,'NO-GO'); check(x=>delete x.sizeOf.receipt);
  check(x=>x.iai.forEach(r=>{if(r.arm==='A1')r.arm='B1';else if(r.arm==='B1')r.arm='A1';}));
  check(x=>x.iai.forEach(r=>r.binarySha256='0'.repeat(64))); check(x=>x.iai[1].binarySha256='0'.repeat(64)); check(x=>x.iai[0].source.identitySha256='0'.repeat(64));
  check(x=>x.identities.C0.binarySha256=x.identities.A.binaries.iai); check(x=>x.identities.B.label='A'); check(x=>x.identities.B.toolchain.rustc='other');
  check(x=>x.identities.A.buildStartedAt=x.identities.A.source.capturedAt); check(x=>x.iai[0].startedAt=x.identities.A.buildStartedAt);
  check(x=>x.iai[0].features.push('alloc-stats')); check(x=>x.iai[0].oracle.linkage.cfg=[]); check(x=>x.iai[0].oracle.linkage.binarySha256=x.identities.A.binaries.iai);
  check(x=>x.iai[0].config.resolved.workCount++); check(x=>x.iai[0].config.configConflictsDelta=1); check(x=>x.iai[0].config.processBoundary='same-process'); check(x=>x.iai[0].config.runId=x.iai[1].config.runId);
  check(x=>x.iai[0].Ir=NaN); check(x=>x.iai[0].Ir=-1); check(x=>x.iai[0].oracle.nonempty_cuts=1);
  check(x=>x.iai.find(r=>hot.includes(r.name)).oracle.routed_scans=0); check(x=>x.iai.find(r=>hot.includes(r.name)).oracle.unobservedReasons={});
  check(x=>x.native[0].position++); check(x=>x.native[0].repetition++); check(x=>x.native[0].n=3); check(x=>x.native[0].entry='direct'); check(x=>x.native[0].features.push('internals'));
  check(x=>x.native[0].config.runId=x.native[1].config.runId); check(x=>x.native[0].binarySha256=x.identities.B.binaries.native);
  for(const n of [1,2,4,8])for(const [metric,limit] of [['p50Ns',5],['p99Ns',20]]) {
    check(x=>x.native.filter(r=>r.n===n&&r.arm==='B').forEach(r=>r[metric]+=limit),'GO');
    check(x=>x.native.filter(r=>r.n===n&&r.arm==='B').forEach(r=>r[metric]+=limit+.01),'NO-GO');
    check(x=>x.native.filter(r=>r.n===n&&r.arm==='AA').forEach(r=>r[metric]+=limit),'GO');
    check(x=>x.native.filter(r=>r.n===n&&r.arm==='AA').forEach(r=>r[metric]+=limit+.01),'INCONCLUSIVE');
  }
  for(const name of hot) {
    check(x=>{ x.iai.find(r=>r.name===name&&r.arm==='A2').Ir=990; x.iai.find(r=>r.name===name&&r.arm==='B2').Ir=1001; },'NO-GO');
    check(x=>{ x.iai.find(r=>r.name===name&&r.arm==='C0').Ir=1020; x.iai.find(r=>r.name===name&&r.arm==='B1').Ir=1020; },'GO');
    check(x=>{ x.iai.find(r=>r.name===name&&r.arm==='C0').Ir=1020; x.iai.find(r=>r.name===name&&r.arm==='B1').Ir=1021; },'NO-GO');
  }
  check(x=>{ x.iai.find(r=>r.name===producer&&r.arm==='A2').Ir=999; x.iai.find(r=>r.name===producer&&r.arm==='B1').Ir=1000+6*4096; },'NO-GO');
  for(const name of benefit.slice(2))check(x=>{ const r=x.iai.find(r=>r.name===name); r.config.requested.workCount++; r.config.resolved.workCount++; });
  for(const key of ['baseHead','patchSha256','untrackedSha256','identitySha256','capturedAt'])check(x=>delete x.identities.A.source[key]);
  for(const key of ['p50Ns','p99Ns','nsPerOp','source','binarySha256','environmentSha256','features','startedAt','receipt','config'])check(x=>delete x.native[0][key]);
  check(x=>x.native[0].p99Ns=99); check(x=>x.native[0].nsPerOp=0);
  check(x=>x.native[0].config.configConflictsDelta=1); check(x=>x.native[0].config.resolved.segmentCount=2);
  check(x=>x.native[0].environmentSha256='0'.repeat(64)); check(x=>x.native[0].startedAt=x.identities.A.buildStartedAt);
  check(x=>x.iai[0].oracle.linkage.featureExactReceipt={...x.iai[0].oracle.linkage.featureExactReceipt,sha256:'0'.repeat(64)});
  check(x=>x.iai[0].oracle.linkage.primaryBinarySha256='0'.repeat(64));
  check(x=>x.iai[0].oracle.linkage.config.runId=x.iai[0].config.runId);
  check(x=>x.iai[0].oracle.linkage.config.resolved.workCount++);
  check(x=>x.iai[0].oracle.linkage.startedAt=x.identities.A.buildStartedAt);
  check(x=>x.iai[0].config.pid=0); check(x=>x.iai[0].receipt.path='../outside');
  check(x=>x.iai[0].features.push('production')); check(x=>x.identities.A.primaryCfg.push('r19_sidecar_summary_oracle'));
  check(x=>x.iai[0].oracle.extra_counter=0); check(x=>x.iai[1].arm='A1');
  check(x=>x.iai.find(r=>mi.includes(r.name)).config.resolved={});
  check(x=>delete x.iai.find(r=>mi.includes(r.name)).config.notApplicableReason);
  check(x=>x.iai[0].oracle.strict_trim_remaining=0);
  check(x=>delete x.iai[0].oracle.unobservedReasons.strict_trim_remaining);
  check(x=>{x.iai[0].oracle.pending_words_visited+=3190;x.iai[0].oracle.pending_words_exchanged+=3190;},'GO');
  check(x=>{for(const r of x.iai.filter(r=>r.arm==='B1'))r.binarySha256=x.identities.B.binaries.iai;
    x.identities.B.binaries.oracle=x.identities.B.binaries.native;
    for(const r of x.iai.filter(r=>r.arm.startsWith('B')&&r.oracle.linkage))r.oracle.linkage.binarySha256=x.identities.B.binaries.native;
    for(const r of x.iai.filter(r=>r.arm.startsWith('B')&&hot.includes(r.name)))r.config.resolvedProvenance.binarySha256=x.identities.B.binaries.native;},'GO');
  // Owner freezes only an upper slope bound; no unapproved lower bound.
  check(x=>x.iai.find(r=>r.name==='growth16_4n'&&r.arm==='B1').Ir=199,'GO');
  for(const k of ['binarySha256','source','environmentSha256','features','cfg','receipt','symbolReceipt','profileReceipt','symbol','runnerVersion','valgrindVersion','collectAtStart','defaultToggle','entryPoint','separateThreads','toggleCollect','generatedArguments','aggregation','childFreeCount','childIr','totalIr','startedAt','control'])check(x=>delete x.iai.find(r=>r.name===producer).producerIrReceipt[k]);
  check(x=>delete x.iai.find(r=>r.name===producer).producerIrReceipt);
  const pr=x=>x.iai.find(r=>r.name===producer).producerIrReceipt;
  check(x=>pr(x).source={...pr(x).source,identitySha256:'0'.repeat(64)});
  check(x=>pr(x).binarySha256='0'.repeat(64)); check(x=>pr(x).environmentSha256='0'.repeat(64));
  check(x=>pr(x).childFreeCount=4095); check(x=>pr(x).childIr=0); check(x=>pr(x).childIr=-1);
  check(x=>pr(x).childIr=pr(x).totalIr+1);
  check(x=>pr(x).collectAtStart=true); check(x=>pr(x).defaultToggle=true); check(x=>pr(x).entryPoint='Default');
  check(x=>pr(x).symbol='broad'); check(x=>pr(x).toggleCollect='broad'); check(x=>pr(x).separateThreads=false);
  check(x=>pr(x).aggregation='parent only'); check(x=>pr(x).totalIr++);
  check(x=>pr(x).control.childIr=1); check(x=>pr(x).control.totalIr=pr(x).totalIr);
  // Amendment B: shared bounded boundary overhead, workload remainder, control linkage.
  const cr=x=>pr(x).control;
  const setOverhead=(x,o)=>{pr(x).boundaryOverheadIr=o;cr(x).childIr=o;pr(x).workloadIr=pr(x).childIr-o;};
  check(x=>setOverhead(x,0)); check(x=>setOverhead(x,17)); check(x=>setOverhead(x,16),'GO'); check(x=>setOverhead(x,1),'GO');
  check(x=>pr(x).workloadIr=pr(x).workloadIr+1);
  check(x=>{pr(x).childIr=pr(x).boundaryOverheadIr;pr(x).workloadIr=0;});
  check(x=>cr(x).workloadIr=1); check(x=>cr(x).childFreeCount=1); check(x=>pr(x).boundaryExecutions=2); check(x=>cr(x).boundaryExecutions=0);
  check(x=>cr(x).childIr=cr(x).totalIr+1);
  for(const k of ['symbolReceipt','profileReceipt','symbol','toggleCollect','cfg','features','source','generatedArguments','runnerVersion','valgrindVersion','collectAtStart','defaultToggle','entryPoint','separateThreads','aggregation','workloadIr','childFreeCount','boundaryExecutions'])check(x=>delete cr(x)[k]);
  for(const k of ['boundaryOverheadIr','workloadIr','boundaryExecutions'])check(x=>delete pr(x)[k]);
  check(x=>cr(x).symbol='<other>::producer_free_window'); check(x=>cr(x).toggleCollect='*'); check(x=>cr(x).cfg=[]);
  check(x=>cr(x).cfg=['r19_prf01_producer_control','extra']); check(x=>cr(x).features=[...primary,'alloc-stats']);
  check(x=>cr(x).source={...cr(x).source,identitySha256:'0'.repeat(64)});
  check(x=>cr(x).collectAtStart=true); check(x=>cr(x).separateThreads=false); check(x=>cr(x).entryPoint='Default'); check(x=>cr(x).defaultToggle=true);
  check(x=>pr(x).control.deltaIr=0); check(x=>pr(x).control.parentSetupUnchanged=false);
  check(x=>pr(x).control.collectionUnchanged=false); check(x=>pr(x).control.environmentSha256='0'.repeat(64));
  check(x=>pr(x).control.binarySha256=pr(x).binarySha256); check(x=>delete pr(x).control.receipt);
  assert.equal(derive({},frozen).verdict,'INCOMPLETE');checks++;
  console.log(JSON.stringify({version:1,status:'PASS',scope:'synthetic selftest only',checks,measurements:0}));
}
const args=process.argv.slice(2);
try {
  if(args.length===1&&args[0]==='--hashes')process.stdout.write(hashText());
  else if(args.length===1&&args[0]==='--selftest')selftest();
  else {
    assert.equal(readFileSync(new URL('docs/perf/R19_PRF01_SIDECAR_SUMMARY_PREREG.sha256',root),'utf8'),hashText());
    if(args.length===1&&args[0]==='--check-hashes')console.log(JSON.stringify({version:1,status:'PASS',check:'raw document/script hashes; no self-hash'}));
    else {assert.ok(args.length===2&&args[0]==='--input','expected --input FILE; no data means INCOMPLETE');const result=derive(JSON.parse(readFileSync(resolve(args[1]),'utf8')),hashes());console.log(JSON.stringify(result,null,2));process.exitCode=result.verdict==='GO'?0:1;}
  }
} catch(e) {console.log(JSON.stringify({version:1,verdict:'INCOMPLETE',reason:e.message}));process.exitCode=1;}
