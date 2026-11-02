C=$HOME/w2m/cg
O=<scratch dir>/rp1/out2
grep -E '^ *[0-9,]+ \(.*\)\s+=> ' $C/rp2.sefer.tree.txt | sed -e 's#\[[^]]*\]##' -e 's#<scratch>/base/src/##' | grep -E 'try_drain_freelist_batch|batch_drain_from_class_cursor|find_segment_with_free|try_carve_batch|reserve_small_segment|drain_segment_sidecar|process_ingress|refill_class_bump|class_cursor' | cut -c1-240 | sort -u | head -40 > $O/tree_calls2.txt
echo ok
