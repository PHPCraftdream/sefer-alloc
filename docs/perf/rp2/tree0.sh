C=$HOME/w2m/cg
O=<scratch dir>/rp1/out2
callgrind_annotate --inclusive=yes --tree=caller $C/rp2.sefer.0.out 2>/dev/null > $C/rp2.sefer.0.tree.txt
grep -E '^ *[0-9,]+ \(.*\)\s+\*' $C/rp2.sefer.0.tree.txt | sed -e 's#\[[^]]*\]##' | cut -c1-200 | head -60 > $O/tree_stars.sefer.0.txt
grep -E '(refill_magazine_miss|refill_magazine_slow)' $C/rp2.sefer.0.tree.txt | grep -E '=>|<' | sed -e 's#\[[^]]*\]##' | cut -c1-260 | head -20 > $O/tree_calls.sefer.0.txt
grep -E '^ *[0-9,]+ \(.*\)\s+=> ' $C/rp2.sefer.0.tree.txt | sed -e 's#\[[^]]*\]##' -e 's#<scratch>/base/src/##' | grep -E 'try_drain_freelist_batch|batch_drain_from_class_cursor|find_segment_with_free|try_carve_batch|reserve_small_segment|drain_segment_sidecar|process_ingress|class_cursor' | cut -c1-240 | sort -u | head -40 > $O/tree_calls2.0.txt
echo ok
