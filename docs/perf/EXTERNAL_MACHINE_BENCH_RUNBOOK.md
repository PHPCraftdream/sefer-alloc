# Прогон бенчмарков на внешней машине — инструкция

Это инструкция для разового прогона на свободной машине, которая не принадлежит проекту. Цель — получить нативные wall-clock цифры, которых нет из GitHub CI. Раннеры CI — общие виртуальные машины: шум там слишком велик для сравнения аллокаторов, а iai-гейты меряют инструкции, а не время.

Результат прогона — архив с сырыми логами и описанием машины. Его присылают обратно владельцу. В репозиторий на внешней машине ничего не коммитится и не пушится.

## 0. Что именно меряем и зачем

| # | Что | Зачем | Приоритет |
|---|---|---|---|
| A1 | `npm run bench:table` — SeferAlloc vs mimalloc vs System, один процесс | канонические таблицы README (ns/op, 16/64/256/1024 B, burst / churn / churn+write / teardown) | обязательно |
| A2 | `scripts/r34_7_causal_harness.mjs` — три отдельных бинаря, у каждого свой `#[global_allocator]`, свежий процесс на каждый повтор, ротация порядка | причинное сравнение без дрейфа хоста (A1 его не исключает) | обязательно |
| A3 | `scripts/paired-ab-runner.mjs` — process-level A/B/B/A, 20 пар, paired t-test + sign test, RSS/commit | статистически значимое сравнение sefer/mimalloc/system + память | обязательно |
| A4 | `examples/malloc_macro` (larson/mstress на `malloc-bench-rs`), развёртка по потокам | многопоточная пропускная способность с cross-thread free | обязательно |
| A5 | `large_realloc` | Large-блоки и realloc vs mimalloc | желательно |
| B1 | `heap_lifecycle_teardown`, `npm run first-alloc`, `npm run dealloc-only` | цена первой аллокации, жизненного цикла потока и free из непривязанного потока. Нативная задержка выбора слота ReuseFirst (R20 PRF-04) на CI не мерилась | желательно |
| B2 | `heap_fanin_production`, `heap_fanin_persistent`, `heap_async_pattern`, `macro_multiseg_steady_state` | cross-thread free и многосегментный steady state: базовая точка до правок раунда 6 (R6-T10/T12/T18/T26) | желательно |
| C1 | `npm run iai` (только Linux + Valgrind) | детерминированный Ir на «чистом» железе, сверка с CI-цифрами `docs/perf/IAI_BASELINE.md` | если есть время |
| C2 | `examples/rss_probe` | RSS под асимметричным cross-thread free | если есть время |

Порядок выполнения — сверху вниз. Если время ограничено, хватит A1–A4.

## 1. Требования к машине

- **ОС:** Linux x86_64 предпочтительно (нужен для C1). Windows 10/11 x86_64 тоже подходит для A и B. Укажите, что именно было.
- **Отдельное железо:** не виртуальная машина. Если это VM — обязательно укажите это в `machine.txt` (см. §3).
- **Простой:** во время прогона машина больше ничем не занята. Закрыть браузер, IDE, индексаторы, обновления, облачную синхронизацию. Не трогать машину до конца прогона.
- **Питание и частота:** ноутбук — от сети, план питания «высокая производительность».
  - Linux: `sudo cpupower frequency-set -g performance`, если доступно.
  - Turbo/boost не отключать, но указать, включён ли он.
- **Ресурсы:** ≥ 8 GiB RAM, ≥ 20 GiB свободного диска (сборки нескольких профилей).
- **Время:** A1–A4 занимают примерно 1–2 часа, включая сборку. Всё вместе — 3–4 часа.

## 2. Подготовка

```text
# Инструменты
rustup toolchain install stable        # стабильный Rust, без rust-toolchain.toml
rustup default stable
node --version                         # нужен Node 22+
git --version
# Только для C1 (Linux):
sudo apt-get install -y valgrind
cargo install iai-callgrind-runner --version "^0.14" --locked

# Исходники — строго по зафиксированному коммиту
git clone https://github.com/PHPCraftdream/sefer-alloc.git
cd sefer-alloc
git checkout 3dbfb3013aede489170f40ce3951a0e9a4787007
git status --short                     # должно быть пусто
```

Коммит `3dbfb301` — текущий `main` до правок раунда 6. Этот прогон служит базовой точкой. Другой коммит берите только по указанию владельца и тогда обязательно запишите его SHA.

`npm install` не нужен: скрипты используют только стандартный Node.

## 3. Паспорт машины (сделать до прогонов)

Создайте каталог для результатов вне дерева исходников:

```text
mkdir -p ../bench-results
```

Linux:

```text
{ date -u; uname -a; lscpu; free -g; cat /proc/cpuinfo | grep -m1 "model name";
  cat /sys/devices/system/cpu/cpu0/cpufreq/scaling_governor 2>/dev/null;
  cat /sys/devices/system/cpu/intel_pstate/no_turbo 2>/dev/null;
  systemd-detect-virt 2>/dev/null; rustc -Vv; cargo -V; node --version;
  git rev-parse HEAD; git status --short; } > ../bench-results/machine.txt 2>&1
```

Windows (PowerShell):

```text
& { Get-Date -AsUTC; Get-ComputerInfo | Select-Object OsName,OsVersion,CsProcessors,CsTotalPhysicalMemory,CsModel;
    Get-CimInstance Win32_Processor | Select-Object Name,NumberOfCores,NumberOfLogicalProcessors,MaxClockSpeed;
    powercfg /getactivescheme; rustc -Vv; cargo -V; node --version;
    git rev-parse HEAD; git status --short } *> ..\bench-results\machine.txt
```

Добавьте в `machine.txt` вручную одну строку: VM или железо, ноутбук или десктоп, что ещё работало во время прогона.

## 4. Прогоны

Каждую команду запускайте из корня репозитория, вывод — целиком в файл. Не прерывайте команду и не запускайте две одновременно.

### A1. Канонические таблицы

```text
npm run bench:table > ../bench-results/A1_bench_table.log 2>&1
```

### A2. Причинное сравнение (свежий процесс на повтор)

```text
node scripts/r34_7_causal_harness.mjs --reps 15 > ../bench-results/A2_r34_7_causal.log 2>&1
```

Если скрипт отвергнет `--reps 15`, запустите без аргументов и сохраните вывод с флагом `--help` рядом.

### A3. Paired A/B/B/A (20 пар, статистика + RSS/commit)

```text
node scripts/paired-ab-runner.mjs --verify-only > ../bench-results/A3_paired_ab_verify.log 2>&1
node scripts/paired-ab-runner.mjs --pairs 20     > ../bench-results/A3_paired_ab.log 2>&1
```

Раннер сам пишет JSON с сырыми сэмплами в `docs/perf/paired_ab_runs/`. После прогона скопируйте эти JSON-файлы в `../bench-results/A3_json/`.

### A4. Многопоточная пропускная способность (malloc-bench-rs)

`examples/malloc_macro.rs` — SeferAlloc vs mimalloc vs System на larson/mstress, развёртка по потокам, с cross-thread free и смешанными размерами:

```text
cargo run --release --example malloc_macro --features "alloc-global alloc-xthread"         > ../bench-results/A4_malloc_macro.log 2>&1
cargo run --release --example malloc_macro --features "alloc-global alloc-xthread pinning" > ../bench-results/A4_malloc_macro_pinned.log 2>&1
```

Второй прогон закрепляет поток *i* за ядром *i*. Если ОС откажет в закреплении, прогон всё равно идёт, но это нужно указать в логе.

Укажите число физических и логических ядер: развёртка по потокам интерпретируется относительно них.

### A5. Large и realloc

```text
cargo bench --bench large_realloc --features "production internals bench-internals" > ../bench-results/A5_large_realloc.log 2>&1
```

### B1. Первая аллокация, жизненный цикл потока, free из непривязанного потока

```text
npm run first-alloc  > ../bench-results/B1_first_alloc.log 2>&1
npm run dealloc-only > ../bench-results/B1_dealloc_only.log 2>&1
cargo bench --bench heap_lifecycle_teardown --features "production internals bench-internals" > ../bench-results/B1_heap_lifecycle_teardown.log 2>&1
```

### B2. Cross-thread free и steady state

```text
cargo bench --bench heap_fanin_production       --features "production internals bench-internals" > ../bench-results/B2_heap_fanin_production.log 2>&1
cargo bench --bench heap_fanin_persistent       --features "production internals bench-internals" > ../bench-results/B2_heap_fanin_persistent.log 2>&1
cargo bench --bench heap_async_pattern          --features "production internals bench-internals" > ../bench-results/B2_heap_async_pattern.log 2>&1
cargo bench --bench macro_multiseg_steady_state --features "production internals bench-internals" > ../bench-results/B2_macro_multiseg.log 2>&1
```

Если какая-то цель не собирается с этим набором фич, сохраните ошибку в тот же лог и переходите к следующей. Не подбирайте фичи наугад.

### C1. iai (только Linux + Valgrind)

```text
npm run iai > ../bench-results/C1_iai.log 2>&1
```

### C2. RSS

```text
cargo run --release --example rss_probe --features "alloc-global alloc-xthread" > ../bench-results/C2_rss_probe.log 2>&1
```

## 5. Повтор для оценки шума

Если время осталось, повторите A1 и A4 ещё раз, в конце, в файлы `A1_bench_table_run2.log` и `A4_malloc_macro_run2.log`. Расхождение двух прогонов на одной машине — оценка шума. Без неё сравнение аллокаторов в пределах ±5 % не интерпретируется.

## 6. Что прислать

```text
cd ..
tar czf bench-results-<имя-машины>-<YYYYMMDD>.tar.gz bench-results
```

На Windows — zip-архив каталога `bench-results`.

В архиве должно быть:
- `machine.txt`;
- все логи `A*`, `B*`, `C*` (даже неудачные);
- каталог `A3_json/`.

В логах не должно быть паролей и токенов. Пути домашнего каталога в логах допустимы: владелец вырежет их перед публикацией.

## 7. Чего НЕ делать

- Не менять код, `Cargo.toml` и фичи. Не обновлять зависимости (`cargo update`).
- Не коммитить и не пушить в репозиторий.
- Не запускать ничего тяжёлого параллельно с прогоном.
- Не обрезать логи. Обрезку по правилам `CLAUDE.md` (R14-10) сделает владелец.

## 8. Как владелец использует результаты

- Логи A1–A5 и B1–B2 становятся базовой точкой «до раунда 6». После слияния perf-юнитов раунда 6 тот же набор прогоняется на той же машине с новым SHA. Регресс или выигрыш признаётся только при одинаковом коммите-основе, той же машине и шуме меньше эффекта (§5).
- Цифры в README обновляются по правилам `CLAUDE.md`: raw-логи + summary CSV + неизменяемая идентичность исходников (SHA). Лог с внешней машины помечается в отчёте как «external host» с паспортом из `machine.txt`.
