# B4 one-class probes (PORT/PORT2 round)

Рабочие версии driver-скриптов и probe-исходников, которыми сняты raw-логи
`docs/perf/_raw_b4_oneclass_{port,port2}-*` и написаны
`docs/perf/B4_ONECLASS_PORT_summary.csv` / `B4_ONECLASS_PORT2_summary.csv`
(генераторы: `scripts/b4_oneclass_port_table.mjs`; исторический генератор §1–§8 —
`scripts/b4_oneclass_table.mjs`).

## Driver-скрипты port2-комплекта (воспроизведение)

Рабочие версии driver-скриптов, снявших port2-логи, — в этом каталоге:

- `oc_mem_hooks_a3.patch` — instrumentation-патч плеча A port2
  (sha256 `1ff7e14000bb078836d7e455732322b857192bcf69439d2e95cbb03c97c9a1ce`,
  байт-в-байт оригинал из scratch-комплекта).
- `oc_rem2_perf.sh` — драйвер callgrind-ячейки судьи hot-путей: пишет шапку
  raw-лога (`identity:` / `features:` / `cmd:`), снимает маргинальные Ir и
  дописывает RC в хвост лога.
- `oc_rem2_memrun.sh` — драйвер memory-судьи (`zz_oc_mem_probe`, режимы
  full/sparse/cold/deadpins): пишет шапку identity/features/cmd, строку
  `MEM …` и RC; прокидывает probe-sha256.
- `oc_rem2_tar.sh` — tar-снимок измеряемого дерева (write-tree/identity
  свидетельство для плеча запуска).
- `prof_win.sh` — Windows-обёртка запуска WSL-профилировщика из хост-сессии
  (проброс `REPO`/`REPO_WSL`/`SCRATCH`).

Примечание о probe-sha256 в mem-raw шапках: `probe-sha256=9daf6992…` в
`_raw_b4_oneclass_port2-{a,b}-{full,sparse,cold,deadpins}.log` посчитан по
CRLF-копии пробы на момент замера; LF-блоб `oc_mem_probe.rs` в коммите даёт
sha256 `493d573e…`. Сами raw-логи не редактируются — оба хэша считаются
корректным свидетельством одного блоба с разной нормализацией концов строк.

## Что чем мерить (судьи)

| Ячейка | Скрипт | Probe | Судья |
|---|---|---|---|
| mstress, larson | `prof_f.sh <wt> <label> [larson\|mstress] [sefer] [FEAT]` | `larson_probe.rs` | MARGINAL Ir/step, callgrind 50k→100k |
| ao | `prof_ao.sh <label> [sefer]` | `alloc_only_probe.rs` | alloc/free Ir/op + EstCycles |
| aoc (rt/c16/c64) | `prof_ao_c.sh <label> <alloc> <mode>` | `alloc_only_const_probe.rs` | alloc/free Ir/op (+cache) |
| batch | `prof_batch.sh <label> <alloc>` | `batch_probe.rs` | alloc/free Ir/блок (256/round) |
| batchc | `prof_batch_c.sh <label> <alloc>` | `batch_only_const_probe.rs` | alloc/free Ir/блок (const 16B) |
| batchl | `prof_batchl.sh <label> <alloc>` | `batch_large_probe.rs` | alloc/free Ir/блок (258/round) |
| foreign | `mt_prof_foreign_free.sh <wt> <label> foreign locality` | `mt_foreign_free_probe.rs` | Ir/free + oracle активации |
| thrash64 | `prof_thrash64.sh <label> [sefer]` | `thrash64_probe.rs` | alloc/free Ir/op + EstCycles (гейт ≤ +1 %) |

## Memory-режимы (судья памяти)

`oc_mem_probe.rs` (example `zz_oc_mem_probe`) — детерминированный «честный»
судья памяти, режимы `full|sparse|cold|deadpins`; строка
`MEM committed_bytes=… config_conflicts_total=…` в raw-логе — измерение.
`oc6_foreign2.rs` — изолированная проба foreign-free (§4). Сборка memory-пробы:
`cargo build --release --example zz_oc_mem_probe --features production,internals,bench-internals`
с изолированным `CARGO_TARGET_DIR`.

## Feature set каждой ячейки (записан в шапке raw-лога строкой `features:`)

- судьи hot-путей (mstress, larson, ao, aoc, batch, batchc, batchl, foreign, thrash64): `production`;
- memory-режимы (full/sparse/cold/deadpins): `production,internals,bench-internals`.

## Требования к raw-логу

Каждый raw-лог обязан иметь шапку: строка `tree-diff-sha256 …` (судьи) и/или
`identity: shoulder=(a|b) … write-tree=… source-manifest-sha256=…` (обязательно
для обоих плеч), строка `features: …`, строка `cmd: bash ${REPO_WSL}/.rush/wsl/<script> …`.
Без identity-строки raw не принимается генератором (RC=1).

## Правила запуска

- Переменные: `REPO` (корень репо), `REPO_WSL` (тот же корень под `/mnt/<drive>/…`
  в WSL), `SCRATCH` (каталог логов вне репо), `AB_DIR` (scratch сборок, по
  умолчанию `$HOME/ab`).
- cargo — всегда с явным изолированным `CARGO_TARGET_DIR` (вне репо, например
  `$AB_DIR/tgt-<label>`) и под явным `timeout 600`.
- Плечо A — чистый checkout измеряемого base-коммита + instrumentation-патч
  (`oc_mem_hooks_a3.patch`, sha256 `1ff7e140…` для port2); плечо B —
  объединённое дерево. Worktree A после замера удаляется.
