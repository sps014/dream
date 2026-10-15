import {chromium} from 'playwright';
import {spawn,execFileSync} from 'node:child_process';
import {mkdir,writeFile} from 'node:fs/promises';
import {createWriteStream} from 'node:fs';
import {fileURLToPath} from 'node:url';
import path from 'node:path';

const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
const output=path.join(root,'target/stdlib-v1-browser');
await mkdir(output,{recursive:true});
const report={revision:execFileSync('git',['rev-parse','HEAD'],{cwd:root,encoding:'utf8'}).trim(),passed:false,started:new Date().toISOString(),steps:[]};
const save=()=>writeFile(path.join(output,'report.json'),JSON.stringify(report,null,2)+'\n');
await save();
let browser;
try {
 browser=await chromium.launch({headless:true});
 for(const release of [false,true]) {
  const port=release?61091:61090;
  const log=createWriteStream(path.join(output,release?'release-server.log':'debug-server.log'));
  const server=spawn(process.env.PYTHON||'python3',['scripts/serve-phase34-browser.py','--port',String(port),...(release?['--release']:[])],{cwd:root,stdio:['ignore','pipe','pipe']});
  server.stdout.pipe(log);server.stderr.pipe(log);
  try {
   const deadline=Date.now()+1200000;
   while(true) {
    if(server.exitCode!==null) throw new Error(`browser fixture server exited ${server.exitCode}`);
    try {if((await fetch(`http://127.0.0.1:${port}/target/phase34-browser/utilities.html`)).ok) break;}catch{}
    if(Date.now()>deadline) throw new Error('browser fixture compilation/startup timed out');
    await new Promise(resolve=>setTimeout(resolve,500));
   }
   for(const name of ['http','io-url','cancellation','cancellation-worker','cli','structured','channels-worker','utilities']) {
    const page=await browser.newPage();
    const step={case:name,profile:release?'release':'debug',passed:false};report.steps.push(step);await save();
    try {
     await page.goto(`http://127.0.0.1:${port}/target/phase34-browser/${name}.html`);
     await page.locator('#result').filter({hasText:/^(PASS|FAIL):/}).waitFor({timeout:120000});
     step.result=await page.locator('#result').innerText();
     step.passed=step.result===`PASS: ${name}`;
     if(!step.passed) throw new Error(step.result);
     console.log(`PASS ${step.profile} ${name}`);
    }catch(error){step.error=String(error);await page.screenshot({path:path.join(output,`${step.profile}-${name}.png`)});throw error;}
    finally{await save();await page.close();}
   }
  }finally{server.kill();log.end();}
 }
 report.passed=true;report.finished=new Date().toISOString();await save();
}finally{await browser?.close();await save();}
