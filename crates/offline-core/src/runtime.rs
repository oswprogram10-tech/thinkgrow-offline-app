use crate::{crypto::DataKey,App,Answer,Draft,Envelope,Error,Result,Signed,Status,Store,Trust,PROTOCOL,SCHEMA};
use ed25519_dalek::SigningKey;
use serde::Serialize;
use std::{path::PathBuf,sync::{Mutex,MutexGuard},time::{Instant,SystemTime,UNIX_EPOCH}};
pub struct NativeAccount {pub subject:String,pub device:String,pub data_key:DataKey,pub device_signer:SigningKey}
struct ClockAnchor {wall_start:i64,monotonic_start:Instant}
pub struct NativeContext {base:PathBuf,app:App,session:Mutex<Option<Store>>,clock:Mutex<ClockAnchor>}
#[derive(Serialize)]
pub struct CoreStatus {pub app_id:&'static str,pub protocol:u32,pub schema:u32,pub registration_required:bool,
    pub local:Option<Status>,pub default_games:u32}
impl NativeContext {
    pub fn new(base:PathBuf,app:App)->Self{Self{base,app,session:Mutex::new(None),clock:Mutex::new(ClockAnchor{wall_start:now(),monotonic_start:Instant::now()})}}
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
        *self.lock()?=Some(store);Ok(())
    }
    fn checked_now(&self)->i64 {
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
    pub fn lock_account(&self)->Result<()>{*self.lock()?=None;Ok(())}
}
pub fn now()->i64{SystemTime::now().duration_since(UNIX_EPOCH).map(|d|d.as_millis().min(i64::MAX as u128) as i64).unwrap_or(0)}
