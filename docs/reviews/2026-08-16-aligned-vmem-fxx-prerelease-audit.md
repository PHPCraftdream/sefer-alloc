# `aligned-vmem` 0.2.0 pre-release audit

Дата: 2026-08-16. Режим: только чтение. Тесты, сборка, clippy и benchmarks не
запускались; изменён только этот файл отчёта.

## Область

Прочитаны `crates/vmem/Cargo.toml`, `README.md`, весь `src/` (включая
`lib.rs`, 3360 строк), тесты, example и benchmark-код. Это статический аудит
текущего checkout. Платформенные утверждения, помеченные ниже как
reasoned-from-spec, не выдаются за результат запуска.

## Итог

Большинство прежних soundness-дефектов уже закрыто: проверки диапазонов,
Android/BSD cfg wiring, Windows lazy path, fault-injection races и Cargo-feature
unification для mock выглядят существенно лучше. Перед публикацией всё ещё
нужно принять решения по двум семантически важным контрактам: Darwin reclaim и
adoption huge-page reservations.

## Findings

### P1 — исправить или явно изменить контракт

1. **Eager `decommit` не выполняет обещанный reclaim/zero-fill на Darwin.**

   `decommit_pages_impl` вызывает `madvise(MADV_DONTNEED)` на всех Unix
   (`src/lib.rs:2635-2654`), а Unix `recommit_pages_impl` всегда возвращает
   `Ok(())` (`src/lib.rs:2660-2678`). На macOS/iOS/tvOS/watchOS
   `MADV_DONTNEED` для anonymous memory advisory-only: старые байты могут
   остаться после decommit+recommit. Это уже подтверждено macOS CI и честно
   описано в rustdoc (`src/lib.rs:1402-1421`), но противоречит базовому
   назначению `decommit` — «return physical backing to the OS».

   До релиза нужно либо сделать Darwin-specific remap/replacement, либо сузить
   публичный контракт до best-effort hint и убрать обещание zero-fill/reclaim.

2. **`from_raw_parts` теряет huge-page state и создаёт fail-open путь.**

   Конструктор всегда устанавливает `granted_huge: false`
   (`src/lib.rs:1000-1007`). Внешний владелец может передать реальную huge-page
   reservation, получить `is_huge() == false`, а затем вызвать `decommit`, хотя
   документация предупреждает, что decommit на huge pages не работает
   (`src/lib.rs:1391-1400`, `1800-1808`). Это публичный межкрейтовый adoption API,
   поэтому нельзя полагаться на то, что вызывающий «и так знает».

   Варианты: принимать явный `granted_huge`, использовать отдельный typed
   adoption path, или консервативно запретить decommit для adopted handles.

### P2 — важные ограничения и ускорения

3. **`decommit_lazy` на поддерживаемых BSD не является lazy.**

   Специальные `MADV_FREE` значения выбираются только для Linux/Android и
   Darwin; FreeBSD, DragonFly, NetBSD и OpenBSD попадают в `MADV_DONTNEED`
   fallback (`src/lib.rs:2710-2757`, constants `2945-2966`). Это корректно и
   задокументировано, но оставляет performance opportunity: добавить BSD cfg
   arms с проверенными значениями и target CI.

4. **На 64-bit Unix каждый reserve держит лишний `align` виртуального адресного
   пространства.**

   Exact-size path отключён (`src/lib.rs:2428-2437`, `2546-2549`), поэтому
   обычный путь делает `mmap(size + align)` и сохраняет всю область
   (`src/lib.rs:2438-2527`). Для `align == PAGE` это лишние 4 KiB на каждую
   reservation; для больших align amplification больше. Это не RSS leak, но
   стоит измерить отдельный fast path для `align <= actual OS page size`.

5. **`reservation_len()` не является фактическим размером OS reservation.**

   Документация это признаёт (`src/lib.rs:576-612`): Windows rounding до 64 KiB
   и host-page rounding могут сделать значение меньше реально занятого VA.
   Это ловушка для accounting и `from_raw_parts`. Лучше переименовать в
   `requested_reservation_len` либо добавить отдельно названный метод для
   фактического OS-granular size, если его можно получить переносимо.

6. **Ошибки `munmap`, `VirtualFree(MEM_DECOMMIT)` и `madvise` отбрасываются.**

   См. `src/lib.rs:2327-2337`, `2624-2629`, `3172-3205`. Для advisory
   `madvise` это приемлемая политика, но для release/decommit это превращает
   backend bookkeeping defect в тихий leak/неосвобождённый RSS. Нужен хотя бы
   diagnostic hook; предпочтительно также fallible `try_decommit`/release API,
   сохранив текущие wrappers для allocator hot path.

7. **Mock `record` может потерять события и хрупок при TLS teardown.**

   Reentrant вызовы намеренно silently dropped (`src/mock.rs:298-312`), а
   `CALLS` защищён `try_with`, но внешний `RECORDING.with` остаётся обычным
   `with`. При сложном порядке уничтожения thread-local значений вызов из
   `Reservation::drop` потенциально может паниковать. Даже если текущий std
   порядок делает это практически недостижимым, mock должен либо явно
   документировать неполный log, либо сделать весь recording fallible/no-op на
   teardown.

### P3 — release hygiene и portability

8. **Edition-2024 unsafe debt.** Несколько FFI helpers остаются `unsafe fn` с
   implicit unsafe operations вместо локальных `unsafe {}` и собственных
   SAFETY-комментариев (Windows/Unix/miri backend; прежний inventory находится
   в `docs/CORRECTNESS_OPEN_ITEMS.md`, item 49). Сейчас edition 2021, но при
   переходе на 2024 это станет hard error. Механически закрыть до migration.

9. **Advertised target matrix шире подтверждённой.** Вручную заданы `MAP_ANON`,
   `MAP_HUGETLB`, `_SC_PAGESIZE`, `off_t` и `MADV_*` constants
   (`src/lib.rs:2766-3100`). Комментарии честно отмечают reasoned-from-spec для
   BSD, MIPS и части Android/tvOS/watchOS, но перед publish нужны compile-only
   CI checks по каждому advertised cfg family и явный supported-target список
   в README. Особо рискованны MIPS huge-pages и BSD lazy path.

10. **Mutable diagnostic statics создают лишнюю публичную поверхность.**
    `UNIX_*`/`WINDOWS_*` counters (`src/lib.rs:216-298`) доступны downstream
    напрямую и могут быть изменены, после чего показания перестают быть
    trustworthy. Storage лучше сделать private/`pub(crate)`, оставив accessors
    и reset. На production path это не влияет.

## Что выглядит хорошо

- `Reservation` владеет всей mapping, явно `Send` и не `Sync`; ownership SAFETY
  аргумент присутствует.
- Проверяются `size + align`, `Layout` и диапазоны до OS calls; Windows lazy
  path сохраняет полную reservation.
- `VmemError` различает invalid argument и OS refusal без фиктивного errno;
  capture timing в основных error paths выдержан.
- Android cfg wiring, mock cfg conversion и runtime-page-size validation закрыли
  реальные ранее найденные дефекты.

## Приоритет перед публикацией

1. Решить Darwin: исправить backend или сузить контракт.
2. Устранить fail-open semantics `from_raw_parts` для huge reservations.
3. Добавить compile-only target matrix и закрыть edition-2024 unsafe debt.
4. Затем измерить Unix small-align fast path и BSD lazy advice.
5. Спрятать mutable diagnostic storage; решить, нужен ли fallible release API.

## Ограничение проверки

По запросу не запускались `cargo test`, `cargo check`, `cargo clippy`, `cargo
doc`, cross-compilation, Miri, CI и benchmarks. Отчёт не утверждает, что
checkout компилируется на всех feature/target комбинациях.

---
---

# PART II — independent full audit pass (fxx, 2026-08-16, read-only)

*This part is a second, independently-conducted full read of the crate
(`Cargo.toml`, all 4,045 lines of `src/`, `README.md`, all 9 test files,
`benches/vmem_bench.rs`, `examples/v20_849_unix_exact_reserve_hit_rate.rs`,
CI rows in `.github/workflows/ci.yml`, and the workspace CHANGELOG for
decision history). It was written without reading Part I first; overlaps are
noted where they exist. Read-only: no tests, builds, or lints were run.*

## II.1 Soundness walk (all `unsafe` sites) — NULL RESULT with one FFI exception

Every `unsafe` block, FFI declaration and pointer-arithmetic site was walked
individually. No unsoundness found in the reachable mainstream-target code.
Specifically verified:

- **Provenance discipline** is genuinely correct: both over-reserve paths
  (`win_reserve_commit`, `unix_reserve`) derive the aligned base via
  `region_ptr.with_addr(base_addr)` (carrying whole-region provenance), and
  every address comparison uses the non-exposing `.addr()`. No
  `usize -> pointer` round-trips anywhere in `src/`.
- **Exactly-once release**: `into_parts`/`into_reservation_parts` both
  `mem::forget(self)` before returning; `Drop` is the only other release
  path; `release(NULL, ..)` is a documented no-op; the miri backend's
  `Layout` reconstruction is protected by the up-front asserts in both
  `release()` (task #947/G-1) and `from_raw_parts` (#719/#776/#910/#916) —
  each assert clause was checked against the documented `# Safety` list.
- **`NonNull::new_unchecked` sites** (`unix_reserve` x2,
  `try_reserve_aligned_exact`, `leak_zeroed_pages`) are all dominated by an
  explicit null check on the same value.
- **`unsafe impl Send for Reservation`** justified (exclusive ownership, no
  TLS affinity); `!Sync` preserved.
- **FFI struct layout**: hand-declared `SystemInfo` matches the real
  `SYSTEM_INFO` (union head flattened to two `WORD`s, `dwActiveProcessorMask`
  as `usize` = `DWORD_PTR` in the correct position). Windows `MEM_*`/
  `PAGE_READWRITE` constants and Unix `PROT_*`, `MAP_PRIVATE`, per-family
  `MAP_ANON`, `MADV_DONTNEED`=4, Linux `MADV_FREE`=8, Darwin
  `MADV_FREE_REUSABLE`=7, FreeBSD/DragonFly `MADV_FREE`=5, NetBSD/OpenBSD=6,
  `MADV_HUGEPAGE`=14, `MAP_FAILED`=-1, and the per-OS `_SC_PAGESIZE` table
  (Darwin 29 / FreeBSD-DragonFly 47 / NetBSD-OpenBSD 28 / bionic 39 /
  glibc-musl 30) were checked against the respective OS headers and are
  correct for the enumerated targets.
- **Bounds-check composition** for the safe `Reservation::{decommit,
  decommit_lazy, recommit, try_recommit, commit_range, try_commit_range}`
  methods: the `end > self.len()` rejection plus the free functions'
  `start >= end` / `start > end` + page-multiple checks jointly exclude every
  out-of-bounds combination — no gap found.
- **Mock/fault-injection protocol**: `validate_size_align` runs before
  `take_reserve_fault` (contract violations don't consume armed faults);
  `arm_fail_next` priority matches docs and tests; the `fetch_update`
  decrement is a genuine RMW; the Release/Acquire pairing on
  `FAIL_AT_TARGET`/`FAIL_AT_COUNTER` is correct; the known unfixed
  re-arm-vs-self-disarm race is honestly documented in the module doc (F15
  scope note) — accepted, not re-flagged.

The one exception to the null result is finding II-1 below — an FFI-signature
defect (UB by declaration) confined to 32-bit musl targets.

## II.2 Correctness / cross-platform findings

### II-1. 32-bit musl targets declare `mmap` with `off_t = i32`, but musl's `off_t` is unconditionally 64-bit — FFI ABI mismatch (UB by declaration); every reservation likely fails on those targets

- **Severity: MEDIUM** (HIGH-shaped defect class — a mismatched foreign-fn
  signature is UB by definition — moderated because the affected targets are
  tier-2, the practical failure is fail-closed, and no such target is in CI)
- **Location:** `src/lib.rs:3083-3099` (`OffT` alias) + the
  `extern "C" fn mmap(..., offset: OffT)` declaration at `src/lib.rs:3101-3114`
- **Description:** The `OffT` alias (tasks #911/#914) selects `i32` for
  `all(target_pointer_width = "32", any(target_os = "linux", target_os = "android"))`,
  `i64` otherwise. Correct for glibc (32-bit glibc `mmap` takes 32-bit
  `off_t` absent `_FILE_OFFSET_BITS=64`) and for bionic — **wrong for musl**:
  musl defines `off_t` as 64-bit on *every* architecture by design ("musl
  only provides the 64-bit off_t interfaces"; `mmap64` is an alias of `mmap`,
  both taking 64-bit offsets). Rust ships 32-bit musl targets matching the
  `i32` arm: `i686-unknown-linux-musl`, `armv7-unknown-linux-musleabihf`
  (tier 2), etc. The in-source comment's claim that the two-arm alias
  "correctly classifies EVERY `cfg(unix)` target" is false. Root cause note:
  this encodes a wrong premise inherited from the 2026-08-07 rust-intel audit
  ("glibc/**musl**'s `mmap` symbol takes a 32-bit off_t on 32-bit" —
  `docs/reviews/2026-08-07-aligned-vmem-rust-intel-audit.md:142`).
- **Failure scenario:** On `i686-unknown-linux-musl` (cdecl stack args) the
  crate pushes 4 bytes for `offset`; musl's `mmap` reads 8. The high word is
  stack garbage; musl's `if (off & OFF_MASK) return EINVAL` validation then
  rejects the call whenever the garbage is nonzero — `reserve_aligned` fails
  (usually always) with a misleading `EINVAL`. Independent of observed
  behavior, the mismatched declaration is UB.
- **Suggested fix:** add `not(target_env = "musl")` to the `i32` arm so musl
  falls into the `i64` catch-all; correct the "classifies EVERY target"
  comment. One-line cfg change — recommend doing this before publish.

### II-2. `Reservation`'s documented invariant "`as_ptr()` … valid for `len()` bytes for the lifetime of this handle" is falsified by the new *safe* `decommit`/`decommit_lazy` methods

- **Severity: MEDIUM** (doc/contract; soundness-adjacent)
- **Location:** `src/lib.rs:488-502` (struct doc) vs. `src/lib.rs:734-765`
  (safe `Reservation::decommit`/`decommit_lazy`, new in this release, #947/A-2)
- **Description:** The struct-level rustdoc still promises unconditional
  validity of the usable span for the handle's whole lifetime. Since the safe
  decommit methods landed, *safe* code can revoke that validity: on Windows,
  `MEM_DECOMMIT` genuinely unmaps the pages, so a subsequent access is a hard
  `STATUS_ACCESS_VIOLATION`. Unsafe callers in consumer crates justify their
  dereferences by citing exactly this guarantee (the README example does:
  "SAFETY: base is valid for r.len() bytes"). Not unsoundness in this crate
  (the deref is the caller's `unsafe`), but a documented-invariant
  contradiction of the kind unsafe callers build proofs on — introduced by
  the same release that should ship the doc fix.
- **Failure scenario:** a consumer type exposes safe `clear_cache()`
  (`r.decommit(0, len)`) and unsafe `read_at(off)` justified by the struct
  doc. On Windows, `clear_cache()` then `read_at(0)` crashes; both call
  sites individually followed the crate's documentation.
- **Suggested fix:** qualify the struct doc and `as_ptr` doc: "…valid for
  `len()` bytes for the lifetime of this handle, except ranges the caller has
  decommitted (via the free functions or the safe methods) and not yet
  recommitted — on Windows such pages are unmapped until `recommit`."

### II-3. `reserve_aligned_huge`: the Linux and Windows usable parameter spaces are **disjoint** — no `(size, align)` can ever obtain huge pages on both platforms, and the Windows-optimal shape is a hard `invalid_argument` on Linux

- **Severity: MEDIUM** (API design / the "best-effort, never fails purely
  because huge pages are unavailable" promise is false in composition)
- **Location:** `src/lib.rs:2418-2427` (Linux 2 MiB-multiple guard, #714) vs.
  `src/lib.rs:1966` + `2251-2268` (Windows: `MEM_LARGE_PAGES` only when
  `align <= 64 KiB`)
- **Description:** Each constraint is documented individually; the composition
  is not, and it is pathological: Linux requires `align >= 2 MiB`
  (2 MiB-multiple), Windows requests large pages only when `align <= 64 KiB`.
  These ranges do not intersect. `reserve_aligned_huge(4 MiB, 4 MiB)` (the
  crate's own flagship segment shape) is structurally never huge on Windows;
  `reserve_aligned_huge(2 MiB, 64 KiB)` can be huge on Windows but on Linux
  is not even an ordinary-pages fallback — it returns
  `Err(VmemError::invalid_argument())`, contradicting the headline promise
  that the API "never fails purely because huge pages are unavailable."
- **Failure scenario:** a cross-platform consumer adopts the only
  Windows-grantable shape (`align = 64 KiB`); their Linux build now fails
  every huge reservation outright (no fallback), an outage the "best-effort
  with fallback" framing says cannot happen. The only portable escape is
  per-platform `cfg`'d arguments — suggested nowhere.
- **Suggested fix:** (a) best: on Windows, attempt the single-call
  `MEM_RESERVE|MEM_COMMIT|MEM_LARGE_PAGES` for any `align` up to the
  large-page minimum — a granted large-page allocation is naturally
  >= 2 MiB-aligned, and the existing unconditional post-call alignment check
  (#917/H2C6) already guarantees correctness on a miss; this makes the
  2/4 MiB segment shape serviceable on both OSes with the same arguments.
  (b) minimum: document the disjointness prominently in
  `reserve_aligned_huge`'s rustdoc + README and soften the headline sentence,
  which the Linux guard's fine print already contradicts.

### II-4. 64-bit Linux huge-page reservations charge `size + align` against the scarce hugetlb pool — up to 2x the necessary huge pages per reservation; the over-reserve is provably unnecessary for `align == 2 MiB`

- **Severity: MEDIUM** (resource waste on the feature's primary platform)
- **Location:** `src/lib.rs:2412-2528` (`unix_reserve` huge path; exact-size
  path compiled out on 64-bit per #944/P-1)
- **Description:** For ordinary pages the 64-bit `size + align` over-reserve
  costs only untouched VA (cheap — Part I's item 4 covers that angle). For
  `MAP_HUGETLB` it is not cheap: hugetlb pages are **reserved against the
  preallocated pool at `mmap` time** (hugetlbfs reservation accounting,
  absent `MAP_NORESERVE`), so `reserve_aligned_huge(4 MiB, 4 MiB)` charges
  8 MiB = 4 huge pages of `nr_hugepages` while only 2 back the usable span,
  for the reservation's lifetime. Yet for `align == 2 MiB` — the minimum the
  contract permits — the kernel already guarantees an anonymous `MAP_HUGETLB`
  mapping is huge-page-aligned (the crate's own #714 comment states this), so
  an exact-size huge `mmap` satisfies the alignment contract by construction
  with zero over-reserve.
- **Failure scenario:** host with `nr_hugepages = 1024` (2 GiB pool); a
  consumer reserving 4 MiB@4 MiB huge segments expects ~512 from the pool but
  gets ~256, after which every further request silently falls back to
  ordinary pages (`is_huge() == false`) at half the expected capacity.
- **Suggested fix:** in the huge path only, try an exact-size `MAP_HUGETLB`
  `mmap` first and accept it when the base satisfies `align` (guaranteed for
  `align == 2 MiB`); fall back to the over-reserve on a miss — the existing
  32-bit fast-path shape, justified here by hugetlb-pool economy rather than
  VA economy.

### II-5. `MADV_HUGEPAGE` is now issued precisely (and only) on `MAP_HUGETLB`-granted mappings, where THP advice can have no effect

- **Severity: LOW** (wasted syscall + misleading doc framing)
- **Location:** `src/lib.rs:2509-2516` (`unix_reserve`) and `2605-2615`
  (`try_reserve_aligned_exact`); framing in `Cargo.toml:48` and
  `reserve_aligned_huge`'s rustdoc ("Linux `MAP_HUGETLB` + `MADV_HUGEPAGE`").
- **Description:** Task #920/V-2 deliberately removed the THP hint from the
  ordinary-pages fallback (defensible: silently substituting THP for the
  requested hugetlbfs pages was judged misleading, and `is_huge()` makes the
  fallback detectable). What remains is the inverse oddity: the hint now
  fires **only** on hugetlbfs VMAs, which the THP machinery does not manage.
  `madvise(MADV_HUGEPAGE)` there is at best an accepted no-op (sets a VMA
  flag khugepaged will never act on) and on several kernel versions an
  outright `EINVAL`; in no case can it do anything — the pages are already
  huge. The crate's sole remaining `MADV_HUGEPAGE` call site is therefore a
  guaranteed-ineffective syscall, and the "MAP_HUGETLB + MADV_HUGEPAGE" doc
  framing implies a composition that (deliberately, since V-2) never occurs.
- **Failure scenario:** none functional — one wasted syscall per granted huge
  reservation; doc readers conclude THP is part of the strategy when it is
  not used at all.
- **Suggested fix:** delete both `libc_madvise_hugepage` call sites (plus the
  helper and the `MADV_HUGEPAGE` constant), or make THP an explicit
  documented opt-in; update the two doc sites either way.

### II-6. `PAGE`'s rustdoc still states decommit/recommit offsets validate against `PAGE` — the validation base moved to `page_size()` (#947/A-1)

- **Severity: LOW** (doc drift with a real misuse path, on the most-read
  constant)
- **Location:** `src/lib.rs:142-158`: "Decommit/recommit offsets passed to
  the validation in [`decommit`] / [`recommit`] must be multiples of this
  value."
- **Description:** Since A-1 all four range functions validate against the
  runtime `page_size()`; a `PAGE`-multiple is necessary but not sufficient.
  `page_size()`'s doc, `decommit`'s doc and the README all say so correctly —
  this one sentence on `PAGE` itself still states the pre-A-1 contract. The
  crate's own tests were bitten by exactly this on Apple Silicon (the several
  #959 `PAGE` → `page_size()` test fixes).
- **Failure scenario:** a consumer on a 16 KiB-page host follows `PAGE`'s
  doc, passes 4 KiB-multiple offsets: every `decommit` silently no-ops (RSS
  never drops) and every `recommit` returns `false` on a range that is fine.
- **Suggested fix:** reword to "must be multiples of the runtime
  [`page_size()`]; this constant is only the guaranteed lower bound."

### II-7. Verified-clean list (checked, not reproduced)

- `align_up_addr` overflow handling; #957's `isize::MAX` rejection in
  `validate_size_align` — consistent classification on all paths and
  platforms.
- `win_reserve_commit`'s `commit_len == size` single-call guard — correct;
  the lazy caller cannot reach the single-call path (properly
  counterfactual-tested since #953 by `windows_lazy_reserve_saves_commit_charge`).
- The V-32 fast-reserve sub-path and its dead-by-construction over-reserve
  else-branch — correct, and honestly documented as defensive.
- Error-capture timing (#713): every `last_os_error()` reads before any
  cleanup FFI on every path re-checked — holds. (Minor INFO: on the huge
  single-call double-failure, the retry's error overwrites the original
  large-page failure code, e.g. `ERROR_PRIVILEGE_NOT_HELD`; arguably the
  right choice, just worth knowing when diagnosing "why no large pages".)
- `leak_zeroed_pages`: rounding, miri zeroing, and leak semantics all match
  the doc.

## II.3 `error.rs` / `mock.rs` / `fault_injection.rs` — minor items

### II-8. `last_os_error_code` maps a theoretical negative `raw_os_error` to a huge `u32` — **INFO**, `error.rs:165-170`; self-consistent downstream (io-bridge routes it to `io::Error::other`), noted for completeness.

### II-9. Well-formed empty-range `recommit`/`commit_range` no-ops are not recorded in the mock log

- **Severity: INFO** — `src/lib.rs:1551-1552`, `1642-1643`: the
  `start == end` early-return runs before `mock::record`, unlike every other
  well-formed call. A consumer test asserting on a `commit_range(x, x)`
  record sees nothing and may misread its own code as not calling. One
  sentence in `mock`'s module doc closes it.

### II-10. `mock::record`'s reentrancy guard also drops *legitimate* nested records — **INFO**; documented behavior (`mock.rs:298-313`), pinned by `tests/mock_reentrancy.rs`; the cross-thread half is in the module doc, the nested-drop half only in the test. Fine for 0.2.0.

*(Part I's item 7 raises the outer `RECORDING.with` vs TLS-teardown concern;
concur it is theoretical-only — `LocalKey::with` on a `Cell` without a `Drop`
impl is `const`-initialized and never runs a destructor, so only the
`RefCell<Vec>` key can be torn down, and that one is already `try_with`.)*

## II.4 README / rustdoc / packaging

The README's platform-caveat section (Windows crash-on-write, huge-page
decommit no-op, Darwin/BSD advisory-only `MADV_DONTNEED`) was checked claim
by claim against the code — accurate and commendably honest.

### II-11. README's "Three exceptions to 'never panics'" list opens with an item that does not panic

- **Severity: LOW** — `README.md:117-134`. Exception (1) is
  `recommit`/`commit_range` rejecting violations, which the same sentence
  says "still don't panic" — it is an exception to the old silent-no-op
  shape, not to no-panic. Only `from_raw_parts` and `release` are genuine
  panic exceptions. A reader skimming for "can this panic inside my
  GlobalAlloc?" gets a false positive first. Retitle ("two panic exceptions,
  one behavior change") or split the list.

### II-12. Rustdoc density of internal task-number archaeology on the public API surface

- **Severity: LOW** (publish-facing polish; a conscious house-style decision)
- **Location:** throughout public rustdoc — e.g. `release`'s `# Panics`
  narrates what "an earlier version of this doc claimed"
  (`src/lib.rs:1275-1288`); `reservation_len`, `from_raw_parts`,
  `reserve_aligned_huge`, `decommit_lazy` each carry multi-paragraph
  task-#NNN history.
- **Description:** the contracts are precise, but a docs.rs reader wades
  through internal review history to find them; this is the crate's biggest
  first-impression cost. Suggested (pre-1.0, not blocking 0.2.0): demote the
  history paragraphs to `//` comments, keep only normative contract in `///`.

### II-13. Packaging sanity — clean (null result)

`[dependencies]` empty (the "zero dependencies" claim holds; the only
dev-dependency `bench-scale-tool = "0.1"` is a version requirement, so
`cargo publish` can resolve it). Licenses in-crate; docs.rs feature list
correct post-#962; `unexpected_cfgs` check-cfg covers `aligned_vmem_mock`;
`rust-version = "1.88"` is sufficient for everything used (`is_multiple_of`
1.87, strict-provenance methods 1.84, `io::Error::other` 1.74). No
crate-local `CHANGELOG.md` — release notes live only in the workspace root
CHANGELOG, which is not part of the published package; consider a short
crate-local changelog for 0.2.0.

## II.5 Tests — coverage gaps

The suite is unusually strong on counterfactual honesty (several tests
document which reverts they were verified against; the vacuous-oracle
histories are recorded in the tests themselves). Remaining gaps:

### II-14. The entire 32-bit Unix exact-size fast path has zero build or execution coverage anywhere

- **Severity: LOW** (dead-untested platform-gated code on a shipping path)
- **Location:** `src/lib.rs:2550-2622` (`try_reserve_aligned_exact`,
  `#[cfg(target_pointer_width = "32")]`); `.github/workflows/ci.yml` has no
  32-bit Unix row (checked: no `i686`/`armv7` job anywhere).
- **Description:** Since #944/P-1 gated the fast path to 32-bit, no CI job
  even *compiles* it — the counters, the #897/U1 unconditional alignment
  check, and the munmap-on-miss cleanup are exercised by nothing. A
  regression (or a cfg typo around it) is invisible to the entire
  verification matrix while the path still ships to 32-bit Unix users.
- **Failure scenario:** a future edit breaks the alignment-miss `munmap`;
  every CI row stays green; published-crate users on 32-bit Unix leak a
  mapping per miss.
- **Suggested fix:** add at least `cargo check --target i686-unknown-linux-gnu`
  (and, after II-1, a musl sibling) to the `aligned-vmem-gates` job; or
  document the path as at-your-own-risk.

### II-15. `From<VmemError> for io::Error`'s high-bit branch (the #946/G-2 fix) is untested

- **Severity: LOW** — `error.rs:147-155`; `tests/vmemerror_io_bridge.rs`
  covers `Some(12)`, invalid-argument, and unknown-code — not a code above
  `i32::MAX`. Reverting the fix to the old unchecked `as i32` cast would go
  unnoticed. One test: `from_os_code(0x8007_000E)` → `raw_os_error() == None`
  and the message preserved via `io::Error::other`.

### II-16. `ReservationParts::new` (V-13) and five of eight `Call` constructors are untested

- **Severity: INFO** — `src/lib.rs:1057-1069`; `tests/mock.rs:149-174` tests
  only `reserve`/`decommit`/`release` constructors. `ReservationParts::new`
  was added specifically to "close the round-trip" and nothing exercises
  `new` → `release_parts`. Two lines each in existing tests.

### II-17. No test observes safe `Reservation::decommit` over the *never-committed tail* of a lazy Windows reservation

- **Severity: INFO** — the safe method's bound is `len()`, which on a lazy
  reservation includes reserved-but-uncommitted pages;
  `VirtualFree(MEM_DECOMMIT)` on such pages is documented by Microsoft as
  succeeding, so this should be fine — but it is the one shape where the safe
  range can cover never-committed memory, and nothing pins it. Cheap to add
  next to the existing #947/A-2 tests.

### II-18. Test nulls (checked, no action)

The `SERIAL`-mutex discipline in `smoke.rs`/`lazy_commit.rs`/
`fault_injection.rs` covers every counter-touching test (no unlocked
counter-writer found per binary); the over-reserved-span retry loop's
keep-alive logic is sound; `mock_reentrancy.rs` is a genuine (non-vacuous)
reentrancy test including the fresh-thread first-push-must-allocate trick;
`fail_next_is_atomic_under_concurrent_callers`'s half-armed oracle is the
correct two-sided shape; `huge_pages.rs` honestly states that the
MAP_HUGETLB-*granted* branch has never executed on any CI host — still the
feature's largest verification gap, but stated rather than hidden.

## II.6 Performance

### II-19. Benchmarks measure decommit/recommit of never-touched pages — the cheap path

- **Severity: LOW** (measurement realism)
- **Location:** `benches/vmem_bench.rs:67-116`
- **Description:** both decommit cycles reserve and immediately decommit
  without ever writing to the span. `madvise(MADV_DONTNEED)` over pages never
  faulted in has almost no work (no PTEs to tear down, no TLB shootdown);
  Windows `MEM_DECOMMIT` of untouched pages likewise skips the expensive
  part. Real allocator churn decommits *dirty* pages, so the reported ns/op
  understate the operation the benches are named after, and any future
  "decommit got faster/slower" verdict from them risks being an artifact.
- **Suggested fix:** add one arm that `write_bytes`-faults the span in before
  decommitting.

### II-20. Performance nulls (checked, no action)

`page_size()` on the range paths is one relaxed load after first use; the
64-bit removal of the exact-size fast path is correctly reasoned (break-even
at 100% hit rate) and the flat 1-syscall over-reserve is optimal in syscall
count (the `size + align` VA hold is cheap — except for hugetlb, finding
II-4); the Windows single-call fast path and V-32 fast-reserve are genuine
savings with the path split observable via `bench-internals`; the historical
"~4.6 µs / 33%" claim is correctly flagged in-source as unverified;
`bench-internals` increments (storage and increments both cfg-gated) compile
out entirely when off.

## II.7 Part II executive summary

| Severity | Count | Findings |
|---|---|---|
| HIGH | 0 | — |
| MEDIUM | 4 | II-1 (32-bit musl `off_t` ABI mismatch / UB-by-declaration), II-2 (`Reservation` validity doc vs. new safe decommit), II-3 (huge-pages Linux/Windows parameter spaces disjoint; headline promise false in composition), II-4 (hugetlb-pool double-consumption on 64-bit Linux) |
| LOW | 7 | II-5 (ineffective `MADV_HUGEPAGE` placement), II-6 (`PAGE` doc drift vs. `page_size()` validation), II-11 (README panic-exceptions list), II-12 (rustdoc task-number density), II-14 (32-bit fast path: zero CI coverage), II-15 (io-bridge overflow branch untested), II-19 (benches measure untouched-page decommit) |
| INFO | 6 | II-8, II-9, II-10, II-16, II-17, plus the error-cause-overwrite note in II-7 |

**Soundness:** null result across every `unsafe` block on all enumerated
mainstream targets, with one FFI-signature exception (II-1) confined to
32-bit musl.

**Release readiness (Part II view):** the crate is in genuinely good shape
for 0.2.0 — precise contracts, honest platform caveats, and a test suite
that is rigorous about counterfactuals. Before publishing I would: fix II-1
(one-line cfg; removes the audit's only UB-class item), fix II-6 (one
sentence on the most-read constant), reconcile II-2's struct doc in the same
release that introduces the safe decommit methods, and make a conscious
decision on II-3's documentation half (the "never fails purely because huge
pages are unavailable" headline is currently false on Linux for the only
shape Windows can grant). II-4 and the rest are fine as tracked post-0.2.0
items. This is consistent with Part I's priority list; the two parts'
findings are complementary (Part I emphasizes the Darwin-contract and
`from_raw_parts`-adoption decisions; Part II adds the musl ABI defect, the
huge-pages cross-platform disjointness, the hugetlb-pool waste, and the
specific doc-drift/test-gap items above).

