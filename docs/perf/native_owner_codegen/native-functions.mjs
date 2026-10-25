import fs from 'node:fs';
import crypto from 'node:crypto';
import assert from 'node:assert/strict';
import {spawnSync} from 'node:child_process';
const results=[];
for(const phase of ['before','after']){
 const id=JSON.parse(fs.readFileSync(`.rush/native-owner/${phase}-symbols.json`,'utf8'));
 const bytes=fs.readFileSync(id.image);assert.equal(crypto.createHash('sha256').update(bytes).digest('hex'),id.sha256);
 const old=JSON.parse(fs.readFileSync('worktrees/own-free-context/docs/perf/own_free_context/native_binaries.json','utf8'));assert.equal(id.sha256,old[phase]);
 const pe=bytes.readUInt32LE(0x3c);assert.equal(bytes.toString('ascii',pe,pe+4),'PE\0\0');
 const sections=bytes.readUInt16LE(pe+6),optional=pe+24,optionalSize=bytes.readUInt16LE(pe+20);assert.equal(bytes.readUInt16LE(optional),0x20b);assert(sections>0&&sections<64);
 const sectionRows=[];for(let i=0;i<sections;i++){const s=optional+optionalSize+i*40;sectionRows.push({virtual:bytes.readUInt32LE(s+12),size:bytes.readUInt32LE(s+16),raw:bytes.readUInt32LE(s+20)});}
 const raw=rva=>{const s=sectionRows.find(s=>rva>=s.virtual&&rva<s.virtual+s.size);assert(s);return s.raw+rva-s.virtual;};
 const exRva=bytes.readUInt32LE(optional+112+3*8),exSize=bytes.readUInt32LE(optional+116+3*8);assert.equal(exSize%12,0);const ex=raw(exRva),funcs=[];
 for(let i=0;i<exSize;i+=12)funcs.push({start:bytes.readUInt32LE(ex+i),end:bytes.readUInt32LE(ex+i+4)});
 const main=id.symbols.find(s=>s.name.includes('own_free_wall4main'));assert(main);
 const image=process.cwd()+'/'+id.image,linux='/mnt/'+image[0].toLowerCase()+image.slice(2).replaceAll(String.fromCharCode(92),'/');
 const p=spawnSync('wsl.exe',['-e','objdump','-d',linux],{encoding:'utf8',maxBuffer:32*1024*1024});assert.equal(p.status,0,p.stderr);
 const lines=p.stdout.split('\n').map(text=>({text,rva:(()=>{const m=/^\s*([0-9a-f]+):/.exec(text);return m?parseInt(m[1],16)-id.image_base:null;})()}));
 const hits=lines.filter(l=>l.rva!==null&&l.rva<main.rva&&l.text.includes('%gs:0x58'));
 const found=[];
 for(const hit of hits){const fn=funcs.find(f=>hit.rva>=f.start&&hit.rva<f.end);assert(fn);if(found.some(f=>f.start===fn.start))continue;
  const code=lines.filter(l=>l.rva!==null&&l.rva>=fn.start&&l.rva<fn.end).map(l=>l.text),file=`.rush/native-owner/${phase}-${fn.start.toString(16)}.asm`;
  fs.writeFileSync(file,code.join('\n')+'\n');
  found.push({...fn,bytes:fn.end-fn.start,file,prologue:code.slice(0,12)});
 }
 results.push({phase,sha256:id.sha256,tlsWrappers:found});
}
fs.writeFileSync('.rush/native-owner/functions.json',JSON.stringify(results,null,2));
for(const r of results){console.log(r.phase);for(const f of r.tlsWrappers)console.log(JSON.stringify({start:f.start.toString(16),bytes:f.bytes,prologue:f.prologue},null,2));}
