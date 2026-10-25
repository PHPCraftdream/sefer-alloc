import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import assert from 'node:assert/strict';
import {gunzipSync} from 'node:zlib';
import {spawnSync} from 'node:child_process';
const inv=JSON.parse(fs.readFileSync('docs/research/inventory.json'));
const checks=JSON.parse(fs.readFileSync('.rush/docs-commit/generator-checks.json'));assert.equal(checks.length,7);assert(checks.every(c=>c.status===0));
fs.writeFileSync('docs/research/validation.json',JSON.stringify({kind:'cargo-free archive/report checks; no new measurements',generator_checks:checks},null,2)+'\n');
fs.mkdirSync('docs/research/reproduction',{recursive:true});
for(const file of ['docs-commit-inventory.mjs','docs-commit-scan.mjs','docs-commit-archive.mjs','docs-commit-prepare.mjs','docs-commit-checks.mjs','docs-commit-manifest.mjs'])fs.copyFileSync(`.rush/${file}`,`docs/research/reproduction/${file}`);
const p=spawnSync('git',['rev-parse','HEAD'],{encoding:'utf8'});assert.equal(p.status,0);const parent=p.stdout.trim();
const logCommand=`git log --format='%H %s' ${parent}..HEAD -- docs`;
let rawCount=0,rawStored=0,rawLogical=0;
for(const tree of inv.trees){
 const location=`docs/research/${tree.slug}/${tree.manifest}`,bytes=fs.readFileSync(location);
 const manifest=JSON.parse(tree.manifest.endsWith('.gz')?gunzipSync(bytes).toString():bytes.toString());
 for(const f of manifest.files)if(!f.deleted&&/\.(?:log(?:\.gz)?|ndjson|callgrind(?:\.log)?(?:\.gz)?|annotate)$/.test(f.source)){rawCount++;rawStored+=f.archived_bytes;rawLogical+=f.uncompressed_bytes;}
}
const manifest={delivery:'owner-path research documentation archive 2026-10-25',parent_before_documentation_commit:parent,commit_range_command:logCommand,expected_taxonomy:'bench(docs)',production_default_features:'unchanged',shipping_source_changes:0,new_features:[],runtime_performance_commits:0,archive_payload_files:inv.total_files,archive_payload_stored_bytes:inv.total_stored_bytes,archive_payload_uncompressed_bytes:inv.total_uncompressed_bytes,raw_log_selection:'suffix .log[.gz], .ndjson, .callgrind[.log][.gz], .annotate',raw_log_files:rawCount,raw_log_stored_bytes:rawStored,raw_log_uncompressed_bytes:rawLogical,normalized_files:inv.normalized_files,verdicts:{scalar_issue:'INCONCLUSIVE / DO NOT PROMOTE',overflow_outline:'NO-GO',half_flush_split:'NO-GO',single_flush:'instruction gain; no shipping acceptance',validated_context_B:'NO-GO FOR PROMOTION',MIR_C:'not selected',native_leaf:'INCONCLUSIVE / DO NOT PROMOTE',native_mixed:'JUDGE FAIL / PRODUCT CANDIDATE NOT OPENED'},verification:'archive hashes/JSON/size/path checks and seven report-generator --check passes; no build/test/benchmark/doc execution'};
fs.mkdirSync('docs/perf/round-manifests',{recursive:true});
fs.writeFileSync('docs/perf/round-manifests/R_NATIVE_OWNER_2026_10_25_MANIFEST.json',JSON.stringify(manifest,null,2)+'\n');
fs.writeFileSync('docs/perf/round-manifests/R_NATIVE_OWNER_2026_10_25_MANIFEST.md',`# Исследовательские раунды owner-path — документная поставка

Дата: 2026-10-25. Категория поставки: **bench(docs)**. Runtime improvements этой поставки: **0**; shipping source и production feature composition не меняются. Это сохранение измерений и выводов, не promotion кандидата.

## Коммиты

Список коммитов поставки выводится из Git, не переписывается вручную:

${'```sh'}
${logCommand}
${'```'}

Manifest создан до этого docs-only commit. Его будущий SHA намеренно не подменён parent SHA. После commit команда возвращает настоящий landing commit этой локальной поставки; parent_before_documentation_commit в companion JSON — только нижняя граница диапазона, не SHA нового отчёта. Push не входит в запрос.

## Feature/source impact

- Production composition: unchanged.
- Новые features: нет.
- Shipping source changes: 0.
- perf(runtime)/perf(opt-in) commits этой поставки: нет.
- Новые wall-clock/Ir/RSS measurements при подготовке commit: нет. Исторические deltas и ограничения остаются в [архивных gate reports](../../research/README.md), а не выдаются за ускорение этой поставки.

## Объём evidence

- Payload: ${inv.total_files} файлов, ${inv.total_stored_bytes} stored bytes, ${inv.total_uncompressed_bytes} uncompressed bytes.
- Raw-log selection: суффиксы .log[.gz], .ndjson, .callgrind[.log][.gz], .annotate; ${rawCount} файлов, ${rawStored} stored bytes, ${rawLogical} uncompressed bytes.
- Private source-path prefixes нормализованы в ${inv.normalized_files} копиях; original/archive hashes сохранены. Metadata и индекс архива считаются отдельно от payload.

Числа получены этим generator из inventory/manifests; companion JSON сохраняет definitions и exact git command.

## Итоги

| Исследование | Verdict |
|---|---|
${Object.entries(manifest.verdicts).map(([k,v])=>`| ${k} | ${v} |`).join('\n')}

Проверки: [validation.json](../../research/validation.json), архивный verifier и семь report-generator --check. Local tests, builds, benchmark reruns и doc-build не выполнялись; correctness/CI gaps остаются. Nested experimental trees сохранены и не сливались как runtime code.
`);
console.log(JSON.stringify({parent,raw_log_files:rawCount,raw_log_stored_bytes:rawStored,archive_payload_files:inv.total_files},null,2));
