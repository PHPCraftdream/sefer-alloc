// Structured raw profiles и native samples → единый CSV/Markdown.
import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import crypto from 'node:crypto';
import {fileURLToPath} from 'node:url';
import {gunzipSync} from 'node:zlib';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'..');
const dir=path.join(root,'docs/perf/own_free_single');
const raw=path.join(dir,'raw');
const json=f=>JSON.parse(fs.readFileSync(path.join(dir,f),'utf8'));
const hash=b=>crypto.createHash('sha256').update(b).digest('hex');
const mean=a=>{assert(a.length);return a.reduce((x,y)=>x+y,0)/a.length;};
const ratio=(a,b)=>{assert(a>0&&b>0&&Number.isFinite(a)&&Number.isFinite(b));const r=a/b;assert(Math.abs(r*b-a)<a*1e-12);return r;};
const fmt=x=>{assert(Number.isFinite(x));return x.toFixed(4);};
function ci(a){assert(a.length===10||a.length===20);const m=mean(a),se=Math.sqrt(a.reduce((s,x)=>s+(x-m)**2,0)/(a.length-1)/a.length),t=a.length===20?2.093024054:2.262157163;return [m-t*se,m+t*se];}
function total(f){const text=fs.readFileSync(f,'utf8'),m=/([\d,]+)\s+\([^)]*\)\s+PROGRAM TOTALS/.exec(text);assert(m,f);return Number(m[1].replaceAll(',',''));}
function patch(name){const p=path.join(dir,name);return fs.existsSync(p)?fs.readFileSync(p):gunzipSync(fs.readFileSync(p+'.gz'));}
for(const phase of ['baseline','candidate']){
 const id=json(`${phase}_identity.json`);
 assert.equal(hash(patch(`${phase}.patch`)),id.patch_sha256);
 assert.equal(hash(fs.readFileSync(path.join(dir,'own_free_profile.rs'))),id.example_sha256);
}
const meta=json('comparison_identity.json');
for(const phase of ['before','after']){
 assert.equal(hash(patch(`comparison-${phase}.patch`)),meta.identities[phase].patch_sha256);
 assert.equal(hash(fs.readFileSync(path.join(dir,'own_free_wall.rs'))),meta.identities[phase].files['examples/own_free_wall.rs']);
}
const profile={baseline:json('baseline_summary.json'),candidate:json('candidate_summary.json')};
const micro=[];
for(const mode of ['rt','16','64']){
 const values={};
 for(const phase of ['baseline','candidate'])for(const arm of ['sefer','mimalloc']){
  const rows=profile[phase].rows.filter(x=>x.mode===mode&&x.arm===arm);assert.equal(rows.length,2);
  for(const r of rows){const a=total(path.join(raw,phase,`p${r.pass}-${arm}-${mode}-50000.annotate`)),b=total(path.join(raw,phase,`p${r.pass}-${arm}-${mode}-100000.annotate`));assert.equal(a,r.ir1);assert.equal(b,r.ir2);assert.equal((b-a)/50000,r.marginal_ir_per_free);}
  assert(Math.abs(rows[0].marginal_ir_per_free-rows[1].marginal_ir_per_free)<=0.01);
  values[`${phase}_${arm}`]=mean(rows.map(r=>r.marginal_ir_per_free));
 }
 assert(Math.abs(values.baseline_mimalloc-values.candidate_mimalloc)<=0.01);
 micro.push({mode,a:values.baseline_sefer,b:values.candidate_sefer,ma:values.baseline_mimalloc,mb:values.candidate_mimalloc});
}
const controls=json('raw/overflow_controls.json');
const controlRows=[];
for(const mode of ['bulk64','bulk16k']){
 const v={};
 for(const phase of ['before','after'])for(const arm of ['sefer','mimalloc']){
  const rows=controls.filter(r=>r.phase===phase&&r.arm===arm&&r.mode===mode);assert.equal(rows.length,2);
  for(const r of rows){const a=total(path.join(raw,`control-${phase}`,`p${r.pass}-${arm}-${mode}-100.annotate`)),b=total(path.join(raw,`control-${phase}`,`p${r.pass}-${arm}-${mode}-200.annotate`));assert.equal(a,r.ir1);assert.equal(b,r.ir2);assert.equal((b-a)/6400,r.marginal_ir_per_free);}
  assert(Math.abs(rows[0].marginal_ir_per_free-rows[1].marginal_ir_per_free)<=0.01);
  v[`${phase}_${arm}`]=mean(rows.map(r=>r.marginal_ir_per_free));
 }
 assert(Math.abs(v.before_mimalloc-v.after_mimalloc)<=0.01);
 controlRows.push({mode,a:v.before_sefer,b:v.after_sefer,ma:v.before_mimalloc,mb:v.after_mimalloc});
}
const events=json('activation.json');
assert(events.every(r=>r.expected===r.actual&&r.actual>=0));
const samples=fs.readFileSync(path.join(raw,'native.ndjson'),'utf8').trim().split('\n').map(JSON.parse);
assert.equal(samples.length,840);
const native=[];
for(const [mode,n] of Object.entries(meta.native_plan.modes))for(const metric of ['workload_ns','process_ns']){
 const xs={a:[],b:[],ma:[],mb:[]},diff=[],mdiff=[];
 for(let block=0;block<20;block++){
  const rows=samples.filter(r=>!r.control&&r.mode===mode&&r.block===block).sort((a,b)=>a.position-b.position);
  assert.deepEqual(rows.map(r=>r.label),meta.native_plan.sequence);
  assert(rows.every(r=>r.steps===n&&r.completed&&r.workload_ns>0&&r.process_ns>r.workload_ns));
  for(const label of Object.keys(xs))xs[label].push(mean(rows.filter(r=>r.label===label).map(r=>r[metric])));
  diff.push(xs.b.at(-1)-xs.a.at(-1));mdiff.push(xs.mb.at(-1)-xs.ma.at(-1));
 }
 const [low,high]=ci(diff),[miLow,miHigh]=ci(mdiff);
 const pairs=mode==='rt'?n+768:mode==='16'||mode==='64'?n+16:n*64;
 const a=mean(xs.a),b=mean(xs.b),ma=mean(xs.ma),mb=mean(xs.mb);
 native.push({mode,metric,n,pairs,a,b,ma,mb,winBefore:ratio(ma,a),winAfter:ratio(mb,b),ba:ratio(b,a),delta:mean(diff),low,high,miLow,miHigh,verdict:high<0?'LOWER':low>0?'HIGHER':'INCONCLUSIVE'});
}
const aa=[];
for(const metric of ['workload_ns','process_ns']){
 const d=[];
 for(let block=0;block<10;block++){
  const rows=samples.filter(r=>r.control&&r.block===block).sort((a,b)=>a.position-b.position);assert.equal(rows.length,4);assert(rows.every(r=>r.label==='a'&&r.mode==='rt'));
  d.push(mean([rows[1][metric],rows[2][metric]])-mean([rows[0][metric],rows[3][metric]]));
 }
 const [low,high]=ci(d);aa.push({metric,delta:mean(d),low,high});
}
const microPass=micro.every(r=>r.b<=r.a);
const controlPass=controlRows.every(r=>r.mode==='bulk16k'?r.b<=r.a-30:r.b<=r.a);
const verdict=!microPass?'NO-GO: FREE MICRO-GATE':!controlPass?'NO-GO: OVERFLOW CONTROL':'EARLY GATES GO; FULL ACCEPTANCE PENDING';
const csvCell=x=>/[,"\n\r]/.test(String(x))?'"'+String(x).replaceAll('"','""')+'"':String(x);
const csvRows=[['axis','mode','metric','a','b','mimalloc_a','mimalloc_b','mi_over_a','mi_over_b','b_over_a','paired_ci_low','paired_ci_high','base_sha','candidate_patch_sha256','features','cpu','os','samples']];
const provenance=[meta.identities.after.base,meta.identities.after.patch_sha256,meta.features,meta.cpu,meta.os];
for(const r of [...micro,...controlRows]){
 const measuredPatch = r.mode.startsWith('bulk') ? meta.identities.after.patch_sha256 : json('candidate_identity.json').patch_sha256;
 const linuxProvenance=[meta.identities.after.base,measuredPatch,meta.features,meta.cpu,'WSL Linux; exact toolchain in baseline_toolchain.log'];
 csvRows.push(['Ir/free',r.mode,'marginal',r.a,r.b,r.ma,r.mb,ratio(r.ma,r.a),ratio(r.mb,r.b),ratio(r.b,r.a),'','',...linuxProvenance,2]);
}
for(const r of native)csvRows.push(['native',r.mode,r.metric,r.a,r.b,r.ma,r.mb,r.winBefore,r.winAfter,r.ba,r.low,r.high,...provenance,20]);
const lines=['# Single-element magazine flush — runtime-кандидат и измеренный эффект','',`Дата отчёта: 2026-10-25. **${verdict}**. Это эксперимент, не принятый runtime change.`,'',
'## Что проверялось','',
'База f80c681b799ef4ef7fa08d79c06896fdf1104fe9. Production diff — два файла: crate-private unsafe one-element flush в AllocCore переиспользует прежний flush_run; HeapCore меняет только cap-eviction call site. Frame/half-flush/guard/counter/policy и public API не менялись. Новый вход не cold, inline-never; общий batch-loop/recycled-run bookkeeping не нужен для единственного элемента. D1′ и scalar issue не включены. Пререгистрация: docs/design/2026-10-24-single-flush-prereg.md. Все source identities записаны до соответствующих сборок; hashes и точные patches находятся в own_free_single/.','',
'## Детерминированный free: относительно mimalloc до/после','',
'Production-only GlobalAlloc facade, WSL Linux, не installed allocator и не native wall clock. Один бинарник каждого source arm содержит обоих allocators. N/2N=50000/100000, два независимых прохода. Ir/free — marginal wrapper-inclusive instruction count, не ns и не чистая стоимость тела allocator. Контроль mimalloc и повторяемость сверены с raw; backend- и source-различия не скрыты.','',
'| Mode | Sefer A Ir/free | Sefer B | mimalloc A | mimalloc B | mi/A | mi/B | B/A |','|---|---:|---:|---:|---:|---:|---:|---:|',
...micro.map(r=>`| ${r.mode} | ${fmt(r.a)} | ${fmt(r.b)} | ${fmt(r.ma)} | ${fmt(r.mb)} | ${fmt(ratio(r.ma,r.a))} | ${fmt(ratio(r.mb,r.b))} | ${fmt(ratio(r.b,r.a))} |`),'',
'## Проверка цены overflow','',
'Bulk round: alloc64, затем free64. Sizes64B/16KiB; 100→200 rounds, normalizer6400 frees; два прохода каждого source/allocator arm. Это намеренно overflow-heavy control, а не такой же regime, как const churn. Callgraph activation oracle: generic/one-element call counts сверены по raw: A generic6/60 per round; B generic6/0 и one-element0/60 на bulk64/bulk16k. activation.json.','',
'| Mode | Sefer A Ir/free | Sefer B | mimalloc A | mimalloc B | mi/A | mi/B | B/A |','|---|---:|---:|---:|---:|---:|---:|---:|',
...controlRows.map(r=>`| ${r.mode} | ${fmt(r.a)} | ${fmt(r.b)} | ${fmt(r.ma)} | ${fmt(r.mb)} | ${fmt(ratio(r.ma,r.a))} | ${fmt(ratio(r.mb,r.b))} | ${fmt(ratio(r.b,r.a))} |`),'',
'## Native Windows: явно относительно mimalloc до/после','',
`CPU ${meta.cpu}; ${meta.os}; features production. 20 fresh-process balanced blocks/mode: A/miA/B/miB/miB/B/miA/A. На block усредняются два одноимённых процесса, далее арифметическое среднее по20 blocks. Win=mean(mimalloc)/mean(Sefer); >1 — Sefer быстрее, <1 — медленнее. B/A<1 — сокращение времени Sefer. CI — Student-t paired mean B−A, df19. Это composite workload с setup/churn/teardown, НЕ pure free latency. Последовательные bulk allocations намеренно активируют другие paths, чем churn.`, '',
'### Timed workload (ns/pair composite)','',
'| Mode | Sefer A | Sefer B | mimalloc A | mimalloc B | Win A | Win B | B/A | Paired delta 95% CI ns/pair | Result |','|---|---:|---:|---:|---:|---:|---:|---:|---|---|',
...native.filter(r=>r.metric==='workload_ns').map(r=>`| ${r.mode} | ${fmt(r.a/r.pairs)} | ${fmt(r.b/r.pairs)} | ${fmt(r.ma/r.pairs)} | ${fmt(r.mb/r.pairs)} | ${fmt(r.winBefore)} | ${fmt(r.winAfter)} | ${fmt(r.ba)} | ${fmt(r.low/r.pairs)} … ${fmt(r.high/r.pairs)} | ${r.verdict} |`),'',
'### Full process (ms/process)','',
'| Mode | A | B | mi A | mi B | B/A | Paired delta 95% CI ms |','|---|---:|---:|---:|---:|---:|---|',
...native.filter(r=>r.metric==='process_ns').map(r=>`| ${r.mode} | ${fmt(r.a/1e6)} | ${fmt(r.b/1e6)} | ${fmt(r.ma/1e6)} | ${fmt(r.mb/1e6)} | ${fmt(r.ba)} | ${fmt(r.low/1e6)} … ${fmt(r.high/1e6)} |`),'',
'### A/A и неизменённый mimalloc','',
...aa.map(r=>`- A/A ${r.metric}: paired delta ${fmt(r.delta)} ns, 95% CI ${fmt(r.low)} … ${fmt(r.high)} ns.`),
...native.filter(r=>r.metric==='workload_ns').map(r=>`- mimalloc ${r.mode}: mean B/A=${fmt(ratio(r.mb,r.ma))}; paired 95% CI ${fmt(r.miLow)} … ${fmt(r.miHigh)} ns/workload.`),'',
'Если A/A или mimalloc control показывает направленный сдвиг, native attribution ограничена; красивое отношение само по себе не доказывает causal speedup. Полная IAI/canonical/Larson/mstress/MT/retention приёмка не заявляется; её выполнение и отсутствие execution/CI receipts отмечаются отдельно, ранний GO не является shipping acceptance.','',
'## Безопасность и состояние проверки','',
'Guard-before-mutation сохранён. One-element entry получает прежний evicted block и тот же class; null/missing-root paths совпадают с generic slice длины1. Stored root разрешается через ту же owner table, затем вызывается неизменённый flush_run; guard logic не копируется. Eviction возвращает в substrate старый magazine block, не текущий Box-frame block. Порядок flush/compact/state/mask/counters сохранён. Новый crate-private unsafe entry с #Safety контрактом находится в существующем magazine seam, caller unsafe block прежний. Новых public hooks или safe raw-pointer metadata API нет.','',
'Новая tests/own_free_single_flush.rs проверяет full16 и smaller byte cap, exact residency/state/live credits и последующий reuse. Она прошла Clippy/compile-check вместе с production internals bench-internals; negative core-only internals check прошёл. Test execution/Miri/Loom/CI не получены, локально не разрешены. Поэтому никаких acceptance/merge claims; commit/push/merge не выполнялись.','',
'Raw artifacts и reproducible scripts: own_free_single/. Проверка отчёта: node scripts/own_free_single_report.mjs --check. Source paths в опубликованных profiles нормализованы, числовые строки сохранены. Free baseline reference переиспользована из предыдущего раунда без изменения original identity; source/toolchain/harness фиксированы. Baseline native/overflow arms свежие. Новые snapshots каждого этапа имеют свои identities.',''];
function emit(file,text){if(process.argv.includes('--check'))assert.equal(fs.readFileSync(file,'utf8'),text);else fs.writeFileSync(file,text);}
emit(path.join(root,'docs/perf/OWN_FREE_SINGLE_FLUSH_GATE.md'),lines.join('\n'));
emit(path.join(root,'docs/perf/OWN_FREE_SINGLE_FLUSH_GATE_summary.csv'),csvRows.map(r=>r.map(csvCell).join(',')).join('\n')+'\n');
console.log(JSON.stringify({verdict,micro,controlRows,native,aa},null,2));
