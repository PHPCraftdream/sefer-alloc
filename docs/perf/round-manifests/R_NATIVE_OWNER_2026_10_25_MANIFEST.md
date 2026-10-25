# Исследовательские раунды owner-path — документная поставка

Дата: 2026-10-25. Категория поставки: **bench(docs)**. Runtime improvements этой поставки: **0**; shipping source и production feature composition не меняются. Это сохранение измерений и выводов, не promotion кандидата.

## Коммиты

Список коммитов поставки выводится из Git, не переписывается вручную:

```sh
git log --format='%H %s' 9f4a4fe41016e465700dc42a0d049d865731f8b0..HEAD -- docs
```

Manifest создан до этого docs-only commit. Его будущий SHA намеренно не подменён parent SHA. После commit команда возвращает настоящий landing commit этой локальной поставки; parent_before_documentation_commit в companion JSON — только нижняя граница диапазона, не SHA нового отчёта. Push не входит в запрос.

## Feature/source impact

- Production composition: unchanged.
- Новые features: нет.
- Shipping source changes: 0.
- perf(runtime)/perf(opt-in) commits этой поставки: нет.
- Новые wall-clock/Ir/RSS measurements при подготовке commit: нет. Исторические deltas и ограничения остаются в [архивных gate reports](../../research/README.md), а не выдаются за ускорение этой поставки.

## Объём evidence

- Payload: 6505 файлов, 30023403 stored bytes, 32624474 uncompressed bytes.
- Raw-log selection: суффиксы .log[.gz], .ndjson, .callgrind[.log][.gz], .annotate; 1039 файлов, 22982464 stored bytes, 22982464 uncompressed bytes.
- Private source-path prefixes нормализованы в 424 копиях; original/archive hashes сохранены. Metadata и индекс архива считаются отдельно от payload.

Числа получены этим generator из inventory/manifests; companion JSON сохраняет definitions и exact git command.

## Итоги

| Исследование | Verdict |
|---|---|
| scalar_issue | INCONCLUSIVE / DO NOT PROMOTE |
| overflow_outline | NO-GO |
| half_flush_split | NO-GO |
| single_flush | instruction gain; no shipping acceptance |
| validated_context_B | NO-GO FOR PROMOTION |
| MIR_C | not selected |
| native_leaf | INCONCLUSIVE / DO NOT PROMOTE |
| native_mixed | JUDGE FAIL / PRODUCT CANDIDATE NOT OPENED |

Проверки: [validation.json](../../research/validation.json), архивный verifier и семь report-generator --check. Local tests, builds, benchmark reruns и doc-build не выполнялись; correctness/CI gaps остаются. Nested experimental trees сохранены и не сливались как runtime code.
