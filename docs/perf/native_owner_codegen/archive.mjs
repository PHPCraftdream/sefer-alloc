import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import assert from 'node:assert/strict';
const hash=b=>crypto.createHash('sha256').update(b).digest('hex');
const out='docs/perf/native_owner_codegen';fs.mkdirSync(out,{recursive:true});
const data=JSON.parse(fs.readFileSync('.rush/native-owner/functions.json','utf8'));
const labels=['alloc_rt','free16','free64','alloc16','alloc64','free_rt'];
const rows=[];
for(const r of data){assert.equal(r.tlsWrappers.length,labels.length);for(const [i,f] of r.tlsWrappers.entries()){
 const text=fs.readFileSync(f.file,'utf8');const name=`${r.phase}-${labels[i]}.asm`;fs.copyFileSync(f.file,out+'/'+name);
 const pushes=f.prologue.filter(l=>/\bpush\s/.test(l)).length;
 const stack=f.prologue.map(l=>/sub\s+\$0x([0-9a-f]+),%rsp/.exec(l)).find(Boolean);assert(stack);
 rows.push({phase:r.phase,wrapper:labels[i],rva:f.start,bytes:f.bytes,pushes,stack_sub:parseInt(stack[1],16),binary_sha256:r.sha256,asm_sha256:hash(text)});
}}
for(const f of ['pdb_symbols.py','native-functions.mjs','archive.mjs'])fs.copyFileSync('.rush/native-owner/'+f,out+'/'+f);
const csv=['phase,wrapper,rva,bytes,pushes,stack_sub,binary_sha256,asm_sha256',...rows.map(r=>Object.values(r).join(','))].join('\n')+'\n';fs.writeFileSync(out+'/summary.csv',csv);
const doc=['# Native owner-path: статическая проверка Windows-MSVC codegen','',
'Это чтение уже построенных PE/PDB, не новый benchmark и не native speedup gate. B ниже — прежний непринятый own-free-context, а не база нового кандидата. SHA сверены с original native_binaries.json до disassembly. Runtime source base: f80c681b799ef4ef7fa08d79c06896fdf1104fe9; features production; toolchain — original comparison_identity.json (Windows MSVC).','',
'## Наблюдения','',
'В проверенных alloc/free wrappers TLS разворачивается в загрузку module TLS index, gs:0x58, TLS module base и LOCAL по offset 0x38; затем sentinel compare. Между этими загрузками и own-heap branch нет вызова lazy-init helper. Это native TLS lowering, не доказательство нулевой latency TLS и не основание менять registry binding.','',
'Const alloc hit не вызывает другие функции, но его общий пролог всё равно сохраняет rsi и выделяет shadow space. В той же unwind-функции находятся non-tail first-bind и refill вызовы и обработка null/process-exit. Вывод: предмет следующей гипотезы — граница завершённого hit и call-bearing continuations, не новые inline/cold перестановки прежнего free-кандидата. Статический frame сам по себе ещё не доказывает native выигрыш.','',
'| Phase | Wrapper | Bytes | Pushes в прологе | sub rsp, bytes |','|---|---|---:|---:|---:|',...rows.map(r=>`| ${r.phase} | ${r.wrapper} | ${r.bytes} | ${r.pushes} | ${r.stack_sub} |`),'',
'B действительно уменьшает число сохраняемых регистров native free, но не меняет alloc-пролог; это совместимо с прежним Ir-выигрышем, не объясняет причинно canonical ухудшения и не пересматривает NO-GO FOR PROMOTION.','',
'Границы функций берутся из PE exception/unwind directory; публичный PDB не содержит имён локальных generic wrappers. Разметка wrappers выполнена по порядку функций перед own_free_wall::main и проверке layout/class-specific count/state операций в assembly. Исходные адреса и hashes — summary.csv. Все шесть отобранных функций каждой стороны сохранены целиком.',''];
fs.writeFileSync('docs/perf/NATIVE_OWNER_CODEGEN_READ.md',doc.join('\n'));
const files=['docs/perf/OPEN_ITEMS.md','docs/CORRECTNESS_OPEN_ITEMS.md',...fs.readdirSync('docs/correctness-open-items').filter(n=>n==='ACTIVE.md'||/^TRACKED_.*\.md$/.test(n)).map(n=>'docs/correctness-open-items/'+n)];
const disposition=[];
for(const file of files){const b=fs.readFileSync(file);const text=b.toString('utf8');const ids=[...text.matchAll(/^(\d+[a-z]?)\.\s+\*\*/gm)].map(m=>m[1]);disposition.push({file,sha256:hash(b),lines:text.split('\n').length,records:ids});}
fs.writeFileSync(out+'/round_start.json',JSON.stringify(disposition,null,2));
const lines=['# Native owner round — round-start disposition','',
'Дата: 2026-10-25. Оба индекса, ACTIVE и все девять thematic correctness tier files прочитаны целиком. Snapshot hashes/records — docs/perf/native_owner_codegen/round_start.json. Архивы не включаются в обязательный round-start read. Это disposition существующих карточек, не закрытие их correctness или perf acceptance.','',
'| Index/file | Item | Disposition |','|---|---|---|'];
for(const f of disposition.filter(f=>f.records.length))for(const id of f.records){let verdict='LEAVE: вне native alloc-path исследования; прежние status/trigger сохраняются.';
 if(f.file==='docs/perf/OPEN_ITEMS.md'&&id==='39')verdict='Проверить optional Windows TLS lowering по существующим binaries; отрицательные выводы про registry/NUMA/alignment не переоткрывать.';
 if(f.file==='docs/perf/OPEN_ITEMS.md'&&['102','67'].includes(id))verdict='Учесть Windows frame/codegen как вход исследования; старые NO-GO не считать базой нового кандидата.';
 if(f.file==='docs/perf/OPEN_ITEMS.md'&&['19','18','44','94'].includes(id))verdict='LEAVE: не повторять LUT/memo/cap/attribute tuning; текущая masked assembly не доказывает историческое отсутствие bounds check.';
 if(f.file.endsWith('TRACKED_correctness_residuals.md')&&id==='164')verdict='LEAVE: accepted known Box defect остаётся; не писать в payload текущего dealloc frame и не заявлять Miri-clean.';
 if(f.file.endsWith('TRACKED_platform_contracts.md')&&id==='185')verdict='LEAVE: сохранить null/process-exit escape и TORN semantics при любом новом alloc entry.';
 if(f.file.endsWith('TRACKED_platform_contracts.md')&&id==='196')verdict='LEAVE: native TLS assembly одного Windows binary не закрывает supported-target first-access/runtime oracle.';
 if(f.file.endsWith('TRACKED_hook_safety.md')&&id==='226')verdict='LEAVE: timing production-only, diagnostic activation отдельно; internals-форма не заменяет production синхронизацию.';
 lines.push(`| ${f.file} | ${id} | ${verdict} |`);
}
fs.writeFileSync('docs/design/2026-10-25-native-owner-round-start.md',lines.join('\n')+'\n');
console.log('Static codegen archive and per-record round-start disposition written.');
