import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import {gzipSync} from 'node:zlib';
const root=process.cwd(),source=path.join(root,'.rush/own-free-split/sanity'),dir=path.join(root,'worktrees/own-free-split/docs/perf/own_free_split');
const raw=path.join(dir,'raw/sanity');fs.mkdirSync(raw,{recursive:true});
const slash=p=>p.replaceAll(String.fromCharCode(92),'/'),win=slash(root),linux='/mnt/'+win[0].toLowerCase()+win.slice(2);
function clean(s){for(const p of [root,win,linux])s=s.split(p).join('<workspace>');return s.replaceAll('/root/','~/');}
const summary={method:'full-program callgrind without toggle; same control binaries',phases:{}};
const codegen={};
for(const phase of ['before','after']){
 const vals=[100,200].map(n=>{const s=fs.readFileSync(path.join(source,`${phase}-${n}.annotate`),'utf8'),m=/([\d,]+)\s+\([^)]*\)\s+PROGRAM TOTALS/.exec(s);assert(m);return Number(m[1].replaceAll(',',''));});
 const binary=fs.readFileSync(path.join(source,`${phase}.binary.sha256`),'utf8').slice(0,64);
 assert.match(binary,/^[0-9a-f]{64}$/);
 assert.equal(binary,fs.readFileSync(path.join(root,`.rush/own-free-split/control-${phase}/binary.sha256`),'utf8').slice(0,64));
 summary.phases[phase]={ir100:vals[0],ir200:vals[1],marginal_ir_per_pair:(vals[1]-vals[0])/6400,binary_sha256:binary};
 const symbols=fs.readFileSync(path.join(source,`${phase}.symbols`),'utf8');
 const row=symbols.split('\n').find(x=>x.includes('>::flush_class_internal')).trim().split(/\s+/);
 const addr=Number.parseInt(row[0],16),bytes=Number.parseInt(row[1],16);
 const caller=fs.readFileSync(path.join(source,`${phase}.caller-asm.txt`),'utf8');
 const sites=caller.split('\n').filter(x=>x.includes('call')&&x.includes('flush_class_internal'));
 assert(sites.length>0&&sites.every(x=>new RegExp('call\\s+'+addr.toString(16)+'\\s+').test(x)));
 codegen[phase]={active_callee_address:addr.toString(16),active_callee_bytes:bytes,caller_sites:sites.map(x=>x.trim()),note:phase==='after'?'direct block argument, one-element specialized body':'slice+length argument, generic batch loop and recycled array'};
}
fs.writeFileSync(path.join(dir,'sanity_summary.json'),JSON.stringify(summary,null,2));
fs.writeFileSync(path.join(dir,'codegen_summary.json'),JSON.stringify(codegen,null,2));
for(const f of fs.readdirSync(source)){
 if(f.endsWith('.binary.sha256'))continue;
 const b=Buffer.from(clean(fs.readFileSync(path.join(source,f),'utf8')));
 assert(b.length<=2*1024*1024);
 fs.writeFileSync(path.join(raw,f+(b.length>200*1024?'.gz':'')),b.length>200*1024?gzipSync(b):b);
}
for(const f of ['sanity.sh','sanity.mjs','archive-sanity.mjs'])fs.copyFileSync(path.join(root,'.rush/own-free-split',f),path.join(dir,'reproduction',f));
console.log(JSON.stringify({summary,codegen},null,2));
