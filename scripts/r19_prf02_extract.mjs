import assert from 'node:assert/strict';
import {readFileSync,writeFileSync} from 'node:fs';
const source=readFileSync('benches/perf_gate_iai.rs','utf8');
const names=['small_churn_16b','small_churn_16b_2n','dealloc_prealloc_only_16b','dealloc_free_only_16b_n8','dealloc_free_only_16b_n9','dealloc_free_only_16b_n16','mimalloc_small_churn_16b','mimalloc_small_churn_16b_2n','large_alloc_free_cycle','alloc_magazine_hit_only_16b','alloc_magazine_prefill_only_16b','dealloc_own_thread_body_only_16b','dealloc_segment_base_of_ptr_probe_only_16b'];
function extract(name){
 const anchor=`fn ${name}(`; const start=source.indexOf(anchor); assert(start>=0);
 const open=source.indexOf('{',start); let depth=1,end=open+1;
 for(;depth;end++){assert(end<source.length);if(source[end]==='{')depth++;if(source[end]==='}')depth--;}
 return source.slice(start,end);
}
const functions=names.map(name=>'#[library_benchmark]\n'+extract(name)).join('\n\n');
writeFileSync('benches/r19_prf02_instrumented_iai.rs',`#[cfg(target_os = "linux")]\nmod gate {\nuse std::alloc::{GlobalAlloc, Layout};\nuse std::hint::black_box;\nuse iai_callgrind::{library_benchmark, library_benchmark_group, main};\nuse mimalloc::MiMalloc;\nuse sefer_alloc::SeferAlloc;\nuse sefer_alloc::registry::{bootstrap, HeapCore, HeapRegistry};\nconst CHURN_OPS: usize = 64;\nconst CHURN_OPS_2N: usize = CHURN_OPS * 2;\nconst MAGAZINE_FILL: usize = 16;\nconst PREFILL_CYCLES: usize = CHURN_OPS / MAGAZINE_FILL;\n${extract('claim_leaked_heap')}\n${functions}\nlibrary_benchmark_group!(name = perf_gate; benchmarks = ${names.join(',')});\nmain!(library_benchmark_groups = perf_gate);\npub fn run(){main();}\n}\n#[cfg(target_os = "linux")]\nfn main(){gate::run();}\n#[cfg(not(target_os = "linux"))]\nfn main(){}\n`);
console.log('Extracted 13 exact function bodies + original lease helper; narrow group only');
