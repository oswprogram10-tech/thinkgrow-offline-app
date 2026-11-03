"use strict";
const $ = id => document.getElementById(id);
const invoke = window.__TAURI__?.core?.invoke;
let accountState, busy = false, idle;
function message(text, error = false) { $("notice").textContent = text; $("notice").classList.toggle("error", error); }
function clearSecrets() { document.querySelectorAll('input[type="password"]').forEach(input => { input.value = ""; }); $("backup-file").value = ""; }
function availability() {
  $("unlock-form").querySelector("fieldset").disabled = busy || !accountState?.platform_supported || !accountState?.accounts.some(a => a.enrolled);
  $("register-form").querySelector("fieldset").disabled = busy || !accountState?.platform_supported || !accountState?.service_configured;
  $("change-pin-form").querySelector("fieldset").disabled = busy || !accountState?.platform_supported;
  const selected = accountState?.accounts.find(a => a.handle === $("account").value);
  $("recover-pin-form").querySelector("fieldset").disabled = busy || !accountState?.platform_supported || !selected?.recovery_ready;
  $("restore-backup-form").querySelector("fieldset").disabled = busy || !accountState?.platform_supported;
  $("export-backup-form").querySelector("fieldset").disabled = busy || !accountState?.platform_supported;
  $("lock").disabled = busy;
  $("get-code").disabled = busy || !accountState?.service_configured;
}
async function refreshAccounts() {
  const selected = $("account").value, pending = $("pending").value;
  accountState = await invoke("account_status");
  $("account").replaceChildren(); $("pending").replaceChildren(new Option("새 계정", ""));
  for (const a of accountState.accounts) {
    if (a.enrolled) $("account").append(new Option(a.label, a.handle));
    $("pending").append(new Option(a.label + (a.enrolled ? " · 자격 갱신" : " · 등록 재시도"), a.handle));
  }
  if (!$("account").options.length) $("account").append(new Option("등록된 계정이 없습니다", ""));
  if ([...$("account").options].some(o => o.value === selected)) $("account").value = selected;
  if ([...$("pending").options].some(o => o.value === pending)) $("pending").value = pending;
  availability();
}
function showSession(core) {
  const local = core.local;
  const role=document.body.dataset.role;
  document.querySelector("h1").textContent=local?({teacher:"수업과 채점",student:"내 수업",parent:"자녀 소식"}[role]||"기기 보관함"):"이 기기에서 시작하기";
  document.querySelector(".lead").textContent=local?({teacher:"제출물을 준비해 채점하고 필요한 게임을 수업에 배정하세요.",student:"준비한 수업을 작성하고 배정된 게임을 실행하세요.",parent:"선생님이 공유한 자녀의 학습 기록과 소식을 확인하세요."}[role]||""):"계정마다 수업 자료를 따로 보관합니다. 사용을 마친 뒤에는 계정을 잠가 주세요.";
  for(const detail of document.querySelectorAll("main > details.security-settings"))detail.hidden=!!local;
  $("account-forms").hidden = !!local; $("workspace").hidden = !local;
  window.thinkgrowSession=core;
  window.dispatchEvent(new CustomEvent("thinkgrow-session", {detail:core}));
  if (!local) return;
  $("access-state").textContent = local.access === "read_write" ? "이 계정의 자료를 사용할 수 있습니다." : "자격이 만료되었거나 기기 시간을 확인해야 합니다. 자료는 보존하며 읽기·복구만 허용합니다.";
  const summary = $("local-summary"); summary.replaceChildren();
  for (const [label, count] of [["보관 중인 초안", local.drafts], ["교실 전송 대기", local.pending_classroom], ["온라인 전송 대기", local.pending_online], ["설치한 게임", local.games_unavailable?"확인 필요":local.installed_games]]) {
    const term = document.createElement("dt"), value = document.createElement("dd"); term.textContent = label; value.textContent = count; summary.append(term, value);
  }
}
function errorMessage(error, command = "") {
  const text = String(error);
  if (command === "recover_account_pin" && text.includes("account unlock failed")) return "복구 암호를 확인해 주세요. 여러 번 틀렸다면 잠시 기다린 뒤 다시 시도하세요.";
  if (text.includes("not configured")) return "기기 등록 서비스가 아직 준비 중입니다. 정식판 공개 후 이용할 수 있습니다.";
  if (text.includes("storage is unavailable")) return "이 기기의 보호 저장소 연결을 아직 지원하지 않습니다. 기존 자료는 지우지 않습니다.";
  if (text.includes("temporarily locked")) return "숫자코드를 확인해 주세요. 여러 번 틀렸다면 잠시 기다린 뒤 다시 시도하세요.";
  if (text.includes("request failed")) return "등록을 완료하지 못했습니다. 연결 상태를 확인한 뒤 같은 계정을 선택해 다시 시도하세요.";
  if (text.includes("6 to 12")) return "잠금 숫자코드는 6~12자리 숫자로 입력해 주세요.";
  if (text.includes("no protected recovery backup")) return "이 계정은 백업을 만든 적이 없습니다. PIN 복구에는 먼저 만든 백업 암호가 필요합니다.";
  if (text.includes("backup password or file")) return "백업 파일이나 암호가 맞지 않습니다. 원본 파일을 그대로 두고 다시 확인해 주세요.";
  if (text.includes("backup exceeds size limit")) return "백업이 현재 32 MiB 제한을 넘습니다. 기존 자료는 그대로 유지됩니다.";
  if (text.includes("backup account")) return "백업의 계정 정보가 올바르지 않습니다. 역할이 같은 앱에서 다시 선택해 주세요.";
  if (text.includes("already exists") || text.includes("revision conflict")) return "이 계정 또는 자료가 이미 이 기기에 있습니다. 기존 계정의 백업 암호로 PIN을 복구하거나 다른 대상 기기를 선택해 주세요.";
  return "계정을 열지 못했습니다. 기존 자료는 보존됩니다. 등록 상태와 기기 시간을 확인해 주세요.";
}
async function operation(command, args) {
  if (busy) return;
  busy = true; availability(); message("기기에서 계정을 확인하고 있습니다.");
  try { if(command === "lock_account" || command === "change_account_pin") await window.thinkgrowBeforeLock?.(); const core = await invoke(command, args); showSession(core); message(command === "change_account_pin" ? "새 잠금 숫자코드를 저장했습니다. 새 코드로 계정을 다시 여세요." : command === "recover_account_pin" ? "복구 문구로 새 숫자코드를 저장했습니다. 새 코드로 계정을 열어 주세요." : core.local ? "계정을 열었습니다. 사용을 마친 뒤에는 잠가 주세요." : "계정을 잠갔습니다. 작성한 자료는 그대로 보관합니다."); }
  catch (error) { try { showSession(await invoke("core_status")); } catch { showSession({local:null}); } message(errorMessage(error, command), true); }
  finally { clearSecrets(); busy = false; try { await refreshAccounts(); } catch { message("계정 보관함을 읽지 못했습니다. 앱을 다시 열어 주세요.", true); } availability(); }
}
$("unlock-form").addEventListener("submit", event => { event.preventDefault(); operation("unlock_account", {handle: $("account").value, pin: $("unlock-pin").value}); });
$("register-form").addEventListener("submit", event => { event.preventDefault(); operation("register_account", {code: $("code").value, pin: $("register-pin").value, label: $("label").value, pendingHandle: $("pending").value || null}); });
$("change-pin-form").addEventListener("submit", event => { event.preventDefault(); if ($("new-pin").value !== $("confirm-pin").value) { message("새 숫자코드를 다시 확인해 주세요.", true); $("confirm-pin").focus(); return; } operation("change_account_pin", {handle: $("account").value, currentPin: $("current-pin").value, newPin: $("new-pin").value}); });
$("account").addEventListener("change", availability);
$("recover-pin-form").addEventListener("submit", event => { event.preventDefault(); if ($("recovery-pin").value !== $("recovery-pin-confirm").value) { message("새 숫자코드를 다시 확인해 주세요.", true); $("recovery-pin-confirm").focus(); return; } operation("recover_account_pin", {handle: $("account").value, passphrase: $("recovery-passphrase").value, newPin: $("recovery-pin").value}); });
function base64FromBytes(bytes) { let binary = ""; for (let offset = 0; offset < bytes.length; offset += 0x8000) binary += String.fromCharCode(...bytes.subarray(offset, offset + 0x8000)); return btoa(binary); }
function bytesFromBase64(encoded) { const binary = atob(encoded), bytes = new Uint8Array(binary.length); for (let index = 0; index < binary.length; index++) bytes[index] = binary.charCodeAt(index); return bytes; }
async function saveBackup(encoded) {
  const file = new File([bytesFromBase64(encoded)], `thinkgrow-offline-${document.body.dataset.role}-${new Date().toISOString().slice(0,10)}.tgbak`, {type:"application/octet-stream"});
  if (navigator.canShare?.({files:[file]})) { try { await navigator.share({files:[file],title:"ThinkGrow 암호화 백업"}); return true; } catch (error) { if (error.name !== "AbortError") throw error; return false; } }
  const url = URL.createObjectURL(file), link = document.createElement("a"); link.href = url; link.download = file.name; link.click(); setTimeout(() => URL.revokeObjectURL(url), 60000); return true;
}
$("export-backup-form").addEventListener("submit", async event => {
  event.preventDefault(); if ($("backup-passphrase").value !== $("backup-passphrase-confirm").value) { message("백업 암호를 다시 확인해 주세요.", true); $("backup-passphrase-confirm").focus(); return; }
  if (busy) return; busy = true; availability(); message("자료를 snapshot하고 암호화 백업을 준비합니다.");
  try { await window.thinkgrowBeforeLock?.(); const encoded = await invoke("export_account_backup", {handle:$("account").value,pin:$("backup-current-pin").value,passphrase:$("backup-passphrase").value}); const saved = await saveBackup(encoded); showSession(await invoke("core_status")); message(saved ? "암호화 백업을 저장했습니다. 이 계정은 백업 암호로 PIN도 복구할 수 있습니다." : "파일 저장을 취소했습니다. 백업 암호는 이 기기에서 PIN 복구용으로 설정됐습니다."); }
  catch (error) { try { showSession(await invoke("core_status")); } catch { showSession({local:null}); } message(errorMessage(error), true); }
  finally { clearSecrets(); busy = false; try { await refreshAccounts(); } catch { message("계정 보관함을 읽지 못했습니다. 앱을 다시 열어 주세요.", true); } availability(); }
});
$("restore-backup-form").addEventListener("submit", async event => {
  event.preventDefault(); if ($("restore-pin").value !== $("restore-pin-confirm").value) { message("새 숫자코드를 다시 확인해 주세요.", true); $("restore-pin-confirm").focus(); return; }
  const file = $("backup-file").files?.[0]; if (!file) { message("ThinkGrow 백업 파일을 선택해 주세요.", true); return; }
  if (file.size > 96 * 1024 * 1024) { message("백업 파일이 현재 크기 제한을 넘습니다. 기존 자료는 변경하지 않았습니다.", true); return; }
  if (busy) return; busy = true; availability(); message("백업 파일을 확인하고 기기 보호 저장소에 복원합니다.");
  try { const raw = new Uint8Array(await file.arrayBuffer()); const handle = await invoke("restore_account_backup", {backupData:base64FromBytes(raw),passphrase:$("restore-passphrase").value,newPin:$("restore-pin").value}); showSession(await invoke("core_status")); await refreshAccounts(); $("account").value = handle; $("pending").value = handle; message("복원했습니다. 온라인 등록 코드를 받아 같은 계정을 갱신하세요. 새 epoch가 발급되면 서버가 epoch를 확인하는 요청은 이전 자격을 거부합니다."); }
  catch (error) { message(errorMessage(error), true); }
  finally { clearSecrets(); busy = false; try { await refreshAccounts(); } catch { message("계정 보관함을 읽지 못했습니다. 앱을 다시 열어 주세요.", true); } availability(); }
});
$("pending").addEventListener("change", () => { $("label").required = !$("pending").value; });
$("get-code").addEventListener("click", async () => { try { await invoke("open_registration_page"); message("브라우저에서 등록 코드를 받은 뒤 이 앱에 입력해 주세요."); } catch { message("브라우저를 열지 못했습니다. 설치판 다운로드 페이지에서 기기 등록을 열어 주세요.", true); } });
$("lock").addEventListener("click", () => operation("lock_account", {}));
// Never restore an unlocked session across app restarts or backgrounding.
async function backgroundLock() {
  clearSecrets(); if (!invoke) return;
  $("workspace").hidden = true;
  let saved=true;try { await window.thinkgrowBeforeLock?.(); } catch { saved=false; }
  try { showSession(await invoke("lock_account")); message(saved?"계정을 잠갔습니다. 숫자코드로 다시 열어 주세요.":"계정을 잠갔습니다. 마지막 편집을 저장하지 못했을 수 있습니다. 다시 열어 저장 상태를 확인하세요.",!saved); } catch { message("계정 잠금을 확인하지 못했습니다. 앱을 닫아 주세요.", true); }
}
document.addEventListener("visibilitychange", () => { if (document.hidden) backgroundLock(); });
for (const name of ["pointerdown", "keydown"]) document.addEventListener(name, () => { clearTimeout(idle); idle = setTimeout(backgroundLock, 5 * 60 * 1000); }, {passive:true});
(async () => {
  if (!invoke) { message("계정 등록과 자료 보관은 설치한 앱에서 사용할 수 있습니다."); return; }
  try {
    await refreshAccounts(); const core=await invoke("core_status");showSession(core);
    message(core.local?"계정을 열었습니다. 사용을 마친 뒤에는 잠가 주세요.":!accountState.platform_supported ? "이 기기의 보호 저장소 연결을 준비 중입니다." : !accountState.service_configured ? "기기 등록 서비스를 준비 중입니다. 정식판 공개 후 새 계정을 등록할 수 있습니다." : "저장된 계정을 열거나 이 기기를 등록하세요.");
  } catch { message("기기 보관함을 확인하지 못했습니다. 기존 자료는 지우지 않습니다.", true); }
})();
