// Проверка архива читает данные; ничего не строит и не запускает.
import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import assert from 'node:assert/strict';
import {gunzipSync} from 'node:zlib';
import {fileURLToPath} from 'node:url';
const base=path.dirname(fileURLToPath(import.meta.url));
const inventory=JSON.parse(fs.readFileSync(path.join(base,'inventory.json')));
const hash=b=>crypto.createHash('sha256').update(b).digest('hex');
let files=0,stored=0,logical=0,normalized=0;
for(const tree of inventory.trees){
 const folder=path.join(base,tree.slug),manifestFile=path.join(folder,tree.manifest);
 const manifestBytes=fs.readFileSync(manifestFile);
 const manifest=JSON.parse(tree.manifest.endsWith('.gz')?gunzipSync(manifestBytes).toString():manifestBytes.toString());
 assert.equal(manifest.base,tree.base);assert.equal(manifest.files.length,tree.files);
 let treeStored=0,treeLogical=0,treeNormalized=0;
 for(const item of manifest.files){
 if(item.deleted)continue;
 const file=path.resolve(folder,item.archive);assert(file.startsWith(folder+path.sep));
 const bytes=fs.readFileSync(file),data=item.archive.endsWith('.gz')?gunzipSync(bytes):bytes;
 assert.equal(hash(bytes),item.archived_sha256,file);assert.equal(hash(data),item.archived_logical_sha256,file);
 assert.equal(bytes.length,item.archived_bytes);assert.equal(data.length,item.uncompressed_bytes);
 if(!item.source_path_normalized)assert.equal(hash(data),item.source_logical_sha256);
 const isDocument=item.archive.endsWith('.md');assert(isDocument||data.length<=2*1024*1024,file);assert(isDocument||bytes.length<200*1024,file);
 const text=data.toString('utf8');
 assert(!/(?:[A-Za-z]:(?:\\+|\/)Users(?:\\+|\/)[^\\/"\s]+|\/home\/[^/\s]+\/(?:\.cargo|\.rustup))/.test(text),`Private home: ${item.archive}`);
 assert(!/(?:ghp_[A-Za-z0-9]{30,}|sk-ant-[A-Za-z0-9_-]{30,}|-----BEGIN (?:RSA |EC |OPENSSH )?PRIVATE KEY-----)/.test(text),`Secret marker: ${item.archive}`);
 if(/\.(json|ndjson)$/.test(item.archive.replace(/\.gz$/,''))){
 if(item.archive.replace(/\.gz$/,'').endsWith('.ndjson'))for(const line of text.trim().split('\n').filter(Boolean))JSON.parse(line);
 else JSON.parse(text);
 }
 treeStored+=bytes.length;treeLogical+=data.length;treeNormalized+=Number(item.source_path_normalized);files++;
 }
 assert.equal(treeStored,tree.stored_bytes);assert.equal(treeLogical,tree.uncompressed_bytes);assert.equal(treeNormalized,tree.normalized_files);
 stored+=treeStored;logical+=treeLogical;normalized+=treeNormalized;
}
assert.equal(files,inventory.total_files);assert.equal(stored,inventory.total_stored_bytes);assert.equal(logical,inventory.total_uncompressed_bytes);assert.equal(normalized,inventory.normalized_files);
const readme=fs.readFileSync(path.join(base,'README.md'),'utf8');
for(const tree of inventory.trees){
 const row=readme.split('\n').find(line=>line.startsWith(`| [${tree.slug}](`));assert(row,tree.slug);
 const cells=row.split('|').slice(1,-1).map(s=>s.trim());assert.equal(Number(cells[2]),tree.files);assert.equal(Number(cells[3]),tree.stored_bytes);
}
assert(readme.includes(String(stored))&&readme.includes(String(logical))&&readme.includes(String(files)));
if(fs.existsSync(path.join(base,'supplements.json'))){
 for(const item of JSON.parse(fs.readFileSync(path.join(base,'supplements.json'))))assert.equal(hash(fs.readFileSync(path.join(base,item.archive))),item.sha256,item.archive);
}
console.log(JSON.stringify({archive_verification:'PASS',files,stored_bytes:stored,uncompressed_bytes:logical,normalized_files:normalized},null,2));
