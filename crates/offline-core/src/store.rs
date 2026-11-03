use crate::{contract::*, crypto::{self, DataKey}, Error, Result};
use ed25519_dalek::SigningKey;
use rusqlite::{params, Connection, OptionalExtension, Transaction};
use std::{fs::{self, OpenOptions}, io::Write, path::{Path,PathBuf}, time::Duration};
use uuid::Uuid;

const SQL:&str=include_str!("schema.sql");
pub struct Store {
    db:Connection, home:PathBuf, app:App, subject:String, device:String,
    key:DataKey, signer:SigningKey, trust:Trust, binding:String,
}
impl Store {
    pub(crate) fn home_path(base:&Path,app:App,subject:&str,device:&str)->Result<PathBuf> {
        id(subject)?;id(device)?;
        let partition=crypto::hash(format!("{}\0{}\0{}",app.id(),subject,device).as_bytes());
        Ok(base.join(app.id()).join(partition))
    }
    /// Native callers supply a vault key and authenticated account identity.
    /// Never accept these parameters directly from an untrusted WebView.
    pub fn open(base:&Path, app:App, subject:&str, device:&str, key:DataKey,
                signer:SigningKey, trust:Trust)->Result<Self> {
        let home=Self::home_path(base,app,subject,device)?;let partition=home.file_name().and_then(|name|name.to_str()).ok_or(Error::Partition)?.to_owned();fs::create_dir_all(home.join("blobs"))?;
        let file=home.join("store.sqlite3"); let existing=file.exists();
        let mut db=Connection::open(&file)?;
        db.busy_timeout(Duration::from_secs(3))?;
        db.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL; PRAGMA foreign_keys=ON;")?;
        let version:u32=db.query_row("PRAGMA user_version",[],|r|r.get(0))?;
        if version>SCHEMA || (existing && version==0) {return Err(Error::Schema);}
        if !existing {
            let tx=db.transaction()?;
            tx.execute_batch(SQL)?;
            tx.execute("INSERT INTO meta(key,value) VALUES('identity',?1)",[format!("{}\0{}\0{}",app.id(),subject,device)])?;
            tx.execute("INSERT INTO meta(key,value) VALUES('public_key',?1)",[crypto::b64(&signer.verifying_key().to_bytes())])?;
            tx.execute("INSERT INTO meta(key,value) VALUES('key_check',?1)",[key.seal(&format!("store/{partition}/key-check"),b"authorized-key")?])?;
            tx.execute("INSERT INTO meta(key,value) VALUES('migration_checksum',?1)",[crypto::hash(SQL.replace("\r\n","\n").as_bytes())])?;
            tx.commit()?;
        }
        let identity:String=db.query_row("SELECT value FROM meta WHERE key='identity'",[],|r|r.get(0))?;
        let public:String=db.query_row("SELECT value FROM meta WHERE key='public_key'",[],|r|r.get(0))?;
        if identity!=format!("{}\0{}\0{}",app.id(),subject,device) ||
            public!=crypto::b64(&signer.verifying_key().to_bytes()) {return Err(Error::Partition);}
        let check:Vec<u8>=db.query_row("SELECT value FROM meta WHERE key='key_check'",[],|r|r.get(0))?;
        if key.open(&format!("store/{partition}/key-check"),&check)?!=b"authorized-key" {return Err(Error::Encryption);}
        let checksum:String=db.query_row("SELECT value FROM meta WHERE key='migration_checksum'",[],|r|r.get(0))?;
        if checksum!=crypto::hash(SQL.replace("\r\n","\n").as_bytes()) {return Err(Error::Schema);}
        let mut store=Self{db,home,app,subject:subject.into(),device:device.into(),key,signer,trust,binding:partition};
        store.recover_blobs()?;
        Ok(store)
    }
    pub fn install_grant(&mut self, signed:&Signed, now:i64)->Result<()> {
        let g:Grant=signed.verify(&self.trust.credentials,"thinkgrow.credential.v1")?;
        if g.protocol!=PROTOCOL || g.app_id!=self.app.id() || g.subject_id!=self.subject ||
            g.device_id!=self.device || g.device_public_key!=crypto::b64(&self.signer.verifying_key().to_bytes()) ||
            g.issued_ms<0 || g.expires_ms<=g.issued_ms || g.expires_ms-g.issued_ms>WEEK_MS ||
            g.issued_ms>now.saturating_add(300_000) || g.epoch>i64::MAX as u64 || g.scopes.len()>1024 {
                return Err(Error::Authorization); }
        for s in &g.scopes {id(&s.school_id)?;id(&s.class_id)?;id(&s.assignment_id)?;}
        let old=self.grant()?;
        if old.as_ref().is_some_and(|old|old.epoch>g.epoch) {return Err(Error::Authorization);}
        let tx=self.db.transaction()?;
        tx.execute("INSERT INTO meta(key,value) VALUES('grant',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            [serde_json::to_string(signed)?])?;
        let current_floor:i64=tx.query_row("SELECT wall_floor FROM counters",[],|r|r.get(0))?;
        let trusted_floor=if old.as_ref().is_some_and(|old|old.epoch<g.epoch){g.issued_ms}else{current_floor.max(now).max(g.issued_ms)};
        tx.execute("UPDATE counters SET wall_floor=?1",[trusted_floor])?;
        tx.commit()?; Ok(())
    }
    fn grant(&self)->Result<Option<Grant>> {
        let raw:Option<String>=self.db.query_row("SELECT value FROM meta WHERE key='grant'",[],|r|r.get(0)).optional()?;
        raw.map(|s|->Result<Grant>{let signed:Signed=serde_json::from_str(&s)?;
            let grant:Grant=signed.verify(&self.trust.credentials,"thinkgrow.credential.v1")?;
            if grant.protocol!=PROTOCOL || grant.app_id!=self.app.id() || grant.subject_id!=self.subject ||
                grant.device_id!=self.device || grant.device_public_key!=crypto::b64(&self.signer.verifying_key().to_bytes()) {
                return Err(Error::Authorization);
            }
            Ok(grant)}).transpose()
    }
    pub fn access(&self,now:i64)->Result<Access> {
        let Some(g)=self.grant()? else{return Ok(Access::ProvisioningRequired)};
        let floor:i64=self.db.query_row("SELECT wall_floor FROM counters",[],|r|r.get(0))?;
        if now<g.issued_ms || now.saturating_add(5000)<floor || now>=g.expires_ms {
            Ok(Access::ReadRecoveryOnly) } else {Ok(Access::ReadWrite)}
    }
    fn writable(&self,now:i64,answer:Option<&Answer>)->Result<Grant> {
        match self.access(now)? {
            Access::ProvisioningRequired=>return Err(Error::Authorization),
            Access::ReadRecoveryOnly=>return Err(Error::ReadRecoveryOnly), _=>()
        }
        let g=self.grant()?.ok_or(Error::Authorization)?;
        if self.app==App::Parent {return Err(Error::Authorization);}
        if let Some(a)=answer {
            if self.app!=App::Student {return Err(Error::Authorization);}
            a.validate()?;
            if !g.scopes.iter().any(|s|s.school_id==a.school_id && s.class_id==a.class_id &&
                s.assignment_id==a.assignment_id){return Err(Error::Authorization);}
        }
        Ok(g)
    }
    pub fn status(&self,now:i64)->Result<Status> {
        // Observe time even on read-only status calls; returning to an earlier
        // wall clock must not restore write access after an observed expiry.
        self.db.execute("UPDATE counters SET wall_floor=max(wall_floor,?1)",[now])?;
        Ok(Status{protocol:PROTOCOL,schema:SCHEMA,access:self.access(now)?,
            drafts:self.db.query_row("SELECT count(*) FROM drafts",[],|r|read_u64(r,0))?,
            pending_classroom:self.pending(Destination::Classroom)?,
            pending_online:self.pending(Destination::Online)?,installed_games:0})
    }
    pub fn pending(&self,dest:Destination)->Result<u64> {
        Ok(self.db.query_row("SELECT count(*) FROM deliveries WHERE destination=?1 AND state NOT IN ('received','cancelled')",
            [dest.text()],|r|read_u64(r,0))?)
    }
    pub fn load_draft(&self,assignment:&str)->Result<Option<Draft>> {
        let raw:Option<(u64,Vec<u8>)>=self.db.query_row(
            "SELECT d.revision,r.cipher FROM drafts d JOIN draft_revisions r ON r.assignment=d.assignment AND r.revision=d.revision WHERE d.assignment=?1",
            [assignment],|r|Ok((read_u64(r,0)?,r.get(1)?))).optional()?;
        raw.map(|(revision,cipher)|Ok(Draft{revision,answer:serde_json::from_slice(
            &self.decrypt(&format!("draft/{assignment}/{revision}"),&cipher)?)?})).transpose()
    }
    pub fn save_draft(&mut self,answer:&Answer,expected:u64,now:i64)->Result<u64> {
        self.writable(now,Some(answer))?;
        let revision=expected.checked_add(1).filter(|r|*r<=i64::MAX as u64).ok_or(Error::Invalid("revision overflow"))?;
        let cipher=self.seal(&format!("draft/{}/{revision}",answer.assignment_id),&serde_json::to_vec(answer)?)?;
        let tx=self.db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let current:Option<u64>=tx.query_row("SELECT revision FROM drafts WHERE assignment=?1",
            [&answer.assignment_id],|r|read_u64(r,0)).optional()?;
        if current.unwrap_or(0)!=expected {return Err(Error::Conflict);}
        tx.execute("INSERT INTO draft_revisions VALUES(?1,?2,?3)",params![answer.assignment_id,revision as i64,cipher])?;
        tx.execute("INSERT INTO drafts VALUES(?1,?2,1) ON CONFLICT(assignment) DO UPDATE SET revision=excluded.revision,dirty=1",
            params![answer.assignment_id,revision as i64])?;
        observe(&tx,now)?; tx.commit()?; Ok(revision)
    }
    pub fn submit(&mut self,assignment:&str,revision:u64,intent:Uuid,now:i64)->Result<Envelope> {
        let draft=self.load_draft(assignment)?.ok_or(Error::Invalid("draft missing"))?;
        let g=self.writable(now,Some(&draft.answer))?;
        if draft.revision!=revision {return Err(Error::Conflict);}
        for blob in &draft.answer.blobs {self.read_blob(blob)?;}
        let payload_hash=crypto::hash(&serde_json::to_vec(&draft.answer)?);
        let existing:Option<(String,Vec<u8>)>=self.db.query_row("SELECT operation_id,cipher FROM outbox WHERE intent=?1",
            [intent.to_string()],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
        if let Some((id,bytes))=existing {
            let e:Envelope=serde_json::from_slice(&self.decrypt(&format!("operation/{id}"),&bytes)?)?;
            if e.operation.payload_hash!=payload_hash || e.operation.draft_revision!=revision {return Err(Error::Conflict);}
            return Ok(e)
        }
        let tx=self.db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        // Recheck the CAS inside the write transaction.
        let current:u64=tx.query_row("SELECT revision FROM drafts WHERE assignment=?1",[assignment],|r|read_u64(r,0))?;
        if current!=revision {return Err(Error::Conflict);}
        tx.execute("UPDATE counters SET seq=seq+1",[])?;
        let seq:u64=tx.query_row("SELECT seq FROM counters",[],|r|read_u64(r,0))?;
        let id=Uuid::new_v4().to_string();
        let operation=Operation{protocol:PROTOCOL,operation_id:id.clone(),origin_device_id:self.device.clone(),
            device_seq:seq,subject_id:self.subject.clone(),app_id:self.app.id().into(),credential_epoch:g.epoch,
            kind:"submission".into(),answer:draft.answer,draft_revision:revision,payload_hash};
        let proof=crypto::sign(&self.signer,"thinkgrow.operation.v1",&serde_json::to_vec(&operation)?);
        let envelope=Envelope{operation,proof};
        let cipher=self.key.seal(&format!("store/{}/operation/{id}",self.binding),&serde_json::to_vec(&envelope)?)?;
        tx.execute("INSERT INTO outbox(operation_id,intent,cipher) VALUES(?1,?2,?3)",params![id,intent.to_string(),cipher])?;
        for dest in [Destination::Classroom,Destination::Online] {
            tx.execute("INSERT INTO deliveries(operation_id,destination,state) VALUES(?1,?2,'pending')",params![id,dest.text()])?;
        }
        tx.execute("UPDATE drafts SET dirty=0 WHERE assignment=?1",[assignment])?;
        observe(&tx,now)?; tx.commit()?; Ok(envelope)
    }
    pub fn lease(&mut self,destination:Destination,now:i64)->Result<Option<Lease>> {
        self.writable(now,None)?;
        let tx=self.db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let row:Option<(String,Vec<u8>,u32)>=tx.query_row(
            "SELECT o.operation_id,o.cipher,d.attempts FROM outbox o JOIN deliveries d ON o.operation_id=d.operation_id
             WHERE d.destination=?1 AND ((d.state='pending' AND d.next_ms<=?2) OR (d.state='sending' AND d.lease_until<=?2))
             ORDER BY o.rowid LIMIT 1",params![destination.text(),now],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
        let Some((id,cipher,attempts))=row else{return Ok(None)};
        let token=Uuid::new_v4().to_string();
        tx.execute("UPDATE deliveries SET state='sending',lease=?1,lease_until=?2,attempts=attempts+1 WHERE operation_id=?3 AND destination=?4",
            params![token,now.saturating_add(60_000),id,destination.text()])?;
        observe(&tx,now)?; tx.commit()?;
        Ok(Some(Lease{operation_id:id.clone(),token,attempts:attempts.saturating_add(1),destination,
            envelope:serde_json::from_slice(&self.decrypt(&format!("operation/{id}"),&cipher)?)?}))
    }
    /// Returns true only for a new durable receipt; replay of exactly the same receipt is idempotent.
    pub fn accept_receipt(&mut self,signed:&Signed,expected:Destination)->Result<bool> {
        let keys=match expected {Destination::Classroom=>&self.trust.classroom_receipts,Destination::Online=>&self.trust.online_receipts};
        let receipt:Receipt=signed.verify(keys,"thinkgrow.receipt.v1")?;
        let cipher:Vec<u8>=self.db.query_row("SELECT cipher FROM outbox WHERE operation_id=?1",
            [&receipt.operation_id],|r|r.get(0))?;
        let envelope:Envelope=serde_json::from_slice(&self.decrypt(&format!("operation/{}",receipt.operation_id),&cipher)?)?;
        let o=&envelope.operation;
        if receipt.protocol!=PROTOCOL || receipt.destination!=expected || receipt.origin_device_id!=self.device ||
            receipt.subject_id!=self.subject || receipt.payload_hash!=o.payload_hash || receipt.server_received_ms<0 {
                return Err(Error::Invalid("receipt binding")); }
        id(&receipt.receipt_id)?;
        let tx=self.db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let old:Option<String>=tx.query_row("SELECT signed FROM receipts WHERE operation_id=?1 AND destination=?2",
            params![receipt.operation_id,expected.text()],|r|r.get(0)).optional()?;
        let encoded=serde_json::to_string(signed)?;
        if let Some(old)=old { if old==encoded {return Ok(false)} return Err(Error::Conflict); }
        tx.execute("INSERT INTO receipts VALUES(?1,?2,?3)",params![receipt.operation_id,expected.text(),encoded])?;
        tx.execute("UPDATE deliveries SET state='received',lease=NULL,lease_until=0 WHERE operation_id=?1 AND destination=?2",
            params![receipt.operation_id,expected.text()])?;
        tx.commit()?; Ok(true)
    }
    pub fn delivery_failed(&mut self,lease:&Lease,state:&str,next_ms:i64,remote:Option<&str>)->Result<()> {
        if !["pending","auth_wait","conflict"].contains(&state) {return Err(Error::Invalid("delivery state"));}
        let tx=self.db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let changed=tx.execute("UPDATE deliveries SET state=?1,next_ms=?2,lease=NULL,lease_until=0
            WHERE operation_id=?3 AND destination=?4 AND state='sending' AND lease=?5",
            params![state,next_ms,lease.operation_id,lease.destination.text(),lease.token])?;
        if changed!=1 {return Err(Error::Lease);}
        if let Some(remote)=remote {
            if remote.len()>MAX_TEXT {return Err(Error::Invalid("remote conflict too large"));}
            let cipher=self.key.seal(&format!("store/{}/conflict/{}",self.binding,lease.operation_id),remote.as_bytes())?;
            tx.execute("INSERT INTO conflicts(operation_id,cipher) VALUES(?1,?2)",params![lease.operation_id,cipher])?;
        }
        tx.commit()?; Ok(())
    }
    pub fn resume_after_renewal(&mut self,now:i64)->Result<()> {
        self.writable(now,None)?;
        self.db.execute("UPDATE deliveries SET state='pending',next_ms=0 WHERE state='auth_wait'",[])?; Ok(())
    }
    pub fn read_blob(&self,hash:&str)->Result<Vec<u8>> {
        if !digest(hash) {return Err(Error::Invalid("blob hash"));}
        let bytes=fs::read(self.home.join("blobs").join(hash))?;
        let plain=self.decrypt(&format!("blob/{hash}"),&bytes)?;
        if crypto::hash(&plain)!=hash {return Err(Error::Encryption);} Ok(plain)
    }
    pub fn put_blob(&mut self,plain:&[u8],now:i64)->Result<String> {
        self.writable(now,None)?;
        if plain.len()>MAX_BLOB {return Err(Error::Invalid("blob size"));}
        let hash=crypto::hash(plain); let file=self.home.join("blobs").join(&hash);
        self.db.execute("INSERT INTO blobs(hash,state) VALUES(?1,'pending') ON CONFLICT(hash) DO NOTHING",[&hash])?;
        if !file.exists() {
            let staging=self.home.join("blobs").join(format!("{}.part",Uuid::new_v4()));
            let mut f=OpenOptions::new().write(true).create_new(true).open(&staging)?;
            f.write_all(&self.seal(&format!("blob/{hash}"),plain)?)?; f.sync_all()?; drop(f);
            fs::rename(&staging,&file)?;
        }
        self.read_blob(&hash)?;
        self.db.execute("UPDATE blobs SET state='ready' WHERE hash=?1",[&hash])?; Ok(hash)
    }
    /// Reconciles journal records; never deletes user data or unexplained files.
    pub fn recover_blobs(&mut self)->Result<Vec<String>> {
        let hashes:Vec<String>=self.db.prepare("SELECT hash FROM blobs")?.query_map([],|r|r.get(0))?.collect::<std::result::Result<_,_>>()?;
        let mut missing=vec![];
        for hash in hashes {
            let valid=self.read_blob(&hash).is_ok();
            self.db.execute("UPDATE blobs SET state=?1 WHERE hash=?2",params![if valid {"ready"} else {"missing"},hash])?;
            if !valid {missing.push(hash);}
        } Ok(missing)
    }
    fn seal(&self,aad:&str,plain:&[u8])->Result<Vec<u8>> {self.key.seal(&format!("store/{}/{aad}",self.binding),plain)}
    fn decrypt(&self,aad:&str,cipher:&[u8])->Result<Vec<u8>> {self.key.open(&format!("store/{}/{aad}",self.binding),cipher)}
    pub fn storage_path(&self)->&Path {&self.home}
}
fn observe(tx:&Transaction<'_>,now:i64)->Result<()> {
    tx.execute("UPDATE counters SET wall_floor=max(wall_floor,?1)",[now])?; Ok(())
}

fn read_u64(row:&rusqlite::Row<'_>,index:usize)->rusqlite::Result<u64> {
    u64::try_from(row.get::<_,i64>(index)?).map_err(|_|rusqlite::Error::InvalidQuery)
}

#[path="lesson_store.rs"]
mod lesson_store;
