// Read-only R20 PRF-01 v2 judge: no builds, subprocesses, measurements or writes.
// Fork of scripts/r19_prf01_sidecar_summary_gate.mjs; what changed is documented
// in docs/perf/R20_PRF01_SIDECAR_SUMMARY_V2_PREREG.md (control geometry per
// docs/perf/R20_CONTROL_GEOMETRY_RULE.md, direct construction cell with
// budget_K = 0, layout-noise ceiling, native axis DEFERRED).
import assert from 'node:assert/strict';
import { fixture } from './r20_prf01_c3_fixture.mjs';
import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import {
  ARMS as arms,
  FEATURES as primary,
  V2_BENEFIT as benefit,
  HOT_FAMILIES as hotFamilies,
  HOT_CELLS as hotCells,
  CONSTRUCTION as construction,
  MIMALLOC as mi,
  PUBLICATION as publication,
  SAME_WORD_PRODUCER as producer,
  DISTINCT_WORD_PRODUCER,
  LARGE_REMOTE_FREE,
  TRIM_CELLS,
  PRODUCERS,
  targetFor,
  ALL_COUNTERS,
  SUMMARY_ONLY_COUNTERS,
  summaryCounterAllowed,
  INVENTORY as ALL_INVENTORY,
  FROZEN_FILES as files,
} from './r20_prf01_c3_support.mjs';
const root = new URL('../', import.meta.url);
const sha = b => createHash('sha256').update(b).digest('hex');
const hashes = () => files.map(path => ({path, sha256:sha(readFileSync(new URL(path, root)))}));
const hashText = () => hashes().map(r => `${r.sha256}  ${r.path}\n`).join('');
const inventory = ALL_INVENTORY;
// Cells whose oracle observes strict_trim_remaining; all other cells record it as unobserved.
const strictTrimObserved = name => name === publication || name === TRIM_CELLS[0];
// Amendment A declarations of the v2 cells: [workCount, blockBytes, alignment].
const OPS = {n:64,'2n':128,'4n':256};
const hotDecl = {r20_prf01_start_one_alloc_free_16b:[1,16,8],
  ...Object.fromEntries(hotFamilies.flatMap(f => Object.entries(OPS).map(([s,ops]) =>
    [`r20_prf01_${f}_${s}`, [ops, f === 'small_churn_16b' ? 16 : f === 'aligned_churn_640b_a128' ? 640 : 256, f === 'aligned_churn_640b_a128' ? 128 : 8]])))};
const SUMMARY_LOAD_SEMANTICS =
  "B summary_hint_loads == 0 is source-derived (C'' performs no producer summary preload); " +
  'the == 0 check is format consistency, not a dynamic absence proof';
const counters = ALL_COUNTERS;
const text = x => assert.ok(typeof x === 'string' && x.trim().length > 0);
const hex = x => { text(x); assert.match(x,/^[0-9a-f]{64}$/); };
const commit = x => { text(x); assert.match(x,/^[0-9a-f]{40}$/); };
const nat = (x, where='') => assert.ok(Number.isSafeInteger(x) && x >= 0, `nat ${where} ${x}`);
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
    assert.equal(r.arm,ctx.arm); assert.equal(r.binarySha256,ctx.rowArm==='C0'?ctx.pins.C0.oracleBinarySha256:ctx.pins[ctx.arm].binaries.oracle); text(r.receipt);
    assert.equal(r.claimPath,'global'); exact([...r.copiedFields].sort(),['cachePolicy','configConflictsDelta','maintenance']);
  } else {
    nat(c.requested.segmentCount); assert.ok(c.requested.segmentCount>0);
  }
  assert.equal(c.configConflictsDelta,0); assert.equal(c.processBoundary,'isolated-process'); text(c.runId); nat(c.pid); assert.ok(c.pid>0);
}
const REHEARSAL_VALIDATED='REHEARSAL: HARNESS VALIDATED (main only; candidate counters unavailable)';
const verdictFor = (verdict,rehearsal) => rehearsal && verdict==='GO (deterministic; native deferred)' ? REHEARSAL_VALIDATED : rehearsal && !['INVALID','INCOMPLETE','UNJUDGEABLE'].includes(verdict) ? `REHEARSAL: ${verdict}` : verdict;
const exitFor = verdict => verdict.startsWith('REHEARSAL: ')||verdict==='GO (deterministic; native deferred)' ? 0 : 1;
function derive(d,frozen) {
  const tables={iai:[],slope:[],hot:[],construction:[],oracle:[],native:[],size:[]};
  try {
    assert.ok(!Object.hasOwn(d,'rehearsal') || typeof d.rehearsal==='boolean');
    const rehearsal=d.rehearsal===true;
    // Counterfactual durability (re-freeze 9): a counterfactual marker (e.g. the runner's --slop
    // negative control) in a REAL (non-rehearsal) input makes the whole input INVALID.
    const cfInvalid=Object.hasOwn(d,'counterfactual')&&!rehearsal;
    // Optional receipt-hash anchor map (acceptance input for scripts/r20_prf01_verify_artifacts.mjs):
    // schema-checked here, NEVER used in the verdict.
    if(Object.hasOwn(d,'receiptHashes')){assert.ok(d.receiptHashes&&typeof d.receiptHashes==='object'&&!Array.isArray(d.receiptHashes));for(const k of Object.keys(d.receiptHashes))hex(d.receiptHashes[k]);}
    assert.equal(d.version,1); exact(d.hashes,frozen); hex(d.environmentSha256);
    for(const k of ['inventoryReceipt','identityReviewReceipt','environmentReceipt','oracleReceipt'])receiptText(d[k]);
    exact(d.inventory,inventory);
    assert.equal(d.entry,'SeferAlloc::GlobalAlloc->HeapCore routed');
    // Native axis (owner requirement, mode 2 (ii)) is DEFERRED to a quiet bare-metal
    // host; it is NOT launched in v2 and the judge refuses a missing deferral marker.
    assert.ok(d.nativeDeferred && typeof d.nativeDeferred==='object');
    assert.equal(d.nativeDeferred.deferred,true);
    text(d.nativeDeferred.method);
    assert.match(d.nativeDeferred.method,/deferred|bare-metal|quiet/i);
    const pins=d.identities;
    assert.ok(!Object.hasOwn(pins.B,'oracleSource'));
    for(const role of ['c3','trim']){hex(pins.A.binaries[role]);hex(pins.B.binaries[role]);}
    hex(pins.C0.c3BinarySha256);hex(pins.C0.trimBinarySha256);
    for(const arm of ['A','B']) {
      const p=pins[arm]; assert.equal(p.label,arm); source(p.source); assert.equal(p.measurementCommit,p.source.baseHead); commit(p.measurementCommit);
      assert.equal(p.environmentSha256,d.environmentSha256);
      for(const k of ['iai','controls','oracle','control'])hex(p.binaries[k]);
      hex(p.binaries.c3);hex(p.binaries.trim);

      assert.ok(time(p.source.capturedAt)<time(p.buildStartedAt));
      features(p.features,primary);
      exact(p.oracleCfg,['r19_sidecar_summary_oracle',...(arm==='B'&&!rehearsal?['r20_prf01_c3_oracle']:[])]); exact(p.primaryCfg,[]);
      text(p.buildReceipt);
    }
    if(rehearsal) exact(pins.B.source,pins.A.source);
    hex(pins.C0.binarySha256); hex(pins.C0.controlsBinarySha256); hex(pins.C0.oracleBinarySha256); exact(pins.C0.source,pins.A.source); text(pins.C0.buildReceipt);
    assert.ok(time(pins.C0.source.capturedAt)<time(pins.C0.buildStartedAt));
    assert.notEqual(pins.C0.binarySha256,pins.A.binaries.iai);
    assert.notEqual(pins.C0.controlsBinarySha256,pins.A.binaries.controls);
    if(!rehearsal) {
      assert.notEqual(pins.A.source.identitySha256,pins.B.source.identitySha256);
      assert.notEqual(pins.A.measurementCommit,pins.B.measurementCommit);
    }
    exact(pins.A.toolchain,pins.B.toolchain); for(const k of ['rustc','target','profile','flags'])text(pins.A.toolchain[k]);
    const c0Role = role => pins.C0[`${role}BinarySha256`];
    const runs=new Set();
    function provenance(r,arm,binary) {
      const p=pins[arm]; exact(r.source,p.source); assert.equal(r.binarySha256,binary); assert.equal(r.environmentSha256,d.environmentSha256);
      assert.ok(time(r.startedAt)>time(p.buildStartedAt)); features(r.features,primary); receiptText(r.receipt);
      config(r.config,mi.includes(r.name)?'mimalloc':hotDecl[r.name]?'hot':'normal',{name:r.name,arm,pins,rowArm:r.arm});
      assert.ok(!runs.has(r.config.runId)); runs.add(r.config.runId);
    }
    function oracle(r,needed) {
      const raw=r.oracle; assert.ok(raw && typeof raw==='object');
      const o={...raw};
      if(!needed)for(const k of [...counters,'unique_pending_words'])o[k]=null;
      exact(Object.keys(o).sort(),[...counters,'unique_pending_words','unobservedReasons','linkage'].sort());
      if(!needed) {
        for(const k of [...counters,'unique_pending_words']) {
          assert.equal(o[k],null);
          text(o.unobservedReasons[k]);
        }
        exact(Object.keys(o.unobservedReasons).sort(),[...counters,'unique_pending_words'].sort());
        assert.equal(o.linkage,null);
        return;
      }
      const observedOracleReasons=[
        ...counters.filter(k=>o[k]===null),
        ...(o.unique_pending_words===null?['unique_pending_words']:[]),
      ].sort();
      const unobservedOracleKeys=Object.keys(o.unobservedReasons).sort();
      exact(unobservedOracleKeys,observedOracleReasons);
      const candidateOnly = ['summary_words_visited','summary_hint_loads','summary_hint_rmw','quiescent_summary_missing_bits'];
      for(const k of ['summary_words_visited','summary_hint_loads','summary_hint_rmw','quiescent_summary_missing_bits']) {
        if(rehearsal && r.arm.startsWith('B')) { assert.equal(o[k],null); text(o.unobservedReasons[k]); }
        else if(r.arm.startsWith('B')) { nat(o[k],`${r.name}/${r.arm}/${k}`); assert.equal(o.unobservedReasons[k],undefined); }
        else if(o[k]===null || o[k]===undefined) text(o.unobservedReasons[k]);
        else { assert.equal(o[k],0); assert.equal(o.unobservedReasons[k],undefined); }
      }
      for(const k of counters)if(k!=='unique_pending_words'&&!candidateOnly.includes(k)) {
        if(SUMMARY_ONLY_COUNTERS.includes(k)&&(r.arm.startsWith('A')||r.arm==='C0')) {
          assert.equal(o[k],0);
          assert.equal(o.unobservedReasons[k],undefined);
        } else if(k==='strict_trim_remaining'&&!strictTrimObserved(r.name)) {
          assert.equal(o[k],null);
          text(o.unobservedReasons[k]);
        } else {
          nat(o[k],`${r.name}/${r.arm}/${k}`);
          assert.equal(o.unobservedReasons[k],undefined);
        }
      }
      if(Object.hasOwn(PRODUCERS,r.name)&&r.arm.startsWith('B')&&!rehearsal) {
        nat(o.quiescent_summary_missing_bits);
        nat(o.sizeof_small_sidecar);
        assert.equal(o.sizeof_small_sidecar,41984);
      }
      if(needed) {
        assert.ok(o.nonempty_cuts<=o.pending_words_exchanged);
        assert.ok(o.pending_words_exchanged<=o.pending_words_visited);
      }
      const l=o.linkage;
      const officialB = r.arm.startsWith('B')&&!rehearsal;
      const p=pins[r.arm.startsWith('B')?'B':'A'];
      assert.equal(l.binarySha256,r.arm==='C0'?pins.C0.oracleBinarySha256:p.binaries.oracle);
      exact(l.source,r.source);
      features(l.features,primary);
      exact(l.cfg,['r19_sidecar_summary_oracle',...(officialB?['r20_prf01_c3_oracle']:[])]);
      assert.equal(l.environmentSha256,d.environmentSha256); text(l.receipt); text(l.featureExactReceipt);
      assert.ok(time(l.startedAt)>time(p.buildStartedAt));
      assert.equal(l.primaryBinarySha256,r.binarySha256);
      exact(l.config.requested,r.config.requested); config(l.config);
      assert.ok(!runs.has(l.config.runId)); runs.add(l.config.runId);
    }
      const nameSet=new Set(inventory);
      for(const row of d.iai)assert.ok(nameSet.has(row.name),'unknown cell name');
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
      const needed=benefit.includes(name)||name===publication||Object.hasOwn(PRODUCERS,name)||TRIM_CELLS.includes(name);

      for(const r of rs) {
        const a=r.arm.startsWith('B')?'B':'A';
        const target=targetFor(r.name),role=target==='c3'?'c3':target==='trim'?'trim':null;
      const binary=role??(hotDecl[r.name]?'controls':'iai');
      const expectedBinary=r.arm==='C0'
        ? (role?c0Role(role):hotDecl[r.name]?pins.C0.controlsBinarySha256:pins.C0.binarySha256)
        : pins[a].binaries[binary];
      provenance(r,a,expectedBinary);
        if(r.arm==='C0')assert.ok(time(r.startedAt)>time(pins.C0.buildStartedAt));
        nat(r.Ir); assert.ok(r.Ir>0); exact(r.config.requested,rs[0].config.requested); oracle(r,needed);
        if(!rehearsal && benefit.includes(name)) {
          assert.ok(r.oracle.routed_scans>0);
          assert.equal(r.oracle.routed_scans,rs[0].oracle.routed_scans);
          assert.equal(r.oracle.nonempty_cuts,0);
          assert.equal(r.oracle.discovery_records_reclaimed,0);
          if(a==='B') {
            assert.equal(r.oracle.pending_words_visited,0);
            assert.ok(r.oracle.summary_words_visited>0);
          } else {
            assert.ok(r.oracle.pending_words_visited>0);
            assert.equal(r.oracle.pending_words_exchanged,r.oracle.pending_words_visited);
          }
          if(a==='A') {
            const expected=name===benefit[0]?319063:name===benefit[1]?1004503:null;
            if(expected!==null) {
              assert.ok(
                Math.abs(r.oracle.pending_words_visited-expected)<=Math.floor(expected*.01),
                'frozen 1% pending count tolerance',
              );
            }
          }
        }
        if(name.startsWith('growth16_')) {
          assert.equal(r.config.requested.workCount,[32768,65536,131072][benefit.indexOf(name)-2]);
          assert.equal(r.config.requested.blockBytes,16);
        }
        if(Object.hasOwn(PRODUCERS,name)) {
          if(r.arm.startsWith('B')&&!rehearsal) {
            nat(r.oracle.quiescent_summary_missing_bits);
            nat(r.oracle.sizeof_small_sidecar);
            assert.equal(r.oracle.quiescent_summary_missing_bits,0,'producer must be quiescent before discovery');
            assert.equal(r.oracle.sizeof_small_sidecar,41984);
            if(name===LARGE_REMOTE_FREE) {
              assert.equal(r.oracle.summary_hint_rmw,0);
              assert.equal(r.oracle.summary_hint_loads,0);
              assert.equal(r.oracle.unique_pending_words,0);
            } else {
              assert.equal(r.oracle.summary_hint_loads,4096);
              const observedRmw=Number(r.oracle.unique_pending_words); nat(observedRmw,'unique pending-word count');
              assert.equal(r.oracle.summary_hint_rmw,observedRmw);
              if(name===producer)assert.ok(observedRmw>0&&observedRmw<r.oracle.summary_hint_loads);
              else assert.equal(observedRmw,4096);
            }
          } else {
            assert.ok(r.oracle.summary_hint_rmw===null||r.oracle.summary_hint_rmw===0);
            assert.ok(r.oracle.summary_hint_loads===null||r.oracle.summary_hint_loads===0);
            const words=r.oracle.unique_pending_words;
            if(name===LARGE_REMOTE_FREE) {
              assert.equal(words,0);
            } else {
              nat(words,'baseline unique pending-word count');
              if(name===DISTINCT_WORD_PRODUCER)assert.equal(words,4096);
              else assert.ok(words>0&&words<4096,'baseline geometry must be observed, not forced to zero');
            }
          }
          const p=r.producerIrReceipt; assert.ok(p && typeof p==='object');
          assert.equal(p.binarySha256,r.binarySha256); exact(p.source,r.source); assert.equal(p.environmentSha256,r.environmentSha256);
          const spec=PRODUCERS[name],c=p.control;
          features(p.features,primary); exact(p.cfg,[]); assert.equal(p.symbol,spec.symbol);
          assert.equal(p.toggleCollect,p.symbol);
          assert.equal(c.symbol,p.symbol);
          assert.equal(c.toggleCollect,p.symbol);
          exact(c.cfg,[spec.cfg]);
          text(p.receipt);
          text(p.symbolReceipt);
          text(p.profileReceipt);
          text(c.receipt);
          text(c.symbolReceipt);
          text(c.profileReceipt);
          assert.match(p.runnerVersion,/^0[.]14[.]/);text(p.valgrindVersion);
          assert.equal(p.collectAtStart,false);
          assert.equal(p.defaultToggle,false);
          assert.equal(p.entryPoint,'Custom');
          assert.equal(p.separateThreads,true);
          text(p.generatedArguments);assert.equal(p.aggregation,'sum of all thread profile Ir');
          nat(p.childIr);assert.ok(p.childIr>0);assert.ok(time(p.startedAt)>time(r.source.capturedAt));
          assert.equal(c.environmentSha256,r.environmentSha256);
          source(c.source);
          hex(c.binarySha256);
          assert.notEqual(c.binarySha256,r.binarySha256);
          assert.equal(c.workload,'child remote frees removed');
          nat(c.childIr);
          nat(c.totalIr);
          nat(p.totalIr);
          assert.equal(p.totalIr,r.Ir);
          assert.equal(p.childIr,c.childIr+p.workloadIr);
          assert.notEqual(c.totalIr,p.totalIr);
          assert.equal(c.deltaIr,p.totalIr-c.totalIr);
          assert.equal(c.totalIr,p.totalIr-c.deltaIr);
          assert.ok(c.deltaIr>0);
          assert.ok(p.childIr<=p.totalIr);
          assert.ok(c.childIr<=c.totalIr);
          assert.equal(p.childFreeCount,spec.freeCount);
          assert.equal(c.childFreeCount,0);
          assert.equal(p.boundaryExecutions,1);
          assert.equal(c.boundaryExecutions,1);
          assert.equal(c.workloadIr,0);
          if(spec.blockBytes!==null)assert.equal(r.config.requested.blockBytes,spec.blockBytes);
          assert.equal(r.config.requested.workCount,spec.freeCount);
          assert.ok(p.boundaryOverheadIr>=1&&p.boundaryOverheadIr<=16);
          assert.equal(c.childIr,p.boundaryOverheadIr);
          nat(p.workloadIr);
          assert.equal(p.workloadIr,p.childIr-p.boundaryOverheadIr);
          assert.ok(p.workloadIr>0);
          exact(c.cfg,[spec.cfg]); features(c.features,primary); exact(c.source,r.source);
text(c.generatedArguments);
          assert.match(c.runnerVersion,/^0[.]14[.]/); text(c.valgrindVersion); assert.equal(c.collectAtStart,false); assert.equal(c.defaultToggle,false);
          assert.equal(c.entryPoint,'Custom'); assert.equal(c.separateThreads,true); assert.equal(c.aggregation,'sum of all thread profile Ir');
          assert.equal(p.control.parentSetupUnchanged,true); assert.equal(p.control.collectionUnchanged,true);
          if(name===producer) {
            assert.equal(r.config.requested.segmentCount,1);
            assert.equal(
              r.scenario,
              'joined remote Small frees; repeated frees in same pending word; ' +
              'no owner discovery during producer window',
            );
          }
        }
        if(name===publication||TRIM_CELLS.includes(name)) {
          assert.ok(r.oracle.routed_scans>0);
          if(name===publication)assert.ok(r.oracle.discovery_records_reclaimed>0);
          const armIndex=arms.indexOf(r.arm);
          if(armIndex>0)for(const k of ['discovery_records_reclaimed','strict_trim_remaining']) {
            if(r.oracle[k]!==null)assert.equal(r.oracle[k],rs[0].oracle[k]);
          }
        }
      }
      // Benefit / producer / mimalloc cross pairs (carried over from R19, exact BigInt arithmetic).
      for(const a of rs)for(const b of rs)if(a.arm[0]==='A'&&b.arm[0]==='B') {
        const delta=BigInt(b.Ir)-BigInt(a.Ir); let percent=null,pass=true,mode='descriptive';
        if(benefit.slice(0,2).includes(name)) { percent=pct(BigInt(b.Ir)*4n,BigInt(a.Ir)); pass=BigInt(b.Ir)*4n<=BigInt(a.Ir); mode='mode1'; }
        if(benefit.slice(2).includes(name)) {mode='mode1';}
        if(mi.includes(name)) { pass=delta===0n; mode='mode1'; }
        if(Object.hasOwn(PRODUCERS,name)) {
          const irDelta=BigInt(b.Ir)-BigInt(a.Ir); mode='mode2';
          const workDelta=BigInt(b.producerIrReceipt.workloadIr)-BigInt(a.producerIrReceipt.workloadIr);
          const budget=BigInt(PRODUCERS[name].freeCount)*(name===DISTINCT_WORD_PRODUCER?10n:6n);
          pass=workDelta<=budget;
          if(name===LARGE_REMOTE_FREE)pass=ABS(irDelta)<=10n;
          if(name!==LARGE_REMOTE_FREE)percent=pct(workDelta,BigInt(PRODUCERS[name].freeCount));
        }
        if(TRIM_CELLS.includes(name)) {
          mode='mode2';
          const t=MAXB(
            10n,
            ABS(BigInt(rs[0].Ir)-BigInt(rs[1].Ir)),
            ABS(BigInt(rs[4].Ir)-BigInt(rs[0].Ir)),
          );
          pass=ABS(delta)<=t;
        }
        if(mode==='mode1')mode1Fail ||= !pass;
        if(mode==='mode2')mode2Fail ||= !pass;
        tables.iai.push({name,A:a.arm,B:b.arm,delta:Number(delta),mode,...(percent?{percent}:{pass:delta===0n}),pass});
      }
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
      // NOTE (re-freeze 9, stated to match behaviour): the UNJUDGEABLE branch below is UNREACHABLE
      // for missing sibling rows -- row collection above (exactly one row per arm per cell) throws
      // INCOMPLETE before the family check runs. UNJUDGEABLE is in practice reachable only for a
      // missing construction cell.
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
        const pass=ABS(delta)<=tSraw;
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
    const countersOk=Object.hasOwn(d,'sizeOf');
    nat(d.sizeOf.A);
    nat(d.sizeOf.B);
    assert.ok(d.sizeOf.A>0);
    assert.ok(d.sizeOf.B>0);
    assert.equal(d.sizeOf.A,41984);
    assert.equal(d.sizeOf.B,41984);
    text(d.sizeOf.receipt);
    const sizePass=d.sizeOf.A===41984&&d.sizeOf.B===41984; mode1Fail ||= !sizePass;
    tables.size.push({A:d.sizeOf.A,B:d.sizeOf.B,numerator:d.sizeOf.B-d.sizeOf.A,expected:0,pass:sizePass});
    // Native axis: DEFERRED (never launched in v2). Any deterministic failure still => NO-GO.
    let modes={mode1:mode1Fail?'NO-GO':'GO',mode2Iai:mode2Fail?'NO-GO':'GO',mode2Native:'DEFERRED'};
    // Rehearsal self-identification (re-freeze 9): the result carries the marker and its usable mode
    // verdicts get the same REHEARSAL: prefix as the global verdict.
    if(rehearsal)modes={...modes,mode1:'REHEARSAL: '+modes.mode1,mode2Iai:'REHEARSAL: '+modes.mode2Iai};
    tables.native.push({axis:'mode2 (ii) native paired A/B',status:'DEFERRED',reason:d.nativeDeferred.method});
    let verdict;
    if(invalid||cfInvalid)verdict='INVALID';
    else if(mode1Fail||mode2Fail||!sizePass)verdict='NO-GO';
    else if(noisy)verdict='INCONCLUSIVE';
    else if(unjudgeable.length)verdict='UNJUDGEABLE';
    else verdict='GO (deterministic; native deferred)';
    return {version:1,rehearsal,verdict:verdictFor(verdict,rehearsal),modes,unjudgeable,tables};
  } catch(e) { return {version:1,...(d&&d.rehearsal===true?{rehearsal:true}:{}),verdict:'INCOMPLETE',modes:{mode1:'INCOMPLETE',mode2Iai:'INCOMPLETE',mode2Native:'DEFERRED'},reason:e.message,tables}; }
}
function selftest() {
  const frozen = [{ path: 'synthetic', sha256: 'a'.repeat(64) }];
  const d = fixture(frozen, {
    inventory, counters, hotDecl, primary, benefit, producer, mi, PRODUCERS,
    targetFor, arms, DISTINCT_WORD_PRODUCER, LARGE_REMOTE_FREE, TRIM_CELLS,
    SUMMARY_ONLY_COUNTERS, publication, strictTrimObserved, hotFamilies, construction,
  });
  let checks = 0;
  const set=(x,name,arm,ir)=>{x.iai.find(r=>r.name===name&&r.arm===arm).Ir=ir;};
  const check=(f,expected='INCOMPLETE')=>{
    const x=structuredClone(d);
    const producerRows=x.iai
      .filter(r=>Object.hasOwn(PRODUCERS,r.name)&&r.producerIrReceipt)
      .map(r=>({
        name:r.name,
        arm:r.arm,
        Ir:r.Ir,
        child:r.producerIrReceipt.childIr,
        ctrl:r.producerIrReceipt.control.childIr,
        overhead:r.producerIrReceipt.boundaryOverheadIr,
        work:r.producerIrReceipt.workloadIr,
      }));
    f(x);
    for(const r of x.iai??[]) if(r.producerIrReceipt) {
      const before=producerRows.find(q=>q.name===r.name&&q.arm===r.arm);
      if(!before)continue;
      const p=r.producerIrReceipt;
      if(p.workloadIr!==before.work) {
        p.childIr=p.boundaryOverheadIr+p.workloadIr;
        r.Ir=Math.max(before.Ir,p.childIr+p.control.childIr+10);
        p.totalIr=r.Ir;
        p.control.totalIr=Math.max(r.Ir-10,p.control.childIr);
        p.control.deltaIr=r.Ir-p.control.totalIr;
      } else if(p.boundaryOverheadIr!==before.overhead) {
        p.childIr=p.boundaryOverheadIr+p.workloadIr;
        r.Ir=Math.max(before.Ir,p.childIr+p.control.childIr+10);
        p.totalIr=r.Ir;
        p.control.totalIr=Math.max(r.Ir-10,p.control.childIr);
        p.control.deltaIr=r.Ir-p.control.totalIr;
      } else if(r.Ir!==before.Ir) {
        p.totalIr=r.Ir;
        p.childIr=before.child+(r.Ir-before.Ir);
        p.boundaryOverheadIr=before.overhead;
        p.workloadIr=p.childIr-p.boundaryOverheadIr;
        p.control.totalIr=Math.max(r.Ir-10,p.control.childIr);
        p.control.deltaIr=r.Ir-p.control.totalIr;
      } else if(p.control&&p.control.childIr===before.ctrl) {
        p.control.childIr=p.boundaryOverheadIr;
      }
    }
    const result=derive(x,frozen);
    assert.equal(
      result.verdict,
      expected,
      `check ${checks+1}: ${result.reason??''} :: ${String(f).slice(0,140)}`,
    );
    checks++;
  };
  const GO='GO (deterministic; native deferred)';
  const REHEARSAL_GO=REHEARSAL_VALIDATED;
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
  check(x=>x.iai.find(r=>r.name===benefit[2]&&r.arm==='A1').oracle.summary_hint_rmw=1);
  check(x=>x.iai.find(r=>r.name===publication&&r.arm==='B2').oracle.discovery_records_reclaimed=0);
  for(const k of ['summary_hint_rmw','summary_hint_loads']) {
    check(x=>x.iai.find(r=>r.name===producer&&r.arm==='A1').oracle[k]=1);
  }
  check(x=>x.iai.find(r=>r.name===producer&&r.arm==='B2').oracle.summary_hint_rmw=0);
  check(x=>x.iai.find(r=>r.name===producer&&r.arm==='B2').oracle.summary_hint_rmw=4096);
  check(x=>x.iai.find(r=>r.name===producer&&r.arm==='B2').oracle.summary_hint_loads=1);
  // Counterfactual durability (re-freeze 9): counterfactual data in a real input is INVALID; in a
  // rehearsal input it stays a prefixed rehearsal result.
  check(x=>x.counterfactual={slop:true,arm:'B1',cell:'r20_prf01_small_churn_16b_2n',deltaIr:11},'INVALID');
  // Optional receiptHashes map: schema-only, never verdict-relevant when well-formed.
  check(x=>x.receiptHashes={},GO);
  check(x=>x.receiptHashes={'logs/A1-perf_gate_iai.log':'zz'});
  check(x=>x.receiptHashes=['x']);
  // Hot families: slope neutrality (two-sided, T_s), linearity, ceiling, UNJUDGEABLE.
  const fam0='r20_prf01_small_churn_16b';
  check(x=>{set(x,fam0+'_2n','B1',112800+10);set(x,fam0+'_4n','B1',125600+20);},GO);
  check(x=>{set(x,fam0+'_2n','B1',112800+11);set(x,fam0+'_4n','B1',125600+22);},'INVALID');
  check(x=>{set(x,fam0+'_2n','B1',112800-10);set(x,fam0+'_4n','B1',125600-20);},GO);
  // Two-sided slope (rule 2.2): an affine B that is FASTER per op by more than T_s fails mode 1, it is not neutral.
  check(x=>{for(const b of ['B1','B2']){set(x,fam0+'_2n',b,112800-11);set(x,fam0+'_4n',b,125600-33);}},'NO-GO');
  check(x=>{for(const b of ['B1','B2']){set(x,fam0+'_2n',b,112800-10);set(x,fam0+'_4n',b,125600-30);}},GO);
  check(x=>{for(const b of ['B1','B2']){set(x,fam0+'_2n',b,112800+11);set(x,fam0+'_4n',b,125600+33);}},'NO-GO');
  check(x=>{set(x,fam0+'_2n','B1',112800-11);set(x,fam0+'_4n','B1',125600-22);},'INVALID');
  check(x=>{set(x,fam0+'_2n','B2',112800+11);set(x,fam0+'_4n','B2',125600+22);},'INVALID');
  check(x=>{set(x,fam0+'_2n','C0',112800+11);set(x,fam0+'_4n','C0',125600+22);},GO);
  check(x=>set(x,fam0+'_4n','A1',128000+11),'INVALID');
  check(x=>x.iai.filter(r=>r.name.startsWith(fam0)&&r.arm==='A2').forEach(r=>{r.Ir+=600;}),'INCONCLUSIVE');
  check(x=>x.iai.filter(r=>r.name.startsWith(fam0)&&r.arm==='C0').forEach(r=>{r.Ir+=600;}),'INCONCLUSIVE');
  check(x=>{x.iai=x.iai.filter(r=>r.name!==mi[0]);});
  // Construction: budget_K = 0 + T_s, anti-false-gain receipt rules.
  const cons=construction;
  check(x=>set(x,cons,'B1',100000+10),GO);
  check(x=>set(x,cons,'B1',100000+11),'NO-GO');
  check(x=>set(x,cons,'B1',100000-10),GO);
  check(x=>set(x,cons,'B1',100000-11),GO);
  check(x=>{set(x,cons,'B1',100000-11);delete x.iai.find(r=>r.name===cons&&r.arm==='B1').constructionGainReceipt;},'INCOMPLETE');
  check(x=>{set(x,cons,'B1',100000-11);x.iai.find(r=>r.name===cons&&r.arm==='B1').constructionGainReceipt.removedPassObserved=false;},'INVALID');
  check(x=>{set(x,cons,'B1',100000-11);delete x.iai.find(r=>r.name===cons&&r.arm==='B1').constructionGainReceipt.sha256;},'INVALID');
  check(x=>set(x,cons,'B2',100000-11),GO);
  check(x=>set(x,cons,'C0',100000+600),'INCONCLUSIVE');
  check(x=>{x.iai=x.iai.filter(r=>r.name!==cons);},'UNJUDGEABLE');
  // Mimalloc exact-zero, sizeOf.
  check(x=>set(x,mi[0],'B2',50001),'NO-GO');
  check(x=>x.sizeOf.B++); check(x=>delete x.sizeOf.receipt);
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
  check(x=>{
    const p=pr(x);
    const r=x.iai.find(q=>q.name===producer&&q.arm==='B1');
    p.boundaryOverheadIr=16;
    p.control.childIr=16;
    p.control.totalIr=100000;
    p.workloadIr=2;
    p.childIr=18;
    r.Ir=100000;
    p.totalIr=100000;
    p.control.totalIr=99990;
    p.control.deltaIr=10;
    r.oracle.summary_hint_rmw=64;
    r.oracle.quiescent_summary_missing_bits=0;
    r.oracle.sizeof_small_sidecar=41984;
  },GO);
  check(x=>{
    const r=x.iai.find(q=>q.name===producer&&q.arm==='B1');
    r.Ir+=24576;
    const p=r.producerIrReceipt;
    p.totalIr=r.Ir;
    p.childIr+=24576;
    p.workloadIr=p.childIr-p.boundaryOverheadIr;
    p.control.totalIr=r.Ir-10;
    p.control.deltaIr=10;
  },GO);
  check(x=>{
    const r=x.iai.find(q=>q.name===producer&&q.arm==='B1');
    r.Ir+=24577;
    const p=r.producerIrReceipt;
    p.totalIr=r.Ir;
    p.childIr+=24577;
    p.workloadIr=p.childIr-p.boundaryOverheadIr;
    p.control.totalIr=r.Ir-10;
    p.control.deltaIr=10;
  },'NO-GO');
  for(const name of Object.keys(PRODUCERS)) {
    const receipt=x=>x.iai.find(r=>r.name===name&&r.arm==='B1').producerIrReceipt;
    check(x=>{receipt(x).boundaryExecutions=2;},'INCOMPLETE');
    check(x=>{receipt(x).control.boundaryExecutions=2;},'INCOMPLETE');
    check(x=>{receipt(x).control.workloadIr=1;},'INCOMPLETE');
  }
  check(x=>pr(x).symbol='broad'); check(x=>pr(x).entryPoint='Default');
  check(x=>pr(x).control.childIr=1); check(x=>delete pr(x).control);
  check(x=>delete x.iai.find(r=>r.name===producer&&r.arm==='B1').producerIrReceipt);
  // Producer threshold boundary: (IrB-IrA)/4096 <= 6 exact.
  check(x=>{
    for(const name of [DISTINCT_WORD_PRODUCER,LARGE_REMOTE_FREE]) {
      for(const arm of ['B1','B2']) {
        const r=x.iai.find(q=>q.name===name&&q.arm===arm);
        r.Ir=100000;
        r.producerIrReceipt.totalIr=r.Ir;
        r.producerIrReceipt.control.totalIr=r.Ir-10;
        r.producerIrReceipt.control.deltaIr=10;
      }
    }
  },GO);
  check(x=>{
    for(const arm of ['B1','B2']) {
      const r=x.iai.find(q=>q.name===DISTINCT_WORD_PRODUCER&&q.arm===arm);
      const p=r.producerIrReceipt;
      p.workloadIr=40960;
      p.childIr=p.boundaryOverheadIr+40960;
      r.Ir=p.childIr;
      p.totalIr=r.Ir;
      p.control.totalIr=r.Ir-10;
      p.control.deltaIr=10;
    }
  },GO);
  check(x=>{
    for(const arm of ['B1','B2']) {
      const r=x.iai.find(q=>q.name===DISTINCT_WORD_PRODUCER&&q.arm===arm);
      const p=r.producerIrReceipt;
      p.workloadIr=40960;
      p.childIr=p.boundaryOverheadIr+40960;
      r.Ir=p.childIr;
      p.totalIr=r.Ir;
      p.control.totalIr=r.Ir-10;
      p.control.deltaIr=10;
    }
  },GO);
  check(x=>{const r=x.iai.find(q=>q.name===fam0+'_n'&&q.arm==='A1');r.config.requested.segmentCount=1;r.config.resolved.segmentCount=1;});
  check(x=>{const r=x.iai.find(q=>q.name===fam0+'_n'&&q.arm==='A1');r.config.declaredProvenance.alignment=16;});
  check(x=>{const r=x.iai.find(q=>q.name===fam0+'_n'&&q.arm==='A1');r.config.resolvedProvenance.copiedFields.push('segmentCount');});
  check(x=>{x.iai.filter(r=>r.name===fam0+'_2n').forEach(r=>{r.config.requested.workCount=65;r.config.resolved.workCount=65;});});
  const rehearse=x=>{
    x.rehearsal=true;
    x.identities.B.oracleCfg=['r19_sidecar_summary_oracle'];
    x.identities.B.source=structuredClone(x.identities.A.source);
    x.identities.B.measurementCommit=x.identities.A.measurementCommit;
    for(const r of x.iai.filter(r=>r.arm.startsWith('B'))) {
      const baseline=x.iai.find(a=>a.name===r.name&&a.arm==='A1');
      for(const key of counters)r.oracle[key]=baseline.oracle[key];
      r.source=x.identities.B.source;
      if(r.oracle.linkage) { r.oracle.linkage.source=r.source; r.oracle.linkage.cfg=['r19_sidecar_summary_oracle']; }
      for(const key of ['summary_words_visited','summary_hint_loads','summary_hint_rmw','quiescent_summary_missing_bits']) {r.oracle[key]=null;r.oracle.unobservedReasons[key]='candidate-only instrumentation absent from main rehearsal';}
      if(r.producerIrReceipt) {
        r.producerIrReceipt.source=r.source; r.producerIrReceipt.control.source=r.source;
      }
    }
  };
  for(const arm of arms) {
    const a=arm.startsWith('B')?'B':'A';
    for(const name of [fam0+'_n',construction])check(x=>x.iai.find(r=>r.name===name&&r.arm===arm).binarySha256=arm==='C0'?x.identities.C0.binarySha256:x.identities[a].binaries.iai);
    check(x=>x.iai.find(r=>r.name===benefit[0]&&r.arm===arm).binarySha256=arm==='C0'?x.identities.C0.controlsBinarySha256:x.identities[a].binaries.controls);
  }
  check(x=>delete x.identities.C0.controlsBinarySha256);
  check(x=>x.identities.C0.controlsBinarySha256='bad');
  check(x=>x.identities.C0.controlsBinarySha256=x.identities.A.binaries.controls);
  check(x=>x.identities.B.source.identitySha256=x.identities.A.source.identitySha256);
  check(x=>{x.identities.B.measurementCommit=x.identities.A.measurementCommit;x.identities.B.source.baseHead=x.identities.A.source.baseHead;});
  for(const key of ['summary_words_visited','summary_hint_loads','summary_hint_rmw','quiescent_summary_missing_bits']) {
    check(x=>{const r=x.iai.find(r=>r.arm==='B1'&&r.name===benefit[0]);r.oracle[key]=null;delete r.oracle.unobservedReasons[key];});
  }
  check(x=>{rehearse(x);return x;},REHEARSAL_GO);
  check(x=>{rehearse(x);x.identities.B.oracleCfg=['wrong'];});
  check(x=>{rehearse(x);const r=x.iai.find(r=>r.arm==='B1'&&r.name===benefit[0]);delete r.oracle.unobservedReasons.summary_words_visited;});
  check(x=>{rehearse(x);x.iai.find(r=>r.arm==='B1'&&r.name===benefit[0]).oracle.linkage.source.identitySha256='bad';});
  check(x=>{rehearse(x);x.identities.B.oracleSource={};});
  for(const value of ['true',null,1])check(x=>x.rehearsal=value);
  check(x=>x.rehearsal=false,GO);
  check(x=>{rehearse(x);set(x,benefit[0],'B1',1000001);},'REHEARSAL: NO-GO');
    check(x=>{rehearse(x);x.counterfactual={slop:true,arm:'B1',cell:'r20_prf01_small_churn_16b_2n',deltaIr:11};},REHEARSAL_GO);
  // Rehearsal result self-identification (re-freeze 9): marker + prefixed mode verdicts, also on INCOMPLETE.
  {
    const mark=x=>{rehearse(x);return x;};
    const res=derive(mark(structuredClone(d)),frozen);
    assert.equal(res.rehearsal,true); assert.match(res.modes.mode1,/^REHEARSAL: /); assert.match(res.modes.mode2Iai,/^REHEARSAL: /); checks++;
    const y=mark(structuredClone(d)); y.nativeDeferred=null;
    const res2=derive(y,frozen); assert.equal(res2.verdict,'INCOMPLETE'); assert.equal(res2.rehearsal,true); checks++;
    assert.equal(exitFor('REHEARSAL: INVALID'),0); checks++;
    assert.equal(exitFor('REHEARSAL: INCOMPLETE'),0); checks++;
    assert.equal(exitFor('REHEARSAL: arbitrary'),0); checks++;
    assert.equal(exitFor('NO-GO'),1); checks++;
    assert.equal(exitFor('INCONCLUSIVE'),1); checks++;
    assert.equal(exitFor('INVALID'),1); checks++;
    assert.equal(exitFor('INCOMPLETE'),1); checks++;
  }
  check(x=>{rehearse(x);set(x,fam0+'_4n','A1',128000+11);},'INVALID');
  check(x=>{rehearse(x);delete x.nativeDeferred;},'INCOMPLETE');
  check(x=>{rehearse(x);x.iai=x.iai.filter(r=>r.name!==construction);},'UNJUDGEABLE');
    for(const [verdict,wrapped,code] of [
      ['GO (deterministic; native deferred)',REHEARSAL_GO,0],
      ['NO-GO','REHEARSAL: NO-GO',0],
      [GO,REHEARSAL_GO,0],
      ['INCONCLUSIVE','REHEARSAL: INCONCLUSIVE',0],
      ['INVALID','INVALID',1],
      ['INCOMPLETE','INCOMPLETE',1],
      ['UNJUDGEABLE','UNJUDGEABLE',1],
    ]) {
    assert.equal(verdictFor(verdict,true),wrapped); assert.equal(exitFor(wrapped),code); checks++;
    assert.equal(
      exitFor(verdict),
      verdict==='GO (deterministic; native deferred)'?0:1,
    );
    checks++;
  }
  assert.equal(exitFor('DEFERRED'),1); checks++;
  assert.equal(derive({},frozen).verdict,'INCOMPLETE');checks++;
  console.log(JSON.stringify({version:1,status:'PASS',scope:'synthetic selftest only',checks,measurements:0}));
}
const args=process.argv.slice(2);
try {
  if(args.length===1&&args[0]==='--hashes')process.stdout.write(hashText());
  else if(args.length===1&&args[0]==='--selftest')selftest();
  else if(args.length===1&&args[0]==='--check-hashes'){
    try { assert.equal(readFileSync(new URL('docs/perf/R20_PRF01_C3_DEFAULT_PREREG.sha256',root),'utf8'),hashText()); }
    catch(e) {
      if(e.code==='ENOENT') {
        throw new Error('not frozen yet: docs/perf/R20_PRF01_C3_DEFAULT_PREREG.sha256');
      }
      throw e;
    }
    console.log(JSON.stringify({version:1,status:'PASS',check:'raw document/script hashes; no self-hash'}));
  }
  else {
    assert.ok(
      args.length===2&&args[0]==='--input',
      'expected --check-hashes or --input FILE',
    );
    const result=derive(
      JSON.parse(readFileSync(resolve(args[1]),'utf8')),
      hashes(),
    );
    console.log(JSON.stringify(result,null,2));
    process.exitCode=exitFor(result.verdict);
  }
} catch(e) {console.log(JSON.stringify({version:1,verdict:'INCOMPLETE',reason:e.message}));process.exitCode=1;}
