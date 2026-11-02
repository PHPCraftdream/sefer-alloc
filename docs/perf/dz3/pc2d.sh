#!/bin/bash
# DZ-3 prototype D (scratch): direct-mapped pin cache of N slots indexed by (base / SEGMENT) % N, incremental sweep; N = 16 and 64
source ~/.cargo/env
W=<scratch dir>
S=$HOME/w2m
O=$W/pc2/outd; mkdir -p $O
for n in 16 64; do
  rm -rf $S/pind$n; mkdir -p $S/pind$n; tar -xf $W/base.tar -C $S/pind$n
  python3 - $S/pind$n/src/registry/heap_core_xthread/route_pin_cache.rs $n <<'PY'
import sys
p, n = sys.argv[1], sys.argv[2]
s = open(p, encoding='utf-8').read()
def rep(a, b):
    global s
    if s.count(a) != 1:
        raise SystemExit('anchor %d: %r' % (s.count(a), a[:60]))
    s = s.replace(a, b)
rep('const PIN_CACHE_SLOTS: usize = 4;', 'const PIN_CACHE_SLOTS: usize = %s;\nconst SWEEP_PER_CALL: usize = 4;\n#[inline(always)]\nfn slot_index(base: usize) -> usize {\n    (base / crate::alloc_core::os::SEGMENT) & (PIN_CACHE_SLOTS - 1)\n}' % n)
rep('    slots: [PinCacheSlot; PIN_CACHE_SLOTS],\n    next_victim: usize,\n}', '    slots: [PinCacheSlot; PIN_CACHE_SLOTS],\n    sweep_cursor: usize,\n}')
rep('        next_victim: 0,\n    };', '        sweep_cursor: 0,\n    };')
rep('''        for slot in &mut self.slots {
            if slot.pin.as_ref().is_some_and(|pin| !pin.linked()) {
                slot.pin = None;
                crate::ghost::ghost_event!("pin-cache/held", slot.base, -1);
                count!(PIN_CACHE_SWEEPS);
            }
        }
    }
}''', '''        for _ in 0..SWEEP_PER_CALL {
            let i = self.sweep_cursor;
            self.sweep_cursor = (i + 1) % PIN_CACHE_SLOTS;
            let slot = &mut self.slots[i];
            if slot.pin.as_ref().is_some_and(|pin| !pin.linked()) {
                slot.pin = None;
                crate::ghost::ghost_event!("pin-cache/held", slot.base, -1);
                count!(PIN_CACHE_SWEEPS);
            }
        }
    }
}''')
rep('''            let Some(slot) = cache
                .slots
                .iter_mut()
                .find(|slot| slot.pin.is_some() && slot.base == base)
            else {''', '''            let idx = slot_index(base);
            let hit = cache.slots[idx].pin.is_some() && cache.slots[idx].base == base;
            if !hit {''')
rep('''                count!(PIN_CACHE_MISSES);
                return CachedPublication::Miss;
            };
            if let Some(pin) = slot.pin.as_ref() {''', '''                count!(PIN_CACHE_MISSES);
                return CachedPublication::Miss;
            }
            let slot = &mut cache.slots[idx];
            if let Some(pin) = slot.pin.as_ref() {''')
rep('''        if let Some(slot) = cache.slots.iter_mut().find(|slot| slot.base == base) {
            if slot.pin.take().is_some() {
                crate::ghost::ghost_event!("pin-cache/held", slot.base, -1);
                count!(PIN_CACHE_EVICTIONS);
            }
            slot.incarnation = incarnation;
            slot.pin = Some(pin);
            crate::ghost::ghost_event!("pin-cache/held", slot.base, 1);
        } else {
            let victim = cache.next_victim;
            cache.next_victim = (victim + 1) % PIN_CACHE_SLOTS;
            let slot = &mut cache.slots[victim];
            if slot.pin.take().is_some() {
                crate::ghost::ghost_event!("pin-cache/held", slot.base, -1);
                count!(PIN_CACHE_EVICTIONS);
            }
            slot.base = base;
            slot.incarnation = incarnation;
            slot.pin = Some(pin);
            crate::ghost::ghost_event!("pin-cache/held", slot.base, 1);
        }''', '''        {
            let slot = &mut cache.slots[slot_index(base)];
            if slot.pin.take().is_some() {
                crate::ghost::ghost_event!("pin-cache/held", slot.base, -1);
                count!(PIN_CACHE_EVICTIONS);
            }
            slot.base = base;
            slot.incarnation = incarnation;
            slot.pin = Some(pin);
            crate::ghost::ghost_event!("pin-cache/held", slot.base, 1);
        }''')
rep('''        if let Some(slot) = cache
            .slots
            .iter_mut()
            .find(|slot| slot.base == base && slot.pin.is_some())
        {
            slot.pin = None;''', '''        let slot = &mut cache.slots[slot_index(base)];
        if slot.base == base && slot.pin.is_some() {
            slot.pin = None;''')
open(p, 'w', encoding='utf-8').write(s)
print('patched', n)
PY
  cp $S/base/examples/g1_rhythm_probe.rs $S/pind$n/examples/g1_rhythm_probe.rs
  (cd $S/pind$n && CARGO_PROFILE_RELEASE_DEBUG=line-tables-only CARGO_TARGET_DIR=$S/tgt-pind$n cargo build -q --release --example g1_rhythm_probe --features production) || exit 1
done
B4=$S/tgt-base/release/examples/g1_rhythm_probe
BD16=$S/tgt-pind16/release/examples/g1_rhythm_probe
BD64=$S/tgt-pind64/release/examples/g1_rhythm_probe
C=$S/cg; cd $C
: > $O/ir.txt
for arm in 4 D16 D64; do
  eval B=\$B$arm
  [ $arm = 4 ] && B=$B4
  for m in 4 5 8 16 64; do
    for o in cyclic seq random; do
      valgrind --tool=callgrind --callgrind-out-file=$C/pd.$arm.$m.$o.out $B pincycle $m $o > /dev/null 2>&1
      callgrind_annotate --inclusive=yes --tree=caller $C/pd.$arm.$m.$o.out 2>/dev/null > $C/pd.tree.txt
      pf=$(grep -E '^ *[0-9,]+ \(.*\)\s+\*\s+.*publish_foreign' $C/pd.tree.txt | sed -e 's#\[[^]]*\]##' | head -1 | awk '{print $1}')
      echo "$arm $m $o publish_foreign_incl=$pf" >> $O/ir.txt
    done
  done
done
: > $O/time.txt
for i in 1 2 3 4 5 6 7 8 9; do
  for m in 4 5 8 16 64; do
    for o in cyclic seq random; do
      for arm in 4 D16 D64; do
        eval B=\$B$arm
        [ $arm = 4 ] && B=$B4
        $B pincycle $m $o | sed "s/^{/{\"arm\":\"$arm\",/" >> $O/time.txt
      done
    done
  done
done
echo pc2d-done
