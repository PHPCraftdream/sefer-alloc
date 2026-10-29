import fs from 'node:fs';
import assert from 'node:assert/strict';
const dir='worktrees/native-alloc-leaf/docs/perf/native_alloc_leaf/native1';
const meta=JSON.parse(fs.readFileSync(`${dir}/identity.json`));
assert(JSON.parse(fs.readFileSync(`${dir}/completed.json`)).samples===840);
const samples=fs.readFileSync(`${dir}/samples.ndjson`,'utf8').trim().split('\n').map(JSON.parse);assert.equal(samples.length,840);
const mean=a=>{assert(a.length);return a.reduce((s,x)=>s+x,0)/a.length;};
const ratio=(a,b)=>{assert(a>0&&b>0);const r=a/b;assert(Math.abs(r*b-a)<a*1e-12);return r;};
function ci(a){assert(a.length===10||a.length===20);const m=mean(a),t=a.length===20?2.093024054:2.262157163,e=t*Math.sqrt(a.reduce((s,x)=>s+(x-m)**2,0)/(a.length-1)/a.length);return [m-e,m+e];}
const native=[];
for(const [mode,n] of Object.entries(meta.modes))for(const metric of ['workload_ns','process_ns']){
 const xs={a:[],b:[],ma:[],mb:[]},diff=[],mdiff=[];
 for(let block=0;block<20;block++){
 const rows=samples.filter(r=>!r.control&&r.mode===mode&&r.block===block).sort((a,b)=>a.position-b.position);assert.deepEqual(rows.map(r=>r.label),meta.sequence);
 assert(rows.every(r=>r.steps===n&&r.completed&&r.workload_ns>0&&r.process_ns>r.workload_ns));
 for(const label of Object.keys(xs))xs[label].push(mean(rows.filter(r=>r.label===label).map(r=>r[metric])));
 diff.push(xs.b.at(-1)-xs.a.at(-1));mdiff.push(xs.mb.at(-1)-xs.ma.at(-1));
 }
 const pairs=mode==='rt'?n+768:mode==='16'||mode==='64'?n+16:n*64;
 const a=mean(xs.a)/pairs,b=mean(xs.b)/pairs,ma=mean(xs.ma)/pairs,mb=mean(xs.mb)/pairs;
 const [low,high]=ci(diff.map(x=>x/pairs)),[miLow,miHigh]=ci(mdiff.map(x=>x/pairs));
 native.push({mode,metric,pairs,before_ns_pair:a,after_ns_pair:b,miBefore_ns_pair:ma,miAfter_ns_pair:mb,winBefore:ratio(ma,a),winAfter:ratio(mb,b),after_over_before:ratio(b,a),delta_ns_pair:mean(diff)/pairs,low,high,miLow,miHigh,verdict:high<0?'LOWER':low>0?'HIGHER':'INCONCLUSIVE'});
}
const aa=[];
for(const metric of ['workload_ns','process_ns']){
 const d=[];for(let block=0;block<10;block++){
 const r=samples.filter(x=>x.control&&x.block===block).sort((a,b)=>a.position-b.position);assert.equal(r.length,4);assert(r.every(x=>x.label==='a'&&x.mode==='rt'));
 d.push((mean([r[1][metric],r[2][metric]])-mean([r[0][metric],r[3][metric]]))/1000768);
 }const [low,high]=ci(d);aa.push({metric,delta_ns_pair:mean(d),low,high});
}
const outputs=new Map(),header=Object.keys(native[0]);
outputs.set(`${dir}/summary.json`,JSON.stringify({native,aa},null,2)+'\n');
outputs.set(`${dir}/summary.csv`,header.join(',')+'\n'+native.map(r=>header.map(k=>r[k]).join(',')).join('\n')+'\n');
outputs.set('worktrees/native-alloc-leaf/docs/perf/NATIVE_ALLOC_LEAF_NATIVE_summary.csv',outputs.get(`${dir}/summary.csv`));
const table=native.filter(r=>r.metric==='workload_ns').map(r=>`| ${r.mode} | ${r.before_ns_pair.toFixed(6)} | ${r.after_ns_pair.toFixed(6)} | ${r.winBefore.toFixed(6)} → ${r.winAfter.toFixed(6)} | ${r.low.toFixed(6)} … ${r.high.toFixed(6)} | ${r.verdict} |`).join('\n');
const processTable=native.filter(r=>r.metric==='process_ns').map(r=>`| ${r.mode} | ${r.before_ns_pair.toFixed(6)} | ${r.after_ns_pair.toFixed(6)} | ${r.low.toFixed(6)} … ${r.high.toFixed(6)} | ${r.verdict} |`).join('\n');
outputs.set('worktrees/native-alloc-leaf/docs/perf/NATIVE_ALLOC_LEAF_NATIVE.md',`# Alloc leaf — native measurement, не shipping acceptance

Direct SeferAlloc GlobalAlloc facade против pinned f80 и mimalloc, production-only Windows-MSVC. Source identities ДО build, binary hashes до/после runs: [native1](native_alloc_leaf/native1/identity.json). Candidate — свежий exact codegen-gated PE attempt1, не пересобран при timing; baseline построен заново тем же rustc. Raw840 process samples: samples.ndjson рядом. 20 balanced blocks каждого режима, 10 A/A blocks runtime. Arithmetic means; paired Student-t95 CI по разности block means B−A. Все samples сохранены; выбросы не исключались. Ratio win = mean(mimalloc)/mean(Sefer), больше лучше; это не ratio отдельных произвольно выбранных samples.

## Полный workload, включая setup/teardown

Нормировка: const n+16 pairs, runtime n+768, bulk64/bulk16k n×64. Не pure alloc latency. Проба не Criterion; отдельной Criterion full-round mean здесь нет.

| Mode | Before ns/pair | After ns/pair | Win mi/Sefer до → после | Paired95 CI delta ns/pair | Verdict |
|---|---:|---:|---:|---:|---|
${table}

## Полный процесс, включая startup/output/exit

| Mode | Before ns/pair | After ns/pair | Paired95 CI delta ns/pair | Verdict |
|---|---:|---:|---:|---|
${processTable}

A/A: ${aa.map(r=>`${r.metric}: delta ${r.delta_ns_pair}, CI ${r.low} … ${r.high}`).join('; ')}.

Это один bounded gate, не доказанная совокупная победа allocator. Отсутствие значимого ухудшения не доказывает нерегрессию; native benefit и WSL Ir — разные оси. Canonical churn16, installed/global, MT и correctness execution не покрываются этим harness. Вердикт этого gate: **INCONCLUSIVE / DO NOT PROMOTE** — все пять workload CI пересекают ноль; полный process не устанавливает benefit. Codegen и Ir механизмы подтверждены, но native-прорыв не установлен. По пререгистрации broad/canonical/installed/MT не запускались: кандидат не прошёл native benefit gate. Это не доказанная регрессия и не доказанная нерегрессия. Повторов до удобного результата и attribute-перебора нет. Следующий эксперимент допустим только с новым причинным evidence; сейчас такой гипотезы не установлено.
`);
for(const [file,text] of outputs){if(process.argv.includes('--check'))assert.equal(fs.readFileSync(file,'utf8'),text,file);else fs.writeFileSync(file,text);}
console.log(JSON.stringify({native,aa},null,2));
