import fs from 'node:fs';
import assert from 'node:assert/strict';
const dir='worktrees/own-free-context/docs/perf/own_free_context';
const meta=JSON.parse(fs.readFileSync(dir+'/comparison_identity.json'));
const samples=fs.readFileSync(dir+'/raw/native.ndjson','utf8').trim().split('\n').map(JSON.parse);assert.equal(samples.length,840);
const mean=a=>{assert(a.length);return a.reduce((s,x)=>s+x,0)/a.length;};
const ratio=(a,b)=>{assert(a>0&&b>0);const r=a/b;assert(Math.abs(r*b-a)<a*1e-12);return r;};
function ci(a){assert(a.length===10||a.length===20);const m=mean(a),t=a.length===20?2.093024054:2.262157163;const e=t*Math.sqrt(a.reduce((s,x)=>s+(x-m)**2,0)/(a.length-1)/a.length);return [m-e,m+e];}
const native=[];
for(const [mode,n] of Object.entries(meta.native_plan.modes))for(const metric of ['workload_ns','process_ns']){
 const xs={a:[],b:[],ma:[],mb:[]},diff=[],mdiff=[];
 for(let block=0;block<20;block++){
  const rows=samples.filter(r=>!r.control&&r.mode===mode&&r.block===block).sort((a,b)=>a.position-b.position);assert.deepEqual(rows.map(r=>r.label),meta.native_plan.sequence);
  assert(rows.every(r=>r.steps===n&&r.completed&&r.workload_ns>0&&r.process_ns>r.workload_ns));
  for(const label of Object.keys(xs))xs[label].push(mean(rows.filter(r=>r.label===label).map(r=>r[metric])));
  diff.push(xs.b.at(-1)-xs.a.at(-1));mdiff.push(xs.mb.at(-1)-xs.ma.at(-1));
 }
 const pairs=mode==='rt'?n+768:mode==='16'||mode==='64'?n+16:n*64;
 const a=mean(xs.a),b=mean(xs.b),ma=mean(xs.ma),mb=mean(xs.mb),[low,high]=ci(diff),[miLow,miHigh]=ci(mdiff);
 native.push({mode,metric,pairs,a,b,ma,mb,winBefore:ratio(ma,a),winAfter:ratio(mb,b),ba:ratio(b,a),delta:mean(diff),low,high,miLow,miHigh,verdict:high<0?'LOWER':low>0?'HIGHER':'INCONCLUSIVE'});
}
const aa=[];
for(const metric of ['workload_ns','process_ns']){
 const d=[];for(let block=0;block<10;block++){
  const r=samples.filter(x=>x.control&&x.block===block).sort((a,b)=>a.position-b.position);assert.equal(r.length,4);assert(r.every(x=>x.label==='a'&&x.mode==='rt'));
  d.push(mean([r[1][metric],r[2][metric]])-mean([r[0][metric],r[3][metric]]));
 }const [low,high]=ci(d);aa.push({metric,delta:mean(d),low,high});
}
fs.writeFileSync(dir+'/native_summary.json',JSON.stringify({native,aa},null,2));
console.log(JSON.stringify({workload:native.filter(r=>r.metric==='workload_ns').map(r=>({...r,a:r.a/r.pairs,b:r.b/r.pairs,ma:r.ma/r.pairs,mb:r.mb/r.pairs,low:r.low/r.pairs,high:r.high/r.pairs})),aa},null,2));
