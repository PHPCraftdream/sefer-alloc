import fs from 'node:fs';
import {spawnSync} from 'node:child_process';
const root=process.cwd(),out=root+'/.rush/own-free-context/clear-asm';fs.mkdirSync(out,{recursive:true});
for(const phase of ['before','after']){
 const messages=fs.readFileSync(root+'/.rush/own-free-context/iai-'+phase+'/build.jsonl','utf8').trim().split('\n').map(x=>{try{return JSON.parse(x)}catch{return null}});
 const bin=messages.find(x=>x?.reason==='compiler-artifact'&&x.target?.name==='perf_gate_iai'&&x.executable).executable;
 const p=spawnSync('wsl.exe',['-e','objdump','-d','-C',bin],{encoding:'utf8',maxBuffer:128*1024*1024});if(p.status!==0)throw Error(p.stderr);
 const chunks=p.stdout.split(/(?=^[0-9a-f]+ <)/m).filter(x=>/^.*<(?:perf_gate_iai::)?[^\n]*alloc_clear_magazine_only_16b[^\n]*>:/m.test(x.split('\n')[0]));
 if(!chunks.length)throw Error('missing clear symbols');fs.writeFileSync(out+'/'+phase+'.asm',chunks.join('\n'));
 console.log(phase,chunks.map(x=>x.split('\n')[0]));
}
