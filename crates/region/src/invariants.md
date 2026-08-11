- **I1 — resolution:** a fresh handle resolves via [`Region::get`] to the
  inserted value until it is [`Region::remove`]d.
- **I2 — tombstone:** after `remove(h)`, `get(h)` returns `None` for
  roughly `2^31` reuse cycles of that slot (a stale handle that has
  survived that many insert/remove cycles may wrap and spuriously
  resolve to a later value). A second `remove(h)` is a no-op `None`.
- **I3 — no ABA:** a stale handle — one whose slot has since been reused —
  does not resolve to a live value for roughly `2^31` reuse cycles of
  that slot. `slotmap`'s `DefaultKey` carries a 32-bit generation (odd =
  occupied, even = vacant): `insert` sets the low bit, `remove` increments
  via `wrapping_add(1)`, so a full occupy/free cycle advances the generation
  by 2 — after `2^31` such cycles it wraps and a very old handle may alias
  a later value. Memory safety is never affected — `slotmap` guarantees
  this even after wrap.
- **I4 — accounting:** [`Region::len`] equals the number of live entries
  and [`Region::is_empty`] agrees.
- **I5 — drop-once:** every live value is dropped exactly once. Successful
  `remove` transfers ownership to the caller without calling `Drop`; values
  still owned when a normally-destroyed `Region` drops are dropped. The crate
  never duplicates or internally forgets values.
- **I6 — slot reuse and bounded growth:** freed slots are reused by
  `insert`; capacity grows to a historical high-water mark of live entries
  and does not increase further under steady-state churn (`slotmap` does
  not physically compact — tombstone slots remain in the backing store;
  I6 guarantees only reuse and bounded growth, not physical density).
- **I7 — instance isolation:** a [`Handle<T>`] resolves only through the
  `Region<T>` instance that minted it. Every accessor stamps its
  `region_id` at construction and checks it before touching the backing
  slotmap; a mismatch is treated exactly like a stale handle. Two
  `Region<T>`s can never alias each other's values through a shared
  `DefaultKey`, even when that key collides (as it commonly does — the
  first insert into any fresh `Region` tends to produce the same key).