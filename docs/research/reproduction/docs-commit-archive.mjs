import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';
import crypto from 'node:crypto';
import assert from 'node:assert/strict';
import {gzipSync,gunzipSync} from 'node:zlib';
const input=JSON.parse(fs.readFileSync('.rush/docs-commit/inventory.json'));
const root=process.cwd(),out='docs/research';assert(!fs.existsSync(out),'Не перезаписывать архив');fs.mkdirSync(out,{recursive:true});
const hash=b=>crypto.createHash('sha256').update(b).digest('hex');
const slash=root.replaceAll('\\','/'),repository=slash.split('/worktrees/')[0];
const prefixes=[root,slash,repository,repository.replaceAll('/','\\'),os.homedir(),os.homedir().replaceAll('\\','/')];
for(const s of [slash,repository])prefixes.push('/mnt/'+s[0].toLowerCase()+s.slice(2));
for(const p of [...prefixes])if(p.includes('\\'))prefixes.push(p.replaceAll('\\','\\\\'));
function normalize(text){
 let result=text;for(const p of [...new Set(prefixes)].sort((a,b)=>b.length-a.length))result=result.split(p).join('<workspace>');
 result=result.replace(/\/home\/[^/\s"']+(?=\/\.cargo|\/\.rustup)/g,'<home>');
 result=result.replace(/[A-Za-z]:(?:\\+|\/)Users(?:\\+|\/)[^\\/"\s]+/g,'<home>');
 return result;
}
const summaries=[];
for(const r of input.filter(r=>r.repo!=='.'&&r.files.length)){
 const slug=path.basename(r.repo),base=`${out}/${slug}`,manifest=[];let storedBytes=0,uncompressedBytes=0,normalizedFiles=0;
 for(const f of r.files){
 assert(f.file.startsWith('docs/')&&!f.file.includes('..'));assert(!f.nonfile);
 if(f.deleted){manifest.push({source:f.file,deleted:true});continue;}
 const bytes=fs.readFileSync(path.join(r.repo,f.file));assert.equal(hash(bytes),f.sha256,`Source drift: ${r.repo}/${f.file}`);
 const wasGzip=f.file.endsWith('.gz'),logical=wasGzip?gunzipSync(bytes):bytes;
 const text=logical.toString('utf8');assert(Buffer.from(text).equals(logical),`Неожиданный binary document: ${f.file}`);
 const normalized=Buffer.from(normalize(text)),changed=!normalized.equals(logical);if(changed)normalizedFiles++;
 const isDocument=f.file.endsWith('.md');assert(isDocument||normalized.length<=2*1024*1024,`Нужен tier3: ${f.file}`);
 const extraGzip=!wasGzip&&!isDocument&&normalized.length>=200*1024;
 const relative=f.file.slice('docs/'.length)+(extraGzip?'.gz':''),destination=`${base}/${relative}`;
 let stored;if(wasGzip||extraGzip)stored=changed||extraGzip?gzipSync(normalized):bytes;else stored=normalized;
 assert(isDocument||stored.length<200*1024,`Слишком большой artifact: ${f.file}`);
 fs.mkdirSync(path.dirname(destination),{recursive:true});assert(!fs.existsSync(destination),destination);fs.writeFileSync(destination,stored);
 manifest.push({source:f.file,archive:relative,source_sha256:f.sha256,source_bytes:bytes.length,source_logical_sha256:hash(logical),archived_sha256:hash(stored),archived_logical_sha256:hash(normalized),source_path_normalized:changed,extra_gzip:extraGzip,archived_bytes:stored.length,uncompressed_bytes:normalized.length});
 storedBytes+=stored.length;uncompressedBytes+=normalized.length;
 }
 const manifestText=JSON.stringify({source_tree:r.repo,base:r.head,scope:'Изменённые, новые и ignored docs; product source не переносился',path_normalization:'Только machine-specific workspace/home prefixes; originals сохранены в source tree',files:manifest},null,2)+'\n';
 const manifestGzip=Buffer.byteLength(manifestText)>=200*1024,manifestName=manifestGzip?'manifest.json.gz':'manifest.json';fs.writeFileSync(`${base}/${manifestName}`,manifestGzip?gzipSync(manifestText):manifestText);
 summaries.push({slug,base:r.head,files:manifest.length,stored_bytes:storedBytes,uncompressed_bytes:uncompressedBytes,normalized_files:normalizedFiles,manifest:manifestName});
}
fs.writeFileSync(`${out}/inventory.json`,JSON.stringify({scope:'Owner-path research documents only; no runtime promotion',trees:summaries,total_files:summaries.reduce((s,r)=>s+r.files,0),total_stored_bytes:summaries.reduce((s,r)=>s+r.stored_bytes,0),total_uncompressed_bytes:summaries.reduce((s,r)=>s+r.uncompressed_bytes,0),normalized_files:summaries.reduce((s,r)=>s+r.normalized_files,0)},null,2)+'\n');
fs.writeFileSync(`${out}/input-inventory.json.gz`,gzipSync(JSON.stringify(input,null,2)));
console.log(JSON.stringify(summaries,null,2));
