import {spawnSync} from 'node:child_process';
const root=process.cwd();
const linux='/mnt/'+root[0].toLowerCase()+root.slice(2).replaceAll(String.fromCharCode(92),'/');
const p=spawnSync('wsl.exe',['-e','bash','-lc','exec bash "$@"','bash',linux+'/.rush/own-free-context/alloc-sanity.sh',linux],{stdio:'inherit'});
if(p.error)throw p.error;
process.exit(p.status??1);
