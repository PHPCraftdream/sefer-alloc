// Сводка вычисляется из сохранённых профилей и per-process/per-sample данных.
import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import crypto from 'node:crypto';
import {fileURLToPath} from 'node:url';
import {gunzipSync} from 'node:zlib';
const here=path.dirname(fileURLToPath(import.meta.url)),perf=path.dirname(here),dir=path.join(perf,'own_free_context');
const read=p=>JSON.parse(fs.readFileSync(p,'utf8'));
const hash=b=>crypto.createHash('sha256').update(b).digest('hex');
const mean=a=>{assert(a.length);return a.reduce((s,x)=>s+x,0)/a.length;};
const ratio=(a,b)=>{assert(a>0&&b>0);const r=a/b;assert(Math.abs(r*b-a)<a*1e-12);return r;};
const fmt=x=>{assert(Number.isFinite(x));return x.toFixed(4);};
function ci(a){assert(a.length===10||a.length===20);const m=mean(a),t=a.length===20?2.093024054:2.262157163,e=t*Math.sqrt(a.reduce((s,x)=>s+(x-m)**2,0)/(a.length-1)/a.length);return [m-e,m+e];}
function patch(p){return fs.existsSync(p)?fs.readFileSync(p):gunzipSync(fs.readFileSync(p+'.gz'));}
for(const round of ['scalar','context','mir']){
 const d=path.join(here,round);
 for(const phase of ['baseline','candidate']){
  const id=read(path.join(d,phase+'_identity.json'));assert.equal(hash(patch(path.join(d,phase+'.patch'))),id.patch_sha256);
  assert.equal(hash(fs.readFileSync(path.join(d,'own_free_profile.rs'))),id.example_sha256);
 }
 const m=read(path.join(d,'comparison_identity.json'));
 for(const phase of ['before','after']){
  assert.equal(hash(patch(path.join(d,'comparison-'+phase+'.patch'))),m.identities[phase].patch_sha256);
  assert.equal(hash(fs.readFileSync(path.join(d,'own_free_wall.rs'))),m.identities[phase].files['examples/own_free_wall.rs']);
 }
}
const rounds=read(path.join(here,'rounds.json')),baseline=read(path.join(here,'context/baseline_summary.json')).rows;
const controls=read(path.join(here,'context/controls.json'));
const frontier=['rt','16','64','bulk64','bulk16k'].map(mode=>{
 const a=mode.startsWith('bulk')?mean(controls.filter(r=>r.phase==='before'&&r.arm==='sefer'&&r.mode===mode).map(r=>r.marginal_ir_per_free)):mean(baseline.filter(r=>r.arm==='sefer'&&r.mode===mode).map(r=>r.marginal_ir_per_free));
 const at=(round,arm='sefer')=>{const r=rounds.filter(r=>r.round===round&&r.arm===arm&&r.mode===mode);assert.equal(r.length,1);return r[0].Ir;};
 const b=at('context'),mi=at('context','mimalloc');assert.equal(mi,at('scalar','mimalloc'));assert.equal(mi,at('mir','mimalloc'));
 return {mode,a,scalar:at('scalar'),context:b,mir:at('mir'),mi,winBefore:ratio(mi,a),winAfter:ratio(mi,b),ba:ratio(b,a)};
});
const meta=read(path.join(dir,'comparison_identity.json'));
const samples=fs.readFileSync(path.join(dir,'raw/native.ndjson'),'utf8').trim().split('\n').map(JSON.parse);assert.equal(samples.length,840);
const native=[];
for(const [mode,n] of Object.entries(meta.native_plan.modes))for(const metric of ['workload_ns','process_ns']){
 const xs={a:[],b:[],ma:[],mb:[]},diff=[],mdiff=[];
 for(let block=0;block<20;block++){
  const rows=samples.filter(r=>!r.control&&r.mode===mode&&r.block===block).sort((a,b)=>a.position-b.position);assert.deepEqual(rows.map(r=>r.label),meta.native_plan.sequence);
  assert(rows.every(r=>r.steps===n&&r.completed&&r.workload_ns>0&&r.process_ns>r.workload_ns));
  for(const label of Object.keys(xs))xs[label].push(mean(rows.filter(r=>r.label===label).map(r=>r[metric])));
  diff.push(xs.b.at(-1)-xs.a.at(-1));mdiff.push(xs.mb.at(-1)-xs.ma.at(-1));
 }
 const pairs=mode==='rt'?n+768:mode==='16'||mode==='64'?n+16:n*64;
 const a=mean(xs.a),b=mean(xs.b),ma=mean(xs.ma),mb=mean(xs.mb),[low,high]=ci(diff),[miLow,miHigh]=ci(mdiff);
 native.push({mode,metric,pairs,a,b,ma,mb,winBefore:ratio(ma,a),winAfter:ratio(mb,b),ba:ratio(b,a),low,high,miLow,miHigh,verdict:high<0?'LOWER':low>0?'HIGHER':'INCONCLUSIVE'});
}
const aa=[];
for(const metric of ['workload_ns','process_ns']){
 const d=[];for(let block=0;block<10;block++){
  const r=samples.filter(x=>x.control&&x.block===block).sort((a,b)=>a.position-b.position);assert.equal(r.length,4);assert(r.every(x=>x.label==='a'&&x.mode==='rt'));
  d.push(mean([r[1][metric],r[2][metric]])-mean([r[0][metric],r[3][metric]]));
 }const [low,high]=ci(d);aa.push({metric,delta:mean(d),low,high});
}
const wide=read(path.join(dir,'wide/summary.json'));
const wi=read(path.join(dir,'wide/identity.json'));
for(const phase of ['before','after'])assert.equal(hash(patch(path.join(dir,'wide',phase+'.patch'))),wi.identities[phase].patch_sha256);
function parse(s){const cells={};let cell=null;for(const raw of s.split(/\r?\n/)){const l=raw.replace(/\x1b\[[0-9;]*m/g,''),h=/^\s*([A-Za-z_]\w*::)+([A-Za-z_]\w*)\b/.exec(l);if(h){cell=h[2];continue;}const m=/^\s*Instructions:\s*([\d,]+)(?:\||\s|$)/.exec(l);if(m&&cell&&cells[cell]===undefined)cells[cell]=Number(m[1].replaceAll(',',''));}return cells;}
for(const [phase,key] of [['before','a'],['after','b']]){
 const values=parse(fs.readFileSync(path.join(dir,'wide',phase+'.log'),'utf8'));assert.equal(Object.keys(values).length,wide.length);for(const r of wide)assert.equal(r[key],values[r.cell]);
}
assert.equal(read(path.join(dir,'wide/completed.json')).binary_drift,false);
const sanity=read(path.join(here,'sanity/summary.json'));
assert(sanity.clear.same_normalized_instruction_sequence);for(const r of sanity.alloc)assert.deepEqual(r.before,r.after);
const canonicalDir=path.join(dir,'canonical');assert.equal(read(path.join(canonicalDir,'completed.json')).sweeps,4);
const cm=read(path.join(canonicalDir,'identity.json')),maps=[];
for(const phase of ['before','after'])assert.equal(hash(patch(path.join(canonicalDir,phase+'.patch'))),cm.arms[phase].patch_sha256);
function walk(d,map){for(const e of fs.readdirSync(d,{withFileTypes:true})){
 const p=path.join(d,e.name);if(e.isDirectory())walk(p,map);else if(e.name==='benchmark.json'){
  const bench=read(p),id=bench.full_id,est=read(path.join(d,'estimates.json')),sample=read(path.join(d,'sample.json'));
  assert.equal(sample.times.length,sample.iters.length);assert(sample.times.length>=10);
  const avg=mean(sample.times.map((t,i)=>t/sample.iters[i]));assert(Math.abs(avg-est.mean.point_estimate)<Math.max(1e-7,avg*1e-8));
  map[id]={mean:avg,samples:sample.times.length};
 }
}}
for(const [i,phase] of cm.sequence.entries()){const map={};walk(path.join(canonicalDir,`criterion-${i}-${phase}`),map);maps.push(map);}
for(const map of maps)assert.deepEqual(Object.keys(map).sort(),Object.keys(maps[0]).sort());
const canonical=[];
for(const group of ['global_alloc','global_alloc_churn','global_alloc_churn_write','global_alloc_churn_with_teardown'])for(const size of ['16B','64B','256B','1024B']){
 const val=(arm,i)=>{const id=`${group}/${arm}/${size}`;assert(maps[i][id],id);return maps[i][id].mean/1024;};
 const a=mean([val('SeferAlloc',0),val('SeferAlloc',3)]),b=mean([val('SeferAlloc',1),val('SeferAlloc',2)]),ma=mean([val('mimalloc',0),val('mimalloc',3)]),mb=mean([val('mimalloc',1),val('mimalloc',2)]);
 canonical.push({cell:group+'/'+size,unit:group.endsWith('with_teardown')?'ns/pair composite':'ns/pair',a,b,ma,mb,winBefore:ratio(ma,a),winAfter:ratio(mb,b),ba:ratio(b,a)});
}
{
 const val=(arm,i)=>{const id=`global_alloc/manual_realloc_sim/${arm}`;assert(maps[i][id],id);return maps[i][id].mean;};
 const a=mean([val('SeferAlloc',0),val('SeferAlloc',3)]),b=mean([val('SeferAlloc',1),val('SeferAlloc',2)]),ma=mean([val('mimalloc',0),val('mimalloc',3)]),mb=mean([val('mimalloc',1),val('mimalloc',2)]);
 canonical.push({cell:'manual_realloc_sim',unit:'ns/chain',a,b,ma,mb,winBefore:ratio(ma,a),winAfter:ratio(mb,b),ba:ratio(b,a)});
}
const sweepRows=maps.flatMap((map,i)=>Object.entries(map).map(([cell,r])=>({sweep:i,phase:cm.sequence[i],cell,mean_ns_per_iteration:r.mean,samples:r.samples})));
const focusDir=path.join(dir,'focused');assert.equal(read(path.join(focusDir,'completed.json')).processes,40);
const focusSamples=fs.readFileSync(path.join(focusDir,'samples.ndjson'),'utf8').trim().split('\n').map(JSON.parse);assert.equal(focusSamples.length,40);
const focus=[];
for(const arm of ['SeferAlloc','mimalloc']){
 const aa=[],bb=[],diff=[];
 for(let block=0;block<10;block++){
  const rows=focusSamples.filter(r=>r.block===block).sort((a,b)=>a.position-b.position);assert.deepEqual(rows.map(r=>r.phase),['before','after','after','before']);
  const v=rows.map(r=>{const row=r.rows.find(r=>r.cell===`global_alloc_churn/${arm}/16B`);assert(row);return row.mean_ns_per_pair;});
  aa.push(mean([v[0],v[3]]));bb.push(mean([v[1],v[2]]));diff.push(bb.at(-1)-aa.at(-1));
 }
 const [low,high]=ci(diff);focus.push({arm,a:mean(aa),b:mean(bb),ba:ratio(mean(bb),mean(aa)),low,high,verdict:low>0?'HIGHER':high<0?'LOWER':'INCONCLUSIVE'});
}
const positive=wide.filter(r=>r.delta>0),count={lower:wide.filter(r=>r.delta<0).length,equal:wide.filter(r=>r.delta===0).length,higher:positive.length};
const lines=['# Owner-free research: измерения трёх runtime-кандидатов','',
'**Вердикт: NO-GO FOR PROMOTION; FREE-INSTRUCTION GAIN; NATIVE SPEEDUP NOT ESTABLISHED.** Ни одна экспериментальная композиция не влита в принятую базу. Нет права на shipping/merge по этому отчёту.','',
'## Механизм и границы','',
'База всех вариантов: f80c681b799ef4ef7fa08d79c06896fdf1104fe9. A (`scalar`) — отдельный one-element magazine flush с прежним flush_run. B (`context`) — A плюс передача проверенного metadata byte-slot/count в overflow helper вместо удержания root/offset в горячем frame. C (`mir`) — B с inline(always) scalar-entry; tradeoff не доминирует B. Для широкой проверки B выбран до broad/native данных. Это не сумма исторических выигрышей; каждый кандидат измерен целиком против f80. D1′, scalar-issue и два отвергнутых cold split не входят в композицию. Production feature composition, политика magazine и producer protocol не менялись.','',
'Guard-before-mutation, current LIVE credit, canonical stored-root provenance и retirement сохраняются. Slot не покидает синхронный helper; старые resident blocks flush, текущий dealloc/Box payload не трогается. Новые unsafe sites: scalar flush entry в существующем seam; private dealloc_with_state_slot и вызов после guard. Slot getter сам память не разыменовывает. Это code-review reasoning, не Miri/CI receipt.','',
'## Предыдущие гипотезы этой исследовательской цепочки','',
'| Попытка | Результат | Урок |','|---|---|---|',
'| Scalar prepared issue | INCONCLUSIVE / DO NOT PROMOTE | Повторяемая source-работа найдена, production-выигрыш не доказан. |',
'| Общий cold overflow helper | NO-GO: OVERFLOW CONTROL | Hot frame дешевле, но cap-eviction — частый путь своего режима, его цена выросла. |',
'| Только full-magazine half-flush split | NO-GO: FREE MICRO-GATE | Scalar/free режим подорожал; source-byte equivalence cap-ветки не фиксирует callee codegen. |',
'Полные прежние отчёты и собственные provenance/raw/CSV сохраняются в соседних worktrees: scalar-issue-prepared/docs/perf/SCALAR_ISSUE_PREPARED_GATE.md, own-free-cost/docs/perf/OWN_FREE_OVERFLOW_OUTLINE_GATE.md и own-free-split/docs/perf/OWN_FREE_HALF_FLUSH_SPLIT_GATE.md. Новые A/B/C не суммируются с этими отвергнутыми кандидатами. D1′ — отдельный предшествующий NO-GO (PR #96), не база этой цепочки.','',
'## Детерминированный frontier, Ir/free','',
'GlobalAlloc facade, production-only, WSL Linux; wrapper-inclusive marginal N/2N, два прохода. Не ns/free и не installed global allocator. Overflow: alloc64 затем free64, 100/200 rounds. Числа каждого варианта и controls сохранены в owner_research/{scalar,context,mir}; shared baseline хранится один раз.','',
'| Mode | f80 | A scalar | B context | C MIR | mimalloc | mi/f80 | mi/B | B/f80 |','|---|---:|---:|---:|---:|---:|---:|---:|---:|',
...frontier.map(r=>`| ${r.mode} | ${fmt(r.a)} | ${fmt(r.scalar)} | ${fmt(r.context)} | ${fmt(r.mir)} | ${fmt(r.mi)} | ${fmt(r.winBefore)} | ${fmt(r.winAfter)} | ${fmt(r.ba)} |`),'',
'B не доминирует A на bulk16k; выбран за hot/const/bulk64. Удалён generic run-discovery/bookkeeping при single-element eviction, а validated slot уменьшает live context обычного free. Это измеренное сокращение инструкций, не доказанный native прорыв. Activation oracle — callgraph generic/scalar/helper в сохранённых control profiles; 6 full flush events/round на bulk64, 60 cap-evictions/round на bulk16k, не одна универсальная «холодная» частота.','',
'## Широкая IAI-матрица B','',
`${wide.length} cells: ${count.lower} ниже, ${count.equal} равны, ${count.higher} выше. Все строки: own_free_context/wide/summary.csv. Exact built ELF запущен напрямую; before/after SHA проверены. Первая попытка через cargo пересобрала binary и отвергнута, её log сохранён отдельно; это не числовое evidence. Версия toolchain/runner не менялась ради результата.`, '',
'| Raw-positive cell | A Ir | B Ir | B−A Ir |','|---|---:|---:|---:|',...positive.map(r=>`| ${r.cell} | ${r.a} | ${r.b} | ${r.delta} |`),'',
`Clear paired increment ${sanity.clear.paired_before}→${sanity.clear.paired_after} (+${sanity.clear.paired_delta}) НЕ доказывает +3 Ir/clear: все 16 развёрнутых clear-блоков имеют одинаковую нормализованную assembly, ${sanity.clear.non_null_clear_instructions} instructions плюс один shared address lea в обоих binaries. Разница находится вне этих блоков; source-prefix не является machine-prefix. Assembly и проверяющий скрипт сохранены в owner_research/sanity.`,
...sanity.alloc.map(r=>`- Actual production alloc ${r.mode}: raw totals ${r.before.totals.join('/')} в обоих binaries; marginal ${fmt(r.before.marginal_ir_per_alloc)} Ir/alloc в обоих.`),'',
'Остальные raw-positive cells не объявлены автоматически fixed/bootstrap cost: полной per-component атрибуции realloc здесь нет. Строгая broad zero-regression приёмка не заявлена. Отдельные actual hit increments и raw totals не смешиваются.','',
'## Native: относительно mimalloc до/после','',
`CPU ${meta.cpu}; ${meta.os}. Features production; native toolchain в comparison_identity.json (отличается от WSL). 20 balanced blocks/mode: A/miA/B/miB/miB/B/miA/A, fresh process для каждого sample. Среднее двух процессов/arm внутри block, затем арифметическое среднее 20 blocks. Win=mean(mi)/mean(Sefer); >1 — Sefer быстрее. CI — nominal Student-t paired B−A, df19, без multiple-comparison correction и без удаления samples. Setup/churn/teardown входят в workload_ns; process_ns включает также startup/output/exit.`, '',
'| Mode | A ns/pair | B ns/pair | mi A | mi B | Win A | Win B | B/A | Paired 95% CI ns/pair | Result |','|---|---:|---:|---:|---:|---:|---:|---:|---|---|',
...native.filter(r=>r.metric==='workload_ns').map(r=>`| ${r.mode} | ${fmt(r.a/r.pairs)} | ${fmt(r.b/r.pairs)} | ${fmt(r.ma/r.pairs)} | ${fmt(r.mb/r.pairs)} | ${fmt(r.winBefore)} | ${fmt(r.winAfter)} | ${fmt(r.ba)} | ${fmt(r.low/r.pairs)} … ${fmt(r.high/r.pairs)} | ${r.verdict} |`),'',
'### Full process, ms/process','', '| Mode | A | B | mi A | mi B | Paired 95% CI ms |','|---|---:|---:|---:|---:|---|',
...native.filter(r=>r.metric==='process_ns').map(r=>`| ${r.mode} | ${fmt(r.a/1e6)} | ${fmt(r.b/1e6)} | ${fmt(r.ma/1e6)} | ${fmt(r.mb/1e6)} | ${fmt(r.low/1e6)} … ${fmt(r.high/1e6)} |`),'',
...aa.map(r=>`- A/A ${r.metric}: inner−outer mean ${fmt(r.delta)} ns; nominal 95% CI df9 ${fmt(r.low)} … ${fmt(r.high)} ns.`),
...native.filter(r=>r.metric==='workload_ns').map(r=>`- mimalloc ${r.mode}: B/A ${fmt(ratio(r.mb,r.ma))}; paired 95% CI B−A ${fmt(r.miLow)} … ${fmt(r.miHigh)} ns/workload.`),'',
'## Canonical bench:table ABBA','',
'Четыре sweep: A/B/B/A, два process-level sweep на arm. Каждая ячейка — среднее arithmetic per-iteration mean из Criterion sample.json двух sweep, не pooled independent samples и не статистическая significance по двум sweep. 1024 pairs/iteration; manual_realloc_sim — ns/полная growth chain. Сохраняются все Criterion samples/estimates и полный bench:table log, включая System. Это прямой GlobalAlloc facade, НЕ installed global allocator.','',
'| Cell | Unit | Sefer A | Sefer B | mi A | mi B | Win A | Win B | B/A |','|---|---|---:|---:|---:|---:|---:|---:|---:|',
...canonical.map(r=>`| ${r.cell} | ${r.unit} | ${fmt(r.a)} | ${fmt(r.b)} | ${fmt(r.ma)} | ${fmt(r.mb)} | ${fmt(r.winBefore)} | ${fmt(r.winAfter)} | ${fmt(r.ba)} |`),'',
'Canonical содержит сигналы ухудшения: меньший Ir/free НЕ закрывает эти ячейки. Все sweep, включая System и остальные benchmark groups, сохранены в owner_research/canonical_sweeps.csv; raw samples не отфильтрованы. Ни общий geometric mean, ни случайно выгодные ячейки не заменяют проверки ухудшений.','',
'### Отдельная проверка churn16B','',
'После обнаружения canonical-сигнала выполнены 10 коротких ABBA blocks точных уже построенных canonical binaries, без Cargo и без изменения source. Каждый из 40 процессов измеряет SeferAlloc и mimalloc в одном и том же bench filter; два процесса каждого source arm на block. Это те же production internals bench-internals и canonical harness, а не прежний production-only micro probe. Mean — arithmetic Criterion per-iteration time/1024; paired nominal 95% CI df9 по block means.','',
'| Arm | A ns/pair | B ns/pair | B/A | Paired 95% CI ns/pair | Result |','|---|---:|---:|---:|---|---|',
...focus.map(r=>`| ${r.arm} | ${fmt(r.a)} | ${fmt(r.b)} | ${fmt(r.ba)} | ${fmt(r.low)} … ${fmt(r.high)} | ${r.verdict} |`),'',
`Focused win mimalloc/Sefer: ${fmt(ratio(focus[1].a,focus[0].a))} → ${fmt(ratio(focus[1].b,focus[0].b))}. Это отдельный follow-up, не подмена первоначального ABBA: оба набора сохранены. Другие canonical ухудшения этим одним filter не закрываются.`, '',
'## Вывод и остатки','',
'Удаление реально исполняемой работы найдено, но «прорыв относительно mimalloc» не доказан. Уменьшение free Ir не гарантирует ускорение полного workload: alloc, адресация, TLS/route, frontend/backend и шум остаются. Случайные attribute permutations и новые caches без trace не продолжаются. Следующий обоснованный масштаб — representative application trace и activation на нужном layer; U31/epoch/protocol redesign требует отдельного доказательства, а не экстраполяции этих чисел.','',
'Test execution, Miri/Loom, CI, MT/Larson/mstress и retention acceptance для B не получены. Написанные regression tests и compile/clippy receipts не заменяют execution. Никаких commit/push/merge, dependency/version bumps или изменений README baseline по эксперименту. Perf item 117 закрыт как NO-GO FOR PROMOTION; история и reopen trigger — OPEN_ITEMS_ARCHIVE.md § L117.','',
'Provenance: owner_research/{scalar,context,mir}/*identity.json и patches; own_free_context/{comparison_identity.json,native_binaries.json,wide/identity.json,wide/attempt2-identity.json,canonical/identity.json}. Source identity зафиксирована до measurement; binary drift guards сохранены. Reproduction scripts — owner_research/reproduction/. Проверка артефакта: node docs/perf/owner_research/generate.mjs --check.',''];
const csv=[['axis','cell','unit','a','b','mi_a','mi_b','mi_over_a','mi_over_b','b_over_a','ci_low','ci_high','base_sha','patch_sha256','features','cpu','os','process_blocks']];
for(const r of native)csv.push(['native',r.mode,r.metric,r.a,r.b,r.ma,r.mb,r.winBefore,r.winAfter,r.ba,r.low,r.high,meta.identities.after.base,meta.identities.after.patch_sha256,'production',meta.cpu,meta.os,20]);
for(const r of canonical)csv.push(['canonical',r.cell,r.unit,r.a,r.b,r.ma,r.mb,r.winBefore,r.winAfter,r.ba,'','',cm.arms.after.base,cm.arms.after.patch_sha256,cm.features,cm.cpu,cm.os,2]);
for(const r of focus)csv.push(['focused_churn16B',r.arm,'ns/pair',r.a,r.b,'','','','',r.ba,r.low,r.high,cm.arms.after.base,cm.arms.after.patch_sha256,cm.features,cm.cpu,cm.os,10]);
for(const r of frontier)csv.push(['Ir/free',r.mode,'marginal',r.a,r.context,r.mi,r.mi,r.winBefore,r.winAfter,r.ba,'','',meta.identities.after.base,r.mode.startsWith('bulk')?meta.identities.after.patch_sha256:read(path.join(here,'context/candidate_identity.json')).patch_sha256,'production',meta.cpu,'WSL Linux; toolchain logs in owner_research',2]);
const cell=x=>/[,"\n\r]/.test(String(x))?'"'+String(x).replaceAll('"','""')+'"':String(x);
function emit(p,s){if(process.argv.includes('--check'))assert.equal(fs.readFileSync(p,'utf8'),s);else fs.writeFileSync(p,s);}
emit(path.join(perf,'OWNER_FREE_RESEARCH_GATE.md'),lines.join('\n'));
emit(path.join(perf,'OWNER_FREE_RESEARCH_GATE_summary.csv'),csv.map(r=>r.map(cell).join(',')).join('\n')+'\n');
emit(path.join(here,'canonical_sweeps.csv'),[['sweep','phase','cell','mean_ns_per_iteration','samples'],...sweepRows.map(r=>[r.sweep,r.phase,r.cell,r.mean_ns_per_iteration,r.samples])].map(r=>r.map(cell).join(',')).join('\n')+'\n');
emit(path.join(here,'summary.json'),JSON.stringify({frontier,native,aa,canonical,focus,wide:count},null,2));
console.log(JSON.stringify({native:native.filter(r=>r.metric==='workload_ns'),canonical,wide:count},null,2));
