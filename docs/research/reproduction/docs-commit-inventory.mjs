import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import assert from 'node:assert/strict';
import {spawnSync} from 'node:child_process';
const root=process.cwd(),dir='.rush/docs-commit';fs.mkdirSync(dir,{recursive:true});
function git(repo,args){const p=spawnSync('git',args,{cwd:repo,encoding:'utf8',maxBuffer:64*1024*1024});assert.equal(p.status,0,p.stderr);return p.stdout;}
const repos=['.',...fs.readdirSync('worktrees').filter(n=>fs.existsSync(`worktrees/${n}/.git`)).map(n=>'worktrees/'+n)];
const result=[];
for(const repo of repos){
 const tracked=git(repo,['diff','HEAD','--name-only','-z','--','docs']).split('\0').filter(Boolean);
 const fresh=git(repo,['ls-files','--others','--exclude-standard','-z','--','docs']).split('\0').filter(Boolean);
 const ignored=git(repo,['ls-files','--others','--ignored','--exclude-standard','-z','--','docs']).split('\0').filter(Boolean);
 const files=[...new Set([...tracked,...fresh,...ignored])].map(file=>{
 const p=path.join(repo,file);if(!fs.existsSync(p))return {file,deleted:true};const s=fs.statSync(p);if(!s.isFile())return {file,nonfile:true};
 const bytes=fs.readFileSync(p);return {file,bytes:bytes.length,sha256:crypto.createHash('sha256').update(bytes).digest('hex'),tracked:tracked.includes(file),ignored:ignored.includes(file)};
 });
 const row={repo,head:git(repo,['rev-parse','HEAD']).trim(),files};result.push(row);
 console.log(JSON.stringify({repo,tracked:tracked.length,new:fresh.length,ignored:ignored.length,bytes:files.reduce((s,f)=>s+(f.bytes??0),0),large:files.filter(f=>f.bytes>=200*1024).map(f=>({file:f.file,bytes:f.bytes})),top:[...new Set(files.map(f=>f.file.split('/').slice(0,4).join('/')))].slice(0,25)}));
}
fs.writeFileSync(`${dir}/inventory.json`,JSON.stringify(result,null,2));
