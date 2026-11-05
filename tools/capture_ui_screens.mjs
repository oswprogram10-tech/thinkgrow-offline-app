// Synthetic screenshots of the three installed apps. Edge CDP only, no network
// service and no real account data. Every value below is made up.
import {createServer} from 'node:http';
import {spawn} from 'node:child_process';
import {existsSync, mkdirSync, mkdtempSync, readFileSync, writeFileSync, rmSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {dirname, join, resolve, basename} from 'node:path';
import {fileURLToPath} from 'node:url';

const root = dirname(dirname(fileURLToPath(import.meta.url)));
const edge = join(process.env['ProgramFiles(x86)'] || 'C:/Program Files (x86)', 'Microsoft/Edge/Application/msedge.exe');
const output = join(root, 'docs/ui-screens/UI-OFF-01');
mkdirSync(output, {recursive: true});
const reportDir = join(root, 'target/ui-screen-capture');
mkdirSync(reportDir, {recursive: true});
const profile = mkdtempSync(join(tmpdir(), 'thinkgrow-capture-ui-'));
const roles = ['teacher', 'student', 'parent'];
const types = {
  'index.html': 'text/html; charset=utf-8', 'app.css': 'text/css',
  'app.js': 'text/javascript', 'theme.js': 'text/javascript',
  'student.js': 'text/javascript', 'materials.js': 'text/javascript',
  'games.js': 'text/javascript', 'roles.js': 'text/javascript',
};

const server = createServer((request, response) => {
  const parts = decodeURIComponent(request.url).split('?')[0].split('/').filter(Boolean);
  const role = parts[0];
  const name = parts[1] || 'index.html';
  if (!roles.includes(role) || !Object.hasOwn(types, name)) {
    response.writeHead(404);
    response.end();
    return;
  }
  const file = join(root, 'target/ui', role, name);
  if (!existsSync(file)) {
    response.writeHead(404);
    response.end();
    return;
  }
  response.writeHead(200, {'Content-Type': types[name]});
  response.end(readFileSync(file));
});
await new Promise(r => server.listen(0, '127.0.0.1', r));
const base = 'http://127.0.0.1:' + server.address().port;

const now = Date.now();
const revision = 'a'.repeat(64);
const lessons = [{
  protocol: 1, app_id: 'com.thinkgrow.offline.student', subject_id: 'synthetic',
  device_id: 'synthetic-device', credential_epoch: 1, issued_ms: now - 3600000,
  expires_ms: now + 86400000, package_revision: revision,
  school_id: 'synthetic-school', class_id: 'synthetic-class',
  assignment_id: 'synthetic-assignment', title: '환경 보호 글쓰기',
  prompt: '환경을 보호하기 위해 우리가 할 수 있는 일을 글로 써 보세요.',
  hint: '서론·본론·결론으로 나누어 쓰면 읽기 쉽습니다.',
  content: {kind: 'essay', min_length: 0, max_length: 100000},
  starts_ms: null, due_ms: null, allow_late: false,
}];
const syntheticAnswer = '환경을 보호하려면 먼저 작은 습관부터 바꿔야 합니다. '
  + '일회용 컵 대신 텀블러를 쓰고, 사용하지 않는 전등을 끄는 일이 그 시작입니다. '
  + '이런 실천이 모이면 우리 반과 학교 전체의 쓰레기를 줄일 수 있습니다.';

function fixtureFor(role, screen) {
  const unlocked = screen === 'workspace' || screen === 'lesson-editor';
  const fixture = {
    unlocked,
    accounts: screen === 'register' ? [] : [{
      handle: 'synthetic-account', label: role === 'teacher' ? '합성 교사' : role === 'student' ? '합성 학생' : '합성 보호자',
      enrolled: true, recovery_ready: false,
    }],
    lessons: role === 'student' ? lessons : [],
    drafts: {},
  };
  if (role === 'teacher' && unlocked) {
    fixture.roleWorkspace = {
      issued_ms: now - 3600000, expires_ms: now + 86400000,
      data: {
        assignments: [{id: 'synthetic-assignment', class_name: '3학년 2반', problem_title: '환경 보호 글쓰기'}],
        selected: {
          id: 'synthetic-assignment', class_name: '3학년 2반', problem_title: '환경 보호 글쓰기',
          prompt: '환경을 보호하기 위해 우리가 할 수 있는 일을 글로 써 보세요.',
          evaluation_mode: 'level', rubrics: [],
          submissions: [{
            answer_version_id: 'synthetic-answer', name: '합성 학생', body: syntheticAnswer,
            base_grading_revision: 1, grade: null,
          }],
        },
        games: [],
      },
    };
  }
  if (role === 'parent' && unlocked) {
    fixture.roleWorkspace = {
      issued_ms: now - 3600000, expires_ms: now + 86400000,
      data: {
        children: [{
          id: 'synthetic-link', student_name: '합성 학생', class_name: '3학년 2반', status: 'active',
          reports: [{
            week_start: '2026-11-02', week_end: '2026-11-06',
            summary: '이번 주에는 환경 보호를 주제로 글을 썼습니다.',
            records: [{problem_title: '환경 보호 글쓰기', content: syntheticAnswer}],
          }],
          wellbeing_summaries: [],
        }],
        inbox: [{
          id: 'synthetic-link',
          messages: [{sender_role: 'teacher', body: '이번 주 글쓰기 과제를 잘 마쳤습니다.'}],
        }],
      },
    };
  }
  if (role === 'student' && unlocked) {
    fixture.roleWorkspace = {
      issued_ms: now - 3600000, expires_ms: now + 86400000,
      data: {games: [], materials: []},
    };
  }
  return fixture;
}

async function main() {
  let browser, ws, id = 0;
  const pending = new Map();
  const send = (method, params = {}) => new Promise((resolveCall, reject) => {
    pending.set(++id, {resolve: resolveCall, reject});
    ws.send(JSON.stringify({id, method, params}));
  });
  const evaluate = async expression => {
    const result = await send('Runtime.evaluate', {expression, returnByValue: true, awaitPromise: true});
    if (result.exceptionDetails) throw Error(JSON.stringify(result.exceptionDetails));
    return result.result.value;
  };
  const sleep = ms => new Promise(r => setTimeout(r, ms));
  const captured = [];

  try {
    browser = spawn(edge, ['--headless', '--disable-extensions', '--disable-gpu', '--no-first-run',
      '--remote-debugging-port=0', '--user-data-dir=' + profile, 'about:blank'], {windowsHide: true, stdio: 'ignore'});
    for (let n = 0; !existsSync(join(profile, 'DevToolsActivePort')); n++) {
      if (n > 100) throw Error('Browser did not start');
      await sleep(100);
    }
    const port = readFileSync(join(profile, 'DevToolsActivePort'), 'utf8').split('\n')[0];
    const tabs = await (await fetch('http://127.0.0.1:' + port + '/json')).json();
    ws = new WebSocket(tabs.find(tab => tab.type === 'page' && tab.url === 'about:blank').webSocketDebuggerUrl);
    await new Promise(r => ws.addEventListener('open', r, {once: true}));
    ws.addEventListener('message', event => {
      const message = JSON.parse(event.data);
      if (message.id) {
        const call = pending.get(message.id);
        pending.delete(message.id);
        message.error ? call.reject(message.error) : call.resolve(message.result);
      }
    });
    await send('Page.enable');

    let initScript;
    async function load(role, screen, width, height) {
      if (initScript) await send('Page.removeScriptToEvaluateOnNewDocument', {identifier: initScript});
      const fixture = fixtureFor(role, screen);
      const source = `try{localStorage.setItem('thinkgrow-theme','light')}catch(e){}
window.__fixture=${JSON.stringify(fixture)};
window.__TAURI__={core:{invoke:async(command,args)=>{
  const c=window.__fixture;
  const local=c.unlocked?{access:'read_write',drafts:3,pending_classroom:1,pending_online:0,installed_games:0}:null;
  if(command==='account_status')return {platform_supported:true,service_configured:true,accounts:c.accounts};
  if(command==='core_status')return {local};
  if(command==='unlock_account'){c.unlocked=true;return {local:local||{access:'read_write',drafts:3,pending_classroom:1,pending_online:0,installed_games:0}};}
  if(command==='register_account'){c.unlocked=true;return {local:{access:'read_write',drafts:0,pending_classroom:0,pending_online:0,installed_games:0}};}
  if(command==='lock_account'){c.unlocked=false;return {local:null};}
  if(command==='list_lessons')return c.lessons;
  if(command==='prepare_lessons')return c.lessons;
  if(command==='load_draft')return null;
  if(command==='save_draft')return 1;
  if(command==='queue_submission')return {operation:{}};
  if(command==='submission_states')return [];
  if(command==='deliver_submissions')return {remaining:0};
  if(command==='load_role_workspace')return c.roleWorkspace||null;
  if(command==='list_role_drafts')return [];
  if(command==='list_class_materials')return [];
  if(command==='list_games')return [];
  throw Error('Unexpected command '+command);
}}};`;
      initScript = (await send('Page.addScriptToEvaluateOnNewDocument', {source})).identifier;
      await send('Emulation.setDeviceMetricsOverride', {width, height, deviceScaleFactor: 1, mobile: width < 700});
      await send('Page.navigate', {url: `${base}/${role}/`});
      for (let n = 0; n < 120; n++) {
        if (await evaluate(`!!document.getElementById('notice')&&!document.getElementById('notice').textContent.includes('확인하고 있습니다')`)) break;
        await sleep(50);
      }
      if (screen === 'lesson-editor') {
        for (let n = 0; n < 100; n++) {
          if (await evaluate(`document.querySelectorAll('#lesson-list button').length>0`)) break;
          await sleep(50);
        }
        await evaluate(`document.querySelector('#lesson-list button').click()`);
        await sleep(150);
        await evaluate(`document.getElementById('answer-text').value=${JSON.stringify(syntheticAnswer)};document.getElementById('answer-text').dispatchEvent(new Event('input',{bubbles:true}))`);
        await sleep(800);
      }
      await sleep(300);
    }

    for (const role of roles) {
      const screens = role === 'student'
        ? ['unlock', 'register', 'workspace', 'lesson-editor']
        : ['unlock', 'register', 'workspace'];
      for (const screen of screens) {
        for (const width of [1280, 390]) {
          const height = width === 390 ? 844 : 900;
          await load(role, screen, width, height);
          for (const theme of ['light', 'dark']) {
            if (theme === 'dark') {
              await evaluate(`document.getElementById('theme-toggle').click()`);
              await sleep(400);
            }
            const file = join(output, `${role}-${screen}-${theme}-${width}.png`);
            const shot = await send('Page.captureScreenshot', {format: 'png', captureBeyondViewport: true});
            writeFileSync(file, Buffer.from(shot.data, 'base64'));
            captured.push(file);
            console.log(file);
          }
        }
      }
    }
  } finally {
    if (ws?.readyState === 1) await send('Browser.close').catch(() => {});
    if (browser?.exitCode === null) browser.kill();
    server.close();
    if (resolve(dirname(profile)) === resolve(tmpdir()) && basename(profile).startsWith('thinkgrow-capture-ui-')) {
      for (let n = 0; n < 10; n++) {
        try {
          rmSync(profile, {recursive: true, force: true});
          break;
        } catch {
          await sleep(200);
        }
      }
    }
  }
  writeFileSync(join(reportDir, 'report.json'), JSON.stringify({
    status: 'passed', screenshots: captured.length, synthetic_only: true, folder: 'docs/ui-screens/UI-OFF-01',
  }, null, 2) + '\n');
}

main().catch(error => {
  console.error(error);
  process.exitCode = 1;
});
