C=$HOME/w2m/cg
O=<scratch dir>/mt1c
for w in mstress larson; do
  grep -E '=> .*(dealloc_shim|alloc_shim)' $C/$w.sefer.tree.txt | cut -c1-260 | head -6 > $O/calls_shim.$w.sefer.txt
  grep -E '=> .*(mi_free|mi_malloc|mi_theap_malloc_aligned|dealloc_shim|alloc_shim)' $C/$w.mi.tree.txt | cut -c1-260 | head -10 > $O/calls_shim.$w.mi.txt
done
echo ok
