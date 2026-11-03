"use strict";
(() => {
  const $ = id => document.getElementById(id), invoke = window.__TAURI__?.core?.invoke;
  if (!invoke) return;
  const role = document.body.dataset.role;
  let session, snapshot, drafts = [], generation = 0, busy = false, dirty = false, saving = Promise.resolve(), timer, current, revision = 0;
  const el = (tag, text) => {const n=document.createElement(tag);if(text!=null)n.textContent=String(text);return n;};
  const notice = (text,error=false) => {$("role-notice").textContent=text;$("role-notice").classList.toggle("error",error);};
  const writable = () => session?.local?.access === "read_write" && snapshot && Date.now()<snapshot.expires_ms;
  const field = (label,control) => {const n=el("label",label);n.htmlFor=control.id;$("role-fields").append(n,control);return control;};
  function controls() {$("role-refresh").disabled=busy||session?.local?.access!=="read_write";const saved=role==="teacher"&&current?drafts.find(d=>d.slot===`grade/${current.answer_version_id}`):null;const needsRefresh=saved?.state==="synced"&&saved.data.base_grading_revision===current?.base_grading_revision;if($("role-fields"))$("role-fields").disabled=busy||!writable()||needsRefresh;}
  async function task(fn) {
    if(busy)return;busy=true;controls();const ticket=generation;
    try{await fn(ticket);}catch(error){if(ticket===generation)notice(String(error).includes("conflict")?"온라인 자료가 바뀌었습니다. 기기의 초안은 보존했습니다. 최신 자료를 받아 비교한 뒤 다시 저장하세요.":"작업을 완료하지 못했습니다. 저장한 초안은 유지됩니다. 연결·기기 자격을 확인하고 다시 시도하세요.",true);}
    finally{if(ticket===generation){busy=false;controls();}}
  }
  function changed(){dirty=true;clearTimeout(timer);timer=setTimeout(()=>persist().catch(()=>notice("기기에 저장하지 못했습니다. 화면을 유지하고 다시 저장하세요.",true)),500);}
  function payload(){
    if(role==="teacher"){
      const scores={};for(const input of $("role-fields").querySelectorAll("[data-rubric]"))if(input.value!=="")scores[input.dataset.rubric]=Number(input.value);
      return {answer_version_id:current.answer_version_id,base_grading_revision:current.base_grading_revision,scores,
        feedback:$("grade-feedback").value,level:$("grade-level")?.value||null,confirm:$("grade-confirm").checked};
    }
    return {link_id:current.id,link_revision:current.link_revision,body:$("parent-message").value};
  }
  async function persist(){
    clearTimeout(timer);if(!dirty||!current||!writable())return saving;
    const data=payload(),ticket=generation,identity=current;dirty=false;
    saving=saving.catch(()=>{}).then(async()=>{
      if(ticket!==generation)return;
      try{
        const d=await invoke("save_role_draft",{kind:role==="teacher"?"grade":"parent_reply",data,expected:revision});
        if(ticket!==generation||identity!==current)return;
        revision=d.revision;drafts=drafts.filter(x=>x.slot!==d.slot);drafts.push(d);notice("기기에 저장했습니다. 온라인 반영 버튼을 누르면 서버에 전송합니다.");
      }catch(e){if(ticket===generation&&identity===current)dirty=true;throw e;}
    });return saving;
  }
  function present(snapshotValue){
    snapshot=snapshotValue;window.thinkgrowRoleWorkspace=snapshot;
    window.dispatchEvent(new CustomEvent("thinkgrow-role-workspace",{detail:snapshot}));
    $("role-list").replaceChildren();$("role-editor").replaceChildren();current=null;revision=0;
    if(!snapshot){notice("온라인에서 자료를 준비하면 이 기기에 보관됩니다.");controls();return;}
    const date=new Intl.DateTimeFormat("ko-KR",{dateStyle:"medium",timeStyle:"short"}).format(snapshot.issued_ms);
    notice(`${date}에 준비한 자료입니다.${Date.now()>=snapshot.expires_ms?" 자격을 갱신한 뒤 새 자료를 받으세요.":" 연결이 없을 때도 보관한 자료를 열 수 있습니다."}`);
    if(role==="teacher"){
      const choices=$("role-assignment"),selected=snapshot.data.selected;choices.replaceChildren(new Option("수업을 선택하세요", ""));
      for(const a of snapshot.data.assignments||[])choices.append(new Option(`${a.class_name||"학급"} · ${a.problem_title||"수업"}`,a.id));
      if(selected){choices.value=selected.id;for(const row of selected.submissions){const button=el("button",row.name+" · "+(row.grade?.confirmed?"채점됨":"채점 대기"));button.type="button";button.className="secondary";button.onclick=()=>task(async()=>{await persist();await saving;showGrade(row);});$("role-list").append(button);}if(!selected.submissions.length)$("role-list").append(el("p","현재 제출된 답안이 없습니다."));}
    }else if(role==="parent"){
      for(const child of snapshot.data.children||[]){const button=el("button",`${child.student_name} · ${child.class_name}${child.status!=="active"?" · 연결 승인 대기":""}`);button.className="secondary";button.onclick=()=>task(async()=>{await persist();await saving;showChild(child);});$("role-list").append(button);}
      if(!snapshot.data.children?.length)$("role-list").append(el("p","승인된 자녀 연결이 없습니다. 온라인 학부모 화면에서 연결 상태를 확인하세요."));
    }else{$("role-list").append(el("p","수업에 배정된 게임을 아래 목록에서 개별 설치할 수 있습니다."));}
    controls();
  }
  function draftFor(slot){return drafts.find(d=>d.slot===slot);}
  function form(title){
    const host=$("role-editor");host.replaceChildren(el("h3",title));
    const form=el("form"),fields=el("fieldset");fields.id="role-fields";form.append(fields);host.append(form);
    form.addEventListener("submit",e=>{e.preventDefault();task(async()=>{await persist();await saving;});});return form;
  }
  function actions(form){
    const save=el("button","기기에 저장"),send=el("button",role==="teacher"?"온라인 채점 반영":"회신 보내기");save.type="submit";send.type="button";
    save.id="role-save";send.id="role-send";
    send.onclick=()=>task(async ticket=>{if(role==="parent"&&!$("parent-message").value.trim()){notice("회신 내용을 먼저 작성하세요.",true);return;}await persist();await saving;const slot=role==="teacher"?`grade/${current.answer_version_id}`:`reply/${current.id}`;const draft=draftFor(slot);if(!draft){notice("먼저 내용을 작성해 기기에 저장하세요.",true);return;}
      const result=await invoke("sync_role_draft",{slot,revision:draft.revision});if(ticket!==generation)return;drafts=drafts.filter(d=>d.slot!==slot);drafts.push(result);if(role==="parent")$("parent-message").value="";notice(role==="teacher"?"채점을 온라인에 반영했습니다. 결과는 비공개입니다. 수정하려면 최신 자료를 다시 받으세요.":"회신을 보냈습니다.");});
    const row=el("div");row.className="answer-actions";row.append(save,send);$("role-fields").append(row);form.addEventListener("input",changed);form.addEventListener("change",changed);
  }
  function showGrade(row){
    current=row;dirty=false;const saved=draftFor(`grade/${row.answer_version_id}`);revision=saved?.revision||0;
    const formElement=form(row.name+"의 답안"),selected=snapshot.data.selected,initial=saved?.data||row.grade||{};
    if(selected.prompt)$("role-fields").append(el("h4","수업 문항"),el("p",selected.prompt));
    if(selected.reference_answer){const reference=el("details");reference.append(el("summary","교사용 참고 답안"),el("p",selected.reference_answer));$("role-fields").append(reference);}
    $("role-fields").append(el("h4","제출한 답안"));
    const answer=el("p",row.body);answer.className="question-text";$("role-fields").append(answer);
    if(row.answer_summary){const summary=el("p",row.answer_summary);summary.className="question-text";$("role-fields").append(summary);}
    if(selected.evaluation_mode==="level") {const select=el("select");select.id="grade-level";select.append(new Option("수준 선택",""));for(const level of ["매우 잘함","잘함","보통","노력 필요"])select.append(new Option(level,level));select.value=initial.level||"";field("성취 수준",select);}
    else for(const [index,rubric] of selected.rubrics.entries()){const input=el("input");input.id=`rubric-${index}`;input.type="number";input.min=0;input.max=rubric.max_score;input.step=1;input.dataset.rubric=rubric.id;input.value=initial.scores?.[rubric.id]??"";field(`${rubric.label} · ${rubric.max_score}점`,input);if(rubric.description)$("role-fields").append(el("p",rubric.description));}
    const feedback=el("textarea");feedback.id="grade-feedback";feedback.maxLength=10000;feedback.value=initial.feedback||"";field("피드백",feedback);
    const check=el("input");check.type="checkbox";check.id="grade-confirm";check.checked=!!initial.confirm||!!initial.confirmed;field("채점 확정",check);
    if(saved)$("role-fields").append(el("p",saved.state==="synced"?"온라인 반영 완료":saved.data.base_grading_revision!==row.base_grading_revision?"온라인 채점이 바뀌었습니다. 기존 초안을 표시합니다. 비교 후 저장하면 현재 판본을 기준으로 새 초안을 만듭니다.":"이 기기에 보관한 초안입니다."));
    actions(formElement);controls();
  }
  function reportRecord(record){
    const p=el("article");p.append(el("h4",record.problem_title||record.title||"학습 기록"));
    for(const key of ["content","body","feedback","summary","level","total_score"])if(record[key]!=null)p.append(el("p",key==="total_score"?`점수: ${record[key]}`:record[key]));return p;
  }
  function showChild(child){
    const host=$("role-editor");host.replaceChildren(el("h3",`${child.student_name} · ${child.class_name}`));current=null;dirty=false;
    for(const report of child.reports||[]){const section=el("article");section.append(el("h4",`${report.week_start||""} ~ ${report.week_end||""}`),el("p",report.summary));for(const record of report.records||[])section.append(reportRecord(record));host.append(section);}
    for(const summary of child.wellbeing_summaries||[])host.append(el("h4","교사가 공유한 생활 요약"),el("p",summary.summary));
    const inbox=(snapshot.data.inbox||[]).find(link=>link.id===child.id);if(!inbox){controls();return;}
    for(const message of inbox.messages||[])host.append(el("h4",message.sender_role==="guardian"?"보낸 회신":"선생님 메시지"),el("p",message.body));
    const read=el("button","메시지 읽음 표시");read.type="button";read.disabled=!writable();read.onclick=()=>task(async()=>{await invoke("role_exchange",{action:"parent_read",data:{link_id:inbox.id}});notice("메시지를 읽음으로 표시했습니다.");});host.append(read);
    for(const n of inbox.notices||[]){const article=el("article");article.append(el("h4",n.title),el("p",n.content));const readNotice=el("button",n.read_at?"읽은 공지":"공지 읽음 표시");readNotice.type="button";readNotice.disabled=!writable();readNotice.onclick=()=>task(async()=>{await invoke("role_exchange",{action:"notice_read",data:{link_id:inbox.id,notice_id:n.id,version:n.version}});notice("공지를 읽음으로 표시했습니다.");});article.append(readNotice);
      const choices=el("select");choices.setAttribute("aria-label",n.title+" 회신 선택");choices.append(new Option("회신을 선택하세요",""));for(const c of n.choices||[])choices.append(new Option(c.label,c.value));choices.value=n.reply?.choice||"";const reply=el("button","공지에 회신");reply.type="button";reply.disabled=!writable();reply.onclick=()=>task(async()=>{if(!choices.value)return;await invoke("role_exchange",{action:"notice_reply",data:{link_id:inbox.id,notice_id:n.id,version:n.version,choice:choices.value,message:""}});notice("선택한 회신을 보냈습니다.");});article.append(choices,reply,el("p",n.notice));host.append(article);}
    const heading=el("h4","선생님께 회신"),formElement=el("form"),fields=el("fieldset");fields.id="role-fields";formElement.append(fields);host.append(heading,formElement);
    current=inbox;const saved=draftFor(`reply/${inbox.id}`);revision=saved?.revision||0;const area=el("textarea");area.id="parent-message";area.maxLength=2000;area.value=saved?.state==="synced"?"":saved?.data.body||"";field("회신 내용",area);actions(formElement);formElement.onsubmit=e=>{e.preventDefault();task(async()=>{await persist();await saving;});};controls();
  }
  $("role-refresh").onclick=()=>task(async ticket=>{await persist();await saving;const reply=await invoke("role_exchange",{action:"workspace",data:role==="teacher"?{assignment_id:$("role-assignment").value||null}:{}});if(ticket!==generation)return;present(reply);});
  const previous=window.thinkgrowBeforeLock;
  window.thinkgrowBeforeLock=async()=>{await previous?.();await persist();await saving;};
  window.addEventListener("thinkgrow-session",async event=>{
    clearTimeout(timer);generation++;busy=false;dirty=false;session=event.detail;current=null;snapshot=null;drafts=[];revision=0;
    $("role-list").replaceChildren();$("role-editor").replaceChildren();window.thinkgrowRoleWorkspace=null;controls();
    if(!session?.local)return;const ticket=generation;
    try{const [cached,local]=await Promise.all([invoke("load_role_workspace"),role==="student"?Promise.resolve([]):invoke("list_role_drafts")]);if(ticket!==generation)return;drafts=local;present(cached);}catch{if(ticket===generation)notice("보관한 자료를 읽지 못했습니다. 계정을 다시 열어 주세요.",true);}
  });
  if(window.thinkgrowSession)window.dispatchEvent(new CustomEvent("thinkgrow-session",{detail:window.thinkgrowSession}));
})();
