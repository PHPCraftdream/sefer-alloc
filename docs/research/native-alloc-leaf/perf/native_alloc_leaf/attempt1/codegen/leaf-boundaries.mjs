import fs from 'node:fs';
import assert from 'node:assert/strict';
const dir='.rush/native-owner',id=JSON.parse(fs.readFileSync(`${dir}/leaf-symbols.json`));
const main=id.symbols.find(s=>s.name.includes('own_free_wall4main'));assert(main);
const text=fs.readFileSync(`${dir}/leaf-full.asm`,'utf8');
const lines=text.split('\n').flatMap(text=>{const m=/^\s*([0-9a-f]+):\s+((?:[0-9a-f]{2}\s+)+)\s*(.*)$/.exec(text);return m?[{text,rva:parseInt(m[1],16)-id.image_base,op:m[3].trim()}]:[];});
const hits=lines.flatMap((l,i)=>l.rva<main.rva&&l.op.includes('%gs:0x58')?[i]:[]);
const results=[];
for(const i of hits){let begin=i;while(begin>0&&lines[begin-1].op!=='int3')begin--;let end=i;while(end<lines.length&&lines[end].op!=='int3')end++;assert(begin>0&&end<lines.length);
const code=lines.slice(begin,end),start=code[0].rva,file=`${dir}/leaf-exact-${start.toString(16)}.asm`;
fs.writeFileSync(file,code.map(l=>l.text).join('\n')+'\n',{flag:'wx'});
results.push({start,end:lines[end].rva,file,bytes:lines[end].rva-start,prologue:code.slice(0,10).map(l=>l.op),calls:code.filter(l=>/^call\b/.test(l.op)).map(l=>l.op),stack:code.filter(l=>/^(push|pop)\b|\b(add|sub)\s+.*%rsp/.test(l.op)).map(l=>l.op)});
}
assert.equal(results.length,6);
fs.writeFileSync(`${dir}/leaf-exact-functions.json`,JSON.stringify({sha256:id.sha256,boundary:'int3-delimited functions; full body and main address references require manual review',functions:results},null,2),{flag:'wx'});
console.log(JSON.stringify(results,null,2));
