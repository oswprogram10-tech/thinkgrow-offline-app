//! Native-only enrollment: fixed HTTPS origin, pinned issuer keys, private device keys.
use crate::{backup::{self,BackupExport,BackupFile},crypto::{self,DataKey},runtime::{now,CoreStatus,NativeAccount,NativeContext},vault::{LocalAccount,Opened,Secrets,Vault,OsProtector,SystemProtector},App,Error,Grant,Result,Signed,Store,Trust,PROTOCOL,WEEK_MS};
use base64::{engine::general_purpose::STANDARD,Engine};
use ed25519_dalek::SigningKey;
use serde::{Deserialize,Serialize};
use std::{collections::{BTreeMap,BTreeSet},fs::{self,OpenOptions},io::Write,path::{Path,PathBuf},sync::{Arc,Mutex,MutexGuard}};
use zeroize::Zeroizing;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Policy { #[serde(default)] registration_enabled:bool, format:u32,service_origin:String,credential_keys:BTreeMap<String,String>,credential_signing_key_ids:BTreeSet<String>,classroom_receipt_keys:BTreeMap<String,String>,online_receipt_keys:BTreeMap<String,String> }
#[derive(Serialize)]
pub struct AccountStatus {pub platform_supported:bool,pub service_configured:bool,pub accounts:Vec<LocalAccount>}
pub trait EnrollmentClient:Send+Sync { fn redeem(&self,request_json:&[u8])->Result<Signed>;
    fn lessons(&self,_request_json:&[u8])->Result<Vec<Signed>> {Err(Error::ServiceNotConfigured)}
    fn role_exchange(&self,_request_json:&[u8])->Result<Signed> {Err(Error::ServiceNotConfigured)}
}
pub struct Registration {base:PathBuf,vault:Vault,app:App,context:Arc<NativeContext>,trust:Trust,active_credential_keys:BTreeSet<String>,origin:String,enrollment_enabled:bool,transport:Option<Arc<dyn EnrollmentClient>>,operation:Mutex<()>}
impl Registration {
    pub fn new(base:&Path,app:App,context:Arc<NativeContext>)->Result<Self> {
        Self::with_adapters(base,app,context,Arc::new(SystemProtector),None)
    }
    pub fn with_adapters(base:&Path,app:App,context:Arc<NativeContext>,protector:Arc<dyn OsProtector>,transport:Option<Arc<dyn EnrollmentClient>>)->Result<Self> {
        let p:Policy=serde_json::from_str(include_str!("../../../release/native-policy.json"))?;
        if p.format!=1 || p.service_origin!="https://schoolclassscorecheck.web.app" {return Err(Error::ServiceNotConfigured)}
        let trust=Trust{credentials:keys(p.credential_keys)?,classroom_receipts:keys(p.classroom_receipt_keys)?,online_receipts:keys(p.online_receipt_keys)?};
        if p.credential_signing_key_ids.iter().any(|id|!trust.credentials.contains_key(id)) {return Err(Error::ServiceNotConfigured)}
        Ok(Self{base:base.to_path_buf(),vault:Vault::with_protector(base,app,protector),app,context,trust,active_credential_keys:p.credential_signing_key_ids,origin:p.service_origin,enrollment_enabled:p.registration_enabled,transport,operation:Mutex::new(())})
    }
    fn guard(&self)->Result<MutexGuard<'_,()>> {self.operation.lock().map_err(|_|Error::AccountLocked)}
    pub fn status(&self)->Result<AccountStatus> {
        Ok(AccountStatus{platform_supported:self.vault.available(),service_configured:self.enrollment_enabled && !self.active_credential_keys.is_empty(),accounts:self.vault.list()?})
    }
    pub fn prepare_lessons(&self)->Result<Vec<crate::lesson::Lesson>> {
        let _guard=self.guard()?;
        self.context.prepare_lessons(|request|if let Some(transport)=&self.transport {transport.lessons(request)}else{fetch_lessons(&self.origin,request)})
    }
    pub fn role_exchange(&self,action:&str,data:serde_json::Value)->Result<crate::workspace::RoleReply>{
        let _guard=self.guard()?;self.context.role_exchange(action,data,|request|self.fetch_role(request))
    }
    pub fn sync_role_draft(&self,slot:&str,revision:u64)->Result<crate::workspace::RoleDraft>{
        let _guard=self.guard()?;self.context.sync_role_draft(slot,revision,|request|self.fetch_role(request))
    }
    fn fetch_role(&self,request:&[u8])->Result<Signed>{
        let signed=if let Some(transport)=&self.transport{transport.role_exchange(request)}else{fetch_role(&self.origin,request)}?;
        if !self.active_credential_keys.contains(&signed.key_id){return Err(Error::Signature)}Ok(signed)
    }
    pub fn lock(&self)->Result<CoreStatus> {let _guard=self.guard()?;self.context.lock_account()?;self.context.status()}
    pub fn unlock(&self,handle:&str,pin:String)->Result<CoreStatus> {
        let pin=Zeroizing::new(pin); let _guard=self.guard()?;
        self.context.lock_account()?;
        let opened=self.vault.open(handle,&pin)?;
        self.attach(&opened,None)?;self.context.status()
    }
    pub fn change_pin(&self,handle:&str,current_pin:String,new_pin:String)->Result<CoreStatus> {
        let current_pin=Zeroizing::new(current_pin);let new_pin=Zeroizing::new(new_pin);let _guard=self.guard()?;
        self.context.lock_account()?;
        self.vault.change_pin(handle,&current_pin,&new_pin)?;
        self.context.status()
    }
    pub fn recover_pin(&self,handle:&str,passphrase:String,new_pin:String)->Result<CoreStatus> {
        let passphrase=Zeroizing::new(passphrase);let new_pin=Zeroizing::new(new_pin);let _guard=self.guard()?;
        self.context.lock_account()?;self.vault.recover_pin(handle,&passphrase,&new_pin)?;self.context.status()
    }
    pub fn export_backup(&self,handle:&str,pin:String,passphrase:String)->Result<String> {
        let pin=Zeroizing::new(pin);let passphrase=Zeroizing::new(passphrase);backup::validate_passphrase(&passphrase)?;
        let _guard=self.guard()?;self.context.lock_account()?;
        let mut opened=self.vault.open(handle,&pin)?;
        if let Some(signed)=&opened.secrets.grant {self.verify(&opened.secrets,signed,false)?;}
        let files=if let Some(subject)=&opened.secrets.subject {
            ensure_plain_directory(&self.base)?;
            let scratch=self.base.join(format!(".thinkgrow-backup-{}",uuid::Uuid::new_v4()));fs::create_dir(&scratch)?;let _scratch=StagingDir(scratch.clone());
            collect_backup_files(&Store::home_path(&self.base,self.app,subject,&opened.secrets.device_id)?,&scratch)?
        }else{Vec::new()};
        let payload=BackupExport{format:1,app_id:self.app.id().into(),handle:opened.handle().into(),label:opened.label().into(),
            enrolled:opened.secrets.subject.is_some() && opened.secrets.grant.is_some(),wall_floor:opened.wall_floor(),secrets:&opened.secrets,files:&files};
        let encrypted=backup::encode(self.app,&passphrase,&payload)?;
        self.vault.configure_recovery(&mut opened,&passphrase)?;
        Ok(STANDARD.encode(encrypted))
    }
    pub fn restore_backup(&self,backup_data:String,passphrase:String,new_pin:String)->Result<String> {
        let backup_data=Zeroizing::new(backup_data);let passphrase=Zeroizing::new(passphrase);let new_pin=Zeroizing::new(new_pin);
        if backup_data.len()>backup::MAX_BACKUP.saturating_mul(4)/3+8 {return Err(Error::Invalid("backup exceeds size limit"))}
        backup::validate_passphrase(&passphrase)?;let _guard=self.guard()?;self.context.lock_account()?;
        let bytes=STANDARD.decode(backup_data.as_bytes()).map_err(|_|Error::Invalid("backup file"))?;
        let payload=backup::decode(self.app,&passphrase,&bytes)?;
        if payload.format!=1 || payload.app_id!=self.app.id() || payload.wall_floor<0 || payload.files.len()>backup::MAX_FILES ||
            payload.handle!=uuid::Uuid::parse_str(&payload.handle).map_err(|_|Error::Invalid("backup account"))?.to_string() ||
            payload.secrets.device_id!=uuid::Uuid::parse_str(&payload.secrets.device_id).map_err(|_|Error::Invalid("backup account"))?.to_string() ||
            payload.label.trim().is_empty() || payload.label.chars().count()>80 || payload.label.chars().any(char::is_control) {return Err(Error::Invalid("backup account"))}
        let enrolled=payload.secrets.subject.is_some() && payload.secrets.grant.is_some();
        if payload.enrolled!=enrolled || payload.secrets.subject.is_some()!=payload.secrets.grant.is_some() {return Err(Error::Partition)}
        let verified_grant=if let Some(signed)=&payload.secrets.grant {
            let grant=self.verify(&payload.secrets,signed,false)?;
            if payload.secrets.subject.as_deref()!=Some(grant.subject_id.as_str()) {return Err(Error::Partition)}
            Some(grant)
        }else{None};
        if let Some(subject)=&payload.secrets.subject {
            crate::contract::id(subject)?;
        }
        if self.vault.list()?.iter().any(|account|account.handle==payload.handle) {return Err(Error::Conflict)}
        if payload.secrets.subject.is_none() && !payload.files.is_empty() {return Err(Error::Partition)}
        let mut final_home=None;
        if let Some(subject)=&payload.secrets.subject {
            if !payload.files.iter().any(|file|file.path=="store.sqlite3") {return Err(Error::Invalid("backup store missing"))}
            let mut total=0usize;let mut seen=BTreeSet::new();
            let stage_root=self.base.join(format!(".thinkgrow-restore-{}",uuid::Uuid::new_v4()));
            ensure_plain_directory(&self.base)?;fs::create_dir(&stage_root)?;let _staging=StagingDir(stage_root.clone());
            let stage_dir=Store::home_path(&stage_root,self.app,subject,&payload.secrets.device_id)?;
            fs::create_dir_all(stage_dir.join("blobs"))?;
            for file in &payload.files {
                let relative=backup_relative_path(&file.path)?;
                if !seen.insert(file.path.clone()) {return Err(Error::Invalid("duplicate backup file"))}
                let clear=STANDARD.decode(file.content.as_bytes()).map_err(|_|Error::Invalid("backup file"))?;
                total=total.checked_add(clear.len()).filter(|size|*size<=backup::MAX_SOURCE).ok_or(Error::Invalid("backup exceeds size limit"))?;
                let destination=stage_dir.join(relative);let mut output=OpenOptions::new().write(true).create_new(true).open(destination)?;
                output.write_all(&clear)?;output.sync_all()?;
            }
            let grant=payload.secrets.grant.as_ref().ok_or(Error::Authorization)?;
            let mut store=Store::open(&stage_root,self.app,payload.secrets.subject.as_deref().ok_or(Error::Partition)?,&payload.secrets.device_id,
                DataKey::new(payload.secrets.data_key),SigningKey::from_bytes(&payload.secrets.signer_seed),self.trust.clone())?;
            store.install_grant(grant,payload.wall_floor.max(verified_grant.as_ref().ok_or(Error::Authorization)?.issued_ms))?;
            if stage_dir.join("lessons.sqlite3").exists() {store.lessons()?;}
            if stage_dir.join("role.sqlite3").exists() {store.role_workspace()?;store.role_drafts()?;}
            drop(store);
            let target=Store::home_path(&self.base,self.app,subject,&payload.secrets.device_id)?;
            if target.exists() {return Err(Error::Conflict)}
            let parent=target.parent().ok_or(Error::VaultUnavailable)?;ensure_plain_directory(&self.base)?;ensure_plain_directory(parent)?;
            fs::rename(&stage_dir,&target)?;final_home=Some(target);
        }
        let imported=self.vault.import_account(&payload.handle,&payload.label,payload.enrolled,payload.wall_floor,payload.secrets,&new_pin,&passphrase);
        if let Err(error)=imported {
            if let Some(path)=final_home {let _=fs::remove_dir_all(path);}
            return Err(error)
        }
        Ok(payload.handle)
    }
    pub fn register(&self,code:String,pin:String,label:String,pending:Option<String>)->Result<CoreStatus> {
        self.register_with(code,pin,label,pending,|request| {
            if let Some(transport)=&self.transport {transport.redeem(&serde_json::to_vec(request)?)}
            else {redeem(&self.origin,request)}
        })
    }
    fn register_with(&self,code:String,pin:String,label:String,pending:Option<String>,send:impl FnOnce(&Redeem<'_>)->Result<Signed>)->Result<CoreStatus> {
        let code=Zeroizing::new(code);let pin=Zeroizing::new(pin);let _guard=self.guard()?;
        if !self.vault.available() {return Err(Error::VaultUnavailable)}
        if !self.enrollment_enabled || self.active_credential_keys.is_empty() {return Err(Error::ServiceNotConfigured)}
        if code.len()<12 || code.len()>256 || !code.bytes().all(|b|b.is_ascii_alphanumeric() || b==b'-' || b==b'_') {return Err(Error::Invalid("registration code"))}
        self.context.lock_account()?;
        let mut opened=if let Some(handle)=pending {self.vault.open(&handle,&pin)?}else{self.vault.create(&pin,&label)?};
        let signer=SigningKey::from_bytes(&opened.secrets.signer_seed);
        let request=Redeem{protocol:PROTOCOL,app_id:self.app.id(),registration_code:&code,
            request_id:opened.handle(),device_id:&opened.secrets.device_id,
            device_public_key:crypto::b64(&signer.verifying_key().to_bytes())};
        // Pending keys are durable before the network request. Retry with the same
        // handle reuses device/request IDs; a lost response must not orphan the key.
        let signed=send(&request)?;
        let grant=self.verify(&opened.secrets,&signed,true)?;
        let mut trusted_time=None;
        if let Some(old_signed)=&opened.secrets.grant {
            let old:Grant=old_signed.verify(&self.trust.credentials,"thinkgrow.credential.v1")?;
            if grant.epoch<old.epoch || grant.subject_id!=old.subject_id || (grant.epoch==old.epoch && signed!=*old_signed) {return Err(Error::Authorization)}
            if grant.epoch>old.epoch {trusted_time=Some(grant.issued_ms)}
        } else {
            trusted_time=Some(grant.issued_ms)
        }
        opened.secrets.subject=Some(grant.subject_id);
        opened.secrets.grant=Some(signed);
        self.vault.save_at(&mut opened,trusted_time)?;
        self.attach(&opened,trusted_time)?;self.context.status()
    }
    fn verify(&self,secrets:&Secrets,signed:&Signed,fresh:bool)->Result<Grant> {
        let g:Grant=signed.verify(&self.trust.credentials,"thinkgrow.credential.v1")?;
        if fresh && !self.active_credential_keys.contains(&signed.key_id) {return Err(Error::Signature)}
        let key=SigningKey::from_bytes(&secrets.signer_seed).verifying_key().to_bytes();
        if g.protocol!=PROTOCOL || g.app_id!=self.app.id() || g.device_id!=secrets.device_id ||
            g.device_public_key!=crypto::b64(&key) || g.issued_ms<0 || g.expires_ms<=g.issued_ms ||
            g.expires_ms-g.issued_ms>WEEK_MS || g.epoch>i64::MAX as u64 || g.scopes.len()>1024 {return Err(Error::Authorization)}
        crate::contract::id(&g.subject_id)?; crate::contract::id(&g.device_id)?;
        for s in &g.scopes {crate::contract::id(&s.school_id)?;crate::contract::id(&s.class_id)?;crate::contract::id(&s.assignment_id)?;}
        if secrets.subject.as_ref().is_some_and(|s|s!=&g.subject_id) {return Err(Error::Partition)}
        Ok(g)
    }
    fn attach(&self,opened:&Opened,trusted_time:Option<i64>)->Result<()> {
        let signed=opened.secrets.grant.as_ref().ok_or(Error::Authorization)?;
        let grant=self.verify(&opened.secrets,signed,false)?;
        let attachment_time=trusted_time.unwrap_or_else(||now().max(opened.wall_floor()));
        self.context.attach_registered_account(NativeAccount{subject:grant.subject_id,device:opened.secrets.device_id.clone(),
            data_key:DataKey::new(opened.secrets.data_key),device_signer:SigningKey::from_bytes(&opened.secrets.signer_seed)},
            self.trust.clone(),Some(signed),attachment_time,trusted_time)
    }
}
fn collect_backup_files(home:&Path,scratch:&Path)->Result<Vec<BackupFile>> {
    let metadata=match fs::symlink_metadata(home) {Ok(meta)=>meta,Err(error) if error.kind()==std::io::ErrorKind::NotFound=>return Ok(Vec::new()),Err(error)=>return Err(error.into())};
    if !metadata.is_dir() || metadata.file_type().is_symlink() {return Err(Error::VaultUnavailable)}
    let mut files=Vec::new();let mut total=0usize;
    for entry in fs::read_dir(home)? {
        let entry=entry?;let path=entry.path();let name=entry.file_name().into_string().map_err(|_|Error::VaultUnavailable)?;
        let meta=fs::symlink_metadata(&path)?;
        if name=="blobs" {
            if !meta.is_dir() || meta.file_type().is_symlink() {return Err(Error::VaultUnavailable)}
            for blob in fs::read_dir(&path)? {
                let blob=blob?;let leaf=blob.file_name().into_string().map_err(|_|Error::VaultUnavailable)?;
                let relative=format!("blobs/{leaf}");backup_relative_path(&relative)?;
                add_backup_file(&blob.path(),relative,&mut files,&mut total)?;
            }
        }else if name=="store.sqlite3" || name=="lessons.sqlite3" || name=="role.sqlite3" {
            let destination=scratch.join(&name);snapshot_database(&path,&destination)?;
            add_backup_file(&destination,name,&mut files,&mut total)?;
        }else if ["store.sqlite3-wal","store.sqlite3-shm","store.sqlite3-journal","lessons.sqlite3-wal","lessons.sqlite3-shm","lessons.sqlite3-journal","role.sqlite3-wal","role.sqlite3-shm","role.sqlite3-journal","games.sqlite3","games.sqlite3-wal","games.sqlite3-shm","games.sqlite3-journal"].contains(&name.as_str()) {
            if !meta.is_file() || meta.file_type().is_symlink() {return Err(Error::VaultUnavailable)}
        }else{
            return Err(Error::Invalid("unsupported account file"));
        }
    }
    files.sort_by(|left,right|left.path.cmp(&right.path));Ok(files)
}
fn add_backup_file(path:&Path,relative:String,files:&mut Vec<BackupFile>,total:&mut usize)->Result<()> {
    let metadata=fs::symlink_metadata(path)?;
    if !metadata.is_file() || metadata.file_type().is_symlink() || files.len()>=backup::MAX_FILES || metadata.len()>backup::MAX_SOURCE as u64 {return Err(Error::Invalid("backup file"))}
    let mut bytes=fs::read(path)?;
    *total=total.checked_add(bytes.len()).filter(|size|*size<=backup::MAX_SOURCE).ok_or(Error::Invalid("backup exceeds size limit"))?;
    files.push(BackupFile{path:relative,content:STANDARD.encode(&bytes)});bytes.fill(0);Ok(())
}
fn backup_relative_path(path:&str)->Result<PathBuf> {
    const DATABASE_FILES:[&str;3]=["store.sqlite3","lessons.sqlite3","role.sqlite3"];
    if DATABASE_FILES.contains(&path) {return Ok(PathBuf::from(path))}
    if let Some(leaf)=path.strip_prefix("blobs/") {
        let valid_hash=leaf.len()==64 && leaf.bytes().all(|byte|byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte));
        let valid_part=leaf.strip_suffix(".part").is_some_and(|id|uuid::Uuid::parse_str(id).is_ok_and(|parsed|parsed.to_string()==id));
        if !leaf.is_empty() && !leaf.bytes().any(|byte|byte==b'/' || byte==b'\\') && (valid_hash || valid_part) {return Ok(PathBuf::from("blobs").join(leaf))}
    }
    Err(Error::Invalid("backup file path"))
}
fn snapshot_database(source:&Path,destination:&Path)->Result<()> {
    let metadata=fs::symlink_metadata(source)?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {return Err(Error::VaultUnavailable)}
    let source_db=rusqlite::Connection::open(source)?;let mut snapshot=rusqlite::Connection::open(destination)?;
    let backup=rusqlite::backup::Backup::new(&source_db,&mut snapshot)?;
    backup.run_to_completion(32,std::time::Duration::from_millis(25),None)?;
    drop(backup);drop(snapshot);drop(source_db);Ok(())
}
fn ensure_plain_directory(path:&Path)->Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if !metadata.is_dir() || metadata.file_type().is_symlink() {return Err(Error::VaultUnavailable)}
            return Ok(())
        }
        Err(error) if error.kind()==std::io::ErrorKind::NotFound => fs::create_dir_all(path)?,
        Err(error) => return Err(error.into()),
    }
    let metadata=fs::symlink_metadata(path)?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {return Err(Error::VaultUnavailable)}
    Ok(())
}
struct StagingDir(PathBuf);
impl Drop for StagingDir {fn drop(&mut self) {let _=fs::remove_dir_all(&self.0);}}
fn keys(raw:BTreeMap<String,String>)->Result<BTreeMap<String,[u8;32]>> {
    raw.into_iter().map(|(id,key)| {crate::contract::id(&id)?;let key=crypto::decode(&key)?.try_into().map_err(|_|Error::Signature)?;Ok((id,key))}).collect()
}
#[derive(Serialize)]
struct Redeem<'a> {protocol:u32,app_id:&'a str,registration_code:&'a str,request_id:&'a str,device_id:&'a str,device_public_key:String}
#[cfg(windows)]
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Redeemed {credential:Signed}
#[cfg(windows)]
fn redeem(origin:&str,request:&Redeem<'_>)->Result<Signed> {
    use std::{io::Read,time::Duration};
    let client=reqwest::blocking::Client::builder().https_only(true).redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(8)).timeout(Duration::from_secs(20)).build().map_err(|_|Error::RegistrationFailed)?;
    let response=client.post(format!("{origin}/api/offline/v1/devices/redeem")).json(request).send().map_err(|_|Error::RegistrationFailed)?;
    if !matches!(response.status().as_u16(),200|201) || response.content_length().is_some_and(|n|n>262144) {return Err(Error::RegistrationFailed)}
    let mut raw=Zeroizing::new(Vec::new());response.take(262145).read_to_end(&mut raw).map_err(|_|Error::RegistrationFailed)?;
    if raw.len()>262144 {return Err(Error::RegistrationFailed)}
    let response:Redeemed=serde_json::from_slice(&raw).map_err(|_|Error::RegistrationFailed)?;Ok(response.credential)
}
#[cfg(not(windows))]
fn redeem(_origin:&str,_request:&Redeem<'_>)->Result<Signed> {Err(Error::VaultUnavailable)}

#[cfg(all(test,windows))]
mod tests {
    use super::*;
    use crate::{Access,Answer,Scope};
    fn setup(base:&Path)->(Registration,SigningKey,Arc<NativeContext>) {
        let issuer=SigningKey::from_bytes(&[91;32]);let context=Arc::new(NativeContext::new(base.to_path_buf(),App::Student));
        let mut registration=Registration::new(base,App::Student,context.clone()).unwrap();
        registration.trust.credentials.clear();registration.enrollment_enabled=true;
        registration.trust.credentials.insert("synthetic-issuer".into(),issuer.verifying_key().to_bytes());
        registration.active_credential_keys.clear();registration.active_credential_keys.insert("synthetic-issuer".into());
        (registration,issuer,context)
    }
    fn response(request:&Redeem<'_>,issuer:&SigningKey)->Signed {
        response_epoch(request,issuer,1)
    }
    fn response_epoch(request:&Redeem<'_>,issuer:&SigningKey,epoch:u64)->Signed {
        let grant=Grant{protocol:PROTOCOL,app_id:request.app_id.into(),subject_id:"student-a".into(),device_id:request.device_id.into(),
            device_public_key:request.device_public_key.clone(),epoch,issued_ms:now()-1000,expires_ms:now()+WEEK_MS-1000,
            scopes:vec![Scope{school_id:"school-a".into(),class_id:"class-a".into(),assignment_id:"assignment-a".into()}]};
        sign(&grant,issuer)
    }
    fn sign(grant:&Grant,issuer:&SigningKey)->Signed {
        let payload=serde_json::to_string(grant).unwrap();let signature=crypto::sign(issuer,"thinkgrow.credential.v1",payload.as_bytes());
        Signed{key_id:"synthetic-issuer".into(),payload,signature}
    }
    #[test]
    fn new_role_responses_require_an_active_issuer_even_when_old_keys_are_cached() {
        struct Transport;
        impl EnrollmentClient for Transport {
            fn redeem(&self,_:&[u8])->Result<Signed>{Err(Error::ServiceNotConfigured)}
            fn role_exchange(&self,_:&[u8])->Result<Signed>{Ok(Signed{key_id:"retired-issuer".into(),payload:"{}".into(),signature:String::new()})}
        }
        let dir=tempfile::tempdir().unwrap();let (mut registration,issuer,_)=setup(dir.path());
        registration.trust.credentials.insert("retired-issuer".into(),issuer.verifying_key().to_bytes());
        registration.transport=Some(Arc::new(Transport));
        assert!(matches!(registration.fetch_role(b"{}"),Err(Error::Signature)));
    }
    #[test]
    fn unconfigured_build_never_creates_a_device_or_calls_server() {
        let dir=tempfile::tempdir().unwrap();let context=Arc::new(NativeContext::new(dir.path().to_path_buf(),App::Student));
        let mut registration=Registration::new(dir.path(),App::Student,context).unwrap();
        registration.enrollment_enabled=false;
        assert!(matches!(registration.register_with("code-for-synthetic-use".into(),"246810".into(),"학생".into(),None,|_|panic!("no network")),Err(Error::ServiceNotConfigured)));
        assert!(registration.vault.list().unwrap().is_empty());
    }
    #[test]
    fn lost_response_reuses_pending_device_then_lock_requires_pin() {
        let dir=tempfile::tempdir().unwrap();let (r,issuer,ctx)=setup(dir.path());let mut first=String::new();
        assert!(r.register_with("code-for-synthetic-use".into(),"246810".into(),"학생".into(),None,|request|{first=request.device_id.into();Err(Error::RegistrationFailed)}).is_err());
        let accounts=r.vault.list().unwrap();assert_eq!(accounts.len(),1);assert!(!accounts[0].enrolled);
        let handle=accounts[0].handle.clone();
        let status=r.register_with("code-for-synthetic-use".into(),"246810".into(),"".into(),Some(handle.clone()),|request|{assert_eq!(request.device_id,first);assert_eq!(request.request_id,handle);Ok(response(request,&issuer))}).unwrap();
        assert!(!status.registration_required);r.lock().unwrap();assert!(ctx.status().unwrap().registration_required);
        assert!(r.unlock(&handle,"000000".into()).is_err());
        assert_eq!(r.unlock(&handle,"246810".into()).unwrap().local.unwrap().access,Access::ReadWrite);
    }
    #[test]
    fn wrong_role_or_device_key_cannot_attach_an_account() {
        for alter_role in [true,false] {
            let dir=tempfile::tempdir().unwrap();let (r,issuer,ctx)=setup(dir.path());
            assert!(r.register_with("code-for-synthetic-use".into(),"246810".into(),"학생".into(),None,|request|{
                let original=response(request,&issuer);let mut g:Grant=serde_json::from_str(&original.payload).unwrap();
                if alter_role {g.app_id=App::Teacher.id().into()}else{g.device_public_key=crypto::b64(&[7;32])}
                Ok(sign(&g,&issuer))
            }).is_err());
            assert!(ctx.status().unwrap().registration_required);
            assert!(!r.vault.list().unwrap()[0].enrolled);
        }
    }
    #[test]
    fn expired_credential_reopens_read_only_with_drafts_preserved() {
        let dir=tempfile::tempdir().unwrap();let (r,issuer,ctx)=setup(dir.path());
        r.register_with("code-for-synthetic-use".into(),"246810".into(),"학생".into(),None,|request|Ok(response(request,&issuer))).unwrap();
        let answer=Answer{school_id:"school-a".into(),class_id:"class-a".into(),assignment_id:"assignment-a".into(),package_revision:"v1".into(),base_answer_revision:0,body:"보존할 답안".into(),blobs:vec![]};
        ctx.save(&answer,0).unwrap();let handle=r.vault.list().unwrap()[0].handle.clone();r.lock().unwrap();
        let mut opened=r.vault.open(&handle,"246810").unwrap();let mut grant:Grant=serde_json::from_str(&opened.secrets.grant.as_ref().unwrap().payload).unwrap();
        grant.epoch=2;grant.issued_ms=now()-WEEK_MS-1000;grant.expires_ms=now()-1000;opened.secrets.grant=Some(sign(&grant,&issuer));r.vault.save(&mut opened).unwrap();
        assert_eq!(r.unlock(&handle,"246810".into()).unwrap().local.unwrap().access,Access::ReadRecoveryOnly);
        assert_eq!(ctx.load("assignment-a").unwrap().unwrap().answer.body,"보존할 답안");
        assert!(matches!(ctx.save(&answer,1),Err(Error::ReadRecoveryOnly)));
    }
    #[test]
    fn protected_backup_restores_data_and_renews_same_device_at_higher_epoch() {
        let source=tempfile::tempdir().unwrap();let (original,issuer,context)=setup(source.path());
        original.register_with("code-for-synthetic-use".into(),"246810".into(),"학생".into(),None,|request|Ok(response(request,&issuer))).unwrap();
        let answer=Answer{school_id:"school-a".into(),class_id:"class-a".into(),assignment_id:"assignment-a".into(),package_revision:"v1".into(),base_answer_revision:0,body:"복원 뒤에도 남을 답안".into(),blobs:vec![]};
        context.save(&answer,0).unwrap();
        let account=original.vault.list().unwrap().remove(0);let original_open=original.vault.open(&account.handle,"246810").unwrap();
        let device_id=original_open.secrets.device_id.clone();let signer_seed=original_open.secrets.signer_seed;let data_key=original_open.secrets.data_key;
        let backup=original.export_backup(&account.handle,"246810".into(),"portable backup recovery phrase".into()).unwrap();

        let target=tempfile::tempdir().unwrap();let (restored,restored_issuer,restored_context)=setup(target.path());
        let handle=restored.restore_backup(backup,"portable backup recovery phrase".into(),"975310".into()).unwrap();
        let restored_account=restored.vault.open(&handle,"975310").unwrap();
        assert_eq!(restored_account.secrets.device_id,device_id);
        assert_eq!(restored_account.secrets.signer_seed,signer_seed);
        assert_eq!(restored_account.secrets.data_key,data_key);
        let renewed=restored.register_with("code-for-synthetic-use".into(),"975310".into(),"".into(),Some(handle),|request|Ok(response_epoch(request,&restored_issuer,2))).unwrap();
        assert_eq!(renewed.local.unwrap().access,Access::ReadWrite);
        assert_eq!(restored_context.load("assignment-a").unwrap().unwrap().answer.body,"복원 뒤에도 남을 답안");
    }
}

#[cfg(windows)]
fn fetch_lessons(origin:&str,request:&[u8])->Result<Vec<Signed>> {
    use std::{io::Read,time::Duration};
    #[derive(Deserialize)] #[serde(deny_unknown_fields)] struct Reply {lessons:Vec<Signed>}
    let client=reqwest::blocking::Client::builder().https_only(true).redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(8)).timeout(Duration::from_secs(25)).build().map_err(|_|Error::RegistrationFailed)?;
    let response=client.post(format!("{origin}/api/offline/v1/lessons/prepare")).header("Content-Type","application/json").body(request.to_vec()).send().map_err(|_|Error::RegistrationFailed)?;
    if response.status().as_u16()!=200 || response.content_length().is_some_and(|n|n>1048576){return Err(Error::RegistrationFailed)}
    let mut raw=Zeroizing::new(Vec::new());response.take(1048577).read_to_end(&mut raw).map_err(|_|Error::RegistrationFailed)?;
    if raw.len()>1048576{return Err(Error::RegistrationFailed)}
    Ok(serde_json::from_slice::<Reply>(&raw).map_err(|_|Error::RegistrationFailed)?.lessons)
}
#[cfg(not(windows))]
fn fetch_lessons(_origin:&str,_request:&[u8])->Result<Vec<Signed>> {Err(Error::VaultUnavailable)}

#[cfg(windows)]
fn fetch_role(origin:&str,request:&[u8])->Result<Signed> {
    use std::{io::Read,time::Duration};
    #[derive(Deserialize)] #[serde(deny_unknown_fields)] struct Reply{reply:Signed}
    let client=reqwest::blocking::Client::builder().https_only(true).redirect(reqwest::redirect::Policy::none()).connect_timeout(Duration::from_secs(8)).timeout(Duration::from_secs(30)).build().map_err(|_|Error::RegistrationFailed)?;
    let response=client.post(format!("{origin}/api/offline/v1/roles/exchange")).header("Content-Type","application/json").body(request.to_vec()).send().map_err(|_|Error::RegistrationFailed)?;
    match response.status().as_u16(){409=>return Err(Error::Conflict),403=>return Err(Error::Authorization),200=>(),_=>return Err(Error::RegistrationFailed)}
    if response.content_length().is_some_and(|n|n>2097152){return Err(Error::RegistrationFailed)}
    let mut raw=Zeroizing::new(Vec::new());response.take(2097153).read_to_end(&mut raw).map_err(|_|Error::RegistrationFailed)?;
    if raw.len()>2097152{return Err(Error::RegistrationFailed)}Ok(serde_json::from_slice::<Reply>(&raw)?.reply)
}
#[cfg(not(windows))]
fn fetch_role(_origin:&str,_request:&[u8])->Result<Signed>{Err(Error::VaultUnavailable)}
