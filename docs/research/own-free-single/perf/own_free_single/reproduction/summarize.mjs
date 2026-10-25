import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
const input=path.resolve(process.argv[2]);
const out=path.resolve(process.argv[3]);
fs.mkdirSync(out,{recursive:true});
function parse(file){
 const text=fs.readFileSync(file,'utf8');
 const total=/([\d,]+)\s+\([^)]*\)\s+PROGRAM TOTALS/.exec(text);
 assert(total,file);const ir=Number(total[1].replaceAll(',',''));
 assert(ir>0,`empty profile ${file}`);
 const groups={};
 for(const line of text.split('-- Auto-annotated source:')[0].split('\n')){
  const m=/^\s*([\d,]+)\s+\([^)]*\)\s+(.+?):(.+)$/.exec(line);
  if(!m)continue;
  const key=m[2];groups[key]=(groups[key]??0)+Number(m[1].replaceAll(',',''));
 }
 assert.equal(Object.values(groups).reduce((a,b)=>a+b,0),ir,`self attribution sum ${file}`);
 return {ir,groups};
}
const rows=[],attribution=[];
for(const pass of [1,2])for(const arm of ['sefer','mimalloc'])for(const mode of ['rt','16','64']){
 const a=parse(path.join(input,`p${pass}-${arm}-${mode}-50000.annotate`));
 const b=parse(path.join(input,`p${pass}-${arm}-${mode}-100000.annotate`));
 const marginal=(b.ir-a.ir)/50000;
 assert(marginal>0);
 rows.push({pass,arm,mode,n1:50000,n2:100000,ir1:a.ir,ir2:b.ir,marginal_ir_per_free:marginal});
 const keys=new Set([...Object.keys(a.groups),...Object.keys(b.groups)]);
 for(const file of keys){const delta=(b.groups[file]??0)-(a.groups[file]??0);if(delta!==0)attribution.push({pass,arm,mode,file,delta_ir:delta,marginal_ir_per_free:delta/50000});}
}
for(const arm of ['sefer','mimalloc'])for(const mode of ['rt','16','64']){
 const r=rows.filter(x=>x.arm===arm&&x.mode===mode);
 assert(Math.abs(r[0].marginal_ir_per_free-r[1].marginal_ir_per_free)<=0.01,`repeatability ${arm}/${mode}`);
}
fs.writeFileSync(path.join(out,`${process.argv[4] ?? 'baseline'}_summary.json`),JSON.stringify({rows,attribution},null,2));
fs.writeFileSync(path.join(out,`${process.argv[4] ?? 'baseline'}_summary.csv`),['pass,arm,mode,n1,n2,ir1,ir2,marginal_ir_per_free',...rows.map(r=>[r.pass,r.arm,r.mode,r.n1,r.n2,r.ir1,r.ir2,r.marginal_ir_per_free].join(','))].join('\n')+'\n');
console.log(JSON.stringify(rows,null,2));
console.log('Sefer runtime self-attribution, pass1:');
console.log(JSON.stringify(attribution.filter(x=>x.arm==='sefer'&&x.mode==='rt'&&x.pass===1).sort((a,b)=>b.delta_ir-a.delta_ir),null,2));
