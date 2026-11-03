//! Windows user-scoped DPAPI + PIN-derived encryption. No plaintext fallback.
use crate::{crypto::DataKey, runtime::now, App, Error, Result, Signed};
use argon2::Argon2;
use rand::{rngs::OsRng, RngCore};
use serde::{Deserialize, Serialize};
use std::{fs::{self, OpenOptions}, io::{Read, Write}, path::{Path, PathBuf}, sync::Arc};
use zeroize::{Zeroize, Zeroizing};

const LIMIT: u64 = 1024 * 1024;
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Protected {
    format: u32, app_id: String, handle: String, label: String, enrolled: bool,
    salt: [u8;32], cipher: Vec<u8>, #[serde(default)] recovery_salt:Option<[u8;32]>,
    #[serde(default)] recovery_cipher:Option<Vec<u8>>, #[serde(default)] recovery_wrap:Option<Vec<u8>>,
    failures: u32, blocked_until: i64, wall_floor: i64,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Secrets {
    pub data_key: [u8;32], pub signer_seed: [u8;32], pub device_id: String,
    pub subject: Option<String>, pub grant: Option<Signed>,
}
impl Drop for Secrets {
    fn drop(&mut self) { self.data_key.zeroize(); self.signer_seed.zeroize(); }
}
pub(crate) struct Opened { pub secrets: Secrets, protected: Protected, pin_key: DataKey, recovery_key:Option<DataKey> }
impl Opened { pub fn handle(&self)->&str { &self.protected.handle } pub(crate) fn wall_floor(&self)->i64 { self.protected.wall_floor } pub(crate) fn label(&self)->&str { &self.protected.label } }
#[derive(Serialize)]
pub struct LocalAccount { pub handle: String, pub label: String, pub enrolled: bool, pub recovery_ready:bool }
pub trait OsProtector: Send + Sync {
    fn available(&self)->bool;
    fn extension(&self)->&'static str {"vault"}
    fn protect(&self,bytes:&[u8],entropy:&[u8])->Result<Vec<u8>>;
    fn unprotect(&self,bytes:&[u8],entropy:&[u8])->Result<Vec<u8>>;
}
pub struct SystemProtector;
impl OsProtector for SystemProtector {
    fn available(&self)->bool {cfg!(windows)}
    fn extension(&self)->&'static str {"dpapi"}
    fn protect(&self,bytes:&[u8],entropy:&[u8])->Result<Vec<u8>> {protect(bytes,entropy)}
    fn unprotect(&self,bytes:&[u8],entropy:&[u8])->Result<Vec<u8>> {unprotect(bytes,entropy)}
}
pub struct Vault { root: PathBuf, app: App, protector:Arc<dyn OsProtector> }
impl Vault {
    pub fn new(base:&Path, app:App)->Self { Self::with_protector(base,app,Arc::new(SystemProtector)) }
    pub fn with_protector(base:&Path,app:App,protector:Arc<dyn OsProtector>)->Self {
        Self{root:base.join("accounts-v1"),app,protector}
    }
    pub fn available(&self)->bool {self.protector.available()}
    fn path(&self, handle:&str)->Result<PathBuf> {
        let id=uuid::Uuid::parse_str(handle).map_err(|_|Error::AccountLocked)?;
        if id.to_string()!=handle {return Err(Error::AccountLocked)}
        Ok(self.root.join(format!("{handle}.{}",self.protector.extension())))
    }
    fn safe_root(&self)->Result<()> {
        if !self.available() {return Err(Error::VaultUnavailable)}
        if self.root.exists() && fs::symlink_metadata(&self.root)?.file_type().is_symlink() {return Err(Error::VaultUnavailable)}
        fs::create_dir_all(&self.root)?; Ok(())
    }
    fn aad(&self,handle:&str)->String {format!("vault/{}/{handle}",self.app.id())}
    fn read(&self,handle:&str)->Result<Protected> {
        self.safe_root()?; let path=self.path(handle)?;
        let meta=fs::symlink_metadata(&path)?;
        if !meta.is_file() || meta.file_type().is_symlink() || meta.len()>LIMIT {return Err(Error::VaultUnavailable)}
        let mut bytes=Vec::new(); fs::File::open(&path)?.take(LIMIT+1).read_to_end(&mut bytes)?;
        if bytes.len() as u64>LIMIT {return Err(Error::VaultUnavailable)}
        let clear=Zeroizing::new(self.protector.unprotect(&bytes,self.app.id().as_bytes())?);
        let mut p:Protected=serde_json::from_slice(&clear)?;
        if !(1..=2).contains(&p.format) || p.handle!=handle || p.app_id!=self.app.id() ||
            (p.recovery_salt.is_some()!=p.recovery_cipher.is_some()) ||
            (p.recovery_salt.is_some()!=p.recovery_wrap.is_some()) || (p.format==1 && p.recovery_salt.is_some()) {return Err(Error::Partition)}
        p.format=2;
        Ok(p)
    }
    fn persist(&self,p:&Protected,create:bool)->Result<()> {
        self.safe_root()?; let path=self.path(&p.handle)?;
        if create && path.exists() {return Err(Error::Conflict)}
        if path.exists() && fs::symlink_metadata(&path)?.file_type().is_symlink() {return Err(Error::VaultUnavailable)}
        let raw=Zeroizing::new(serde_json::to_vec(p)?);
        if raw.len() as u64>LIMIT {return Err(Error::VaultUnavailable)}
        let sealed=self.protector.protect(&raw,self.app.id().as_bytes())?;
        let temp=self.root.join(format!("{}.part",uuid::Uuid::new_v4()));
        let mut file=OpenOptions::new().write(true).create_new(true).open(&temp)?;
        file.write_all(&sealed)?; file.sync_all()?; drop(file);
        // Atomic replacement; never truncate a user's existing key file.
        replace(&temp,&path,!create)?; Ok(())
    }
    pub fn list(&self)->Result<Vec<LocalAccount>> {
        if !self.available() {return Ok(vec![])}
        self.safe_root()?; let mut result=vec![];
        for entry in fs::read_dir(&self.root)? {
            let path=entry?.path(); if path.extension().and_then(|s|s.to_str())!=Some(self.protector.extension()) {continue}
            if result.len()>=64 {return Err(Error::VaultUnavailable)}
            let handle=path.file_stem().and_then(|s|s.to_str()).ok_or(Error::VaultUnavailable)?;
            let p=self.read(handle)?;
            result.push(LocalAccount{handle:p.handle,label:p.label,enrolled:p.enrolled,recovery_ready:p.recovery_cipher.is_some()});
        }
        result.sort_by(|a,b|a.label.cmp(&b.label).then(a.handle.cmp(&b.handle))); Ok(result)
    }
    pub(crate) fn create(&self,pin:&str,label:&str)->Result<Opened> {
        validate_pin(pin)?;
        if label.trim().is_empty() || label.chars().count()>80 || label.chars().any(char::is_control) {return Err(Error::Invalid("account label"))}
        if self.list()?.len()>=64 {return Err(Error::VaultUnavailable)}
        let handle=uuid::Uuid::new_v4().to_string();
        let mut salt=[0;32]; let mut data_key=[0;32]; let mut signer_seed=[0;32];
        OsRng.fill_bytes(&mut salt); OsRng.fill_bytes(&mut data_key); OsRng.fill_bytes(&mut signer_seed);
        let secrets=Secrets{data_key,signer_seed,device_id:uuid::Uuid::new_v4().to_string(),subject:None,grant:None};
        data_key.zeroize(); signer_seed.zeroize();
        let key=derive(pin,&salt)?; let raw=Zeroizing::new(serde_json::to_vec(&secrets)?);
        let p=Protected{format:2,app_id:self.app.id().into(),handle:handle.clone(),label:label.trim().into(),
            enrolled:false,salt,cipher:key.seal(&self.aad(&handle),&raw)?,recovery_salt:None,recovery_cipher:None,recovery_wrap:None,failures:0,blocked_until:0,wall_floor:now()};
        self.persist(&p,true)?; Ok(Opened{secrets,protected:p,pin_key:key,recovery_key:None})
    }
    pub(crate) fn open(&self,handle:&str,pin:&str)->Result<Opened> {
        validate_pin(pin)?; let mut p=self.read(handle)?; let current=now();
        if current<p.blocked_until {return Err(Error::AccountLocked)}
        // Record failure before doing expensive work, including process termination.
        // A failed PIN attempt is not trusted evidence of time: advancing the
        // protected high-water here would let a mistyped PIN poison the account.
        p.failures=p.failures.saturating_add(1);
        if p.failures>=5 {p.blocked_until=current.saturating_add(30_000i64.saturating_mul(1i64 << ((p.failures-5)/5).min(10)));}
        self.persist(&p,false)?;
        let key=derive(pin,&p.salt)?;
        let raw=Zeroizing::new(key.open(&self.aad(handle),&p.cipher).map_err(|_|Error::AccountLocked)?);
        let secrets:Secrets=serde_json::from_slice(&raw).map_err(|_|Error::AccountLocked)?;
        let recovery_key=if let Some(wrapped)=&p.recovery_wrap {
            let clear=Zeroizing::new(key.open(&self.recovery_wrap_aad(handle),wrapped).map_err(|_|Error::AccountLocked)?);
            let bytes:[u8;32]=clear.as_slice().try_into().map_err(|_|Error::AccountLocked)?;
            Some(DataKey::new(bytes))
        }else{None};
        p.failures=0; p.blocked_until=0; p.wall_floor=p.wall_floor.max(current); self.persist(&p,false)?;
        Ok(Opened{secrets,protected:p,pin_key:key,recovery_key})
    }
    pub(crate) fn save(&self,opened:&mut Opened)->Result<()> {
        self.save_at(opened,None)
    }
    pub(crate) fn save_at(&self,opened:&mut Opened,trusted_time:Option<i64>)->Result<()> {
        opened.protected.enrolled=opened.secrets.subject.is_some() && opened.secrets.grant.is_some();
        opened.protected.wall_floor=trusted_time.unwrap_or_else(||opened.protected.wall_floor.max(now()));
        let raw=Zeroizing::new(serde_json::to_vec(&opened.secrets)?);
        opened.protected.cipher=opened.pin_key.seal(&self.aad(opened.handle()),&raw)?;
        if let (Some(recovery_key),Some(_))=(&opened.recovery_key,opened.protected.recovery_salt) {
            opened.protected.recovery_cipher=Some(recovery_key.seal(&self.recovery_aad(opened.handle()),&raw)?);
            let bytes=Zeroizing::new(recovery_key.export());
            opened.protected.recovery_wrap=Some(opened.pin_key.seal(&self.recovery_wrap_aad(opened.handle()),&bytes[..])?);
            opened.protected.format=2;
        }
        self.persist(&opened.protected,false)
    }
    pub(crate) fn change_pin(&self,handle:&str,current_pin:&str,new_pin:&str)->Result<()> {
        validate_pin(new_pin)?;
        let mut opened=self.open(handle,current_pin)?;
        let mut salt=[0;32];OsRng.fill_bytes(&mut salt);
        let new_key=derive(new_pin,&salt)?;
        opened.protected.salt=salt;
        opened.protected.failures=0;opened.protected.blocked_until=0;
        opened.pin_key=new_key;
        self.save_at(&mut opened,None)
    }
    pub(crate) fn import_account(&self,handle:&str,label:&str,enrolled:bool,wall_floor:i64,secrets:Secrets,pin:&str,passphrase:&str)->Result<()> {
        validate_pin(pin)?;validate_recovery(passphrase)?;
        if label.trim().is_empty() || label.chars().count()>80 || label.chars().any(char::is_control) {return Err(Error::Invalid("account label"))}
        if wall_floor<0 || self.list()?.len()>=64 {return Err(Error::VaultUnavailable)}
        if self.path(handle)?.exists() {return Err(Error::Conflict)}
        let mut salt=[0;32];OsRng.fill_bytes(&mut salt);let pin_key=derive(pin,&salt)?;
        let mut recovery_salt=[0;32];OsRng.fill_bytes(&mut recovery_salt);let recovery_key=derive_recovery(passphrase,&recovery_salt)?;
        let raw=Zeroizing::new(serde_json::to_vec(&secrets)?);
        let recovery_bytes=Zeroizing::new(recovery_key.export());
        let p=Protected{format:2,app_id:self.app.id().into(),handle:handle.into(),label:label.trim().into(),enrolled,salt,
            cipher:pin_key.seal(&self.aad(handle),&raw)?,recovery_salt:Some(recovery_salt),
            recovery_cipher:Some(recovery_key.seal(&self.recovery_aad(handle),&raw)?),
            recovery_wrap:Some(pin_key.seal(&self.recovery_wrap_aad(handle),&recovery_bytes[..])?),
            failures:0,blocked_until:0,wall_floor};
        self.persist(&p,true)
    }
    fn recovery_aad(&self,handle:&str)->String {format!("recovery/{}/{handle}",self.app.id())}
    fn recovery_wrap_aad(&self,handle:&str)->String {format!("recovery-wrap/{}/{handle}",self.app.id())}
    pub(crate) fn configure_recovery(&self,opened:&mut Opened,passphrase:&str)->Result<()> {
        let mut salt=[0;32];OsRng.fill_bytes(&mut salt);
        let recovery_key=derive_recovery(passphrase,&salt)?;
        opened.protected.recovery_salt=Some(salt);opened.protected.format=2;opened.recovery_key=Some(recovery_key);
        self.save_at(opened,None)
    }
    pub(crate) fn recover_pin(&self,handle:&str,passphrase:&str,new_pin:&str)->Result<()> {
        validate_pin(new_pin)?;validate_recovery(passphrase)?;
        let mut p=self.read(handle)?;let current=now();
        if current<p.blocked_until {return Err(Error::AccountLocked)}
        let salt=p.recovery_salt.ok_or(Error::RecoveryUnavailable)?;
        let cipher=p.recovery_cipher.clone().ok_or(Error::RecoveryUnavailable)?;
        // Only a valid recovery passphrase may advance the trusted local floor.
        p.failures=p.failures.saturating_add(1);
        if p.failures>=5 {p.blocked_until=current.saturating_add(30_000i64.saturating_mul(1i64<<((p.failures-5)/5).min(10)));}
        self.persist(&p,false)?;
        let recovery_key=derive_recovery(passphrase,&salt)?;
        let raw=Zeroizing::new(recovery_key.open(&self.recovery_aad(handle),&cipher).map_err(|_|Error::AccountLocked)?);
        let _secrets:Secrets=serde_json::from_slice(&raw).map_err(|_|Error::AccountLocked)?;
        let mut pin_salt=[0;32];OsRng.fill_bytes(&mut pin_salt);
        let pin_key=derive(new_pin,&pin_salt)?;
        let recovery_bytes=Zeroizing::new(recovery_key.export());
        p.salt=pin_salt;p.cipher=pin_key.seal(&self.aad(handle),&raw)?;
        p.recovery_wrap=Some(pin_key.seal(&self.recovery_wrap_aad(handle),&recovery_bytes[..])?);
        p.failures=0;p.blocked_until=0;p.wall_floor=p.wall_floor.max(current);p.format=2;
        self.persist(&p,false)
    }
}
fn validate_pin(pin:&str)->Result<()> {
    if !(6..=12).contains(&pin.len()) || !pin.bytes().all(|b|b.is_ascii_digit()) {return Err(Error::Invalid("PIN must contain 6 to 12 digits"))} Ok(())
}
fn derive(pin:&str,salt:&[u8;32])->Result<DataKey> {
    let mut bytes=Zeroizing::new([0;32]);
    Argon2::default().hash_password_into(pin.as_bytes(),salt,bytes.as_mut()).map_err(|_|Error::Encryption)?;
    Ok(DataKey::new(*bytes))
}
fn validate_recovery(passphrase:&str)->Result<()> {
    if !(12..=128).contains(&passphrase.chars().count()) || passphrase.bytes().any(|byte|byte.is_ascii_control()) {return Err(Error::Invalid("recovery phrase"))} Ok(())
}
fn derive_recovery(passphrase:&str,salt:&[u8;32])->Result<DataKey> {
    validate_recovery(passphrase)?;let mut bytes=Zeroizing::new([0;32]);
    Argon2::default().hash_password_into(passphrase.as_bytes(),salt,bytes.as_mut()).map_err(|_|Error::Encryption)?;
    Ok(DataKey::new(*bytes))
}
#[cfg(windows)]
fn dpapi(bytes:&[u8],entropy:&[u8],encrypt:bool)->Result<Vec<u8>> {
    use windows_sys::Win32::{Foundation::LocalFree,Security::Cryptography::{CryptProtectData,CryptUnprotectData,CRYPT_INTEGER_BLOB,CRYPTPROTECT_UI_FORBIDDEN}};
    if bytes.is_empty() || bytes.len() as u64>LIMIT+4096 {return Err(Error::VaultUnavailable)}
    let input=CRYPT_INTEGER_BLOB{cbData:bytes.len() as u32,pbData:bytes.as_ptr() as *mut _};
    let entropy=CRYPT_INTEGER_BLOB{cbData:entropy.len() as u32,pbData:entropy.as_ptr() as *mut _};
    let mut output=CRYPT_INTEGER_BLOB{cbData:0,pbData:std::ptr::null_mut()};
    // No LOCAL_MACHINE flag: other Windows users must not decrypt these keys.
    let ok=unsafe {if encrypt {CryptProtectData(&input,std::ptr::null(),&entropy,std::ptr::null(),std::ptr::null(),CRYPTPROTECT_UI_FORBIDDEN,&mut output)}
        else {CryptUnprotectData(&input,std::ptr::null_mut(),&entropy,std::ptr::null(),std::ptr::null(),CRYPTPROTECT_UI_FORBIDDEN,&mut output)}};
    if ok==0 {return Err(Error::VaultUnavailable)}
    let result=if output.cbData as u64>LIMIT+4096 || output.pbData.is_null() {Err(Error::VaultUnavailable)}
        else {Ok(unsafe {std::slice::from_raw_parts(output.pbData,output.cbData as usize)}.to_vec())};
    unsafe {if !output.pbData.is_null(){
        if !encrypt {std::slice::from_raw_parts_mut(output.pbData,output.cbData as usize).zeroize();}
        LocalFree(output.pbData as *mut _);
    }}
    result
}
fn protect(bytes:&[u8],entropy:&[u8])->Result<Vec<u8>> {
    #[cfg(windows)] {dpapi(bytes,entropy,true)}
    #[cfg(not(windows))] {let _=(bytes,entropy);Err(Error::VaultUnavailable)}
}
fn unprotect(bytes:&[u8],entropy:&[u8])->Result<Vec<u8>> {
    #[cfg(windows)] {dpapi(bytes,entropy,false)}
    #[cfg(not(windows))] {let _=(bytes,entropy);Err(Error::VaultUnavailable)}
}
fn replace(from:&Path,to:&Path,overwrite:bool)->Result<()> {
    #[cfg(windows)] {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Storage::FileSystem::{MoveFileExW,MOVEFILE_REPLACE_EXISTING,MOVEFILE_WRITE_THROUGH};
        let from:Vec<u16>=from.as_os_str().encode_wide().chain(Some(0)).collect();
        let to:Vec<u16>=to.as_os_str().encode_wide().chain(Some(0)).collect();
        let flags=MOVEFILE_WRITE_THROUGH | if overwrite {MOVEFILE_REPLACE_EXISTING}else{0};
        if unsafe{MoveFileExW(from.as_ptr(),to.as_ptr(),flags)}==0 {return Err(std::io::Error::last_os_error().into())} Ok(())
    }
    #[cfg(not(windows))] {
        if !overwrite && to.exists() {return Err(Error::Conflict)}
        fs::rename(from,to)?;
        if let Some(parent)=to.parent() {fs::File::open(parent)?.sync_all()?;}
        Ok(())
    }
}

#[cfg(all(test,windows))]
mod tests {
    use super::*;
    #[test]
    fn protected_keys_require_pin_and_do_not_reveal_plaintext() {
        let dir=tempfile::tempdir().unwrap();let v=Vault::new(dir.path(),App::Student);
        let account=v.create("246810","학생 계정").unwrap();let id=account.handle().to_owned();
        let bytes=fs::read(v.path(&id).unwrap()).unwrap();
        assert!(!bytes.windows(b"signer_seed".len()).any(|w|w==b"signer_seed"));
        assert!(matches!(v.open(&id,"000000"),Err(Error::AccountLocked)));
        let reopened=v.open(&id,"246810").unwrap();
        assert_eq!(crypto_hash(&account.secrets.data_key),crypto_hash(&reopened.secrets.data_key));
    }
    fn crypto_hash(bytes:&[u8])->String {crate::crypto::hash(bytes)}
    #[test]
    fn failures_are_durable_and_account_data_survives_lockout() {
        let dir=tempfile::tempdir().unwrap();let v=Vault::new(dir.path(),App::Teacher);
        let id=v.create("246810","교사").unwrap().handle().to_owned();
        for _ in 0..5 {assert!(v.open(&id,"000000").is_err());}
        let fresh=Vault::new(dir.path(),App::Teacher);
        assert!(matches!(fresh.open(&id,"246810"),Err(Error::AccountLocked)));
        let mut state=fresh.read(&id).unwrap();assert_eq!(state.failures,5);
        state.blocked_until=now()-1;fresh.persist(&state,false).unwrap();
        assert!(fresh.open(&id,"246810").is_ok());
        assert_eq!(fresh.read(&id).unwrap().failures,0);
    }
    #[test]
    fn different_role_and_corrupted_files_fail_without_replacement() {
        let dir=tempfile::tempdir().unwrap();let v=Vault::new(dir.path(),App::Student);
        let id=v.create("246810","학생").unwrap().handle().to_owned();let path=v.path(&id).unwrap();
        assert!(Vault::new(dir.path(),App::Teacher).open(&id,"246810").is_err());
        let mut bytes=fs::read(&path).unwrap();let last=bytes.len()-1;bytes[last]^=1;fs::write(&path,&bytes).unwrap();
        assert!(v.open(&id,"246810").is_err());assert_eq!(fs::read(path).unwrap(),bytes);
    }
    #[test]
    fn rollback_allows_pin_unlock_for_recovery_and_preserves_high_water() {
        let dir=tempfile::tempdir().unwrap();let v=Vault::new(dir.path(),App::Student);
        let id=v.create("246810","학생").unwrap().handle().to_owned();let mut p=v.read(&id).unwrap();
        p.wall_floor=now()+3600000;v.persist(&p,false).unwrap();
        assert!(v.open(&id,"246810").is_ok());assert!(v.read(&id).unwrap().wall_floor>now());
    }
    #[test]
    fn pin_change_and_recovery_preserve_account_keys() {
        let dir=tempfile::tempdir().unwrap();let v=Vault::new(dir.path(),App::Student);
        let mut account=v.create("246810","학생 계정").unwrap();let id=account.handle().to_owned();
        let data_key=account.secrets.data_key;let signer_seed=account.secrets.signer_seed;
        v.configure_recovery(&mut account,"correct backup recovery phrase").unwrap();
        v.change_pin(&id,"246810","135790").unwrap();
        assert!(matches!(v.open(&id,"246810"),Err(Error::AccountLocked)));
        let changed=v.open(&id,"135790").unwrap();
        assert_eq!(changed.secrets.data_key,data_key);assert_eq!(changed.secrets.signer_seed,signer_seed);
        assert!(v.recover_pin(&id,"wrong backup recovery phrase","112233").is_err());
        assert!(v.open(&id,"135790").is_ok());
        v.recover_pin(&id,"correct backup recovery phrase","112233").unwrap();
        assert!(matches!(v.open(&id,"135790"),Err(Error::AccountLocked)));
        let recovered=v.open(&id,"112233").unwrap();
        assert_eq!(recovered.secrets.data_key,data_key);assert_eq!(recovered.secrets.signer_seed,signer_seed);
    }
}
