import fs from 'node:fs';
import assert from 'node:assert/strict';
const root=process.cwd(),base=root+'/.rush/own-free-context';
const rows=[];
for(const mode of ['rt','16','64']){
 const r={mode};
 for(const phase of ['before','after']){
  const totals=[50000,100000].map(n=>Number(fs.readFileSync(`${base}/alloc-sanity/${phase}-${mode}-${n}.callgrind`,'utf8').match(/^summary: (\d+)/m)[1]));
  r[phase]={totals,marginal_ir_per_alloc:(totals[1]-totals[0])/50000};
 }
 assert.deepEqual(r.before,r.after);rows.push(r);
}
const seq={};
for(const phase of ['before','after']){
 const chunks=fs.readFileSync(`${base}/clear-asm/${phase}.asm`,'utf8').split(/(?=^[0-9a-f]+ <)/m);
 const body=chunks.find(x=>/^.*<perf_gate_iai::alloc_clear_magazine_only_16b::__iai_callgrind_wrapper_mod::alloc_clear_magazine_only_16b>:/m.test(x.split('\n')[0]));assert(body);
 const ins=body.split('\n').map(l=>/^\s*[0-9a-f]+:\s+(?:[0-9a-f]{2}\s+)+\s*([a-z][a-z0-9]*\s+.*|ret)\s*$/.exec(l)?.[1]).filter(Boolean);
 const masks=ins.flatMap((s,i)=>s.includes('andb')&&s.includes('$0x3f,')?[i]:[]);assert.equal(masks.length,17);
 const start=masks[1]-7,end=masks.at(-1)+1;
 seq[phase]=ins.slice(start,end).map(s=>s.replace(/0x[0-9a-f]+\(%rsp\)/g,'STACK(%rsp)').replace(/je\s+[0-9a-f]+.*$/,'je NEXT'));
 assert.equal(seq[phase].filter(s=>s.includes('andb')).length,16);
 assert.equal(seq[phase].length,129);
}
assert.deepEqual(seq.before,seq.after);
const wide=JSON.parse(fs.readFileSync(root+'/worktrees/own-free-context/docs/perf/own_free_context/wide/summary.json','utf8'));
const find=n=>{const r=wide.find(r=>r.cell===n);assert(r);return r;};
const full=find('alloc_clear_magazine_only_16b'),prefix=find('alloc_clear_magazine_only_16b_prefix');
const result={alloc:rows,clear:{same_normalized_instruction_sequence:true,non_null_clear_instructions:128,shared_black_box_address_instruction:1,paired_before:full.a-prefix.a,paired_after:full.b-prefix.b,paired_delta:(full.b-prefix.b)-(full.a-prefix.a),verdict:'Парное вычитание не изолирует clear: 16 clear-блоков одинаковы; разница находится вне них. Это не доказательство полной нерегрессии всех alloc workloads.'},normalized_clear_sequence:seq.before};
fs.writeFileSync(`${base}/sanity-summary.json`,JSON.stringify(result,null,2));console.log(JSON.stringify({...result,normalized_clear_sequence:undefined},null,2));
