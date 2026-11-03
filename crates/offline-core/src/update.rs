use crate::{crypto, App, Error, Result, PROTOCOL, SCHEMA};
use minisign_verify::{PublicKey, Signature};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize,Serialize};
use sha2::{Digest,Sha256};
use std::{collections::BTreeSet, fs::{self,File,OpenOptions},io::{Read,Write},path::{Path,PathBuf}};
use semver::Version;
#[derive(Clone,Debug,Serialize,Deserialize,PartialEq,Eq)]
#[serde(rename_all="snake_case")]
pub enum Kind { App, Shared, Game }
#[derive(Clone,Debug,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Artifact {
    pub component:String, pub kind:Kind, pub app_id:String, pub platform:String, pub version:String,
    pub min_core:String, pub protocol_min:u32, pub protocol_max:u32, pub schema_min:u32, pub schema_max:u32,
    pub url:String, pub bytes:u64, pub sha256:String, pub signature:String,
}
#[derive(Clone,Debug,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Catalog {
    pub format:u32, pub channel:String, pub sequence:u64, pub issued_ms:i64, pub expires_ms:i64,
    pub online_repository:String, pub online_commit:String, pub offline_commit:String,
    pub artifacts:Vec<Artifact>,
}
pub struct VerifiedCatalog { data:Catalog, public_key:String, fingerprint:String }
impl VerifiedCatalog {
    pub fn data(&self)->&Catalog {&self.data}
}
pub struct Ledger { db:Connection, home:PathBuf, app:App, platform:String, channel:String }
#[derive(Debug,Serialize)]
pub struct Staged { path:PathBuf, component:String, version:String, route:String }
impl Ledger {
    pub fn open(base:&Path,app:App,platform:&str,channel:&str)->Result<Self> {
        if !["stable","beta"].contains(&channel) ||
            !["windows-x86_64","android-aarch64","ios-aarch64"].contains(&platform) ||
            (app==App::Teacher && platform!="windows-x86_64") ||
            (app==App::Parent && platform=="windows-x86_64") {return Err(Error::Invalid("update target"));}
        let home=base.join(app.id()).join(platform).join(channel);fs::create_dir_all(home.join("staging"))?;
        let db=Connection::open(home.join("updates.sqlite3"))?;
        db.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL;
            CREATE TABLE IF NOT EXISTS floor(id INTEGER PRIMARY KEY CHECK(id=1),seq INTEGER,hash TEXT,wall INTEGER);
            INSERT OR IGNORE INTO floor VALUES(1,0,'',0);
            CREATE TABLE IF NOT EXISTS staged(component TEXT PRIMARY KEY,version TEXT,hash TEXT,path TEXT,expires INTEGER,sequence INTEGER);")?;
        Ok(Self{db,home,app,platform:platform.into(),channel:channel.into()})
    }
    /// Pinned public key is native configuration, never an update-server supplied key.
    pub fn accept_catalog(&mut self,raw:&[u8],signature:&str,pinned_public_key:&str,now:i64)->Result<VerifiedCatalog> {
        if raw.len()>2*1024*1024 {return Err(Error::Invalid("catalog size"));}
        let key=PublicKey::decode(pinned_public_key).map_err(|_|Error::Signature)?;
        let sig=Signature::decode(signature).map_err(|_|Error::Signature)?;
        key.verify(raw,&sig,false).map_err(|_|Error::Signature)?;
        let c:Catalog=serde_json::from_slice(raw)?;
        let tx=self.db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let (floor,old_hash,wall):(i64,String,i64)=tx.query_row("SELECT seq,hash,wall FROM floor WHERE id=1",[],
            |r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?;
        let fingerprint=crypto::hash(raw);
        if c.format!=1 || c.channel!=self.channel || c.sequence==0 || c.sequence>i64::MAX as u64 ||
            c.sequence<(floor as u64) || (c.sequence==(floor as u64) && fingerprint!=old_hash) ||
            now.saturating_add(5000)<wall || now<c.issued_ms || now>=c.expires_ms ||
            c.expires_ms<=c.issued_ms || c.expires_ms.saturating_sub(c.issued_ms)>31*24*60*60*1000 ||
            !commit(&c.online_commit) || !commit(&c.offline_commit) ||
            c.online_repository!="oswprogram10-tech/school-class-allinone-platform" || c.artifacts.len()>256 {
                return Err(Error::Invalid("catalog provenance, sequence, time or channel")); }
        let mut identities=BTreeSet::new();
        for a in &c.artifacts {
            if !slug(&a.component) || !crate::digest(&a.sha256) || a.bytes==0 || a.bytes>1024*1024*1024 ||
                Version::parse(&a.version).is_err() || Version::parse(&a.min_core).is_err() ||
                a.protocol_min>a.protocol_max || a.schema_min>a.schema_max ||
                !identities.insert((&a.component,&a.app_id,&a.platform)) {return Err(Error::Invalid("artifact contract"));}
            let uri=url::Url::parse(&a.url).map_err(|_|Error::Invalid("artifact URL"))?;
            if uri.scheme()!="https" || uri.host_str().is_none() || !uri.username().is_empty() || uri.password().is_some() ||
                uri.fragment().is_some() {return Err(Error::Invalid("HTTPS artifact URL required"));}
        }
        tx.execute("UPDATE floor SET seq=?1,hash=?2,wall=max(wall,?3) WHERE id=1",params![c.sequence as i64,fingerprint,now])?;
        tx.commit()?;
        Ok(VerifiedCatalog{data:c,public_key:pinned_public_key.into(),fingerprint})
    }
    pub fn stage(&mut self,catalog:&VerifiedCatalog,component:&str,current_version:&str,core_version:&str,
                 source:&Path,now:i64)->Result<Staged> {
        let a=catalog.data.artifacts.iter().find(|a|a.component==component && a.app_id==self.app.id() &&
            a.platform==self.platform).ok_or(Error::Invalid("no artifact for this app and platform"))?;
        let floor_hash:String=self.db.query_row("SELECT hash FROM floor WHERE id=1",[],|r|r.get(0))?;
        if now<catalog.data.issued_ms || now>=catalog.data.expires_ms || floor_hash!=catalog.fingerprint {return Err(Error::Invalid("catalog superseded"));}
        if a.protocol_min>PROTOCOL || a.protocol_max<PROTOCOL || a.schema_min>SCHEMA || a.schema_max<SCHEMA ||
            Version::parse(core_version).map_err(|_|Error::Invalid("core version"))? <
                Version::parse(&a.min_core).map_err(|_|Error::Invalid("minimum core"))? ||
            Version::parse(&a.version).map_err(|_|Error::Invalid("version"))? <=
                Version::parse(current_version).map_err(|_|Error::Invalid("installed version"))? {
                return Err(Error::Invalid("update compatibility or downgrade")); }
        let raw_sig=String::from_utf8(crypto::decode(&a.signature)?).map_err(|_|Error::Signature)?;
        let sig=Signature::decode(&raw_sig).map_err(|_|Error::Signature)?;
        let key=PublicKey::decode(&catalog.public_key).map_err(|_|Error::Signature)?;
        let mut verifier=key.verify_stream(&sig).map_err(|_|Error::Signature)?;
        let staging=self.home.join("staging").join(format!("{}.part",uuid::Uuid::new_v4()));
        let mut guard=Temporary(staging.clone());
        let mut out=OpenOptions::new().write(true).create_new(true).open(&staging)?;
        let mut input=File::open(source)?; let mut hash=Sha256::new();let mut bytes=0u64;let mut buf=[0;65536];
        loop {let count=input.read(&mut buf)?;if count==0 {break}
            bytes=bytes.checked_add(count as u64).ok_or(Error::Invalid("artifact length"))?;
            if bytes>a.bytes {return Err(Error::Invalid("artifact longer than manifest"));}
            hash.update(&buf[..count]);verifier.update(&buf[..count]);out.write_all(&buf[..count])?;}
        if bytes!=a.bytes || format!("{:x}",hash.finalize())!=a.sha256 {return Err(Error::Invalid("artifact hash or length"));}
        verifier.finalize().map_err(|_|Error::Signature)?;
        // The signed catalog binds version and role even for older updater signature comments.
        out.sync_all()?; drop(out);
        let final_path=self.home.join("staging").join(format!("{}-{}-{}",a.component,a.version,a.sha256));
        fs::rename(&staging,&final_path)?;
        guard.0=PathBuf::new();
        self.db.execute("INSERT INTO staged VALUES(?1,?2,?3,?4,?5,?6)
            ON CONFLICT(component) DO UPDATE SET version=excluded.version,hash=excluded.hash,path=excluded.path,expires=excluded.expires,sequence=excluded.sequence",
            params![a.component,a.version,a.sha256,final_path.to_string_lossy(),catalog.data.expires_ms,catalog.data.sequence as i64])?;
        let route=match a.kind {Kind::Game=>"approved-game-installer",Kind::Shared=>"versioned-shared-package",
            Kind::App=>match self.platform.as_str(){"windows-x86_64"=>"tauri-desktop-updater","android-aarch64"=>"android-package-manager",_=>"apple-distribution"}};
        Ok(Staged{path:final_path,component:a.component.clone(),version:a.version.clone(),route:route.into()})
    }
    pub fn installation_gate(&self,staged:&Staged,class_running:bool,user_confirmed:bool,now:i64)->Result<()> {
        if class_running || !user_confirmed {return Err(Error::Deferred);}
        let recorded:Option<(String,String,i64,i64)>=self.db.query_row("SELECT hash,path,expires,sequence FROM staged WHERE component=?1 AND version=?2",
            params![staged.component,staged.version],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional()?;
        let (expected,path,expires,sequence)=recorded.ok_or(Error::Invalid("candidate not staged"))?;
        let (floor,wall):(i64,i64)=self.db.query_row("SELECT seq,wall FROM floor WHERE id=1",[],|r|Ok((r.get(0)?,r.get(1)?)))?;
        if Path::new(&path)!=staged.path || now>=expires || now.saturating_add(5000)<wall || sequence!=floor {
            return Err(Error::Invalid("candidate expired, superseded or changed")); }
        // Recheck the persisted bytes immediately before handing them to a platform installer.
        let mut file=File::open(&staged.path)?;let mut h=Sha256::new();let mut buf=[0u8;65536];
        loop{let n=file.read(&mut buf)?;if n==0{break}h.update(&buf[..n]);}
        if format!("{:x}",h.finalize())!=expected {return Err(Error::Signature);} Ok(())
    }
}
struct Temporary(PathBuf);
impl Drop for Temporary {fn drop(&mut self){if !self.0.as_os_str().is_empty(){let _=fs::remove_file(&self.0);}}}
fn commit(s:&str)->bool{s.len()==40 && s.bytes().all(|b|b.is_ascii_hexdigit())}
fn slug(s:&str)->bool{!s.is_empty() && s.len()<=80 && s.bytes().all(|b|b.is_ascii_lowercase()||b.is_ascii_digit()||b==b'-')}

impl Staged { pub fn path(&self)->&Path {&self.path} pub fn route(&self)->&str {&self.route} }
