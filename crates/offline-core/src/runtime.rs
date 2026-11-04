use crate::{crypto::DataKey,Access,App,Answer,Destination,Draft,Envelope,Error,Result,Signed,Status,Store,SubmissionState,Trust,PROTOCOL,SCHEMA};
use crate::transport::{DeliverySummary,Transport};
use ed25519_dalek::SigningKey;
use serde::Serialize;
use std::{path::PathBuf,sync::{atomic::{AtomicU64,Ordering},Mutex,MutexGuard},time::{Instant,SystemTime,UNIX_EPOCH}};
pub struct NativeAccount {pub subject:String,pub device:String,pub data_key:DataKey,pub device_signer:SigningKey}
struct ClockAnchor {wall_start:i64,monotonic_start:Instant}
pub struct NativeContext {base:PathBuf,app:App,session:Mutex<Option<Store>>,session_generation:AtomicU64,clock:Mutex<ClockAnchor>,game:Mutex<Option<crate::game_host::GameHost>>,ephemeral_files:Mutex<Vec<PathBuf>>}
#[derive(Serialize)]
pub struct CoreStatus {pub app_id:&'static str,pub protocol:u32,pub schema:u32,pub registration_required:bool,
    pub local:Option<Status>,pub default_games:u32}
impl NativeContext {
    pub fn new(base:PathBuf,app:App)->Self{crate::class_materials::cleanup_stale_ephemeral(app);Self{base,app,session:Mutex::new(None),session_generation:AtomicU64::new(0),clock:Mutex::new(ClockAnchor{wall_start:now(),monotonic_start:Instant::now()}),game:Mutex::new(None),ephemeral_files:Mutex::new(Vec::new())}}
    /// Called only by a native registration/reauthentication adapter with vault-held keys.
    /// This is deliberately not exposed as a Tauri command.
    pub fn attach_registered_account(&self,account:NativeAccount,trust:Trust,grant:Option<&Signed>,now:i64,trusted_time:Option<i64>)->Result<()> {
        let store_now=trusted_time.unwrap_or(now);
        let mut store=Store::open(&self.base,self.app,&account.subject,&account.device,account.data_key,account.device_signer,trust)?;
        if let Some(g)=grant {store.install_grant(g,store_now)?;}
        if store.access(store_now)?==crate::Access::ProvisioningRequired {return Err(Error::Authorization);}
        if let Some(server_time)=trusted_time {
            *self.clock.lock().map_err(|_|Error::Invalid("trusted clock unavailable"))?=ClockAnchor{wall_start:server_time,monotonic_start:Instant::now()};
        }
        {
            let mut session=self.lock()?;
            *session=Some(store);
            self.session_generation.fetch_add(1,Ordering::SeqCst);
        }
        Ok(())
    }
    pub(crate) fn checked_now(&self)->i64 {
        let actual=now();
        let Ok(anchor)=self.clock.lock() else{return actual};
        let expected=anchor.wall_start.saturating_add(anchor.monotonic_start.elapsed().as_millis().min(i64::MAX as u128) as i64);
        actual.max(expected)
    }
    fn lock(&self)->Result<MutexGuard<'_,Option<Store>>>{self.session.lock().map_err(|_|Error::Invalid("native session unavailable"))}
    pub fn status(&self)->Result<CoreStatus>{
        let current=self.checked_now();let session=self.lock()?;let local=session.as_ref().map(|s|s.status(current)).transpose()?;
        Ok(CoreStatus{app_id:self.app.id(),protocol:PROTOCOL,schema:SCHEMA,
            registration_required:local.is_none(),local,default_games:0})
    }
    pub fn load(&self,assignment:&str)->Result<Option<Draft>>{
        self.lock()?.as_ref().ok_or(Error::Authorization)?.load_draft(assignment)
    }
    pub fn save(&self,answer:&Answer,expected:u64)->Result<u64>{
        let current=self.checked_now();self.lock()?.as_mut().ok_or(Error::Authorization)?.save_draft(answer,expected,current)
    }
    pub fn submit(&self,assignment:&str,revision:u64,intent:&str)->Result<Envelope>{
        let intent=uuid::Uuid::parse_str(intent).map_err(|_|Error::Invalid("submission intent"))?;
        let current=self.checked_now();self.lock()?.as_mut().ok_or(Error::Authorization)?.submit(assignment,revision,intent,current)
    }
    pub fn lessons(&self)->Result<Vec<crate::lesson::Lesson>> {self.lock()?.as_ref().ok_or(Error::Authorization)?.lessons()}
    pub fn prepare_lessons(&self,fetch:impl FnOnce(&[u8])->Result<Vec<Signed>>)->Result<Vec<crate::lesson::Lesson>> {
        let current=self.checked_now();
        // Hold the native session lock across transport: a background account lock
        // waits for this operation, so a reply cannot attach to another account.
        let guard=self.lock()?;let store=guard.as_ref().ok_or(Error::Authorization)?;
        let request=serde_json::to_vec(&store.lesson_request(current)?)?;
        let signed=fetch(&request)?;store.install_lessons(&signed,current)?;store.lessons()
    }
    pub fn save_prepared(&self,answer:&Answer,expected:u64)->Result<u64>{let current=self.checked_now();self.lock()?.as_mut().ok_or(Error::Authorization)?.save_prepared_draft(answer,expected,current)}
    pub fn submit_prepared(&self,assignment:&str,revision:u64,intent:&str)->Result<Envelope>{
        let intent=uuid::Uuid::parse_str(intent).map_err(|_|Error::Invalid("submission intent"))?;
        let current=self.checked_now();self.lock()?.as_mut().ok_or(Error::Authorization)?.submit_prepared(assignment,revision,intent,current)
    }
    pub(crate) fn credential(&self)->Result<Signed> {
        self.lock()?.as_ref().ok_or(Error::Authorization)?.signed_grant()
    }
    pub(crate) fn deliver_online(&self,transport:&mut dyn Transport)->Result<DeliverySummary> {
        let mut summary=DeliverySummary::default();
        let current=self.checked_now();
        {
            let mut session=self.lock()?;
            let Some(store)=session.as_mut() else{return Ok(summary)};
            if self.app!=App::Student || store.access(current)?!=Access::ReadWrite {
                return Ok(summary);
            }
        }
        // Network I/O must not hold the native session lock: drafts, lesson
        // changes and account locking stay responsive during a delivery pass.
        for _ in 0..20 {
            let now=self.checked_now();
            let (lease,generation)={
                let mut session=self.lock()?;
                let Some(store)=session.as_mut() else{return Ok(summary)};
                let lease=match store.lease(Destination::Online,now) {
                    Ok(Some(lease))=>lease,
                    Ok(None)=>break,
                    Err(Error::Authorization)|Err(Error::ReadRecoveryOnly)=>return Ok(summary),
                    Err(error)=>return Err(error),
                };
                (lease,self.session_generation.load(Ordering::SeqCst))
            };
            let reply=transport.send(Destination::Online,&lease.envelope);
            let settled=self.checked_now();
            let mut session=self.lock()?;
            if self.session_generation.load(Ordering::SeqCst)!=generation {break}
            let Some(store)=session.as_mut() else{break};
            match crate::transport::settle(store,&lease,reply,settled)? {
                crate::transport::Tick::Idle=>break,
                crate::transport::Tick::Received=>summary.received=summary.received.saturating_add(1),
                crate::transport::Tick::Deferred=>summary.deferred=summary.deferred.saturating_add(1),
                crate::transport::Tick::AuthenticationRequired=>summary.auth_required=summary.auth_required.saturating_add(1),
                crate::transport::Tick::Conflict=>summary.conflicts=summary.conflicts.saturating_add(1),
            }
        }
        let mut session=self.lock()?;
        if let Some(store)=session.as_mut(){summary.remaining=store.pending(Destination::Online)?;}
        Ok(summary)
    }
    pub fn submission_states(&self,assignment:&str)->Result<Vec<SubmissionState>> {
        self.lock()?.as_ref().ok_or(Error::Authorization)?.submission_states(assignment)
    }
    pub fn role_workspace(&self)->Result<Option<crate::workspace::RoleReply>>{self.lock()?.as_ref().ok_or(Error::Authorization)?.role_workspace()}
    pub fn role_drafts(&self)->Result<Vec<crate::workspace::RoleDraft>>{self.lock()?.as_ref().ok_or(Error::Authorization)?.role_drafts()}
    pub fn save_role_draft(&self,kind:&str,data:serde_json::Value,expected:u64)->Result<crate::workspace::RoleDraft>{self.lock()?.as_ref().ok_or(Error::Authorization)?.save_role_draft(kind,data,expected,self.checked_now())}
    pub fn role_exchange(&self,action:&str,data:serde_json::Value,fetch:impl FnOnce(&[u8])->Result<Signed>)->Result<crate::workspace::RoleReply>{
        let session=self.lock()?;let store=session.as_ref().ok_or(Error::Authorization)?;let id=uuid::Uuid::new_v4().to_string();
        if !["workspace","install_game","assign_game","parent_read","notice_read","notice_reply","download_material_chunk","assign_material","game_session","material_session"].contains(&action){return Err(Error::Authorization)}
        let request=store.role_request(action,data,&id,self.checked_now())?;let reply=fetch(&request)?;store.accept_role_reply(&reply,action,&id,self.checked_now())
    }
    pub fn sync_role_draft(&self,slot:&str,revision:u64,fetch:impl FnOnce(&[u8])->Result<Signed>)->Result<crate::workspace::RoleDraft>{
        let session=self.lock()?;let store=session.as_ref().ok_or(Error::Authorization)?;let draft=store.role_draft(slot,revision)?;
        if draft.state=="synced"{return Ok(draft)}
        let request=store.role_request(&draft.kind,draft.data.clone(),&draft.operation_id,self.checked_now())?;
        let signed=match fetch(&request){Ok(s)=>s,Err(Error::Conflict)=>{store.finish_role_draft(draft,None)?;return Err(Error::Conflict)},Err(e)=>return Err(e)};
        store.accept_role_reply(&signed,&draft.kind,&draft.operation_id,self.checked_now())?;store.finish_role_draft(draft,Some(signed))
    }
    pub fn installed_games(&self)->Result<Vec<crate::workspace::GameInfo>>{self.lock()?.as_ref().ok_or(Error::Authorization)?.installed_games()}
    pub fn launch_game(self:&std::sync::Arc<Self>,id:&str)->Result<crate::game_host::Launch>{
        self.launch_game_for(id,None)
    }
    pub fn launch_game_for(self:&std::sync::Arc<Self>,id:&str,assignment:Option<&str>)->Result<crate::game_host::Launch>{
        self.stop_game()?;let (run,bundle)=self.lock()?.as_ref().ok_or(Error::Authorization)?.start_game_for(id,assignment,self.checked_now())?;
        let (host,launch)=crate::game_host::GameHost::start(std::sync::Arc::downgrade(self),self.app,run,bundle,None,vec![])?;
        *self.game.lock().map_err(|_|Error::Authorization)?=Some(host);Ok(launch)
    }
    pub fn stop_game(&self)->Result<()>{*self.game.lock().map_err(|_|Error::Authorization)?=None;Ok(())}
    pub fn launch_material_app(self:&std::sync::Arc<Self>,id:&str,token:Option<&str>)->Result<crate::game_host::Launch>{
        self.stop_game()?;
        let (run,bundle,origins)=self.lock()?.as_ref().ok_or(Error::Authorization)?.start_material_app(id,token,self.checked_now())?;
        let (host,launch)=crate::game_host::GameHost::start(std::sync::Arc::downgrade(self),self.app,run,bundle,token.map(str::to_owned),origins)?;
        *self.game.lock().map_err(|_|Error::Authorization)?=Some(host);Ok(launch)
    }
    pub fn game_run(&self,id:&str)->Result<crate::workspace::GameRun>{self.lock()?.as_ref().ok_or(Error::Authorization)?.game_run(id,self.checked_now())}
    pub fn record_game_event(&self,id:&str,event:serde_json::Value)->Result<serde_json::Value>{self.lock()?.as_ref().ok_or(Error::Authorization)?.game_event(id,event,self.checked_now())}
    pub fn list_materials(&self)->Result<Vec<crate::MaterialInfo>>{self.lock()?.as_ref().ok_or(Error::Authorization)?.list_materials()}
    pub fn save_material_chunk(&self,chunk:&crate::MaterialChunk,info:&crate::MaterialInfo)->Result<bool>{self.lock()?.as_ref().ok_or(Error::Authorization)?.save_material_chunk(chunk,info,self.checked_now())}
    pub fn get_material(&self,id:&str)->Result<Option<crate::MaterialInfo>>{self.lock()?.as_ref().ok_or(Error::Authorization)?.get_material(id)}
    pub fn delete_material(&self,id:&str)->Result<()>{self.lock()?.as_ref().ok_or(Error::Authorization)?.delete_material(id)}
    pub fn open_material_file(&self,id:&str)->Result<PathBuf>{
        let guard=self.lock()?;let store=guard.as_ref().ok_or(Error::Authorization)?;
        let info=store.get_material(id)?.ok_or(Error::Authorization)?;
        if info.subtype=="app" {
            if !matches!(info.app_kind.as_deref(),Some("installer"|"os_installer")){return Err(Error::Authorization)}
            store.authorize_installer(&info,self.checked_now())?;
        }
        let (_info,path)=store.export_ephemeral_material(id)?;
        if let Ok(mut files) = self.ephemeral_files.lock() { files.push(path.clone()); }
        Ok(path)
    }
    pub fn launch_online_game_for(self:&std::sync::Arc<Self>,id:&str,assignment:Option<&str>,session_token:&str)->Result<crate::game_host::Launch>{
        self.stop_game()?;let (run,bundle,ticket)=self.lock()?.as_ref().ok_or(Error::Authorization)?.start_online_game_for(id,assignment,session_token,self.checked_now())?;
        let (host,launch)=crate::game_host::GameHost::start(std::sync::Arc::downgrade(self),self.app,run,bundle,Some(session_token.to_string()),ticket.allowed_origins)?;
        *self.game.lock().map_err(|_|Error::Authorization)?=Some(host);Ok(launch)
    }
    pub fn cleanup_ephemeral(&self) {
        if let Ok(mut files) = self.ephemeral_files.lock() {
            for p in files.drain(..) {
                let _ = std::fs::remove_file(&p);
                if let Some(parent) = p.parent() { let _ = std::fs::remove_dir(parent); }
            }
        }
    }
    pub fn lock_account(&self)->Result<()>{
        self.stop_game()?;self.cleanup_ephemeral();
        let mut session=self.lock()?;
        *session=None;
        self.session_generation.fetch_add(1,Ordering::SeqCst);
        Ok(())
    }
}
impl Drop for NativeContext {
    fn drop(&mut self) { self.cleanup_ephemeral(); }
}
pub fn now()->i64{SystemTime::now().duration_since(UNIX_EPOCH).map(|d|d.as_millis().min(i64::MAX as u128) as i64).unwrap_or(0)}
