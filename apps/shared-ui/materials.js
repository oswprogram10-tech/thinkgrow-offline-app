"use strict";
(() => {
  const $ = id => document.getElementById(id), invoke = window.__TAURI__?.core?.invoke;
  if (!invoke || !$("materials-section")) return;

  let session, snapshot, epoch = 0, busy = false;
  const student = document.body.dataset.role === "student";
  const catalogKey = mat => student ? `${mat.assignment_id}::${mat.id}` : mat.id;
  const selectedMaterial = () => snapshot?.data.materials?.find(m => catalogKey(m) === $("material-catalog")?.value);
  const node = (tag, text) => { const n = document.createElement(tag); if (text != null) n.textContent = text; return n; };
  const notice = (text, error = false) => {
    const el = $("material-notice");
    if (!el) return;
    el.textContent = text;
    el.classList.toggle("error", error);
  };
  const active = () => session?.local?.access === "read_write";

  function gate() {
    if ($("material-download")) $("material-download").disabled = busy || !active() || !$("material-catalog")?.value;
    if ($("material-assign")) $("material-assign").disabled = busy || !active() || !$("material-catalog")?.value || !$("role-assignment")?.value;
  }

  async function operation(fn) {
    if (busy) return;
    busy = true;
    gate();
    const ticket = epoch;
    try {
      await fn(ticket);
    } catch (e) {
      if (ticket === epoch) {
        notice(String(e).includes("denied") || String(e).includes("ACCESS")
          ? "수업 자료 접근 권한이 없습니다. 기기 자격과 배정 상태를 확인하세요."
          : `자료 작업을 완료하지 못했습니다: ${e.message || e}`, true);
      }
    } finally {
      if (ticket === epoch) {
        busy = false;
        gate();
      }
    }
  }

  function formatBytes(bytes) {
    if (!bytes || bytes <= 0) return "0 B";
    if (bytes < 1024) return bytes + " B";
    if (bytes < 1048576) return (bytes / 1024).toFixed(1) + " KB";
    return (bytes / 1048576).toFixed(1) + " MB";
  }

  function subtypeLabel(st) {
    const map = {
      pdf: "PDF",
      hwp: "한글",
      ppt: "PPT",
      video: "동영상",
      audio: "음악",
      app: "앱"
    };
    return map[st] || st || "자료";
  }

  async function list(ticket = epoch) {
    let items = [];
    try {
      items = await invoke("list_class_materials");
    } catch (e) {
      if (ticket === epoch) notice("보관된 자료를 읽지 못했습니다.", true);
      return;
    }
    if (ticket !== epoch) return;
    const container = $("installed-materials");
    if (!container) return;
    container.replaceChildren();

    for (const item of items) {
      const row = node("div");
      row.className = "material-row";

      const info = node("div");
      info.className = "material-info";

      const titleLine = node("div");
      const badge = node("span", subtypeLabel(item.subtype));
      badge.className = "material-badge";
      titleLine.append(badge, node("strong", item.title));

      const meta = node("div", `${item.version ? 'v' + item.version + ' · ' : ''}${formatBytes(item.size ?? item.size_bytes)} · ${item.offline_available ? '오프라인 이용 가능' : '온라인 전용'} · ${item.filename || ''}`);
      meta.className = "material-meta";
      info.append(titleLine, meta);

      const actions = node("div");
      actions.className = "material-actions";

      const isWebApp = item.subtype === "app" && (item.app_kind === "web_app" || item.app_kind === "web");
      const isInstaller = item.subtype === "app" && (item.app_kind === "os_installer" || item.app_kind === "installer");

      let openBtn;
      if (isWebApp) {
        openBtn = node("button", "앱 실행");
        openBtn.className = "secondary";
        openBtn.disabled = !active();
        openBtn.onclick = () => operation(async t => {
          let sessionToken=null;
          if(item.offline_available===false){
            if(!navigator.onLine){notice("온라인 전용 앱입니다. 인터넷에 연결한 뒤 실행해 주세요.",true);return;}
            const reply=await invoke("role_exchange",{action:"material_session",data:{material_id:item.id,assignment_id:item.assignment_id||null}});
            sessionToken=reply.data.session_token;
          }
          const launch = await invoke("launch_material_app", {id:item.id,sessionToken});
          if(t !== epoch) return;
          window.thinkgrowOnlineModule=item.offline_available===false;
          const frame = node("iframe"); frame.title = item.title;
          frame.sandbox = "allow-scripts allow-same-origin";
          frame.referrerPolicy = "no-referrer"; frame.src = launch.url;
          $("game-stage").replaceChildren(frame); $("game-stop").hidden = false; $("game-stop").textContent="앱 닫기";
          $("game-stage").scrollIntoView({behavior:"smooth",block:"start"});
          notice("앱을 실행했습니다.");
        });
      } else if (isInstaller) {
        openBtn = node("button", "설치 파일 실행");
        openBtn.className = "secondary";
        openBtn.disabled = !active();
        openBtn.onclick = () => operation(async t => {
          if (!confirm(`외부 OS 설치 파일(${item.filename})입니다. 서명과 무결성이 검증된 파일만 설치를 진행해야 합니다. 계속하시겠습니까?`)) return;
          notice("설치 파일을 준비하여 운영체제 설치 프로그램으로 넘깁니다...");
          await invoke("open_material_file", { id: item.id });
          if (t === epoch) {
            notice(`설치 프로그램을 실행했습니다 (${item.filename}).`);
          }
        });
      } else {
        openBtn = node("button", "열기");
        openBtn.className = "secondary";
        openBtn.disabled = !active();
        openBtn.onclick = () => operation(async t => {
          notice("자료를 안전한 임시 영역에 복호화하여 엽니다...");
          await invoke("open_material_file", { id: item.id });
          if (t === epoch) {
            notice(`자료를 열었습니다 (${item.filename}). 계정을 잠그면 임시 파일은 즉시 안전하게 파기됩니다.`);
          }
        });
      }

      const delBtn = node("button", "삭제");
      delBtn.className = "secondary";
      delBtn.disabled = !active();
      delBtn.onclick = () => operation(async t => {
        if (!confirm(`'${item.title}' 자료를 기기에서 삭제하시겠습니까?`)) return;
        await invoke("delete_material", { id: item.id });
        if (t === epoch) {
          notice("기기에서 자료를 삭제했습니다.");
          await list(t);
        }
      });

      actions.append(openBtn, delBtn);
      row.append(info, actions);
      container.append(row);
    }

    if (!items.length) {
      container.append(node("p", "보관된 수업 자료가 없습니다. 목록에서 필요한 자료를 내려받으세요."));
    }
  }

  // Chunked resumable download
  async function downloadMaterial(chosen, ticket) {
    const progress = $("material-progress");
    if (progress) {
      progress.hidden = false;
      progress.value = 0;
    }
    notice(`'${chosen.title}' 자료를 내려받고 있습니다...`);

    const chunkSize = 262144; // 256 KB
    let offset = 0;
    let totalSize = chosen.size_bytes || null;
    let expectedSha256 = chosen.source_sha256;

    while (totalSize === null || offset < totalSize) {
      if (ticket !== epoch) return;
      const exchangeData = {
        material_id: chosen.id,
        offset,
        length: chunkSize,
        ...(student ? { assignment_id: chosen.assignment_id } : {})
      };

      const reply = await invoke("role_exchange", {
        action: "download_material_chunk",
        data: exchangeData
      });

      const res = reply?.data || reply;
      if (!res || !res.chunk_data_b64) {
        throw new Error("청크 데이터를 수신하지 못했습니다.");
      }

      totalSize = res.total_size;
      expectedSha256 = res.sha256 || expectedSha256;

      await invoke("save_material_chunk", {
        chunk: {
          material_id: chosen.id,
          offset,
          length: res.length,
          total_size: totalSize,
          sha256: expectedSha256,
          filename: chosen.filename || res.filename || "material.dat",
          chunk_data_b64: res.chunk_data_b64
        },
        info: {
          id: chosen.id,
          version: chosen.version || "1.0",
          subtype: chosen.subtype || "pdf",
          app_kind: chosen.app_kind || null,
          title: chosen.title,
          filename: chosen.filename || res.filename || "material.dat",
          size: totalSize,
          sha256: expectedSha256,
          assignment_id: chosen.assignment_id || null,
          teacher_id: chosen.teacher_id || null,
          sync_at: Date.now(),
          offline_available: chosen.offline_available !== false
        }
      });

      offset += res.length;
      if (progress && totalSize > 0) {
        progress.value = Math.min(100, Math.round((offset / totalSize) * 100));
      }
    }

    if (progress) progress.hidden = true;
    notice(`'${chosen.title}' 자료를 암호화 보관함에 안전하게 내려받았습니다 (해시 검증 완료).`);
    await list(ticket);
  }

  if ($("material-download")) {
    $("material-download").onclick = () => operation(async ticket => {
      const chosen = selectedMaterial();
      if (!chosen) {
        notice("내려받을 자료를 선택하세요.", true);
        return;
      }
      await downloadMaterial(chosen, ticket);
    });
  }

  if ($("material-assign")) {
    $("material-assign").onclick = () => operation(async ticket => {
      const assignment = $("role-assignment")?.value;
      const chosen = selectedMaterial();
      if (!assignment || !chosen) {
        notice("수업과 자료를 먼저 선택하세요.", true);
        return;
      }
      await invoke("role_exchange", {
        action: "assign_material",
        data: {
          assignment_id: assignment,
          material_id: chosen.id
        }
      });
      if (ticket === epoch) {
        notice(`'${chosen.title}' 자료를 수업에 배정했습니다. 학생은 온라인 자료를 새로 받은 뒤 개별 내려받을 수 있습니다.`);
      }
    });
  }

  if ($("material-catalog")) $("material-catalog").onchange = gate;
  if ($("role-assignment")) $("role-assignment").addEventListener("change", gate);

  const workspaceChanged = event => {
    snapshot = event.detail;
    const cat = $("material-catalog");
    if (cat) {
      cat.replaceChildren(new Option("수업 자료를 선택하세요", ""));
      for (const m of snapshot?.data.materials || []) {
        const label = `${subtypeLabel(m.subtype)} · ${m.title} · ${m.version ? 'v' + m.version : ''}${student && m.assignment_id ? ' · 수업 ' + m.assignment_id : ''}`;
        cat.append(new Option(label, catalogKey(m)));
      }
    }
    gate();
    if (session?.local) list().catch(() => notice("보관된 자료를 읽지 못했습니다.", true));
  };

  const sessionChanged = async event => {
    epoch++;
    session = event.detail;
    busy = false;
    snapshot = null;
    $("installed-materials")?.replaceChildren();
    $("material-catalog")?.replaceChildren(new Option("수업 자료를 선택하세요", ""));
    if ($("material-progress")) $("material-progress").hidden = true;
    gate();
    if (session?.local) {
      const ticket = epoch;
      try {
        await list(ticket);
      } catch {
        if (ticket === epoch) notice("보관된 자료를 읽지 못했습니다.", true);
      }
    }
  };

  window.addEventListener("thinkgrow-role-workspace", workspaceChanged);
  window.addEventListener("thinkgrow-session", sessionChanged);
  if (window.thinkgrowSession) {
    sessionChanged({ detail: window.thinkgrowSession }).then(() => {
      if (window.thinkgrowRoleWorkspace) workspaceChanged({ detail: window.thinkgrowRoleWorkspace });
    });
  }
})();
