C=$HOME/w2m/cg
O=<scratch dir>/rp1/out2
callgrind_annotate --inclusive=yes --tree=caller $C/rp2.sefer.943718.out 2>/dev/null > $C/rp2.sefer.tree.txt
grep -E '^ *[0-9,]+ \(.*\)\s+\*' $C/rp2.sefer.tree.txt | sed -e 's#\[[^]]*\]##' | cut -c1-200 | head -60 > $O/tree_stars.sefer.txt
grep -E '(refill_magazine_miss|refill_magazine_slow|find_segment_with_free|drain_segment_sidecar|ingress_discovery|try_drain_freelist_batch|try_carve_batch|flush_class_internal|publish_foreign|trim_for_recycle|release_segment|discover|drain_ingress)' $C/rp2.sefer.tree.txt | grep -E '=>|<' | sed -e 's#\[[^]]*\]##' | cut -c1-260 | head -60 > $O/tree_calls.sefer.txt
echo ok
