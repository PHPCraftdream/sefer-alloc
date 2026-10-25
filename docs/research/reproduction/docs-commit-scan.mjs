import fs from 'node:fs';
import path from 'node:path';
import {gunzipSync} from 'node:zlib';
const repos=JSON.parse(fs.readFileSync('.rush/docs-commit/inventory.json'));
const root=process.cwd(),prefix=root.replaceAll('\\','/').split('/worktrees/')[0];
const results=[];
for(const r of repos){let uncompressed=0,oversized=[],privateFiles=[];
 for(const f of r.files){if(f.deleted||f.nonfile)continue;const b=fs.readFileSync(path.join(r.repo,f.file)),data=f.file.endsWith('.gz')?gunzipSync(b):b;uncompressed+=data.length;
 if(data.length>2*1024*1024)oversized.push({file:f.file,bytes:data.length});
 const text=data.toString('utf8'),flags=[];
 if(text.includes(prefix)||text.includes(prefix.replaceAll('/','\\'))||text.includes('/mnt/'+prefix[0].toLowerCase()+prefix.slice(2)))flags.push('workspace');
 if(/(?:[A-Za-z]:[\\/]Users[\\/][^\\/\s]+|\/home\/[^/\s]+\/\.cargo)/.test(text))flags.push('private-home');
 if(flags.length)privateFiles.push({file:f.file,flags});
 }
 const row={repo:r.repo,files:r.files.length,uncompressed,oversized,privateFiles};results.push(row);console.log(JSON.stringify({...row,privateFiles:privateFiles.slice(0,12),privateCount:privateFiles.length}));
}
fs.writeFileSync('.rush/docs-commit/scan.json',JSON.stringify(results,null,2));
