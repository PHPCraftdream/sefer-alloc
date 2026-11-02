#!/bin/bash
# MT1: call counts and inclusive Ir of the cold magazine events, callgrind T=1 300000 ops (outputs kept under $HOME)
S=$HOME/w2m; B=$S/tgt-base/release/examples/g1_mt_one
O=<scratch dir>/mt1c
C=$S/cg; mkdir -p $C; cd $C
for w in mstress larson; do
  for a in sefer mi; do
    valgrind --tool=callgrind --callgrind-out-file=$C/$w.$a.out $B $w $a 1 300000 > /dev/null 2>&1
    callgrind_annotate --inclusive=yes --tree=caller $C/$w.$a.out 2>/dev/null > $C/$w.$a.tree.txt
  done
  grep -E 'free_magazine_park|free_magazine_full_fast|refill_magazine_miss|refill_magazine_slow|reload_overflow|depot_reload|dealloc_own_base' $C/$w.sefer.tree.txt | cut -c1-230 | head -40 > $O/calls.$w.sefer.txt
  grep -E '_mi_page_retire|_mi_malloc_generic|mi_page_fresh_alloc|_mi_page_free|_mi_page_unfull|_mi_page_free_collect|_mi_page_queue' $C/$w.mi.tree.txt | cut -c1-230 | head -30 > $O/calls.$w.mi.txt
done
echo calls-done
