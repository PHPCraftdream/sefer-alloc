#!/bin/bash
# DZ-3 prototype (scratch): 16 slots + MRU memo + incremental sweep; Ir per foreign free and time per free vs base (4 slots)
source ~/.cargo/env
W=<scratch dir>
S=$HOME/w2m
O=$W/pc2/outm; mkdir -p $O
rm -rf $S/pinm; mkdir -p $S/pinm; tar -xf $W/base.tar -C $S/pinm
python3 - $S/pinm/src/registry/heap_core_xthread/route_pin_cache.rs <<'PY'
import sys
p = sys.argv[1]
s = open(p, encoding='utf-8').read()
def rep(a, b):
    global s
    if s.count(a) != 1:
        raise SystemExit('anchor %d: %r' % (s.count(a), a[:60]))
    s = s.replace(a, b)
rep('const PIN_CACHE_SLOTS: usize = 4;', 'const PIN_CACHE_SLOTS: usize = 16;\nconst SWEEP_PER_CALL: usize = 4;')
rep('    slots: [PinCacheSlot; PIN_CACHE_SLOTS],\n    next_victim: usize,\n}', '    slots: [PinCacheSlot; PIN_CACHE_SLOTS],\n    next_victim: usize,\n    last_hit: usize,\n    sweep_cursor: usize,\n}')
rep('        next_victim: 0,\n    };', '        next_victim: 0,\n        last_hit: 0,\n        sweep_cursor: 0,\n    };')
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
            else {''', '''            let hint = cache.last_hit;
            let found = if cache.slots[hint].pin.is_some() && cache.slots[hint].base == base {
                Some(hint)
            } else {
                cache
                    .slots
                    .iter()
                    .position(|slot| slot.pin.is_some() && slot.base == base)
            };
            let Some(idx) = found else {''')
rep('''                count!(PIN_CACHE_MISSES);
                return CachedPublication::Miss;
            };
            if let Some(pin) = slot.pin.as_ref() {''', '''                count!(PIN_CACHE_MISSES);
                return CachedPublication::Miss;
            };
            cache.last_hit = idx;
            let slot = &mut cache.slots[idx];
            if let Some(pin) = slot.pin.as_ref() {''')
rep('''            slot.incarnation = incarnation;
            slot.pin = Some(pin);
            crate::ghost::ghost_event!("pin-cache/held", slot.base, 1);
        } else {''', '''            slot.incarnation = incarnation;
            slot.pin = Some(pin);
            crate::ghost::ghost_event!("pin-cache/held", slot.base, 1);
        } else {''')
rep('''            slot.base = base;
            slot.incarnation = incarnation;
            slot.pin = Some(pin);
            crate::ghost::ghost_event!("pin-cache/held", slot.base, 1);
        }''', '''            slot.base = base;
            slot.incarnation = incarnation;
            slot.pin = Some(pin);
            crate::ghost::ghost_event!("pin-cache/held", slot.base, 1);
            cache.last_hit = victim;
        }''')
open(p, 'w', encoding='utf-8').write(s)
print('patched')
PY
cp $S/base/examples/g1_rhythm_probe.rs $S/pinm/examples/g1_rhythm_probe.rs
(cd $S/pinm && CARGO_PROFILE_RELEASE_DEBUG=line-tables-only CARGO_TARGET_DIR=$S/tgt-pinm cargo build -q --release --example g1_rhythm_probe --features production) || exit 1
B4=$S/tgt-base/release/examples/g1_rhythm_probe
BM=$S/tgt-pinm/release/examples/g1_rhythm_probe
C=$S/cg; cd $C
: > $O/ir.txt
for arm in 4 M; do
  if [ $arm = 4 ]; then B=$B4; else B=$BM; fi
  for m in 4 5 8 16 64; do
    for o in cyclic seq random; do
      valgrind --tool=callgrind --callgrind-out-file=$C/pm.$arm.$m.$o.out $B pincycle $m $o > /dev/null 2>&1
      callgrind_annotate --inclusive=yes --tree=caller $C/pm.$arm.$m.$o.out 2>/dev/null > $C/pm.tree.txt
      pf=$(grep -E '^ *[0-9,]+ \(.*\)\s+\*\s+.*publish_foreign' $C/pm.tree.txt | sed -e 's#\[[^]]*\]##' | head -1 | awk '{print $1}')
      echo "$arm $m $o publish_foreign_incl=$pf" >> $O/ir.txt
    done
  done
done
: > $O/time.txt
for i in 1 2 3 4 5 6 7 8 9; do
  for m in 4 5 8 16 64; do
    for o in cyclic seq random; do
      for arm in 4 M; do
        if [ $arm = 4 ]; then B=$B4; else B=$BM; fi
        $B pincycle $m $o | sed "s/^{/{\"arm\":\"$arm\",/" >> $O/time.txt
      done
    done
  done
done
echo pc2m-done
