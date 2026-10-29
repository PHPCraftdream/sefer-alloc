import {spawnSync} from 'node:child_process';
const root=process.cwd().replaceAll(String.fromCharCode(92),'/');
const wsl='/mnt/'+root[0].toLowerCase()+root.slice(2);
const p=spawnSync('wsl.exe',['-e','bash','-lc','exec bash "$@"','bash',wsl+'/.rush/own-free-split/sanity.sh',wsl],{stdio:'inherit'});
process.exit(p.status??1);
