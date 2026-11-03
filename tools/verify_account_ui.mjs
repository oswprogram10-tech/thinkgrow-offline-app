// Synthetic browser checks only. No account keys, network service or installer runs.
import assert from 'node:assert/strict';
import {createServer} from 'node:http';
import {spawn} from 'node:child_process';
import {existsSync, mkdirSync, mkdtempSync, readFileSync, writeFileSync, rmSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {dirname, join, resolve, basename} from 'node:path';
import {fileURLToPath} from 'node:url';
const root=dirname(dirname(fileURLToPath(import.meta.url)));
const edge=join(process.env['ProgramFiles(x86)'] || 'C:/Program Files (x86)','Microsoft/Edge/Application/msedge.exe');
const output=join(root,'target/account-ui-validation'); mkdirSync(output,{recursive:true});
const profile=mkdtempSync(join(tmpdir(),'thinkgrow-native-ui-'));
const types={'index.html':'text/html; charset=utf-8','app.css':'text/css','app.js':'text/javascript','student.js':'text/javascript'};
const server=createServer((request,response)=>{
  const name=request.url==='/'?'index.html':request.url.slice(1);
  if(!Object.hasOwn(types,name)){response.writeHead(404);response.end();return;}
  response.writeHead(200,{'Content-Type':types[name]}); response.end(readFileSync(join(root,'target/ui/student',name)));
});
await new Promise(r=>server.listen(0,'127.0.0.1',r));
const base='http://127.0.0.1:'+server.address().port;
let browser,ws,id=0; const pending=new Map();
const send=(method,params={})=>new Promise((resolve,reject)=>{pending.set(++id,{resolve,reject});ws.send(JSON.stringify({id,method,params}));});
const evaluate=async expression=>{const result=await send('Runtime.evaluate',{expression,returnByValue:true,awaitPromise:true});if(result.exceptionDetails)throw Error(JSON.stringify(result.exceptionDetails));return result.result.value;};
const sleep=ms=>new Promise(r=>setTimeout(r,ms));
let passed=0;
try{
  browser=spawn(edge,['--headless','--disable-extensions','--disable-gpu','--no-first-run','--remote-debugging-port=0','--user-data-dir='+profile,'about:blank'],{windowsHide:true,stdio:'ignore'});
  for(let n=0;!existsSync(join(profile,'DevToolsActivePort'));n++){if(n>100)throw Error('Browser did not start');await sleep(100);}
  const port=readFileSync(join(profile,'DevToolsActivePort'),'utf8').split('\n')[0];
  const tabs=await(await fetch('http://127.0.0.1:'+port+'/json')).json();
  ws=new WebSocket(tabs.find(tab=>tab.type==='page'&&tab.url==='about:blank').webSocketDebuggerUrl); await new Promise(r=>ws.addEventListener('open',r,{once:true}));
  ws.addEventListener('message',e=>{const m=JSON.parse(e.data);if(m.id){const p=pending.get(m.id);pending.delete(m.id);m.error?p.reject(m.error):p.resolve(m.result);}});
  await send('Page.enable');
  let script;
  async function load(configuration,width=1100,height=1100){
    if(script)await send('Page.removeScriptToEvaluateOnNewDocument',{identifier:script});
    const source=`window.__fixture=${JSON.stringify(configuration)};window.__TAURI__={core:{invoke:async(command,args)=>{
      const c=window.__fixture;const local=c.unlocked?{access:c.expired?'read_recovery_only':'read_write',drafts:2,pending_classroom:1,pending_online:1,installed_games:0}:null;
      if(command==='account_status')return {platform_supported:c.supported!==false,service_configured:!!c.configured,accounts:c.accounts?[{handle:'synthetic-account',label:'합성 학생',enrolled:true}]:[]};
      if(command==='unlock_account'){if(args.pin!=='246810')throw Error('account unlock failed or temporarily locked; data preserved');c.unlocked=true;return {local:{access:c.expired?'read_recovery_only':'read_write',drafts:2,pending_classroom:1,pending_online:1,installed_games:0}};}
      if(command==='list_lessons')return [];
      if(command==='lock_account'){c.unlocked=false;return {local:null};} if(command==='core_status')return {local};throw Error('Unexpected command');
    }}};`;
    script=(await send('Page.addScriptToEvaluateOnNewDocument',{source})).identifier;
    await send('Emulation.setDeviceMetricsOverride',{width,height,deviceScaleFactor:1,mobile:width<700});
    await send('Page.navigate',{url:base});
    for(let n=0;n<100;n++){if(await evaluate(`!!document.getElementById('notice')&&!document.getElementById('notice').textContent.includes('확인하고 있습니다')`))break;await sleep(50);}
  }
  await load({configured:false});
  assert.equal(await evaluate(`document.querySelector('#register-form fieldset').disabled`),true);
  assert.match(await evaluate(`document.getElementById('notice').textContent`),/준비 중/);passed++;
  await send('Page.captureScreenshot',{format:'png',captureBeyondViewport:true}).then(s=>writeFileSync(join(output,'desktop.png'),Buffer.from(s.data,'base64')));
  await load({configured:true,accounts:true,expired:true},390,1100);
  assert.equal(await evaluate('document.documentElement.scrollWidth<=innerWidth'),true);passed++;
  await evaluate(`document.getElementById('unlock-pin').value='000000';document.getElementById('unlock-form').requestSubmit()`);
  await sleep(300);
  assert.equal(await evaluate(`document.getElementById('unlock-pin').value`),'');
  assert.match(await evaluate(`document.getElementById('notice').textContent`),/숫자코드/);passed++;
  await evaluate(`document.getElementById('unlock-pin').value='246810';document.getElementById('unlock-form').requestSubmit()`);
  await sleep(300);
  assert.equal(await evaluate(`document.getElementById('workspace').hidden`),false);
  assert.match(await evaluate(`document.getElementById('access-state').textContent`),/읽기·복구/);passed++;
  await evaluate(`document.getElementById('lock').click()`);await sleep(300);
  assert.equal(await evaluate(`document.getElementById('workspace').hidden`),true);passed++;
  await send('Page.captureScreenshot',{format:'png',captureBeyondViewport:true}).then(s=>writeFileSync(join(output,'mobile.png'),Buffer.from(s.data,'base64')));
  await load({supported:false});
  assert.equal(await evaluate(`document.querySelector('#unlock-form fieldset').disabled`),true);
  assert.equal(await evaluate(`document.querySelector('#register-form fieldset').disabled`),true);passed++;
  const report={status:'passed',checks:passed,synthetic_only:true,native_runtime_test:false,production_server_test:false};
  writeFileSync(join(output,'report.json'),JSON.stringify(report,null,2)+'\n'); console.log(JSON.stringify(report));
}finally{
  if(ws?.readyState===1)await send('Browser.close').catch(()=>{});
  if(browser?.exitCode===null)browser.kill();
  server.close();
  if(resolve(dirname(profile))===resolve(tmpdir())&&basename(profile).startsWith('thinkgrow-native-ui-')){
    for(let n=0;n<10;n++){try{rmSync(profile,{recursive:true,force:true});break;}catch{await sleep(200);}}
  }
}
