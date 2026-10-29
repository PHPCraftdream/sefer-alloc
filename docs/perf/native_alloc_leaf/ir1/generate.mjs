import fs from 'node:fs';
import assert from 'node:assert/strict';
import crypto from 'node:crypto';
const dir='worktrees/native-alloc-leaf/docs/perf/native_alloc_leaf/ir1',samples=JSON.parse(fs.readFileSync(`${dir}/samples.json`));
const hash=b=>crypto.createHash('sha256').update(b).digest('hex');
function emit(file,text){if(process.argv.includes('--check'))assert.equal(fs.readFileSync(file,'utf8'),text,file);else fs['writeFileSync'](file,text);}
const archivedManifest=fs.existsSync(`${dir}/raw-manifest.json`)?JSON.parse(fs.readFileSync(`${dir}/raw-manifest.json`)):null;
if(archivedManifest)for(const r of samples)for(const [n,total] of [[r.n1,r.ir1],[r.n2,r.ir2]]){
 const file=`${r.phase}-${r.arm}-${r.mode}-${r.axis}-${n}.callgrind.log`,text=fs.readFileSync(`${dir}/raw/${file}`,'utf8');
 const record=archivedManifest.find(x=>x.file===file);assert(record);assert.equal(hash(text),record.excerpt_sha256);
 assert.equal(Number(/^summary:\s*(\d+)/m.exec(text)[1]),total);
}
const comparisons=[];
for(const arm of ['sefer','mimalloc'])for(const mode of ['rt','16','64','bulk64','bulk16k'])for(const axis of ['alloc','free','pair']){
 const pair=['before','after'].map(phase=>samples.find(r=>r.phase===phase&&r.arm===arm&&r.mode===mode&&r.axis===axis));assert(pair.every(Boolean));
 for(const r of pair)assert.equal((r.ir2-r.ir1)/r.operations,r.marginal_ir);
 const [a,b]=pair,delta=b.marginal_ir-a.marginal_ir;comparisons.push({arm,mode,axis,before:a.marginal_ir,after:b.marginal_ir,delta});
}
const activations=[];
for(const phase of ['before','after'])for(const mode of ['rt','16','64','bulk64','bulk16k']){
 const n=mode.startsWith('bulk')?200:100000,file=`.rush/native-owner/ir-${phase}/sefer-${mode}-pair-${n}.callgrind`;
 const archivedFile=`${phase}-sefer-${mode}-pair-${n}.callgrind.log`;
 const text=fs.readFileSync(archivedManifest?`${dir}/raw/${archivedFile}`:file,'utf8'),names=new Map();
 for(const m of text.matchAll(/^(?:c?fn)=\((\d+)\) (.*)$/gm))names.set(m[1],m[2]);
 const counts=new Map();let callee=null;
 for(const line of text.split('\n')){
 const m=/^cfn=\((\d+)\)/.exec(line);if(m)callee=m[1];
 const c=/^calls=(\d+)\b/.exec(line);if(c){assert(callee);counts.set(callee,(counts.get(callee)??0)+Number(c[1]));callee=null;}
 }
 const selected=[...counts].flatMap(([id,calls])=>{const name=names.get(id)??'';return /refill_magazine_slow$|alloc_bound_small_slow$|alloc_unbound_slow$|flush_magazine_class$/.test(name)?[{name,calls}]:[];});
 const expected_allocs=mode.startsWith('bulk')?n*64:n+(mode==='rt'?768:16);
 const observed_allocs=[...counts].reduce((sum,[id,calls])=>sum+(/own_free_(?:profile|wall)::w_alloc/.test(names.get(id)??'')&&(names.get(id)??'').includes('SeferAlloc')?calls:0),0);
 assert.equal(observed_allocs,expected_allocs,`${phase}/${mode}: actual alloc wrapper activation`);
 const refills=selected.filter(r=>r.name.endsWith('::refill_magazine_slow')).reduce((s,r)=>s+r.calls,0);assert(refills>0);
 if(mode==='16'||mode==='64')assert.equal(refills,1);
 if(phase==='after'){
 const bound=selected.filter(r=>r.name.endsWith('::alloc_bound_small_slow')).reduce((s,r)=>s+r.calls,0);
 const unbound=selected.filter(r=>r.name.endsWith('::alloc_unbound_slow')).reduce((s,r)=>s+r.calls,0);
 assert.equal(unbound,1);assert.equal(bound+unbound,refills);
 }
 activations.push({phase,mode,n,expected_allocs,observed_allocs,magazine_hits:observed_allocs-refills,refills,callgraph:selected});
 const excerpt=text.split('\n').filter(line=>/^(?:c?fn|calls)=/.test(line)).join('\n')+'\n';
 emit(`${dir}/${phase}-${mode}-callgraph.log`,'# TRUNCATED — function names and call edges only; instruction/source lines omitted. Full profile scratch hash in activation.json; reproduce via leaf-ir.sh production profile.\n'+excerpt);
 activations.at(-1).raw_callgrind_sha256=archivedManifest?archivedManifest.find(x=>x.file===archivedFile).original_sha256:hash(text);
}
for(const mode of ['rt','16','64','bulk64','bulk16k'])assert.equal(activations.find(r=>r.phase==='before'&&r.mode===mode).refills,activations.find(r=>r.phase==='after'&&r.mode===mode).refills);
emit(`${dir}/activation.json`,JSON.stringify(activations,null,2));
emit(`${dir}/comparison.json`,JSON.stringify(comparisons,null,2));
emit(`${dir}/summary.csv`,'arm,mode,axis,before_Ir,after_Ir,delta_Ir\n'+comparisons.map(r=>Object.values(r).join(',')).join('\n')+'\n');
emit('worktrees/native-alloc-leaf/docs/perf/NATIVE_ALLOC_LEAF_IR_summary.csv',fs.readFileSync(`${dir}/summary.csv`,'utf8'));
const table=comparisons.filter(r=>r.arm==='sefer').map(r=>`| ${r.mode} | ${r.axis} | ${r.before} | ${r.after} | ${r.delta} |`).join('\n');
emit('worktrees/native-alloc-leaf/docs/perf/NATIVE_ALLOC_LEAF_IR.md',`# Native alloc leaf — WSL instruction gate (не native speedup)

Production-only direct GlobalAlloc facade, baseline f80 и candidate patch из [identity](native_alloc_leaf/ir1/identity.json). Не installed allocator. 120 profiles; сырые totals: [samples.json](native_alloc_leaf/ir1/samples.json), per-profile receipts, [CSV](native_alloc_leaf/ir1/summary.csv). Marginal Ir = (total2−total1)/operation_delta. Pair — весь process с harness/setup/teardown, не сумма отдельно собранных alloc/free windows. N/2N — один и тот же ELF в каждом arm; bootstrap не объявляется per-op cost.

| Mode | Axis | Before Ir/op | After Ir/op | Delta Ir/op |
|---|---|---:|---:|---:|
${table}

Все free marginal totals равны baseline; alloc и full pair ниже во всех пяти workloads. Строгой причинной атрибуции каждого instruction нет; compiler/platform отличаются от Windows. Незначительные mimalloc pair deltas сохранены в CSV, не скрыты. [Callgraph activation](native_alloc_leaf/ir1/activation.json) хранит частоты refill/bound continuation/flush по каждому Sefer arm; raw call-edge excerpts рядом. Отсутствие helper символа при inlining не доказывает отсутствие механизма.

Вердикт: **survivor для native gate**, не GO FOR PROMOTION. Следующий этап — balanced native before/candidate/mimalloc и A/A; canonical churn16 остаётся обязательным control при выживании. Tests/CI acceptance отсутствует.
`);
console.log(JSON.stringify({comparisons,activations},null,2));
