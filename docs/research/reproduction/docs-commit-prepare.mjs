import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import assert from 'node:assert/strict';
const input=JSON.parse(fs.readFileSync('.rush/docs-commit/inventory.json'));
const inventory=JSON.parse(fs.readFileSync('docs/research/inventory.json'));
const supplements=[];
const source='worktrees/scalar-issue-prepared/scripts/scalar_issue_report.mjs',archive='scalar-issue-prepared/reproduction/scalar_issue_report.mjs';
fs.mkdirSync(path.dirname('docs/research/'+archive),{recursive:true});assert(!fs.existsSync('docs/research/'+archive));
const bytes=fs.readFileSync(source);fs.writeFileSync('docs/research/'+archive,bytes);supplements.push({source,archive,restore_to:'scripts/scalar_issue_report.mjs',sha256:crypto.createHash('sha256').update(bytes).digest('hex')});
fs.writeFileSync('docs/research/supplements.json',JSON.stringify(supplements,null,2)+'\n');
const slugs=new Set(inventory.trees.map(t=>t.slug)),changes=[];
for(const item of input.find(r=>r.repo==='.').files.filter(f=>f.file.endsWith('.md')&&!f.file.startsWith('docs/checkpoints/'))){
 const old=fs.readFileSync(item.file,'utf8');
 let text=old.replace(/worktrees\/([^/`\s)]+)\/docs\//g,(all,slug)=>slugs.has(slug)?`docs/research/${slug}/`:all);
 text=text.replace(/\]\(\.\.\/\.\.\/docs\/research\//g,'](../research/');
 if(text!==old){fs.writeFileSync(item.file,text);changes.push(item.file);}
}
fs.writeFileSync('.rush/docs-commit/primary-link-edits.json',JSON.stringify(changes,null,2));
const verdicts={
 'scalar-issue-prepared':'INCONCLUSIVE / DO NOT PROMOTE; correctness execution не подтверждена',
 'own-free-cost':'NO-GO: overflow control',
 'own-free-split':'NO-GO: free micro-gate',
 'own-free-single':'Сокращение Ir; shipping acceptance отсутствует',
 'own-free-context':'NO-GO FOR PROMOTION; native speedup не установлен',
 'own-free-inline':'Не доминирует B; не выбран',
 'native-alloc-leaf':'INCONCLUSIVE / DO NOT PROMOTE',
 'native-mixed-judge':'JUDGE FAIL / PRODUCT CANDIDATE NOT OPENED'
};
const reports={
 'scalar-issue-prepared':'SCALAR_ISSUE_PREPARED_GATE.md',
 'own-free-cost':'OWN_FREE_OVERFLOW_OUTLINE_GATE.md',
 'own-free-split':'OWN_FREE_HALF_FLUSH_SPLIT_GATE.md',
 'own-free-single':'OWN_FREE_SINGLE_FLUSH_GATE.md',
 'own-free-context':'OWNER_FREE_RESEARCH_GATE.md',
 'native-alloc-leaf':'NATIVE_ALLOC_LEAF_NATIVE.md',
 'native-mixed-judge':'NATIVE_MIXED_CALIBRATION.md'
};
const table=inventory.trees.map(t=>`| [${t.slug}](${t.slug}/${reports[t.slug]?'perf/'+reports[t.slug]:'design/2026-10-25-scalar-mir-prereg.md'}) | ${verdicts[t.slug]} | ${t.files} | ${t.stored_bytes} |`).join('\n');
fs.writeFileSync('docs/research/README.md',`# Архив исследований owner-path — 2026-10-24/25

Это архив документации и воспроизводимости, **не принятие экспериментального allocator-кода**. Shipping source, Cargo feature composition, версии, examples и tests основной ветки этим коммитом не меняются. Файлы исходного кода и patches внутри архива — измерительные snapshots, не подключённые к сборке проекта.

## Исследования

| Snapshot | Итог | Архивируемых документов и артефактов | Stored bytes |
|---|---|---:|---:|
${table}

Общий размер payload: ${inventory.total_stored_bytes} stored bytes, ${inventory.total_uncompressed_bytes} uncompressed bytes; ${inventory.total_files} файлов из восьми исследовательских trees. Метаданные manifests, этот индекс и verifier считаются отдельно. Все количества выведены из [inventory.json](inventory.json), не оценены вручную.

## Что сохранено

- Все изменённые, новые и ignored файлы внутри docs/ исследовательских trees, включая пререгистрации, отчёты, companion CSV, raw samples, failed attempts, source identities, patches, asm и reproduction scripts.
- Изменённые ARCHITECTURE/OPEN_ITEMS snapshots вложенных trees — историческое состояние эксперимента. Они **не заменяют** действующие [основной perf index](../perf/OPEN_ITEMS.md) и [correctness index](../CORRECTNESS_OPEN_ITEMS.md).
- Для каждого tree: pinned base SHA и ${'`manifest.json`'} либо ${'`manifest.json.gz`'}, связывающий исходный docs/path с архивной копией. Отдельные зависимости воспроизводимости — [supplements.json](supplements.json).
- Основная [общая картина](../design/2026-10-25-owner-path-global-picture.md) и checkpoints остаются вне исторических snapshots. Checkpoints не переписывались: их ссылки отражают состояние на момент записи; этот архив появился позже.

## Нормализация и storage policy

В ${inventory.normalized_files} архивных копиях удалены machine-specific workspace/Cargo-home prefixes. Это исправление архивирования, не изменение измеренных чисел; оригинальные файлы в ignored trees не менялись. Manifests содержат original SHA-256, original logical SHA-256, archive SHA-256 и archive logical SHA-256; normalized copies не выдаются за byte-identical originals. Старые внутренние raw-hash manifests остаются историческими и могут ссылаться на original hash: для проверки нынешней копии использовать внешний archive manifest.

Raw/JSON artifacts от200KiB дополнительно gzip-сжаты; extra_gzip отмечен в manifest. Уже сжатые snapshots сохраняются либо перепакованы только после нормализации private prefixes. Ни один raw artifact не превышает2MiB uncompressed; длинные Markdown indexes — документы, не raw logs. Архив не содержит построенных executables или target/ caches.

## Проверка без сборок и без benchmark execution

${'```sh'}
node docs/research/verify.mjs
${'```'}

Verifier проверяет каждый archived hash, decompression, JSON/NDJSON, размеры, приватные home prefixes и inventory totals. Это проверка сохранности архива, **не повтор correctness/performance acceptance**. Предыдущие checks генераторов выполнялись в исходных trees до архивирования; повторные build/test/bench сейчас не выполнялись.

## Восстановление контекста отчёта

Base всех snapshots: f80c681b799ef4ef7fa08d79c06896fdf1104fe9. Это pinned принятая база исследования, не заявление о текущем remote main.

Архивная директория docs/research/<slug>/ соответствует исходной worktrees/<slug>/docs/. Для отдельного воспроизведения сначала создать НОВЫЙ изолированный worktree от base в разрешённом workspace, затем по manifest восстановить docs/<source path> и распаковать extra_gzip entries до исходного имени. Для supplements использовать restore_to. Не копировать архив поверх текущих dirty trees.

Исторические scripts и source identities сохранены с исходным namespace worktrees/<slug> и .rush/. Они не являются универсальными portable commands из текущего положения архива: сначала восстановить layout, описанный конкретным report/reproduction manifest. Для восстановления измеренного product source использовать записанный source patch и archived inputs; такой шаг не входит в docs-only commit и не означает разрешения выполнять benchmark или тесты. Абсолютные машинные пути из старого окружения намеренно не восстанавливаются.

Runtime изменений и принятых speedup claims в этой документной поставке: **0**. Итоги NO-GO/INCONCLUSIVE/FAIL и пробелы tests/CI сохранены.
`);
console.log(JSON.stringify({archive_payload_files:inventory.total_files,archive_payload_bytes:inventory.total_stored_bytes,primary_doc_links_updated:changes},null,2));
