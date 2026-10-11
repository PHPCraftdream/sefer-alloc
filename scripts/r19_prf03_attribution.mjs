import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import crypto from 'node:crypto';
const root = path.resolve(import.meta.dirname, '..');
const base = 'docs/perf/R19_PRF03_ROUTE_ATTRIBUTION';
const threshold = 200;
const read = name => fs.readFileSync(path.join(root, `docs/perf/_raw_r19_prf03_${name}.log`), 'utf8');
const atomicAddresses = {next_incarnation_CAS_Ir:['0x7318e'], register_initial_lock_CAS_Ir:['0x732a6'], remove_initial_lock_CAS_Ir:['0x75446'], refs_decrement_Ir:['0x758aa','0x758df']};
function metadata(text) {
  const entries = text.split(/\r?\n/).filter(l => /^[a-z_][a-z_0-9]*=/.test(l)).map(l => [l.slice(0,l.indexOf('=')),l.slice(l.indexOf('=')+1)]);
  assert.equal(new Set(entries.map(([k])=>k)).size, entries.length, 'duplicate metadata');
  const m=Object.fromEntries(entries);
  for(const k of ['source_sha','source_tree']) assert.match(m[k] ?? '', /^[a-f0-9]{40}$/);
  for(const k of ['binary_sha256','src_benches_patch_sha256']) assert.match(m[k] ?? '', /^[a-f0-9]{64}$/);
  assert.equal(m.src_benches_patch_sha256,crypto.createHash('sha256').update('').digest('hex'));
  assert.equal(m.initial_git_diff_src_benches,'empty'); assert.equal(m.build_exit,'0');
  assert.equal(m.features,'production bench-internals internals');
  for(const k of ['os','architecture','cpu','rustc','cargo','valgrind','callgrind_annotate','iai_runner','binary']) assert(m[k]?.length,`missing ${k}`);
  assert(!path.isAbsolute(m.binary) && !m.binary.includes('..'));
  assert(Number.isSafeInteger(Number(m.sample_count_per_arm)) && Number(m.sample_count_per_arm)>0);
  assert.match(m.build_command,/--locked/);
  return m;
}
function parse(text) {
  assert.match(text,/positions: instr line/);
  const numbers = label => {const match=new RegExp(`^${label}: (.*)$`,'m').exec(text); assert(match,`missing ${label}`); const ns=match[1].trim().split(/\s+/).map(Number); assert(ns.every(n=>Number.isSafeInteger(n)&&n>=0)); return ns;};
  const summary=numbers('summary'); assert.deepEqual(summary,numbers('totals'));
  let fn='',callee='',calls=0,edge=false;
  const edges=[],instructions=[];
  for(const l of text.split(/\r?\n/)) {
    if(l.startsWith('fn=')) fn=l.slice(3);
    else if(l.startsWith('cfn=')) callee=l.slice(4);
    else if(l.startsWith('calls=')) {calls=Number(l.slice(6).split(' ')[0]);assert(Number.isSafeInteger(calls)&&calls>0);edge=true;}
    else if(/^0x[\da-f]+ \d+ /.test(l)) {const t=l.split(/\s+/),ir=Number(t[2]);assert(Number.isSafeInteger(ir)&&ir>=0); if(edge){edges.push({caller:fn,callee,calls,ir,address:t[0]});edge=false;}else instructions.push({fn,address:t[0],ir});}
  }
  assert(!edge,'unterminated call edge'); return {summary,edges,instructions};
}
const sum = (xs,field)=>xs.reduce((s,x)=>s+x[field],0);
function metrics(x) {
  const register=x.edges.filter(e=>e.caller.includes('SegmentTable>::register_payload')&&e.callee.includes('RouteDirectory>::register'));
  const remove=x.edges.filter(e=>e.caller.includes('SegmentTable>::unregister')&&e.callee.includes('RouteRegistration as core::ops::drop::Drop>::drop'));
  const alloc=x.edges.filter(e=>e.caller.includes('RouteDirectory>::register')&&['0x731a7','0x731c7'].includes(e.address)&&e.callee==='malloc');
  const dealloc=x.edges.filter(e=>e.caller.includes('RouteRegistration as')&&['0x75951','0x75968'].includes(e.address)&&e.callee==='free');
  const result={Instructions:x.summary[0],route_register_inclusive:sum(register,'ir'),route_remove_and_nested_drop_inclusive:sum(remove,'ir'),Large_register_calls:sum(register,'calls'),Large_remove_calls:sum(remove,'calls'),Large_System_alloc_calls:sum(alloc,'calls'),Large_System_dealloc_calls:sum(dealloc,'calls')};
  result.route_disjoint_sum=result.route_register_inclusive+result.route_remove_and_nested_drop_inclusive;
  for(const [label,addresses] of Object.entries(atomicAddresses)) result[label]=sum(x.instructions.filter(i=>addresses.includes(i.address)),'ir');
  return result;
}
function calculate(p,h) {
  const a=metrics(p),b=metrics(h),rows=Object.keys(a).map(k=>[k,a[k],b[k],b[k]-a[k]]);
  const deltas=Object.fromEntries(rows.map(([k,,,d])=>[k,d]));
  assert(deltas.Instructions>0 && deltas.route_register_inclusive>=0 && deltas.route_remove_and_nested_drop_inclusive>=0);
  assert(deltas.route_disjoint_sum<=deltas.Instructions);
  return {rows,deltas};
}
function selftest() {
  const fixture='positions: instr line\nsummary: 2\nfn=test\n0x10 0 2\ntotals: 2\n';
  assert.deepEqual(parse(fixture).summary,[2]);
  assert.throws(()=>parse('summary: 1\ntotals: 2'));
  assert.throws(()=>parse(fixture.replace('totals: 2','totals: 3')));
  assert.throws(()=>parse(fixture.replace('0x10 0 2','0x10 0 NaN')));
  const empty={summary:[10],edges:[],instructions:[]};assert.throws(()=>calculate(empty,empty));
  const bad={summary:[11],edges:[{caller:'SegmentTable>::register_payload',callee:'RouteDirectory>::register',calls:1,ir:2}],instructions:[]};assert.throws(()=>calculate(empty,bad));
  const identity=read('identity');metadata(identity);
  assert.throws(()=>metadata(identity.replace(/^source_sha=.*$/m,'source_sha=bad')));
  assert.throws(()=>metadata(identity.replace(/^binary_sha256=.*$/m,'')));
  assert.throws(()=>metadata(identity.replace(/^sample_count_per_arm=.*$/m,'sample_count_per_arm=0')));
  assert.throws(()=>metadata(identity+'\nsource_sha=bad\n'));
  console.log('PASS selftest: parser, totals, arithmetic overflow, missing/malformed/duplicate metadata and sample count negative fixtures');
}
function generate() {
  const m=metadata(read('identity')),p=parse(read('prefill')),h=parse(read('hit'));
  const {rows,deltas:d}=calculate(p,h);
  const oracle=read('oracle'),cli=read('iai_cli'),mapping=read('objdump');
  const values=Object.fromEntries([...oracle.matchAll(/^(hits_before_prefill|hits_after_prefill|hits_after_terminal|terminal_hit_delta)\s*= (\d+)$/gm)].map(x=>[x[1],Number(x[2])]));
  assert.equal(values.hits_after_terminal-values.hits_after_prefill,values.terminal_hit_delta);
  assert.equal(values.terminal_hit_delta,1);
  const consistency=/^consistency_status=(.+)$/m.exec(cli)?.[1];assert.equal(consistency,'NOT_OBTAINED');
  for(const a of Object.values(atomicAddresses).flat().filter(a=>a!=='0x758df')) assert(mapping.includes(a.slice(2)+':'),`mapping missing ${a}`);
  const oracleStatus='NOT_OBTAINED_EXACT_HEAPCORE_DBG';
  const acceptance='INCOMPLETE';
  const numeric=d.route_disjoint_sum>=threshold?'GE_THRESHOLD':'LT_THRESHOLD';
  const decision=d.route_disjoint_sum>=threshold?'Conditional next gate after required evidence is completed; not implementation B':'Conditional A after required evidence is completed';
  const percent=d.route_disjoint_sum/d.Instructions*100;
  assert(Math.abs(percent*d.Instructions/100-d.route_disjoint_sum)<1e-8);
  const fields=['source_sha','source_tree','src_benches_patch_sha256','features','os','architecture','cpu','rustc','cargo','valgrind','callgrind_annotate','iai_runner','binary_sha256','sample_count_per_arm'];
  const common=[...fields.map(k=>m[k]),oracleStatus,consistency,acceptance];
  const quote=v=>'"'+String(v).replaceAll('"','""')+'"';
  const header=['metric','prefill','hit','delta',...fields,'required_oracle_status','actual_iai_consistency_status','acceptance'];
  const emptyMetadata=common.map(()=> '');
  const csv=[header,...rows.map((row,i)=>[...row,...(i===0?common:emptyMetadata)]),['numeric_threshold_Ir','','',threshold,...emptyMetadata],['supplementary_public_stats_tail_hit_delta','','',values.terminal_hit_delta,...emptyMetadata]].map(row=>row.map(quote).join(',')).join('\n')+'\n';
  const table=['| Metric | Prefill | Hit | Hit minus prefill |','|---|---:|---:|---:|',...rows.map(row=>'| '+row.join(' | ')+' |')].join('\n');
  const md=`# PRF-03 step 1 — Large route attribution (measurement-only; INCOMPLETE)\n\nSource SHA: ${m.source_sha}; tree: ${m.source_tree}. Empty src/benches patch SHA-256: ${m.src_benches_patch_sha256}. Source identity captured before measurement; binary/environment metadata observed afterward.\n\nFeatures: ${m.features}. OS: ${m.os}; architecture: ${m.architecture}; CPU: ${m.cpu}. Toolchain: ${m.rustc}; ${m.cargo}. Valgrind: ${m.valgrind}; annotate: ${m.callgrind_annotate}; iai runner: ${m.iai_runner}. Binary SHA-256: ${m.binary_sha256}. Samples per arm: ${m.sample_count_per_arm}.\n\n## Entry point\n\nHeapCore::alloc/dealloc through claim_leaked_heap (bootstrap::ensure plus registry lease), existing large_cache_prefill_only_4mib / large_cache_hit_only_4mib pair. This layer exercises the heap allocation/free implementation behind SeferAlloc's production dispatch without replacing it with standalone AllocCore. It does not measure the outer GlobalAlloc dispatch.\n\n${table}\n\nRoute share: ${d.route_disjoint_sum}/${d.Instructions} = ${percent.toFixed(3)}%. Arithmetic asserted: ${d.route_disjoint_sum} <= ${d.Instructions}; remainder ${d.Instructions-d.route_disjoint_sum} Ir. Numeric threshold: **${d.route_disjoint_sum} >= ${threshold} Ir = ${numeric}**. Acceptance: **${acceptance}**, required oracle=${oracleStatus}, actual iai consistency=${consistency}. **${decision}. Item 86 remains open; this report does not close it or authorize B.**\n\n## Inclusive accounting and atomic mapping\n\nRegister is measured on SegmentTable::register_payload -> RouteDirectory::register edges, excluding primordial bootstrap. RouteDirectory::remove and EntryHandle::drop are inlined into RouteRegistration::drop: count its inclusive edge once, including nested System/glibc deallocation, never add nested drop again. System.alloc/dealloc are inlined as malloc/free. Counts cover Large route objects only, not primordial or directory block allocation.\n\nAtomic values are computed from raw instruction records; objdump address-to-instruction mapping is retained in _raw_r19_prf03_objdump.log. next_incarnation is 0x7318e; initial register shard-lock CAS is 0x732a6; remove shard-lock CAS is 0x75446; executed refs decrement is 0x758aa (alternate 0x758df has no executed record). Initial register-lock count includes two common cold-path reacquisitions represented as a second record at the same address. Each locked RMW counts one Ir, not native latency or contention.\n\n## Required evidence still missing\n\nRequired exact-layer dbg_large_cache_hits oracle: **NOT OBTAINED**. The existing R32-7 example is only supplementary public stats().large_cache_hits evidence with alloc-stats added, through SeferAlloc: ${values.hits_before_prefill} -> ${values.hits_after_prefill} -> ${values.hits_after_terminal}; tail delta ${values.terminal_hit_delta}. It is not the required HeapCore dbg oracle and does not satisfy it. No existing-file edits were made to manufacture that oracle; routed-scan observation also remains absent.\n\nActual iai Instructions: **NOT OBTAINED**. Existing runner --help and --version were examined; narrow binary CLI probes for each function exited zero with zero Instructions rows. Runner 0.14.2 filters meta.bench_name (benchmark file), not individual library functions; passing the file would execute the forbidden full group and was not attempted. See _raw_r19_prf03_iai_cli.log. Callgrind summary equals totals; iai SummaryParser would read the Ir event from totals, but that source-level compatibility is not independent actual iai consistency. No tolerance can replace missing data; acceptance remains incomplete.\n\n## Reproduction (Ubuntu-24.04, this worktree root only)\n\nBuild: ${m.build_command}\n\nBinary: ${m.binary}\n\nPrefill single-process dispatch: --iai-run perf_gate 53 0; hit: --iai-run perf_gate 54 0. Exact bounded valgrind commands are in _raw_r19_prf03_prefill.log / _raw_r19_prf03_hit.log; both use --dump-instr=yes. Inclusive annotate each resulting profile separately with callgrind_annotate --inclusive=yes --show=Ir --threshold=100 --auto=no. An initial wildcard matching outer and inner wrappers yielded 4 Ir and was rejected; only inner-wrapper collection is used.\n\nGenerate without writes: node scripts/r19_prf03_attribution.mjs\n\nCheck byte-exact artifacts without writes: node scripts/r19_prf03_attribution.mjs --check\n\nSynthetic negative fixtures without writes: node scripts/r19_prf03_attribution.mjs --selftest\n\nEvidence: _raw_r19_prf03_identity.log, _raw_r19_prf03_prefill.log, _raw_r19_prf03_hit.log, _raw_r19_prf03_oracle.log, _raw_r19_prf03_objdump.log, _raw_r19_prf03_iai_cli.log. All tables and status rows are emitted by one checked script from these logs.\n\n## Risks and scope\n\nOne sample per arm; no repeated-profile stability claim. Normal optimized bench has no inline DWARF line tables: mapping uses binary disassembly and dynamic records. Attribution is route cost, not proven achievable B savings; no native MT result or runtime speedup. Before any future B gate preserve SEC-03 address check first and owner's counted-no-op policy. Existing item 86 is stale relative to owner Question4 and deliberately unchanged in this scope. New tests/examples/benches: 0; mutants: none; existing source/bench/Cargo/docs edits: none; git writes: none.\n\nProposed subject: bench: attribute the Large cache route share (PRF-03 step 1)\n\nProposed body: Attribute isolated Large-cache arms with disjoint inclusive System/glibc costs and reproducible metadata. Record the >=${threshold} Ir numeric result while retaining INCOMPLETE acceptance for missing exact-layer oracle and actual iai consistency; no runtime implementation or item closure.\n\nQuestions: no new policy decision required; completion of missing evidence requires an authorized measurement surface or a supported narrow runner path.\n`;
  return {csv,md};
}
const args=process.argv.slice(2);assert(args.every(a=>['--check','--selftest','--self-test'].includes(a)),'unknown option');
if(args.includes('--selftest')||args.includes('--self-test')) selftest();
else {const {csv,md}=generate(); if(args.includes('--check')) {assert.equal(fs.readFileSync(path.join(root,base+'_summary.csv'),'utf8'),csv);assert.equal(fs.readFileSync(path.join(root,base+'.md'),'utf8'),md);console.log('PASS --check: generated CSV and Markdown byte-identical; metadata/arithmetic/oracle status assertions passed; no writes');}else console.log('CSV\n'+csv+'MARKDOWN\n'+md);}
