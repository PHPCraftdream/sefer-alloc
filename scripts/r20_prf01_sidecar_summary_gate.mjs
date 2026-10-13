// Read-only R20 PRF-01 v2 judge: no builds, subprocesses, measurements or writes.
// Fork of scripts/r19_prf01_sidecar_summary_gate.mjs; what changed is documented
// in docs/perf/R20_PRF01_SIDECAR_SUMMARY_V2_PREREG.md (control geometry per
// docs/perf/R20_CONTROL_GEOMETRY_RULE.md, direct construction cell with
// budget_K = 0, layout-noise ceiling, native axis DEFERRED).
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
const root = new URL('../', import.meta.url);
const files = ['docs/perf/R20_PRF01_SIDECAR_SUMMARY_V2_PREREG.md', 'scripts/r20_prf01_sidecar_summary_gate.mjs',
  'scripts/r20_prf01_sidecar_summary_run.mjs', 'docs/perf/R20_CONTROL_GEOMETRY_RULE.md',
  'benches/r20_prf01_controls_iai.rs', 'benches/support/r20_prf01_controls.rs',
  'benches/support/r19_prf01.rs', 'benches/perf_gate_iai.rs', 'tests/r19_prf01_sidecar_oracle.rs',
  'examples/r19_prf01_oracle.rs'];
const sha = b => createHash('sha256').update(b).digest('hex');
const hashes = () => files.map(path => ({path, sha256:sha(readFileSync(new URL(path, root)))}));
const hashText = () => hashes().map(r => `${r.sha256}  ${r.path}\n`).join('');
const arms = ['A1','A2','B1','B2','C0'];
const primary = ['bench-internals','internals','production'];
const benefit = ['multiseg_cold_256k','seg_cycle_decommit_256k','growth16_n','growth16_2n','growth16_4n'];
// v2: the hot controls are JUDGED on the N/2N slope (rule 2.2), each family with
// its own 2N and 4N sibling in benches/r20_prf01_controls_iai.rs (rule 2.5).
const hotFamilies = ['small_churn_16b','churn_256b','churn_write_256b','aligned_churn_640b_a128'];
const hotCells = hotFamilies.flatMap(f => ['n','2n','4n'].map(s => `r20_prf01_${f}_${s}`));
const construction = 'r20_prf01_start_one_alloc_free_16b';
const mi = ['mimalloc_small_churn_16b','mimalloc_small_churn_16b_2n','mimalloc_churn_256b','mimalloc_cold_alloc_free_256x16b','mimalloc_cold_alloc_free_256x16b_2n','mimalloc_cold_alloc_free_256x16b_4n','mimalloc_cold_alloc_only_256x16b','mimalloc_cold_alloc_only_256x16b_2n','mimalloc_cold_alloc_only_256x16b_4n','mimalloc_cold_alloc_free_256x64b','mimalloc_recycle_alloc_free_256x16b','mimalloc_recycle_alloc_free_256x64b','mimalloc_bootstrap_proxy'];
const publication = 'r19_prf01_publication';
const producer = 'r19_prf01_joined_producer_same_word';
const inventory = [...benefit,...hotCells,construction,...mi,publication,producer];
// Amendment A declarations of the v2 cells: [workCount, blockBytes, alignment].
const OPS = {n:64,'2n':128,'4n':256};
const hotDecl = {r20_prf01_start_one_alloc_free_16b:[1,16,8],
  ...Object.fromEntries(hotFamilies.flatMap(f => Object.entries(OPS).map(([s,ops]) =>
    [`r20_prf01_${f}_${s}`, [ops, f === 'small_churn_16b' ? 16 : f === 'aligned_churn_640b_a128' ? 640 : 256, f === 'aligned_churn_640b_a128' ? 128 : 8]])))};
const counters = ['pending_words_visited','pending_words_exchanged','nonempty_cuts','discovery_records_reclaimed','routed_scans','summary_words_visited','summary_rmw','summary_load','strict_trim_remaining'];
const text = x => assert.ok(typeof x === 'string' && x.trim().length > 0);
const hex = x => { text(x); assert.match(x,/^[0-9a-f]{64}$/); };
const commit = x => { text(x); assert.match(x,/^[0-9a-f]{40}$/); };
const nat = x => assert.ok(Number.isSafeInteger(x) && x >= 0);
const positive = x => assert.ok(Number.isFinite(x) && x > 0);
const exact = (a,b) => assert.deepEqual(a,b);
const features = (a,b) => { assert.ok(Array.isArray(a)); exact([...a].sort(),[...b].sort()); };
const time = x => { text(x); assert.match(x,/^\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d(?:\.\d{3})?Z$/); const t=Date.parse(x); assert.ok(Number.isFinite(t)); return t; };
const receiptText = r => { text(r); assert.ok(!/[\:]/.test(r) && !r.includes('..')); };
const BIG = x => { nat(x); return BigInt(x); };
const ABS = x => { assert.ok(typeof x==='bigint'); return x < 0n ? -x : x; };
const MAXB = (a,b,c) => { let m=a; if(b>m)m=b; if(c>m)m=c; return m; };
// Layout-noise ceiling (PRF-04 lesson): numerator |A1-A2| or |C0-A1|, denominator the A1 value.
const noiseCeiling = a1 => Math.max(10, Math.floor(a1 / 200));
const pct = (num, den) => `${num < 0n ? '-' : '+'}${(num < 0n ? -num : num) * 100n * 100n / den}/10000 (numerator ${num}, denominator ${den})`;
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
    // The body runs only under Callgrind; geometry fields are inventory DECLARATIONS
    // (Amendment A of R19, carried over; segmentCount is unobserved, null).
    const [work,block,alignment] = hotDecl[ctx.name]; assert.ok(Array.isArray(hotDecl[ctx.name]));
    assert.strictEqual(c.requested.segmentCount,null); assert.strictEqual(c.resolved.segmentCount,null);
    assert.equal(c.requested.workCount,work); assert.equal(c.requested.blockBytes,block);
    assert.ok(c.unobservedReasons && typeof c.unobservedReasons==='object'); text(c.unobservedReasons.segmentCount);
    const d=c.declaredProvenance; assert.ok(d && typeof d==='object'); text(d.receipt); assert.equal(d.cell,ctx.name);
    exact(d.fields,['workCount','blockBytes','alignment']); assert.equal(d.alignment,alignment);
    const r=c.resolvedProvenance; assert.ok(r && typeof r==='object'); text(r.runId); nat(r.pid); assert.ok(r.pid>0);
    assert.equal(r.arm,ctx.arm); assert.equal(r.binarySha256,ctx.pins[ctx.arm].binaries.oracle); text(r.receipt);
    assert.equal(r.claimPath,'global'); exact([...r.copiedFields].sort(),['cachePolicy','configConflictsDelta','maintenance']);
  } else {
    nat(c.requested.segmentCount); assert.ok(c.requested.segmentCount>0);
  }
  assert.equal(c.configConflictsDelta,0); assert.equal(c.processBoundary,'isolated-process'); text(c.runId); nat(c.pid); assert.ok(c.pid>0);
}
function derive(d,frozen) {
  const tables={iai:[],slope:[],hot:[],construction:[],oracle:[],native:[],size:[]};
  try {
    assert.equal(d.version,1); exact(d.hashes,frozen); hex(d.environmentSha256);
    for(const k of ['inventoryReceipt','identityReviewReceipt','environmentReceipt','oracleReceipt'])receiptText(d[k]);
    exact(d.inventory,inventory);
    assert.equal(d.entry,'SeferAlloc::GlobalAlloc->HeapCore routed');
    // Native axis (owner requirement, mode 2 (ii)) is DEFERRED to a quiet bare-metal
    // host; it is NOT launched in v2 and the judge refuses a missing deferral marker.
    assert.ok(d.nativeDeferred && typeof d.nativeDeferred==='object');
    assert.equal(d.nativeDeferred.deferred,true); text(d.nativeDeferred.method);
    assert.match(d.nativeDeferred.method,/batch/,'deferred method must describe the batch-timer replacement');
    const pins=d.identities;
    for(const arm of ['A','B']) {
      const p=pins[arm]; assert.equal(p.label,arm); source(p.source); assert.equal(p.measurementCommit,p.source.baseHead); commit(p.measurementCommit);
      assert.equal(p.environmentSha256,d.environmentSha256);
      for(const k of ['iai','controls','oracle','control'])hex(p.binaries[k]);
      assert.ok(time(p.source.capturedAt)<time(p.buildStartedAt));
      features(p.features,primary);
      exact(p.oracleCfg,['r19_sidecar_summary_oracle']); exact(p.primaryCfg,[]);
      text(p.buildReceipt);
    }
    hex(pins.C0.binarySha256); exact(pins.C0.source,pins.A.source); text(pins.C0.buildReceipt);
    assert.ok(time(pins.C0.source.capturedAt)<time(pins.C0.buildStartedAt));
    assert.notEqual(pins.C0.binarySha256,pins.A.binaries.iai);
    assert.notEqual(pins.A.source.identitySha256,pins.B.source.identitySha256);
    assert.notEqual(pins.A.measurementCommit,pins.B.measurementCommit);
    exact(pins.A.toolchain,pins.B.toolchain); for(const k of ['rustc','target','profile','flags'])text(pins.A.toolchain[k]);
    const runs=new Set();
    function provenance(r,arm,binary) {
      const p=pins[arm]; exact(r.source,p.source); assert.equal(r.binarySha256,binary); assert.equal(r.environmentSha256,d.environmentSha256);
      assert.ok(time(r.startedAt)>time(p.buildStartedAt)); features(r.features,primary); receiptText(r.receipt);
      config(r.config,mi.includes(r.name)?'mimalloc':hotDecl[r.name]?'hot':'normal',{name:r.name,arm,pins});
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
      assert.equal(l.environmentSha256,d.environmentSha256); text(l.receipt); text(l.featureExactReceipt);
      assert.ok(time(l.startedAt)>time(p.buildStartedAt));
      assert.equal(l.primaryBinarySha256,r.binarySha256);
      exact(l.config.requested,r.config.requested); config(l.config);
      assert.ok(!runs.has(l.config.runId)); runs.add(l.config.runId);
    }
    assert.ok(d.iai.length<=inventory.length*5,'iai row count exceeds the inventory');
    let mode1Fail=false,mode2Fail=false,noisy=false,invalid=false;
    const unjudgeable=[];
    const cells=new Map();
    for(const name of inventory) {
      if(name===construction && arms.some(a=>!d.iai.some(r=>r.name===name&&r.arm===a))) {
        unjudgeable.push('construction'); tables.construction.push({cell:construction,status:'UNJUDGEABLE (no direct construction cell; excluded, never a pass)'});
      }
      if(!unjudgeable.includes('construction') || name!==construction) {
      const rs=arms.map(arm=>{const found=d.iai.filter(r=>r.name===name && r.arm===arm); assert.equal(found.length,1); return found[0];}); cells.set(name,rs);
      const needed=benefit.includes(name)||name===publication||name===producer;
      for(const r of rs) {
        const a=r.arm.startsWith('B')?'B':'A';
        provenance(r,a,r.arm==='C0'?pins.C0.binarySha256:pins[a].binaries.iai);
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
          features(p.features,primary); exact(p.cfg,[]); text(p.receipt); text(p.symbolReceipt); text(p.profileReceipt); text(p.control.receipt);
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
          text(c.symbolReceipt); text(c.profileReceipt); assert.equal(c.symbol,p.symbol); assert.equal(c.toggleCollect,p.symbol);
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
      const [a1,a2,b1,b2,c0]=rs.map(r=>r.Ir);
      // Benefit / producer / mimalloc cross pairs (carried over from R19, exact BigInt arithmetic).
      for(const a of rs.slice(0,2))for(const b of rs.slice(2,4)) {
        const delta=BigInt(b.Ir)-BigInt(a.Ir); let percent=null,pass=true,mode='descriptive';
        if(benefit.slice(0,2).includes(name)) { percent=pct(BigInt(b.Ir)*4n,BigInt(a.Ir)); pass=BigInt(b.Ir)*4n<=BigInt(a.Ir); mode='mode1'; }
        if(mi.includes(name)) { pass=delta===0n; mode='mode1'; }
        if(name===producer) { percent=pct(delta,4096n); pass=delta<=24576n; mode='mode2'; }
        if(mode==='mode1')mode1Fail ||= !pass; if(mode==='mode2')mode2Fail ||= !pass;
        tables.iai.push({name,A:a.arm,B:b.arm,delta:Number(delta),mode,...(percent?{percent}:{pass:delta===0n}),pass});
      }
      if(needed)tables.oracle.push({name,arms:rs.map(r=>({arm:r.arm,...r.oracle}))});
      }
    }
    // growth16 slope of B, unchanged owner threshold <= 2.5 (exact BigInt).
    for(const arm of ['B1','B2']) {
      const [n,n2,n4]=benefit.slice(2).map(name=>cells.get(name).find(r=>r.arm===arm).Ir);
      const denominator=BigInt(n2)-BigInt(n),numerator=BigInt(n4)-BigInt(n2);
      const pass=denominator>0n && numerator*2n<=denominator*5n; mode1Fail ||= !pass;
      tables.slope.push({arm,numerator:numerator.toString(),denominator:denominator.toString(),limit:'<= 2.5 (owner)',pass});
    }
    // Hot families: per-operation neutrality on the N/2N slope (rule 2.2), linearity on 4N (2.3),
    // layout-noise ceiling on the raw N cell and both windows.
    for(const f of hotFamilies)judgeHotFamily(f);
    function judgeHotFamily(f) {
      const cell=s=>`r20_prf01_${f}_${s}`;
      if(['n','2n','4n'].some(s=>arms.some(a=>!d.iai.some(r=>r.name===cell(s)&&r.arm===a)))) {
        unjudgeable.push(f); tables.hot.push({family:f,status:'UNJUDGEABLE (missing sibling; excluded, never a pass)'}); return;
      }
      const get=(a,s)=>cells.get(cell(s)).find(r=>r.arm===a).Ir;
      const row={family:f,status:'judged',arms:{},checks:{}};
      const ir=a=>({n:get(a,'n'),'2n':get(a,'2n'),'4n':get(a,'4n')});
      for(const a of arms)row.arms[a]=ir(a);
      const s=a=>BIG(ir(a)['2n'])-BIG(ir(a).n);
      const tSraw=MAXB(BigInt(10),ABS(s('A1')-s('A2')),ABS(s('C0')-s('A1')));
      row.T_s=Number(tSraw);
      row.checks.slope=[];
      for(const a of ['A1','A2'])for(const b of ['B1','B2']) {
        const delta=s(b)-s(a);
        const pass=delta<=tSraw;
        row.checks.slope.push({pair:`${b} vs ${a}`,slopeDelta:delta.toString(),T_s:tSraw.toString(),check:'two-sided |slope_B - slope_A| <= T_s',pass});
        if(!pass)mode1Fail=true;
      }
      // Linearity (rule 2.3): hot churn is expected affine; failure is INVALID.
      row.checks.linearity=arms.map(a=>{
        const dev=ABS((BIG(ir(a)['4n'])-BIG(ir(a)['2n']))-2n*s(a));
        return {arm:a,deviation:dev.toString(),bound:tSraw.toString(),pass:dev<=tSraw};
      });
      if(row.checks.linearity.some(x=>!x.pass))invalid=true;
      // Noise ceiling per raw N cell and per window; violation is INCONCLUSIVE, never a wider budget.
      const val=(a,label)=>label==='n'?BIG(ir(a).n):label==='n->2n'?BIG(ir(a)['2n'])-BIG(ir(a).n):BIG(ir(a)['4n'])-BIG(ir(a)['2n']);
      row.checks.noise=['n','n->2n','2n->4n'].map(label=>{
        const a1v=val('A1',label),ceiling=noiseCeiling(Number(a1v));
        const aa=ABS(val('A1',label)-val('A2',label)),c0=ABS(val('C0',label)-val('A1',label));
        const pass=aa<=BigInt(ceiling)&&c0<=BigInt(ceiling);
        if(!pass)noisy=true;
        return {quantity:label,aa:aa.toString(),c0:c0.toString(),ceiling:`max(10, floor(A1/200)) = ${ceiling} (numerator |A1-A2| or |C0-A1|, denominator A1=${Number(a1v)})`,pass};
      });
      if(!row.checks.noise.every(x=>x.pass))row.status='INCONCLUSIVE (layout noise above the ceiling)';
      tables.hot.push(row);
    }
    // Construction: the direct start cell, one-sided budget_K = 0 net work plus the
    // qualified noise T_s of that cell (owner decision 2026-10-12). An unexplained
    // GAIN is never an automatic pass: delta < -T_s requires a constructionGainReceipt.
    if(!arms.every(a=>d.iai.some(r=>r.name===construction&&r.arm===a))) {
      unjudgeable.push('construction'); tables.construction.push({cell:construction,status:'UNJUDGEABLE (no direct construction cell; excluded, never a pass)'});
    } else {
      const rs=cells.get(construction);
      const v=a=>rs.find(r=>r.arm===a).Ir;
      const row={cell:construction,status:'judged',arms:Object.fromEntries(arms.map(a=>[a,v(a)])),checks:{}};
      const tSraw=MAXB(BigInt(10),ABS(BIG(v('A1'))-BIG(v('A2'))),ABS(BIG(v('C0'))-BIG(v('A1'))));
      row.T_s=Number(tSraw); row.budget_K='0 Ir of net work (owner, 2026-10-12) + T_s';
      const ceil=noiseCeiling(v('A1'));
      const aa=ABS(BIG(v('A1'))-BIG(v('A2'))),c0=ABS(BIG(v('C0'))-BIG(v('A1')));
      row.checks.noise={aa:aa.toString(),c0:c0.toString(),ceiling:`max(10, floor(A1/200)) = ${ceil}`,pass:aa<=BigInt(ceil)&&c0<=BigInt(ceil)};
      if(!row.checks.noise.pass)noisy=true;
      row.checks.R_construct=[];
      for(const a of ['A1','A2'])for(const b of ['B1','B2']) {
        const delta=BIG(v(b))-BIG(v(a));
        const pass=delta<=tSraw;
        const gain=delta<-tSraw;
        let failure=null;
        if(gain) {
          const recs=['B1','B2'].map(arm=>rs.find(r=>r.arm===arm).constructionGainReceipt);
          const gainMissing=recs.some(r=>!r);
          if(gainMissing)failure='INCOMPLETE (unexplained construction gain: constructionGainReceipt missing)';
          else if(recs.some(r=>r.removedPassObserved!==true||typeof r.path!=='string'||!r.path.trim()||typeof r.sha256!=='string'||!/^[0-9a-f]{64}$/.test(r.sha256))) {
            invalid=true; row.status='INVALID (construction gain receipt does not observe the removed pass)';
          }
        }
        row.checks.R_construct.push({pair:`${b} vs ${a}`,delta:delta.toString(),budget_K:`0 + ${tSraw.toString()} (T_s)`,pass,gain:gain?(failure??(invalid?'INVALID (unexplained gain)':'explained by the retained decomposition receipt')):null});
        if(failure)throw new Error(failure);
        if(!pass)mode2Fail=true;
      }
      tables.construction.push(row);
    }
    nat(d.sizeOf.A); nat(d.sizeOf.B); assert.ok(d.sizeOf.A>0); text(d.sizeOf.receipt);
    const sizePass=d.sizeOf.B-d.sizeOf.A===512; mode1Fail ||= !sizePass;
    tables.size.push({A:d.sizeOf.A,B:d.sizeOf.B,numerator:d.sizeOf.B-d.sizeOf.A,expected:512,pass:sizePass});
    // Native axis: DEFERRED (never launched in v2). Any deterministic failure still => NO-GO.
    const modes={mode1:mode1Fail?'NO-GO':'GO',mode2Iai:mode2Fail?'NO-GO':'GO',mode2Native:'DEFERRED'};
    tables.native.push({axis:'mode2 (ii) native paired A/B',status:'DEFERRED',reason:d.nativeDeferred.method});
    let verdict;
    if(invalid)verdict='INVALID';
    else if(mode1Fail||mode2Fail)verdict='NO-GO';
    else if(noisy)verdict='INCONCLUSIVE';
    else if(unjudgeable.length)verdict='UNJUDGEABLE';
    else verdict='INCONCLUSIVE (native deferred)';
    return {version:1,verdict,modes,unjudgeable,tables};
  } catch(e) { return {version:1,verdict:'INCOMPLETE',modes:{mode1:'INCOMPLETE',mode2Iai:'INCOMPLETE',mode2Native:'DEFERRED'},reason:e.message,tables}; }
}
function fixture(frozen) {
  const h=c=>c.repeat(64),src=c=>({baseHead:c.repeat(40),patchSha256:h(c),untrackedSha256:h(c),identitySha256:h(c),capturedAt:'2026-10-11T00:00:00Z'});
  const p=(c,bs)=>({label:c==='a'?'A':'B',source:src(c),measurementCommit:c.repeat(40),environmentSha256:h('e'),binaries:{iai:h(bs[0]),controls:h(bs[3]),oracle:h(bs[1]),control:h(bs[2])},buildStartedAt:'2026-10-11T01:00:00Z',features:primary,primaryCfg:[],oracleCfg:['r19_sidecar_summary_oracle'],buildReceipt:'synthetic/build',toolchain:{rustc:'synthetic',target:'synthetic',profile:'bench',flags:'none'}});
  let id=0;
  const cfg=(workCount=1,blockBytes=16)=>{const requested={maintenance:false,workCount,segmentCount:1,blockBytes,cachePolicy:'default'};return {requested,resolved:{...requested},configConflictsDelta:0,processBoundary:'isolated-process',runId:`synthetic-${++id}`,pid:id};};
  const hotCfg=(name,a)=>{const [w,b,al]=hotDecl[name];const requested={maintenance:false,workCount:w,segmentCount:null,blockBytes:b,cachePolicy:'default'};return {requested,resolved:{...requested},configConflictsDelta:0,processBoundary:'isolated-process',runId:`synthetic-${++id}`,pid:id,unobservedReasons:{segmentCount:'measured body has no paired geometry observer'},declaredProvenance:{receipt:'synthetic/inventory',cell:name,fields:['workCount','blockBytes','alignment'],alignment:al},resolvedProvenance:{runId:'probe-'+id,pid:999,arm:a,binarySha256:a==='B'?h('6'):h('2'),receipt:'synthetic/probe',claimPath:'global',copiedFields:['cachePolicy','configConflictsDelta','maintenance']}};};
  const d={version:1,hashes:frozen,entry:'SeferAlloc::GlobalAlloc->HeapCore routed',environmentSha256:h('e'),inventoryReceipt:'synthetic/inventory',identityReviewReceipt:'synthetic/review',environmentReceipt:'synthetic/env',oracleReceipt:'synthetic/oracle',nativeDeferred:{deferred:true,method:'batch-timer native paired A/B on a quiet bare-metal host (deferred; not launched in v2)'},identities:{A:p('a','1234'),B:p('b','5678'),C0:{source:src('a'),binarySha256:h('7'),buildStartedAt:'2026-10-11T01:00:00Z',buildReceipt:'synthetic/c0'}},ancestry:{result:'A ancestor B',chain:['a'.repeat(40),'b'.repeat(40)],receipt:'synthetic/ancestry'},sizeOf:{A:41984,B:42496,receipt:'synthetic/size'},inventory,iai:[]};
  function row(arm,binary,name,configuration){return {name,arm,environmentSha256:d.environmentSha256,binarySha256:binary,source:d.identities[arm.startsWith('B')?'B':'A'].source,startedAt:'2026-10-11T02:00:00Z',features:primary,receipt:'synthetic/log',config:configuration};}
  const linkage=(r,pin)=>({binarySha256:pin.binaries.oracle,primaryBinarySha256:r.binarySha256,source:r.source,features:primary,cfg:['r19_sidecar_summary_oracle'],environmentSha256:d.environmentSha256,receipt:'synthetic/oracle-log',featureExactReceipt:'synthetic/review',startedAt:r.startedAt,config:{...r.config,runId:`link-${++id}`}});
  for(const name of inventory)for(const arm of arms) {
    const a=arm.startsWith('B')?'B':'A',isB=a==='B',pin=d.identities[a];
    const isHot=hotDecl[name]!==undefined,work=isHot?hotDecl[name][0]:name===producer?4096:name.startsWith('growth16_')?[32768,65536,131072][benefit.indexOf(name)-2]:1;
    const r={...row(arm,arm==='C0'?h('7'):pin.binaries.iai,name,isHot?hotCfg(name,a):cfg(work,name.startsWith('growth16_')||name===producer?16:258752))};
    if(mi.includes(name))r.config={requested:null,resolved:null,configConflictsDelta:null,notApplicableReason:'mimalloc does not resolve SeferAlloc configuration',processBoundary:'isolated-process',runId:r.config.runId,pid:r.config.pid};
    const needed=benefit.includes(name)||name===publication||name===producer;
    r.oracle=Object.fromEntries(counters.map(k=>[k,needed?0:null])); r.oracle.unobservedReasons=needed?{}:Object.fromEntries(counters.map(k=>[k,'compiled out in primary; no paired observer for this control'])); r.oracle.linkage=null;
    if(needed) {
      r.oracle.linkage=linkage(r,pin);
      if(benefit.includes(name)) { r.oracle.routed_scans=99; if(isB)r.oracle.summary_words_visited=64; else {const v=name===benefit[0]?319063:name===benefit[1]?1004503:100; r.oracle.pending_words_visited=v; r.oracle.pending_words_exchanged=v;} }
      if(name===publication) {r.oracle.routed_scans=1;r.oracle.discovery_records_reclaimed=34;r.oracle.strict_trim_remaining=0;}
      else {r.oracle.strict_trim_remaining=null;r.oracle.unobservedReasons={strict_trim_remaining:'No strict trim remainder observation in this workload window'};}
      if(name===producer) {r.scenario='joined remote Small frees; repeated frees in same pending word; no owner discovery during producer window';if(isB){r.oracle.summary_rmw=1;r.oracle.summary_load=4096;}}
    }
    if(name===producer) r.producerIrReceipt={binarySha256:r.binarySha256,source:r.source,environmentSha256:r.environmentSha256,features:primary,cfg:[],receipt:'synthetic/p',symbolReceipt:'synthetic/nm',profileReceipt:'synthetic/prof',symbol:'<perf_gate_iai::prf01_support::R19Prf01>::producer_free_window',runnerVersion:'0.14.2',valgrindVersion:'synthetic',collectAtStart:false,defaultToggle:false,entryPoint:'Custom',separateThreads:true,toggleCollect:'<perf_gate_iai::prf01_support::R19Prf01>::producer_free_window',generatedArguments:'synthetic',aggregation:'sum of all thread profile Ir',childFreeCount:4096,childIr:900,boundaryOverheadIr:8,workloadIr:892,boundaryExecutions:1,totalIr:r.Ir,startedAt:r.startedAt,control:{receipt:'synthetic/c',workload:'child remote frees removed',source:r.source,binarySha256:h('8'),environmentSha256:r.environmentSha256,childIr:8,workloadIr:0,childFreeCount:0,boundaryExecutions:1,symbolReceipt:'synthetic/cnm',profileReceipt:'synthetic/cprof',symbol:'<perf_gate_iai::prf01_support::R19Prf01>::producer_free_window',toggleCollect:'<perf_gate_iai::prf01_support::R19Prf01>::producer_free_window',cfg:['r19_prf01_producer_control'],features:primary,generatedArguments:'synthetic',runnerVersion:'0.14.2',valgrindVersion:'synthetic',collectAtStart:false,defaultToggle:false,entryPoint:'Custom',separateThreads:true,aggregation:'sum of all thread profile Ir',totalIr:100,deltaIr:r.Ir-100,parentSetupUnchanged:true,collectionUnchanged:true}};
    if(name===construction&&arm.startsWith('B'))r.constructionGainReceipt={path:'docs/perf/r20_prf01_construction_gain_decomposition.txt',sha256:h('9'),removedPassObserved:true,decomposition:'retained Callgrind A/B functional decomposition: the 4096-word pending pass is gone'};
    d.iai.push(r);
  }
  // Ir values: benefits satisfy their thresholds; hot families are affine with a
  // per-op slope of 100 (s = 6400 over 64 ops); construction A1=A2=B1=B2=C0 equal.
  const setIr=(name,arm,ir)=>{d.iai.find(r=>r.name===name&&r.arm===arm).Ir=ir;};
  for(const arm of arms) {
    setIr(benefit[0],arm,arm.startsWith('B')?1000000:4000000);
    setIr(benefit[1],arm,arm.startsWith('B')?1000000:12000000);
    for(const name of benefit.slice(2))setIr(name,arm,100000+benefit.indexOf(name)*1000);
    for(const f of hotFamilies)for(const [i,s] of ['n','2n','4n'].entries())setIr(`r20_prf01_${f}_${s}`,arm,100000+6400*(1<<i));
    setIr(construction,arm,100000);
    for(const name of mi)setIr(name,arm,50000);
    setIr(publication,arm,arm.startsWith('B')?1000:100000);
    setIr(producer,arm,arm.startsWith('B')?100000:100000);
  }
  for(const r of d.iai.filter(r=>r.name===producer)){r.producerIrReceipt.totalIr=r.Ir;r.producerIrReceipt.control.deltaIr=r.Ir-100;}
  return d;
}
function selftest() {
  const frozen=[{path:'synthetic',sha256:'a'.repeat(64)}],d=fixture(frozen); let checks=0;
  const set=(x,name,arm,ir)=>{x.iai.find(r=>r.name===name&&r.arm===arm).Ir=ir;};
  const check=(f,expected='INCOMPLETE')=>{const x=structuredClone(d); const pr0=x.iai.find(r=>r.name===producer&&r.producerIrReceipt)?.producerIrReceipt; const bo0=pr0?.boundaryOverheadIr,w0=pr0?.workloadIr,c0=pr0?.control?.childIr; f(x); for(const r of x.iai??[])if(r.name===producer&&r.producerIrReceipt){if(r.producerIrReceipt.control&&r.producerIrReceipt.control.childIr===c0)r.producerIrReceipt.control.childIr=r.producerIrReceipt.boundaryOverheadIr;if(r.producerIrReceipt.workloadIr===w0)r.producerIrReceipt.workloadIr=r.producerIrReceipt.childIr-r.producerIrReceipt.boundaryOverheadIr;r.producerIrReceipt.totalIr=r.Ir;if(r.producerIrReceipt.control)r.producerIrReceipt.control.deltaIr=r.Ir-r.producerIrReceipt.control.totalIr;if(r.Ir<1000)r.producerIrReceipt.workloadIr=r.producerIrReceipt.childIr-r.producerIrReceipt.boundaryOverheadIr;} const result=derive(x,frozen); assert.equal(result.verdict,expected,`check ${checks+1}: ${result.reason??''} :: ${String(f).slice(0,140)}`); checks++;};
  const GO='INCONCLUSIVE (native deferred)';
  // Baseline + global verdict: deterministic axes pass, native deferred (GO is unreachable in v2).
  check(()=>{},GO);
  // Deferral marker and freeze integrity.
  check(x=>delete x.nativeDeferred); check(x=>x.nativeDeferred.deferred=false);
  check(x=>x.nativeDeferred.method='per-sample wall-clock timers');
  check(x=>x.version=2); check(x=>x.hashes=[]); check(x=>x.inventory.pop()); check(x=>x.iai.pop());
  check(x=>x.entry='AllocCore'); for(const key of ['inventoryReceipt','environmentReceipt','identityReviewReceipt','oracleReceipt'])check(x=>delete x[key]);
  // Benefit thresholds, exact BigInt boundaries (B/A <= 0.25 <=> 4B <= A).
  for(const arm of ['B1','B2']) {
    check(x=>set(x,benefit[0],arm,1000000),GO);
    check(x=>set(x,benefit[0],arm,1000001),'NO-GO');
    check(x=>set(x,benefit[1],arm,3000001),'NO-GO');
    check(x=>set(x,'growth16_4n',arm,104000+1501),'NO-GO');
    check(x=>set(x,'growth16_4n',arm,104000+1500),GO);
    check(x=>set(x,'growth16_n',arm,102000+601),'NO-GO');
    check(x=>set(x,'growth16_n',arm,102000+600),GO);
  }
  check(x=>set(x,benefit[0],'A2',3999999),'NO-GO');
  // Oracle activation and count tolerance.
  check(x=>x.iai.find(r=>r.name===benefit[0]&&r.arm==='B1').oracle.routed_scans=0);
  check(x=>x.iai.find(r=>r.name===benefit[0]&&r.arm==='B1').oracle.pending_words_visited=1);
  check(x=>x.iai.find(r=>r.name===benefit[0]&&r.arm==='B1').oracle.summary_words_visited=0);
  check(x=>x.iai.find(r=>r.name===benefit[0]&&r.arm==='A1').oracle.pending_words_visited+=3191);
  check(x=>x.iai.find(r=>r.name===benefit[2]&&r.arm==='A2').oracle.routed_scans=98);
  check(x=>x.iai.find(r=>r.name===benefit[2]&&r.arm==='A1').oracle.summary_rmw=1);
  check(x=>x.iai.find(r=>r.name===publication&&r.arm==='B2').oracle.discovery_records_reclaimed=0);
  for(const k of ['summary_rmw','summary_load']){check(x=>x.iai.find(r=>r.name===producer&&r.arm==='B2').oracle[k]=0);check(x=>x.iai.find(r=>r.name===producer&&r.arm==='A1').oracle[k]=1);}
  // Hot families: slope neutrality (two-sided, T_s), linearity, ceiling, UNJUDGEABLE.
  const fam0='r20_prf01_small_churn_16b';
  check(x=>{set(x,fam0+'_2n','B1',112800+10);set(x,fam0+'_4n','B1',125600+20);},GO);
  check(x=>{set(x,fam0+'_2n','B1',112800+11);set(x,fam0+'_4n','B1',125600+22);},'INVALID');
  check(x=>{set(x,fam0+'_2n','B1',112800-10);set(x,fam0+'_4n','B1',125600-20);},GO);
  check(x=>{set(x,fam0+'_2n','B1',112800-11);set(x,fam0+'_4n','B1',125600-22);},'INVALID');
  check(x=>{set(x,fam0+'_2n','B2',112800+11);set(x,fam0+'_4n','B2',125600+22);},'INVALID');
  check(x=>{set(x,fam0+'_2n','C0',112800+11);set(x,fam0+'_4n','C0',125600+22);},'INCONCLUSIVE (native deferred)');
  check(x=>set(x,fam0+'_4n','A1',128000+11),'INVALID');
  check(x=>x.iai.filter(r=>r.name.startsWith(fam0)&&r.arm==='A2').forEach(r=>{r.Ir+=600;}),'INCONCLUSIVE');
  check(x=>x.iai.filter(r=>r.name.startsWith(fam0)&&r.arm==='C0').forEach(r=>{r.Ir+=600;}),'INCONCLUSIVE');
  check(x=>{x.iai=x.iai.filter(r=>!(r.name===fam0+'_2n'&&r.arm==='B1'));});
  // Construction: budget_K = 0 + T_s, anti-false-gain receipt rules.
  const cons=construction;
  check(x=>set(x,cons,'B1',100000+10),GO);
  check(x=>set(x,cons,'B1',100000+11),'NO-GO');
  check(x=>set(x,cons,'B1',100000-10),GO);
  check(x=>set(x,cons,'B1',100000-11),'INCONCLUSIVE (native deferred)');
  check(x=>{set(x,cons,'B1',100000-11);delete x.iai.find(r=>r.name===cons&&r.arm==='B1').constructionGainReceipt;},'INCOMPLETE');
  check(x=>{set(x,cons,'B1',100000-11);x.iai.find(r=>r.name===cons&&r.arm==='B1').constructionGainReceipt.removedPassObserved=false;},'INVALID');
  check(x=>{set(x,cons,'B1',100000-11);delete x.iai.find(r=>r.name===cons&&r.arm==='B1').constructionGainReceipt.sha256;},'INVALID');
  check(x=>set(x,cons,'B2',100000-11),'INCONCLUSIVE (native deferred)');
  check(x=>set(x,cons,'C0',100000+600),'INCONCLUSIVE');
  check(x=>{x.iai=x.iai.filter(r=>r.name!==cons);},'UNJUDGEABLE');
  // Mimalloc exact-zero, sizeOf.
  check(x=>set(x,mi[0],'B2',50001),'NO-GO');
  check(x=>x.sizeOf.B++,'NO-GO'); check(x=>delete x.sizeOf.receipt);
  // Identity, provenance, linkage.
  check(x=>x.identities.C0.binarySha256=x.identities.A.binaries.iai);
  check(x=>x.identities.B.label='A'); check(x=>x.identities.B.toolchain.rustc='other');
  check(x=>x.identities.A.buildStartedAt=x.identities.A.source.capturedAt);
  check(x=>x.iai[0].startedAt=x.identities.A.buildStartedAt);
  check(x=>x.iai[0].features.push('alloc-stats'));
  check(x=>x.iai[0].binarySha256='0'.repeat(64));
  check(x=>x.iai[0].source.identitySha256='0'.repeat(64));
  check(x=>x.iai[0].oracle.linkage.cfg=[]);
  check(x=>x.iai[0].oracle.linkage.binarySha256=x.identities.A.binaries.iai);
  check(x=>x.iai[0].config.resolved.workCount++);
  check(x=>x.iai[0].config.configConflictsDelta=1);
  check(x=>x.iai[0].config.processBoundary='same-process');
  check(x=>x.iai[0].config.runId=x.iai[1].config.runId);
  check(x=>x.iai[0].config.pid=0);
  check(x=>x.iai[0].receipt='../outside');
  check(x=>x.iai[0].Ir=NaN); check(x=>x.iai[0].Ir=-1);
  // Producer Amendment B coverage boundaries.
  const pr=x=>x.iai.find(r=>r.name===producer&&r.arm==='B1').producerIrReceipt;
  check(x=>pr(x).boundaryOverheadIr=0); check(x=>pr(x).boundaryOverheadIr=17);
  check(x=>pr(x).boundaryOverheadIr=16,GO);
  check(x=>pr(x).childIr=pr(x).boundaryOverheadIr);
  check(x=>pr(x).symbol='broad'); check(x=>pr(x).entryPoint='Default');
  check(x=>pr(x).control.childIr=1); check(x=>delete pr(x).control);
  check(x=>delete x.iai.find(r=>r.name===producer&&r.arm==='B1').producerIrReceipt);
  // Producer threshold boundary: (IrB-IrA)/4096 <= 6 exact.
  check(x=>set(x,producer,'B1',100000+24576),GO);
  check(x=>set(x,producer,'B1',100000+24577),'NO-GO');
  // Hot config declarations.
  check(x=>{const r=x.iai.find(q=>q.name===fam0+'_n'&&q.arm==='A1');r.config.requested.segmentCount=1;r.config.resolved.segmentCount=1;});
  check(x=>{const r=x.iai.find(q=>q.name===fam0+'_n'&&q.arm==='A1');r.config.declaredProvenance.alignment=16;});
  check(x=>{const r=x.iai.find(q=>q.name===fam0+'_n'&&q.arm==='A1');r.config.resolvedProvenance.copiedFields.push('segmentCount');});
  check(x=>{x.iai.filter(r=>r.name===fam0+'_2n').forEach(r=>{r.config.requested.workCount=65;r.config.resolved.workCount=65;});});
  assert.equal(derive({},frozen).verdict,'INCOMPLETE');checks++;
  console.log(JSON.stringify({version:1,status:'PASS',scope:'synthetic selftest only',checks,measurements:0}));
}
const args=process.argv.slice(2);
try {
  if(args.length===1&&args[0]==='--hashes')process.stdout.write(hashText());
  else if(args.length===1&&args[0]==='--selftest')selftest();
  else {
    assert.equal(readFileSync(new URL('docs/perf/R20_PRF01_SIDECAR_SUMMARY_V2_PREREG.sha256',root),'utf8'),hashText());
    if(args.length===1&&args[0]==='--check-hashes')console.log(JSON.stringify({version:1,status:'PASS',check:'raw document/script hashes; no self-hash'}));
    else {assert.ok(args.length===2&&args[0]==='--input','expected --input FILE; no data means INCOMPLETE');const result=derive(JSON.parse(readFileSync(resolve(args[1]),'utf8')),hashes());console.log(JSON.stringify(result,null,2));process.exitCode=result.verdict==='INCONCLUSIVE (native deferred)'?0:1;}
  }
} catch(e) {console.log(JSON.stringify({version:1,verdict:'INCOMPLETE',reason:e.message}));process.exitCode=1;}
