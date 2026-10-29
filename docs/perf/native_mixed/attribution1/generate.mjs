import fs from 'node:fs';
import assert from 'node:assert/strict';
import {gunzipSync,gzipSync} from 'node:zlib';
const repo='worktrees/native-mixed-judge',dir=`${repo}/docs/perf/native_mixed/attribution1`;
function parse(text){
 assert(/^events: Ir\s*$/m.test(text),'Только Ir');
 const names=new Map();for(const m of text.matchAll(/^(?:c?fn)=\((\d+)\) (.+)$/gm))names.set(m[1],m[2]);
 const self=new Map(),calls=new Map(),inclusive=new Map();let fn=null,callee=null,callCost=false;
 for(const line of text.split('\n')){
 let m=/^fn=\((\d+)\)/.exec(line);if(m){fn=m[1];callee=null;callCost=false;continue;}
 m=/^cfn=\((\d+)\)/.exec(line);if(m){callee=m[1];continue;}
 m=/^calls=(\d+)\b/.exec(line);if(m){assert(callee);calls.set(callee,(calls.get(callee)??0)+Number(m[1]));callCost=true;continue;}
 if(!/^[-+*\d]/.test(line))continue;
 const fields=line.trim().split(/\s+/);if(fields.length!==2||!/^\d+$/.test(fields[1]))continue;
 const value=Number(fields[1]);
 if(callCost){inclusive.set(callee,(inclusive.get(callee)??0)+value);callCost=false;callee=null;}
 else{assert(fn);self.set(fn,(self.get(fn)??0)+value);}
 }
 const total=Number(/^summary:\s*(\d+)/m.exec(text)[1]);assert(total>0);
 assert.equal([...self.values()].reduce((s,x)=>s+x,0),total,'Self-cost partition must equal PROGRAM TOTALS');
 return {total,functions:[...new Set([...self.keys(),...calls.keys()])].map(id=>({id,name:names.get(id)??`#${id}`,self_ir:self.get(id)??0,inclusive_edge_ir:inclusive.get(id)??0,calls:calls.get(id)??0})).sort((a,b)=>b.self_ir-a.self_ir)};
}
const profiles=[];
for(const arm of ['sefer','mimalloc'])for(const repeats of [120,241]){
 const stem=`${dir}/${arm}-${repeats}.callgrind`,text=fs.existsSync(stem)?fs.readFileSync(stem,'utf8'):gunzipSync(fs.readFileSync(stem+'.gz')).toString();
 profiles.push({arm,repeats,...parse(text)});
}
const rows=[];
for(const arm of ['sefer','mimalloc']){
 const a=profiles.find(p=>p.arm===arm&&p.repeats===120),b=profiles.find(p=>p.arm===arm&&p.repeats===241);
 rows.push({arm,category:'TOTAL',ir120:a.total,ir241:b.total,delta_ir_per_full_round:(b.total-a.total)/121,calls120:null,calls241:null});
 for(const [category,re] of Object.entries({rust_dealloc:/__rust_dealloc$/,workload_container:/::run_workload$/})){
 const sum=(p,key)=>p.functions.filter(f=>re.test(f.name)).reduce((s,f)=>s+f[key],0);
 const ir120=sum(a,'self_ir'),ir241=sum(b,'self_ir');
 if(ir120||ir241)rows.push({arm,category:`self:${category}`,ir120,ir241,delta_ir_per_full_round:(ir241-ir120)/121,calls120:sum(a,'calls'),calls241:sum(b,'calls')});
 }
 const categories={refill:/::refill_magazine_slow$/,flush:/::flush_class_internal$/,drain:/::try_drain_freelist_batch$/,finish_refill:/::finish_magazine_refill$/,flush_run:/::flush_run$/,discovery:/::find_segment_with_free_impl$/,reclaim:/::drain_dirty_segments$/};
 for(const [category,re] of Object.entries(categories)){
 const pick=p=>p.functions.filter(f=>re.test(f.name)),x=pick(a),y=pick(b);
 if(!x.length&&!y.length)continue;
 const sum=(xs,key)=>xs.reduce((s,f)=>s+f[key],0),ir120=sum(x,'inclusive_edge_ir'),ir241=sum(y,'inclusive_edge_ir');
 rows.push({arm,category,ir120,ir241,delta_ir_per_full_round:(ir241-ir120)/121,calls120:sum(x,'calls'),calls241:sum(y,'calls')});
 }
}
const seferTotal=rows.find(r=>r.arm==='sefer'&&r.category==='TOTAL').delta_ir_per_full_round;
const miTotal=rows.find(r=>r.arm==='mimalloc'&&r.category==='TOTAL').delta_ir_per_full_round;
const instructionRatio=miTotal/seferTotal;assert(Math.abs(instructionRatio*seferTotal-miTotal)<miTotal*1e-12);
const shares=rows.filter(r=>r.arm==='sefer'&&r.category!=='TOTAL').map(r=>({category:r.category,numerator:r.delta_ir_per_full_round,denominator:seferTotal,fraction:r.delta_ir_per_full_round/seferTotal}));
const outputs=new Map();outputs.set(`${dir}/shares.json`,JSON.stringify({mi_over_sefer_ir:instructionRatio,shares},null,2)+'\n');outputs.set(`${dir}/function_profiles.json.gz`,gzipSync(JSON.stringify(profiles,null,2)+'\n'));outputs.set(`${dir}/summary.json`,JSON.stringify(rows,null,2)+'\n');
const meta=JSON.parse(fs.readFileSync(`${dir}/identity.json`));
const csvRows=rows.map(r=>({...r,base:meta.base,features:meta.features,cpu:meta.cpu,os:meta.host_os,profiles:4}));
const header=Object.keys(csvRows[0]);outputs.set(`${repo}/docs/perf/NATIVE_MIXED_ATTRIBUTION_summary.csv`,header.join(',')+'\n'+csvRows.map(r=>header.map(k=>JSON.stringify(r[k]??'NA')).join(',')).join('\n')+'\n');
outputs.set(`${repo}/docs/perf/NATIVE_MIXED_ATTRIBUTION.md`,`# Installed mixed workload — WSL Ir attribution, не native verdict\n\nPinned f80 production; actual installed global allocator, общий churn16/64/256/1024+Vec workload. Native judge calibration FAILED; product native A/B не выполнялся. Raw4profiles/sourceidentity: [attribution1](native_mixed/attribution1/identity.json). Повторы120/241; один warmup полный workload вне timing, но внутри collected process. Marginal Ir=(total241−total120)/121fullrounds. Одна fullround=run_workload с8внутренними rounds и полным setup/teardown. Это не Ir/alloc и не ns/pair.\n\n| Arm | Bucket (self: — exclusive; events — inclusive) | Ir120 | Ir241 | Marginal Ir/full round | Calls120 | Calls241 |\n|---|---|---:|---:|---:|---:|---:|\n${rows.map(r=>`| ${r.arm} | ${r.category} | ${r.ir120} | ${r.ir241} | ${r.delta_ir_per_full_round} | ${r.calls120??'NA'} | ${r.calls241??'NA'} |`).join('\n')}\n\nEvent inclusive costs могут быть вложены, их нельзя складывать. Отсутствие символа не доказывает отсутствие пути: inlining/tailcalls требуют проверки. Self-cost partition каждого profile проверен на точное равенство PROGRAM TOTALS; top function self costs в function_profiles.json.gz. Ir share не является wall-clock share. Old B4 event mix/estimates не переносятся на этот workload. Fresh source/codegen reading требуется до выбора изменения.\n\n## Решение и границы\n\nFull marginal instruction ratio mimalloc/Sefer=${instructionRatio}; это Ir ratio, НЕ native speed ratio. Доли buckets (каждая со своим numerator/denominator):\n\n${shares.map(r=>`- ${r.category}: ${r.numerator}/${r.denominator} = ${r.fraction} от полного marginal Ir.`).join('\n')}\n\nКрупнейшие подтверждённые self-контейнеры — dealloc и run_workload; последний содержит как алгоритм workload, так и инлайненный allocator, и не является pure alloc cost. Одинаковый source workload не доказывает одинаковый машинный harness floor между arms. Refill/flush существенны, но вывод о доминировании дорогих переходов именно здесь не подтверждён. Discovery72calls в обоих N — bootstrap, не marginal victim. Старые S1/R3 выводы не переоткрываются.\n\nОтдельно прочитан mi_bin exactELF: оптимизированное leaf-тело, не доказательство C -O0; CFLAGS overrides в build output отсутствуют. Значительные aligned/generic mimalloc call counts требуют отдельной layer/regime проверки перед любым обобщением, а не изменения чужого allocator для удобного сравнения.\n\n**Вердикт: ATTRIBUTION COMPLETE / PRODUCT CANDIDATE NOT OPENED.** Пригодного native judge для meaningful5/100 baseline нет; новый causal native bottleneck и новый безопасный механизм его устранения не установлены. Native A/B, broad/canonical/MT, локальные tests/Miri/Loom не запускались. Это bounded stop, не опровержение возможности ускорения. Raw source paths нормализованы в архивных profiles; receipt сохраняет hash оригинала.\n\n`);
for(const [file,text] of outputs){if(process.argv.includes('--check')){if(Buffer.isBuffer(text))assert.deepEqual(fs.readFileSync(file),text,file);else assert.equal(fs.readFileSync(file,'utf8'),text,file);}else fs.writeFileSync(file,text);}
console.log(JSON.stringify({rows,top_self:profiles.filter(p=>p.repeats===241).map(p=>({arm:p.arm,total:p.total,top:p.functions.slice(0,12)}))},null,2));
