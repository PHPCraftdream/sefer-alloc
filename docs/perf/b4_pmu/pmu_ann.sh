#!/bin/bash
P=/usr/lib/linux-tools-6.8.0-147/perf
$P report -i ~/ab/mix.data --no-children --sort symbol --stdio 2>/dev/null | grep -E "alloc_core_small_impl|flush_class" | sed 's/^ *//' | cut -c1-400
S=$($P report -i ~/ab/mix.data --no-children --sort symbol --stdio 2>/dev/null | grep -oE "_RNvMNtNtNtNtCs[^ ]*alloc_core_small_impl[^ ]*" | head -1)
echo "SYM=$S" | rustfilt 2>/dev/null || echo "SYM=$S"
$P annotate -i ~/ab/mix.data --stdio -s "$S" 2>/dev/null | grep -vE "^\s*0\.00 |^\s*:\s*$" | awk '$1+0 >= 0.8' | head -60
