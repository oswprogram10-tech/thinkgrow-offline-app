"use strict";
(() => {
  const $=id=>document.getElementById(id),invoke=window.__TAURI__?.core?.invoke;
  if(!invoke||!$("games-section"))return;
  let session,snapshot,epoch=0,busy=false;
  const student=document.body.dataset.role==="student";
  const catalogKey=game=>student?`${game.assignment_id}::${game.id}`:game.id;
  const selectedGame=()=>snapshot?.data.games?.find(game=>catalogKey(game)===$("game-catalog").value);
  const node=(tag,text)=>{const n=document.createElement(tag);if(text!=null)n.textContent=text;return n;};
  const notice=(text,error=false)=>{$("game-notice").textContent=text;$("game-notice").classList.toggle("error",error);};
  const active=()=>session?.local?.access==="read_write";
  function gate(){$("game-install").disabled=busy||!active()||!$("game-catalog").value;$("game-assign")&&($("game-assign").disabled=busy||!active());}
  async function operation(fn){if(busy)return;busy=true;gate();const ticket=epoch;try{await fn(ticket);}catch(e){if(ticket===epoch)notice(String(e).includes("unsupported")?"이 게임은 오프라인 실행 계약을 지원하지 않습니다.":"게임을 준비하지 못했습니다. 연결·기기 자격·게임 판본을 확인하세요. 기존 설치와 기록은 유지됩니다.",true);}finally{if(ticket===epoch){busy=false;gate();}}}
  async function list(ticket=epoch){
    const games=await invoke("list_games");if(ticket!==epoch)return;$("installed-games").replaceChildren();
    const count=document.querySelectorAll('#local-summary dd')[3];if(count)count.textContent=games.length;
    for(const game of games){
      const bindings=student?(snapshot?.data.games||[]).filter(g=>g.id===game.id&&g.source_sha256===game.source_sha256):[null];
      if(student&&!bindings.length){$("installed-games").append(node("p",`${game.title} · 수업 자료를 새로 받아 배정을 확인하세요.`));continue;}
      for(const binding of bindings){
        const onlineOnly=game.can_run_offline===false;
        const row=node("div"),play=node("button",`${game.title} · ${game.version}${binding?' · 수업 '+binding.assignment_id:''}${onlineOnly?' (온라인 전용)':''} 실행`);row.className="game-row";play.disabled=!active();
        play.onclick=()=>operation(async t=>{
          let launch;
          if(onlineOnly){
            if(!navigator.onLine){
              notice("오프라인 실행 불가: 승인된 온라인 전용 게임은 기기에 보관할 수 있지만, 오프라인에서는 실행할 수 없습니다. 인터넷에 연결해 주세요.",true);
              return;
            }
            notice("온라인 실행 세션을 확인하고 있습니다...");
            const sessRes=await invoke("role_exchange",{action:"game_session",data:{game_id:game.id,...(binding?.assignment_id?{assignment_id:binding.assignment_id}:{})}});
            const sess=sessRes?.data||sessRes;
            if(!sess||!sess.session_token){notice("유효한 온라인 실행 세션을 발급받지 못했습니다.",true);return;}
            launch=await invoke("launch_online_game",{gameId:game.id,assignmentId:binding?.assignment_id||null,sessionToken:sess.session_token});
          }else{
            launch=await invoke("launch_game",{gameId:game.id,assignmentId:binding?.assignment_id||null});
          }
          if(t!==epoch)return;window.thinkgrowOnlineModule=onlineOnly;const frame=node("iframe");frame.title=game.title;frame.sandbox="allow-scripts allow-same-origin";frame.referrerPolicy="no-referrer";frame.src=launch.url;$("game-stage").replaceChildren(frame);$("game-stop").hidden=false;$("game-stop").textContent="게임 닫기";notice(onlineOnly?"온라인 게임을 실행했습니다. 유효한 실행 세션으로 연결되었습니다.":"게임을 실행했습니다. 진행 기록은 이 기기에 보관됩니다.");
        });row.append(play);$("installed-games").append(row);
      }
    }
    if(!games.length)$("installed-games").append(node("p","설치한 게임이 없습니다. 목록에서 필요한 게임만 선택하세요."));
  }
  async function stop(){window.thinkgrowOnlineModule=false;$("game-stage").replaceChildren();$("game-stop").hidden=true;await invoke("stop_game");}
  $("game-stop").onclick=()=>operation(async()=>{await stop();notice("게임을 종료했습니다. 저장된 진행 기록은 유지됩니다.");});
  $("game-catalog").onchange=gate;
  $("game-install").onclick=()=>operation(async ticket=>{const chosen=selectedGame();if(!chosen)return;await invoke("role_exchange",{action:"install_game",data:{game_id:chosen.id,...(student?{assignment_id:chosen.assignment_id}:{})}});if(ticket!==epoch)return;await list(ticket);notice("게임을 검증해 기기에 설치했습니다.");});
  if($("game-assign"))$("game-assign").onclick=()=>operation(async ticket=>{const assignment=$("role-assignment").value,id=$("game-catalog").value;if(!assignment||!id){notice("수업과 게임을 먼저 선택하세요.",true);return;}await invoke("role_exchange",{action:"assign_game",data:{assignment_id:assignment,game_id:id}});if(ticket===epoch)notice("이 수업에 게임 판본을 배정했습니다. 학생은 자료를 새로 받은 뒤 개별 설치할 수 있습니다.");});
  const workspaceChanged=event=>{snapshot=event.detail;$("game-catalog").replaceChildren(new Option("게임을 선택하세요",""));for(const g of snapshot?.data.games||[])$("game-catalog").append(new Option(`${g.title} · ${g.version}${student?' · 수업 '+g.assignment_id:''}`,catalogKey(g)));gate();if(session?.local)list().catch(()=>notice("설치한 게임을 읽지 못했습니다.",true));};
  const sessionChanged=async event=>{window.thinkgrowOnlineModule=false;epoch++;session=event.detail;busy=false;snapshot=null;$("game-stage").replaceChildren();$("installed-games").replaceChildren();$("game-catalog").replaceChildren(new Option("게임을 선택하세요",""));$("game-stop").hidden=true;gate();if(session?.local){const ticket=epoch;try{await list(ticket);}catch{if(ticket===epoch)notice("설치한 게임을 읽지 못했습니다. 자료는 보존됩니다.",true);}}};
  window.addEventListener("offline",()=>{if(window.thinkgrowOnlineModule){stop().catch(()=>{});notice("연결이 끊겨 온라인 앱 실행을 중지했습니다.",true);}});window.addEventListener("thinkgrow-role-workspace",workspaceChanged);window.addEventListener("thinkgrow-session",sessionChanged);
  if(window.thinkgrowSession)sessionChanged({detail:window.thinkgrowSession}).then(()=>{if(window.thinkgrowRoleWorkspace)workspaceChanged({detail:window.thinkgrowRoleWorkspace});});
  const previous=window.thinkgrowBeforeLock;window.thinkgrowBeforeLock=async()=>{await previous?.();await stop();};
})();
