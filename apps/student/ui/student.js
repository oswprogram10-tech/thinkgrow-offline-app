"use strict";
(() => {
  const $ = id => document.getElementById(id), invoke = window.__TAURI__?.core?.invoke;
  if (!invoke) return;
  let session = null, lessons = [], current = null, revision = 0, baseRevision = 0;
  let generation = 0, dirty = false, failed = false, timer, saving = Promise.resolve(), serial = 0, queuedIntent = null;
  let onlinePending = false, delivering = false;
  let read = () => ({format:1,body:"",structured_answer:{}});
  const notice = (text,error=false) => {$("lesson-notice").textContent=text;$("lesson-notice").classList.toggle("error",error);};
  const answerState = (text,error=false) => {$("answer-state").textContent=text;$("answer-state").classList.toggle("error",error);};
  const pad = value => String(value).padStart(2,"0");
  const stamp = ms => {const date=new Date(ms);return date.getFullYear()+"-"+pad(date.getMonth()+1)+"-"+pad(date.getDate())+" "+pad(date.getHours())+":"+pad(date.getMinutes());};
  const writable = () => session?.local?.access === "read_write" && current && Date.now()>=current.issued_ms && Date.now()<current.expires_ms && (current.starts_ms===null || Date.now()>=current.starts_ms);
  function gate() {$("prepare-lessons").disabled=!session?.local || session.local.access!=="read_write";$("answer-fields").disabled=!writable();}
  function wipe() {clearTimeout(timer);generation++;current=null;lessons=[];read=()=>({format:1,body:"",structured_answer:{}});$("answer-controls").replaceChildren();$("lesson-list").replaceChildren();$("lesson-title").textContent="";$("lesson-prompt").textContent="";$("lesson-hint").textContent="";$("lesson-editor").hidden=true;dirty=false;failed=false;queuedIntent=null;}
  const element = (tag,text) => {const el=document.createElement(tag);if(text!==undefined)el.textContent=text;return el;};
  function edited() {dirty=true;queuedIntent=null;serial++;clearTimeout(timer);timer=setTimeout(()=>persist(),450);answerState("기기에 저장하는 중입니다.");}
  function controls(lesson,response) {
    const host=$("answer-controls"), config=lesson.content, answer=response?.structured_answer||{};host.replaceChildren();
    const result = () => ({format:1,body:"",structured_answer:{}});
    if(config.kind==="essay" || config.kind==="short_answer") {
      const id="answer-text",label=element("label",config.kind==="essay"?"답안 작성":"짧은 답");label.htmlFor=id;
      const area=element("textarea");area.id=id;area.maxLength=config.max_length;area.value=config.kind==="essay"?(response?.body||""):(answer.text||"");area.addEventListener("input",edited);host.append(label,area);
      read=()=>config.kind==="essay"?{format:1,body:area.value,structured_answer:{}}:{format:1,body:area.value,structured_answer:{text:area.value}};
    } else if(config.kind==="true_false" || config.kind==="multiple_choice") {
      const options=config.kind==="true_false"?[{id:true,text:"참"},{id:false,text:"거짓"}]:config.options;
      const key=config.kind==="true_false"?"selected":"selected_option_id",inputs=[];
      for(const [index,option] of options.entries()) {
        const row=element("div");row.className="answer-choice";const input=element("input");input.type="radio";input.name="answer-choice";input.id="choice-"+index;input.checked=answer[key]===option.id;input.addEventListener("change",edited);
        const label=element("label",option.text);label.htmlFor=input.id;row.append(input,label);host.append(row);inputs.push({input,value:option.id});
      }
      read=()=>{const r=result(),selected=inputs.find(i=>i.input.checked);if(selected)r.structured_answer[key]=selected.value;return r;};
    } else if(config.kind==="matching") {
      const selects=[];
      for(const [index,item] of config.left.entries()) {
        const label=element("label",item.text),select=element("select");select.id="match-"+index;label.htmlFor=select.id;select.append(new Option("연결할 항목 선택",""));for(const right of config.right)select.append(new Option(right.text,right.id));
        select.value=answer.pairs?.find(p=>p.left_id===item.id)?.right_id||"";select.addEventListener("change",edited);host.append(label,select);selects.push({id:item.id,select});
      }
      read=()=>({format:1,body:"",structured_answer:{pairs:selects.filter(s=>s.select.value).map(s=>({left_id:s.id,right_id:s.select.value}))}});
    } else if(config.kind==="ordering") {
      let order=answer.ordered_ids?.length===config.items.length?[...answer.ordered_ids]:config.items.map(i=>i.id);
      function render() {host.replaceChildren();for(const [index,id] of order.entries()) {const item=config.items.find(i=>i.id===id);if(!item)throw Error("unknown order item");const row=element("div");row.className="order-row";row.append(element("span",(index+1)+". "+item.text));for(const [direction,label] of [[-1,"위로"],[1,"아래로"]]){const button=element("button",label);button.type="button";button.disabled=index+direction<0||index+direction>=order.length;button.setAttribute("aria-label",item.text+" "+label);button.addEventListener("click",()=>{[order[index],order[index+direction]]=[order[index+direction],order[index]];render();edited();});row.append(button);}host.append(row);}}
      render();read=()=>({format:1,body:"",structured_answer:{ordered_ids:[...order]}});
    } else if(config.kind==="fill_in_blank") {
      const inputs=[];for(const [index,id] of config.blanks.entries()) {const label=element("label",(index+1)+"번 빈칸"),input=element("input");input.id="blank-"+index;input.type="text";input.maxLength=2500;label.htmlFor=input.id;input.value=answer.blanks?.[id]||"";input.addEventListener("input",edited);host.append(label,input);inputs.push({id,input});}
      read=()=>({format:1,body:"",structured_answer:{blanks:Object.fromEntries(inputs.map(v=>[v.id,v.input.value]))}});
    } else throw Error("unsupported question");
  }
  function repaintList() {$("lesson-list").replaceChildren();for(const lesson of lessons){const button=element("button",lesson.title||"제목 없는 수업");button.type="button";button.className="secondary";button.setAttribute("aria-current",String(current?.assignment_id===lesson.assignment_id));button.addEventListener("click",()=>open(lesson));$("lesson-list").append(button);}}
  async function refreshStates() {
    if(!current)return;
    const assignment=current.assignment_id;
    try{const states=await invoke("submission_states",{assignment});if(!current||current.assignment_id!==assignment||dirty||failed)return;
      const latest=states&&states[0];if(!latest)return;
      if(latest.online_state==="received"&&latest.server_received_ms!=null)answerState("제출 완료(서버 수신 "+stamp(latest.server_received_ms)+")");
      else if(latest.online_state==="auth_wait")answerState("다시 로그인해야 보낼 수 있습니다");
      else if(latest.online_state==="conflict")answerState("서버에 이미 다른 제출이 있거나 마감되었습니다. 선생님께 확인해 주세요.");
      else if(latest.online_state==="pending"||latest.online_state==="sending")answerState("제출 대기 중: 인터넷이 연결되면 보냅니다");
    }catch{}
  }
  async function deliver() {
    if(!session?.local||delivering)return;
    delivering=true;let failedDelivery=false;
    try{const summary=await invoke("deliver_submissions");onlinePending=(summary.remaining||0)>0;}
    catch{failedDelivery=true;onlinePending=(session?.local?.pending_online||0)>0;}
    finally{delivering=false;}
    if(failedDelivery){if(current&&!dirty)answerState("다음에 다시 보냅니다.");return;}
    await refreshStates();
  }
  async function open(lesson) {
    if(dirty){await persist();await saving;if(dirty){notice("아직 저장하지 못한 답안이 있습니다. 현재 수업에서 다시 저장해 주세요.",true);return;}}
    const mine=++generation;current=null;$("lesson-editor").hidden=true;gate();
    try {const draft=await invoke("load_draft",{assignment:lesson.assignment_id});if(mine!==generation)return;
      if(draft && draft.answer.package_revision!==lesson.package_revision)throw Error("pinned revision mismatch");
      const response=draft?JSON.parse(draft.answer.body):null;if(response&&(response.format!==1||!response.structured_answer))throw Error("unsupported saved answer");
      current=lesson;revision=draft?.revision||0;baseRevision=draft?.answer.base_answer_revision||0;dirty=false;failed=false;queuedIntent=null;
      $("lesson-title").textContent=lesson.title;$("lesson-prompt").textContent=lesson.prompt;$("lesson-hint").textContent=lesson.hint;$("hint-box").hidden=!lesson.hint;$("hint-box").open=false;
      controls(lesson,response);$("lesson-editor").hidden=false;answerState(writable()?(revision?"기기에 저장한 답안을 열었습니다.":"답안은 이 기기에 자동 저장됩니다."):"자격 또는 수업 준비 기간을 확인해 주세요. 보관한 자료는 읽을 수 있습니다.");repaintList();gate();refreshStates();
    }catch{if(mine===generation)notice("보관한 답안을 열지 못했습니다. 자료를 덮어쓰지 않았습니다.",true);}
  }
  function persist() {
    clearTimeout(timer);if(!current||!dirty||!writable())return saving;
    const mine=generation,change=serial,lesson=current,response=read();
    saving=saving.then(async()=>{if(mine!==generation)return;const answer={school_id:lesson.school_id,class_id:lesson.class_id,assignment_id:lesson.assignment_id,package_revision:lesson.package_revision,base_answer_revision:baseRevision,body:JSON.stringify(response),blobs:[]};
      try{revision=await invoke("save_draft",{answer,expectedRevision:revision});if(mine!==generation)return;dirty=serial!==change;failed=false;answerState("이 기기에 저장했습니다."+(dirty?" 추가 변경을 저장하고 있습니다.":""));}
      catch{if(mine!==generation)return;failed=true;dirty=true;answerState("저장하지 못했습니다. 화면의 답안을 유지했습니다. 기기 자격·입력·충돌 상태를 확인하고 다시 저장해 주세요.",true);}
    });return saving;
  }
  $("save-answer").addEventListener("click",()=>{if(current&&writable()){dirty=true;serial++;persist();}});
  $("answer-form").addEventListener("submit",async event=>{event.preventDefault();if(!current||!writable())return;const mine=generation;$("queue-answer").disabled=true;
    try{if(!revision){dirty=true;serial++;}await persist();await saving;if(mine!==generation||dirty||failed)return;queuedIntent ||= crypto.randomUUID();await invoke("queue_submission",{assignment:current.assignment_id,revision,intent:queuedIntent});if(mine===generation){onlinePending=true;answerState("제출 대기 중: 인터넷이 연결되면 보냅니다");refreshStates();deliver();}}
    catch{if(mine===generation)answerState("제출 대기열에 보관하지 못했습니다. 작성한 초안은 유지합니다.",true);}finally{if(mine===generation)$("queue-answer").disabled=!writable();}
  });
  const lessonRefusals={
    OFFLINE_CLASS_NOT_READY:"학교에 연결되지 않은 학급의 수업이 있어 준비하지 못했습니다. 선생님께 학급을 학교에 연결해 달라고 말씀드리세요.",
    OFFLINE_SCOPE_DENIED:"기기를 등록한 뒤 수업 연결이 바뀌었습니다. 온라인에서 기기 등록을 다시 하고 수업을 골라 주세요.",
    OFFLINE_LESSON_NOT_OPEN:"아직 열리지 않았거나 이미 끝난 수업이 있습니다. 선생님께 수업이 열려 있는지 확인해 주세요.",
    OFFLINE_QUESTION_UNSUPPORTED:"이 앱에서 아직 풀 수 없는 문항 유형이 있습니다. 그 수업은 온라인에서 작성해 주세요.",
    OFFLINE_REQUIRED_CONTENT_UNSUPPORTED:"첨부 자료나 게임이 필요한 수업이 있습니다. 그 수업은 온라인에서 작성해 주세요.",
    OFFLINE_LESSON_TOO_LARGE:"수업 자료가 너무 커서 한 번에 준비하지 못했습니다. 기기 등록 때 수업을 나눠 골라 주세요.",
    OFFLINE_DEVICE_REVOKED:"이 기기 등록이 해제되었습니다. 온라인에서 기기 등록을 다시 해 주세요."
  };
  function prepareFailure(error){const code=String(error?.message??error??"").match(/^lesson preparation rejected: ([A-Z_]+)$/)?.[1];return (code&&Object.hasOwn(lessonRefusals,code)?lessonRefusals[code]:"수업을 준비하지 못했습니다. 온라인 연결과 기기 등록 때 선택한 수업을 확인하세요.")+" 기존 자료는 보존됩니다.";}
  $("prepare-lessons").addEventListener("click",async()=>{if(!session?.local||dirty){notice("작성 중인 답안을 먼저 기기에 저장하세요.",true);return;}const mine=generation;$("prepare-lessons").disabled=true;notice("등록한 수업을 준비하고 있습니다.");try{const prepared=await invoke("prepare_lessons");if(mine!==generation)return;lessons=prepared;repaintList();notice("수업을 이 기기에 보관했습니다. 이제 인터넷 없이 작성할 수 있습니다.");}catch(error){if(mine===generation)notice(prepareFailure(error),true);}finally{gate();}});
  async function applySession(core){session=core;wipe();gate();if(!session.local)return;const mine=generation;try{lessons=await invoke("list_lessons");if(mine!==generation)return;repaintList();notice(lessons.length?"보관한 수업을 선택하세요.":"기기에 보관한 수업이 없습니다. 온라인에서 수업을 준비하세요.");}catch{if(mine===generation)notice("수업 보관함을 열지 못했습니다. 기존 자료는 보존됩니다.",true);}onlinePending=(session.local.pending_online||0)>0;deliver();}
  window.addEventListener("thinkgrow-session",event=>applySession(event.detail));
  window.addEventListener("online",()=>{deliver();});
  if(window.thinkgrowSession) applySession(window.thinkgrowSession);
  window.thinkgrowBeforeLock = async () => {clearTimeout(timer);await persist();await saving;};
  setInterval(gate,30000);
  setInterval(()=>{if(onlinePending||(session?.local?.pending_online||0)>0)deliver();},60000);
})();
