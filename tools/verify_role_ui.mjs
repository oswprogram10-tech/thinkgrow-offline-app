// Isolated synthetic browser checks; never attaches to a signed-in browser.
import assert from 'node:assert/strict';
import {createServer} from 'node:http';
import {spawn} from 'node:child_process';
import {existsSync,mkdirSync,mkdtempSync,readFileSync,writeFileSync,rmSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {dirname,join} from 'node:path';
import {fileURLToPath} from 'node:url';
const root=dirname(dirname(fileURLToPath(import.meta.url))),output=join(root,'target/role-ui-validation');mkdirSync(output,{recursive:true});
const edge=join(process.env['ProgramFiles(x86)']||'C:/Program Files (x86)','Microsoft/Edge/Application/msedge.exe');
const profile=mkdtempSync(join(tmpdir(),'thinkgrow-role-ui-'));let escapeRequests=0;
const server=createServer((req,res)=>{
 if(req.url==='/escape'){escapeRequests++;res.end('unexpected');return;}
 const match=/^\/(teacher|student|parent)\/(index\.html|app\.css|app\.js|roles\.js|games\.js|student\.js)$/.exec(req.url);
 if(!match){res.writeHead(404);res.end();return;}
 res.setHeader('Content-Type',match[2].endsWith('.js')?'application/javascript':match[2].endsWith('.css')?'text/css':'text/html; charset=utf-8');res.end(readFileSync(join(root,'target/ui',match[1],match[2])));
});await new Promise(r=>server.listen(0,'127.0.0.1',r));const base='http://127.0.0.1:'+server.address().port;
const game=createServer((req,res)=>{res.setHeader('Content-Type','text/html; charset=utf-8');res.setHeader('Content-Security-Policy',"default-src 'none'; script-src 'unsafe-inline'; connect-src 'self'; sandbox allow-scripts allow-same-origin");res.end(`<p>합성 게임 실행</p><script>let escaped=false;try{parent.document.body.dataset.escaped='yes';escaped=true}catch{}fetch(${JSON.stringify(base+'/escape')}).catch(()=>{});parent.postMessage({testGameReady:true,escaped,title:document.querySelector('p').textContent},'*');</script>`);});
await new Promise(r=>game.listen(0,'127.0.0.1',r));const gameBase='http://127.0.0.1:'+game.address().port;
const sleep=ms=>new Promise(r=>setTimeout(r,ms));let browser,ws,next=0,script,passed=0;const pending=new Map();
const send=(method,params={})=>new Promise((resolve,reject)=>{pending.set(++next,{resolve,reject});ws.send(JSON.stringify({id:next,method,params}));});
const evaluate=async expression=>{const r=await send('Runtime.evaluate',{expression,returnByValue:true,awaitPromise:true});if(r.exceptionDetails)throw Error(JSON.stringify(r.exceptionDetails));return r.result.value;};
async function until(expression){for(let i=0;i<100;i++){if(await evaluate(expression))return;await sleep(50);}throw Error('Timed out: '+expression);}
async function load(role,width=1100,delay=0){
 if(script)await send('Page.removeScriptToEvaluateOnNewDocument',{identifier:script});
 const source=`(()=>{const role=${JSON.stringify(role)},gameURL=${JSON.stringify(gameBase+'/index.html')};let unlocked=true,draft=null,installed=false;
 const data=role==='teacher'?{role,assignments:[{id:'a1',class_name:'합성 학급',problem_title:'생각을 정리해요'}],selected:{id:'a1',title:'생각을 정리해요',prompt:'자신의 생각과 근거를 써 보세요.',reference_answer:'교사용 참고 답안',evaluation_mode:'score',rubrics:[{id:'r1',label:'근거의 구체성',max_score:5,description:'생각을 뒷받침하는 근거를 살핍니다.'}],submissions:[{name:'합성 학생',student_id:'s1',answer_version_id:'v1',body:'<img src=x onerror=alert(1)> 저는 함께 해결하면 좋겠습니다.',structured_answer:{},base_grading_revision:'legacy'}]},games:[{id:'g1',title:'합성 게임',version:'1.0.0'}]}:
 role==='parent'?{role,children:[{id:'link1',student_name:'합성 자녀',class_name:'합성 학급',status:'active',reports:[{week_start:'2026-11-02',week_end:'2026-11-06',summary:'서로의 생각을 듣고 근거를 찾았습니다.',records:[{title:'함께 해결하기',content:'교사가 선택한 공개 학습 기록'}]}],wellbeing_summaries:[]}],inbox:[{id:'link1',link_revision:3,messages:[{sender_role:'teacher',body:'이번 주에는 근거를 찾는 활동을 했습니다.'}],notices:[{id:'n1',title:'학급 안내',content:'다음 주 활동을 안내합니다.',version:1,choices:[{value:'acknowledge',label:'확인했습니다'}],notice:'앱 회신은 공식 동의를 대체하지 않습니다.'}]}]}:{role,games:[{id:'g1',title:'합성 게임',version:'1.0.0',source_sha256:'a'.repeat(64),assignment_id:'a1'}]};
 const snapshot={issued_ms:Date.now(),expires_ms:Date.now()+600000,data};window.__calls=[];window.__gameReady=null;window.addEventListener('message',e=>{if(e.origin===${JSON.stringify(gameBase)}&&e.data.testGameReady)window.__gameReady=e.data});
 const local=()=>unlocked?{access:'read_write',drafts:0,pending_classroom:0,pending_online:0,installed_games:installed?1:0}:null;
 window.__TAURI__={core:{invoke:async(command,args={})=>{window.__calls.push({command,args});
 if(command==='account_status')return {platform_supported:true,service_configured:true,accounts:[{handle:'account',label:'합성 계정',enrolled:true}]};
 if(command==='core_status')return {local:local()};if(command==='load_role_workspace'){await new Promise(r=>setTimeout(r,${delay}));return snapshot;}
 if(command==='list_role_drafts'||command==='list_lessons')return [];if(command==='list_games')return installed?[{id:'g1',title:'합성 게임',version:'1.0.0',source_sha256:'a'.repeat(64)}]:[];
 if(command==='role_exchange'){if(args.action==='install_game')installed=true;return snapshot;}
 if(command==='save_role_draft'){draft={slot:role==='teacher'?'grade/v1':'reply/link1',revision:(draft?.revision||0)+1,operation_id:'synthetic',data:args.data,state:'local'};return draft;}
 if(command==='sync_role_draft')return {...draft,state:'synced'};if(command==='launch_game')return {url:gameURL,run_id:'synthetic'};
 if(command==='stop_game')return null;if(command==='lock_account'){unlocked=false;return {local:null};}throw Error('Unexpected command: '+command);
 }}};})();`;
 script=(await send('Page.addScriptToEvaluateOnNewDocument',{source})).identifier;
 await send('Emulation.setDeviceMetricsOverride',{width,height:1100,deviceScaleFactor:1,mobile:width<700});
 await send('Page.navigate',{url:base+'/'+role+'/index.html'});
 await until(`window.__calls?.some(c=>c.command==='load_role_workspace')`);
 if(!delay)await until(`document.getElementById('role-list').children.length>0`);
}
async function shot(name){const s=await send('Page.captureScreenshot',{format:'png',captureBeyondViewport:true});writeFileSync(join(output,name+'.png'),Buffer.from(s.data,'base64'));}
try{
 browser=spawn(edge,['--headless','--disable-extensions','--disable-gpu','--no-first-run','--remote-debugging-port=0','--user-data-dir='+profile,'about:blank'],{windowsHide:true,stdio:'ignore'});
 for(let n=0;!existsSync(join(profile,'DevToolsActivePort'));n++){if(n>100)throw Error('Browser did not start');await sleep(100);}
 const port=readFileSync(join(profile,'DevToolsActivePort'),'utf8').split('\n')[0],tabs=await(await fetch('http://127.0.0.1:'+port+'/json')).json();
 ws=new WebSocket(tabs.find(t=>t.type==='page').webSocketDebuggerUrl);await new Promise(r=>ws.addEventListener('open',r,{once:true}));
 ws.addEventListener('message',e=>{const m=JSON.parse(e.data);if(m.id){const p=pending.get(m.id);pending.delete(m.id);m.error?p.reject(m.error):p.resolve(m.result);}});await send('Page.enable');
 await load('teacher');await evaluate(`document.querySelector('#role-list button').click()`);await until(`!!document.getElementById('grade-feedback')`);
 assert.equal(await evaluate(`document.querySelector('#role-editor img')===null`),true);passed++;
 await evaluate(`document.querySelector('[data-rubric]').value='4';document.getElementById('grade-feedback').value='구체적인 근거를 잘 찾았어요.';document.getElementById('grade-confirm').checked=true;document.getElementById('grade-feedback').dispatchEvent(new Event('input',{bubbles:true}));document.getElementById('role-save').click()`);
 await until(`window.__calls.some(c=>c.command==='save_role_draft')`);await evaluate(`document.getElementById('role-send').click()`);await until(`document.getElementById('role-notice').textContent.includes('온라인에 반영')`);
 assert.equal(await evaluate(`window.__calls.find(c=>c.command==='save_role_draft').args.data.scores.r1`),4);passed++;await shot('teacher-desktop');
 await load('parent',390);await evaluate(`document.querySelector('#role-list button').click()`);await until(`!!document.getElementById('parent-message')`);
 assert.equal(await evaluate(`document.documentElement.scrollWidth<=innerWidth`),true);assert.match(await evaluate(`document.getElementById('role-editor').textContent`),/공개 학습 기록/);passed++;
 await evaluate(`document.getElementById('parent-message').value='집에서도 함께 이야기하겠습니다.';document.getElementById('parent-message').dispatchEvent(new Event('input',{bubbles:true}));document.getElementById('role-save').click()`);await until(`window.__calls.some(c=>c.command==='save_role_draft')`);
 await evaluate(`document.getElementById('role-send').click()`);await until(`document.getElementById('role-notice').textContent.includes('회신을 보냈')`);passed++;await shot('parent-mobile');
 await load('student');await evaluate(`document.getElementById('game-catalog').value='a1::g1';document.getElementById('game-catalog').dispatchEvent(new Event('change'));document.getElementById('game-install').click()`);await until(`document.querySelector('#installed-games button')`);
 await evaluate(`document.querySelector('#installed-games button').click()`);await until(`window.__gameReady`);
 assert.equal(await evaluate(`window.__gameReady.escaped`),false);assert.equal(await evaluate(`window.__gameReady.title`),'합성 게임 실행');assert.equal(escapeRequests,0);assert.equal(await evaluate(`document.querySelector('#game-stage iframe').sandbox.value`),'allow-scripts allow-same-origin');passed++;await evaluate(`document.querySelector('#game-stage iframe').scrollIntoView({block:'center'});new Promise(r=>requestAnimationFrame(()=>requestAnimationFrame(r)))`);await shot('student-game');
 await evaluate(`document.getElementById('lock').click()`);await until(`document.getElementById('workspace').hidden`);assert.equal(await evaluate(`document.querySelector('#game-stage iframe')===null`),true);passed++;
 await load('teacher',1100,500);await evaluate(`document.getElementById('lock').click()`);await sleep(800);assert.equal(await evaluate(`document.getElementById('role-list').children.length`),0);passed++;
 const report={status:'passed',checks:passed,synthetic_only:true,native_runtime:false,production_server:false};writeFileSync(join(output,'report.json'),JSON.stringify(report,null,2)+'\n');console.log(JSON.stringify(report));
}finally{if(ws?.readyState===1)try{await Promise.race([send('Browser.close'),sleep(1500)]);}catch{}ws?.close();browser?.kill();await new Promise(r=>server.close(r));await new Promise(r=>game.close(r));await sleep(1000);try{rmSync(profile,{recursive:true,force:true,maxRetries:20,retryDelay:200});}catch(e){console.error('Synthetic browser profile cleanup deferred: '+e.code);}}
