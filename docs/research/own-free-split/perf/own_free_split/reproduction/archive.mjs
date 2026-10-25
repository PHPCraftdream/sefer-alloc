import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import crypto from 'node:crypto';
import {gzipSync} from 'node:zlib';
const root=process.cwd();
const dest=path.join(root,'worktrees/own-free-split/docs/perf/own_free_split');
const slash=p=>p.replaceAll(String.fromCharCode(92),'/');
const win=slash(root),linux='/mnt/'+win[0].toLowerCase()+win.slice(2);
function clean(s){for(const prefix of [root,win,linux])s=s.split(prefix).join('<workspace>');return s.replaceAll('/root/','~/');}
function calls(text,part){
 const names=new Map();
 for(const line of text.split('\n')){const m=/^(?:c)?fn=\((\d+)\) (.+)$/.exec(line);if(m)names.set(m[1],m[2]);}
 let target=null,total=0;
 for(const line of text.split('\n')){
  const c=/^cfn=\((\d+)\)/.exec(line);if(c)target=c[1];
  const n=/^calls=(\d+)/.exec(line);if(n&&names.get(target)?.includes(part))total+=Number(n[1]);
 }
 return total;
}
const activation=[];
for(const phase of ['baseline','candidate','control-before','control-after']){
 const source=path.join(root,'.rush/own-free-split',phase),out=path.join(dest,'raw',phase);fs.mkdirSync(out,{recursive:true});
 for(const file of fs.readdirSync(source)){
  if(!/\.(callgrind|annotate|stdout|stderr)$/.test(file))continue;
  const bytes=fs.readFileSync(path.join(source,file));
  assert(bytes.length<=2*1024*1024,`artifact needs excerpt tier: ${phase}/${file}`);
  let text=clean(bytes.toString('utf8'));
  if(file.endsWith('.callgrind'))text='# Source/build paths normalized; numeric data unchanged.\n'+text;
  if(Buffer.byteLength(text)>200*1024)fs.writeFileSync(path.join(out,file+'.gz'),gzipSync(text));else fs.writeFileSync(path.join(out,file),text);
  if(phase.startsWith('control-')&&file.includes('-sefer-')&&file.endsWith('.callgrind')){
   const m=/^p(\d)-sefer-(bulk64|bulk16k)-(100|200)\.callgrind$/.exec(file);assert(m);
   const n=Number(m[3]),perRound=m[2]==='bulk64'?6:60;
   const actual=calls(bytes.toString('utf8'),'flush_class_internal');
   activation.push({phase,file,event:'flush_class_internal',expected:n*perRound,actual});
   if(phase==='control-after')activation.push({phase,file,event:'dealloc_full_magazine',expected:m[2]==='bulk64'?n*perRound:0,actual:calls(bytes.toString('utf8'),'dealloc_full_magazine')});
  }
 }
 if(fs.existsSync(path.join(source,'toolchain.log')))fs.writeFileSync(path.join(dest,`${phase}_toolchain.log`),clean(fs.readFileSync(path.join(source,'toolchain.log'),'utf8')));
}
fs.writeFileSync(path.join(dest,'activation.json'),JSON.stringify(activation,null,2));
assert(activation.every(r=>r.actual===r.expected),'activation oracle mismatch');
for(const file of fs.readdirSync(dest))if(file.endsWith('.patch')){const p=path.join(dest,file);fs.writeFileSync(p+'.gz',gzipSync(fs.readFileSync(p)));fs.unlinkSync(p);}
const reproduction=path.join(dest,'reproduction');fs.mkdirSync(reproduction,{recursive:true});
for(const file of ['profile.sh','profile.mjs','controls.sh','compare.mjs','summarize.mjs','archive.mjs','source-check.mjs'])fs.copyFileSync(path.join(root,'.rush/own-free-split',file),path.join(reproduction,file));
console.log('Archived profiles, source patches and runners; activation oracle passed.');
