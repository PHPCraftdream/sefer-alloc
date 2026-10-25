import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import assert from 'node:assert/strict';
import {gunzipSync} from 'node:zlib';
const root=process.cwd(),perf=path.join(root,'worktrees/own-free-context/docs/perf'),archive=path.join(perf,'owner_research');
for(const f of ['focused.mjs','wide.mjs','inventory.mjs'])fs.copyFileSync(path.join(root,'.rush/own-free-context',f),path.join(archive,'reproduction',f));
const rows=[];
function walk(dir){for(const e of fs.readdirSync(dir,{withFileTypes:true})){
 const file=path.join(dir,e.name);if(e.isDirectory()){walk(file);continue;}
 if(e.name==='artifact_inventory.json')continue;
 const b=fs.readFileSync(file),raw=file.endsWith('.gz')?gunzipSync(b):b;
 let s=raw.toString('utf8');
 // Шаблон очистки домашнего каталога — не частный путь машины.
 if(file.endsWith('.mjs'))s=s.replaceAll("replaceAll('/root/','~/')",'PATH_NORMALIZATION');
 assert(!/[CD]:[\\/]|\/mnt\/[cd]\/|\/root\//.test(s),`Частный путь: ${file}`);
 assert(b.length<=200*1024,`Артефакт больше 200 KiB: ${file}`);
 assert(raw.length<=2*1024*1024,`Нужен внешний raw storage: ${file}`);
 rows.push({file:path.relative(perf,file).split(path.sep).join('/'),bytes:b.length,uncompressed_bytes:raw.length,sha256:crypto.createHash('sha256').update(b).digest('hex')});
}}
walk(archive);walk(path.join(perf,'own_free_context'));
const inventory={files:rows.length,stored_bytes:rows.reduce((s,r)=>s+r.bytes,0),uncompressed_bytes:rows.reduce((s,r)=>s+r.uncompressed_bytes,0),commits_created:0,sorted_entries_sha256:crypto.createHash('sha256').update(JSON.stringify(rows.sort((a,b)=>a.file.localeCompare(b.file)))).digest('hex')};
fs.writeFileSync(path.join(archive,'artifact_inventory.json'),JSON.stringify(inventory,null,2));
console.log(JSON.stringify({...inventory,entries:undefined},null,2));
