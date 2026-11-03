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
const output=join(root,'target/student-ui-validation'); mkdirSync(output,{recursive:true});
const profile=mkdtempSync(join(tmpdir(),'thinkgrow-student-ui-'));
const types={'index.html':'text/html; charset=utf-8','app.css':'text/css','app.js':'text/javascript','student.js':'text/javascript','materials.js':'text/javascript','games.js':'text/javascript','roles.js':'text/javascript'};
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
  const now=Date.now(), item=(id,text)=>({id,text});
  const contents=[{kind:'essay',min_length:0,max_length:100000},{kind:'true_false'},{kind:'multiple_choice',options:[item('a','첫 선택'),item('b','두 번째')]},{kind:'matching',left:[item('l','왼쪽')],right:[item('r','오른쪽')]},{kind:'ordering',items:[item('first','첫 항목'),item('second','두 번째')]},{kind:'short_answer',max_length:10000},{kind:'fill_in_blank',blanks:['blank-a','blank-b']}];
  const fixtures=contents.map((content,i)=>({protocol:1,app_id:'com.thinkgrow.offline.student',subject_id:'synthetic',device_id:'synthetic-device',credential_epoch:1,issued_ms:now-1000,expires_ms:now+86400000,package_revision:String(i).repeat(64),school_id:'school',class_id:'class',assignment_id:'assignment-'+i,title:'수업 '+i,prompt:'문항 내용 <script>window.__injected=true</script>',hint:'도움말',content,starts_ms:null,due_ms:null,allow_late:false}));
  async function load(configuration={},width=1100,height=1300){
    if(script)await send('Page.removeScriptToEvaluateOnNewDocument',{identifier:script});
    const source=`window.__fixture=${JSON.stringify({lessons:fixtures,drafts:{},unlocked:true,...configuration})};window.__TAURI__={core:{invoke:async(command,args)=>{
      const c=window.__fixture,local=c.unlocked?{access:c.expired?'read_recovery_only':'read_write',drafts:Object.keys(c.drafts).length,pending_classroom:0,pending_online:0,installed_games:0}:null;
      if(command==='account_status')return {platform_supported:true,service_configured:true,accounts:[{handle:'synthetic-account',label:'검사용 학생',enrolled:true}]};
      if(command==='core_status')return {local};if(command==='list_lessons')return c.lessons;
      if(command==='prepare_lessons'){if(c.offline)throw Error('offline');return c.lessons;}
      if(command==='load_draft')return c.drafts[args.assignment]||null;
      if(command==='save_draft'){if(c.failSaving)throw Error('revision conflict');const old=c.drafts[args.answer.assignment_id];if((old?.revision||0)!==args.expectedRevision)throw Error('conflict');const revision=args.expectedRevision+1;c.drafts[args.answer.assignment_id]={revision,answer:JSON.parse(JSON.stringify(args.answer))};return revision;}
      if(command==='queue_submission'){c.queued=JSON.parse(JSON.stringify(c.drafts[args.assignment]));return {operation:{}};}
      if(command==='lock_account'){c.unlocked=false;return {local:null};}if(command==='unlock_account'){c.unlocked=true;return {local:{access:'read_write',drafts:Object.keys(c.drafts).length,pending_classroom:0,pending_online:0,installed_games:0}};}throw Error('Unexpected command '+command);
    }}};`;
    script=(await send('Page.addScriptToEvaluateOnNewDocument',{source})).identifier;
    await send('Emulation.setDeviceMetricsOverride',{width,height,deviceScaleFactor:1,mobile:width<700});await send('Page.navigate',{url:base});
    for(let n=0;n<100;n++){if(await evaluate(`document.querySelectorAll('#lesson-list button').length===7`))break;await sleep(50);}
    assert.equal(await evaluate(`document.querySelectorAll('#lesson-list button').length`),7);
  }
  async function open(index){await evaluate(`document.querySelectorAll('#lesson-list button')[${index}].click()`);await sleep(100);assert.equal(await evaluate(`document.getElementById('lesson-title').textContent`),'수업 '+index);}
  async function edit(expression){await evaluate(expression);await sleep(750);}
  await load();await open(0);
  assert.equal(await evaluate('window.__injected'),undefined);assert.match(await evaluate(`document.getElementById('lesson-prompt').textContent`),/<script>/);passed++;
  await edit(`document.getElementById('answer-text').value='자동 저장한 답안';document.getElementById('answer-text').dispatchEvent(new Event('input',{bubbles:true}))`);
  assert.equal(await evaluate(`JSON.parse(window.__fixture.drafts['assignment-0'].answer.body).body`),'자동 저장한 답안');passed++;
  await evaluate(`document.getElementById('answer-form').requestSubmit()`);await sleep(150);
  await edit(`document.getElementById('answer-text').value='새 초안';document.getElementById('answer-text').dispatchEvent(new Event('input',{bubbles:true}))`);
  assert.equal(await evaluate(`JSON.parse(window.__fixture.queued.answer.body).body`),'자동 저장한 답안');assert.equal(await evaluate(`JSON.parse(window.__fixture.drafts['assignment-0'].answer.body).body`),'새 초안');passed++;
  await open(1);await edit(`document.getElementById('choice-1').click()`);assert.equal(await evaluate(`JSON.parse(window.__fixture.drafts['assignment-1'].answer.body).structured_answer.selected`),false);passed++;
  await open(2);await edit(`document.getElementById('choice-0').click()`);assert.equal(await evaluate(`JSON.parse(window.__fixture.drafts['assignment-2'].answer.body).structured_answer.selected_option_id`),'a');passed++;
  await open(3);await edit(`document.getElementById('match-0').value='r';document.getElementById('match-0').dispatchEvent(new Event('change',{bubbles:true}))`);assert.deepEqual(await evaluate(`JSON.parse(window.__fixture.drafts['assignment-3'].answer.body).structured_answer.pairs`),[{left_id:'l',right_id:'r'}]);passed++;
  await open(4);await edit(`document.querySelector('.order-row button:last-child').click()`);assert.deepEqual(await evaluate(`JSON.parse(window.__fixture.drafts['assignment-4'].answer.body).structured_answer.ordered_ids`),['second','first']);passed++;
  await open(5);await edit(`document.getElementById('answer-text').value='짧은 답';document.getElementById('answer-text').dispatchEvent(new Event('input',{bubbles:true}))`);assert.equal(await evaluate(`JSON.parse(window.__fixture.drafts['assignment-5'].answer.body).structured_answer.text`),'짧은 답');passed++;
  await open(6);await edit(`document.getElementById('blank-0').value='빈칸의 답';document.getElementById('blank-0').dispatchEvent(new Event('input',{bubbles:true}))`);assert.equal(await evaluate(`JSON.parse(window.__fixture.drafts['assignment-6'].answer.body).structured_answer.blanks['blank-a']`),'빈칸의 답');passed++;
  await open(0);assert.equal(await evaluate(`document.getElementById('answer-text').value`),'새 초안');passed++;
  await evaluate(`window.__fixture.offline=true;document.getElementById('prepare-lessons').click()`);await sleep(100);assert.equal(await evaluate(`document.getElementById('answer-text').value`),'새 초안');assert.match(await evaluate(`document.getElementById('lesson-notice').textContent`),/기존 자료는 보존/);passed++;
  await evaluate(`document.getElementById('answer-text').value='잠금 직전 답안';document.getElementById('answer-text').dispatchEvent(new Event('input',{bubbles:true}));document.getElementById('lock').click()`);await sleep(150);assert.equal(await evaluate(`JSON.parse(window.__fixture.drafts['assignment-0'].answer.body).body`),'잠금 직전 답안');assert.equal(await evaluate(`document.getElementById('answer-controls').childElementCount`),0);passed++;
  const drafts=await evaluate('window.__fixture.drafts');await load({drafts},390,1300);await open(0);assert.equal(await evaluate(`document.getElementById('answer-text').value`),'잠금 직전 답안');assert.equal(await evaluate('document.documentElement.scrollWidth<=innerWidth'),true);passed++;
  await send('Page.captureScreenshot',{format:'png',captureBeyondViewport:true}).then(s=>writeFileSync(join(output,'mobile.png'),Buffer.from(s.data,'base64')));
  await load({drafts},1100,1300);await open(3);await send('Page.captureScreenshot',{format:'png',captureBeyondViewport:true}).then(s=>writeFileSync(join(output,'desktop.png'),Buffer.from(s.data,'base64')));
  await load({drafts,expired:true});await open(0);assert.equal(await evaluate(`document.getElementById('answer-fields').disabled`),true);assert.equal(await evaluate(`document.getElementById('answer-text').value`),'잠금 직전 답안');passed++;
  await load({drafts,failSaving:true});await open(0);await edit(`document.getElementById('answer-text').value='저장 실패에도 유지할 답안';document.getElementById('answer-text').dispatchEvent(new Event('input',{bubbles:true}))`);await evaluate(`document.querySelectorAll('#lesson-list button')[1].click()`);await sleep(150);assert.equal(await evaluate(`document.getElementById('lesson-title').textContent`),'수업 0');assert.equal(await evaluate(`document.getElementById('answer-text').value`),'저장 실패에도 유지할 답안');passed++;
  const report={status:'passed',checks:passed,synthetic_only:true,native_runtime_test:false,production_server_test:false};writeFileSync(join(output,'report.json'),JSON.stringify(report,null,2)+'\n');console.log(JSON.stringify(report));
}finally{
  if(ws?.readyState===1)await send('Browser.close').catch(()=>{});
  if(browser?.exitCode===null)browser.kill();
  server.close();
  if(resolve(dirname(profile))===resolve(tmpdir())&&basename(profile).startsWith('thinkgrow-student-ui-')){
    for(let n=0;n<10;n++){try{rmSync(profile,{recursive:true,force:true});break;}catch{await sleep(200);}}
  }
}
