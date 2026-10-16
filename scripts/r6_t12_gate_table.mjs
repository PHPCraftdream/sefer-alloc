// R6-T12 gate table: derived from the summary CSV; asserts B - A = delta per row.
// Usage: node scripts/r6_t12_gate_table.mjs docs/perf/R6_T12_OWNER_DRAIN_CONTEXT_GATE_summary.csv
import fs from 'node:fs';const rows=fs.readFileSync(process.argv[2],'utf8').trim().split('\n').slice(1).map(l=>l.split(','));
const want=['r19_prf01_publication','seg_cycle_decommit_256k','multiseg_cold_256k','growth16_n','growth16_2n','growth16_4n','cold_alloc_only_256x16b','dealloc_free_only_16b','small_churn_16b'];
console.log('| cell | A Ir | B Ir | delta Ir | delta / A |');console.log('|---|---:|---:|---:|---:|');
for(const w of want){const r=rows.find(x=>x[0]===w);const a=+r[1],b=+r[2],d=+r[3];if(b-a!==d)throw new Error('delta mismatch '+w);
console.log(`| \`${w}\` | ${a} | ${b} | ${d>0?'+':''}${d} | ${d>0?'+':''}${d} / ${a} = ${(100*d/a).toFixed(2)} % |`);}
const pos=rows.filter(r=>+r[3]>0).length,neg=rows.filter(r=>+r[3]<0).length,zero=rows.length-pos-neg;
console.log(`\ncells: ${rows.length}; delta > 0: ${pos}; delta < 0: ${neg}; delta = 0: ${zero}`);
