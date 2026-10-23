# D1′ / PR96 — manifest эксперимента

Это отдельный эксперимент, не новый общий раунд проекта. Срез commit history включает delivery commit. Последующая синхронизация этого manifest и CI receipts исключены из фиксированного верхнего endpoint; это документационные дополнения, не новые measurements.

Команда: git log --reverse --format="%H|%s" f80c681b799ef4ef7fa08d79c06896fdf1104fe9..c52b6dc6b1a6b7c3ba8820e852216787c60f2b65

| Commit | Категория | Subject |
| --- | --- | --- |
| 6358cad7d6cae03e11a688813607417feef4fd09 | feat(sidecar) | feat(sidecar): prototype owner-initialized chunked ingress |
| 76bbf7e874b0cb96f8577a33f1fdceba8298ed10 | test(sidecar) | test(sidecar): remove obsolete class-pass duplication |
| d031f03cdc14efdcc501840868491de7d8a9891e | test(sidecar) | test(sidecar): align chunked seam inventory and evidence |
| 734395b7bb12bdd21f605c07de5426b2e4e717ef | docs(evidence) | docs(evidence): record chunked safety receipts |
| c52b6dc6b1a6b7c3ba8820e852216787c60f2b65 | bench(sidecar) | bench(sidecar): record D1-prime comparative NO-GO evidence |

## Изменения и verdict

- production feature composition: без изменений. Алгоритм storage отличается в экспериментальной ветке, но не принят и не слит в main.
- Verdict item112: NO-GO / NOT ACCEPTED; никаких shipping speedups.
- Эта доставка: bench (измерения, исправление benchmark harness, тест и evidence), product src не меняется.
- Числа и ограничения: ../D1_PRIME_CHUNKED_SIDECAR_GATE.md; абсолютные значения, raw/marginal и wall/process axes раздельны.
- Raw artifacts текущей доставки: 193 файлов, 1250818 bytes; SHA-256 и размеры: ../d1_prime_chunked_sidecar/summaries/artifact_manifest.json. Счёт включает выбранные Criterion JSON, а не только имена _raw_*.log.
- Новая проверка CI для delivery commit: PENDING; предыдущий safety receipt не подменяет её.
