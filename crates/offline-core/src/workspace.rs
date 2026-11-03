//! Encrypted, role-bound snapshots and durable actions. Game scores are untrusted observations.
use super::*;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use base64::{engine::general_purpose::STANDARD, Engine};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RoleReply {
    pub protocol:u32, pub app_id:String, pub subject_id:String, pub device_id:String,
    pub credential_epoch:u64, pub issued_ms:i64, pub expires_ms:i64,
    pub request_id:String, pub action:String, pub data:Value,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RoleDraft {pub slot:String,pub revision:u64,pub operation_id:String,pub kind:String,pub data:Value,pub state:String,pub receipt:Option<Signed>}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionTicket {
    pub protocol:u32, pub game_id:String, pub device_id:String, pub subject_id:String,
    pub credential_epoch:u64, pub issued_ms:i64, pub expires_ms:i64,
    #[serde(default)] pub allowed_origins:Vec<String>,
}
pub fn valid_online_origin(origin:&str)->bool {
    url::Url::parse(origin).is_ok_and(|u| matches!(u.scheme(),"https"|"wss") && u.host_str().is_some()
        && u.username().is_empty() && u.password().is_none() && u.query().is_none()
        && u.fragment().is_none() && u.origin().ascii_serialization()==origin)
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GameFile {pub data:String,pub sha256:String}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GameBundle {pub id:String,pub title:String,pub version:String,pub runtime:String,pub host_api:u32,pub entry:String,pub source_sha256:String,pub files:BTreeMap<String,GameFile>,#[serde(default)]pub network_mode:Option<String>}
#[derive(Clone, Serialize, Deserialize)]
pub struct GameInfo {pub id:String,pub title:String,pub version:String,pub source_sha256:String,#[serde(default)]pub network_mode:Option<String>,#[serde(default)]pub can_run_offline:bool}
#[derive(Clone, Serialize, Deserialize)]
pub struct GameRun {pub id:String,pub game:GameInfo,pub assignment_id:Option<String>,pub events:Vec<Value>,pub checkpoint:Value}

pub fn safe_game_path(path:&str)->bool {
    !path.is_empty() && path.len()<=240 && path.bytes().all(|b|b.is_ascii_alphanumeric() || b"-_.~/".contains(&b)) && path.split('/').all(|p|!p.is_empty() && p!="." && p!="..")
}
impl GameBundle {
    pub fn validate(&self)->Result<()> {
        id(&self.id)?;id(&self.version)?;
        if self.title.len()>512 || self.runtime!="static" || self.host_api!=1 || !digest(&self.source_sha256) || self.files.is_empty() || self.files.len()>128 || !self.files.contains_key(&self.entry) || !self.entry.ends_with(".html"){return Err(Error::Invalid("unsupported offline game"))}
        let mut total=0;
        for (path,file) in &self.files {
            if !safe_game_path(path) || file.data.len()>700000 || !digest(&file.sha256){return Err(Error::Invalid("game file"))}
            let bytes=STANDARD.decode(&file.data).map_err(|_|Error::Invalid("game encoding"))?;total+=bytes.len();
            if bytes.len()>524288 || total>1048576 || crypto::hash(&bytes)!=file.sha256{return Err(Error::Signature)}
        }Ok(())
    }
    pub fn info(&self)->GameInfo{

        let is_offline = self.network_mode.as_deref().unwrap_or("offline_solo") == "offline_solo";

        GameInfo{id:self.id.clone(),title:self.title.clone(),version:self.version.clone(),source_sha256:self.source_sha256.clone(),network_mode:self.network_mode.clone(),can_run_offline:is_offline}

    }
}
impl Store {
    fn role_db(&self,domain:&str)->Result<Connection> {
        let path=self.home.join(format!("{domain}.sqlite3"));let exists=path.exists();
        if exists && fs::symlink_metadata(&path)?.file_type().is_symlink(){return Err(Error::Partition)}
        let mut db=Connection::open(path)?;db.busy_timeout(Duration::from_secs(3))?;db.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL;")?;
        let version:u32=db.query_row("PRAGMA user_version",[],|r|r.get(0))?;
        if exists && version!=1{return Err(Error::Schema)}
        if !exists{let tx=db.transaction()?;tx.execute_batch("CREATE TABLE objects(id TEXT PRIMARY KEY,cipher BLOB NOT NULL); CREATE TABLE binding(cipher BLOB NOT NULL); PRAGMA user_version=1;")?;tx.execute("INSERT INTO binding VALUES(?1)",[self.seal(&format!("{domain}/binding"),b"role-schema-1")?])?;tx.commit()?;}
        let binding:Vec<u8>=db.query_row("SELECT cipher FROM binding",[],|r|r.get(0))?;
        if self.decrypt(&format!("{domain}/binding"),&binding)?!=b"role-schema-1"{return Err(Error::Partition)}Ok(db)
    }
    fn role_read<T:DeserializeOwned>(&self,domain:&str,key:&str)->Result<Option<T>> {
        let row:Option<Vec<u8>>=self.role_db(domain)?.query_row("SELECT cipher FROM objects WHERE id=?1",[key],|r|r.get(0)).optional()?;
        row.map(|bytes|Ok(serde_json::from_slice(&self.decrypt(&format!("{domain}/{key}"),&bytes)?)?)).transpose()
    }
    fn role_write<T:Serialize>(&self,domain:&str,key:&str,value:&T)->Result<()> {
        let cipher=self.seal(&format!("{domain}/{key}"),&serde_json::to_vec(value)?)?;
        self.role_db(domain)?.execute("INSERT INTO objects VALUES(?1,?2) ON CONFLICT(id) DO UPDATE SET cipher=excluded.cipher",params![key,cipher])?;Ok(())
    }
    fn role_writable(&self,now:i64)->Result<Grant>{
        self.db.execute("UPDATE counters SET wall_floor=max(wall_floor,?1)",[now])?;
        if self.access(now)?!=Access::ReadWrite{return Err(Error::ReadRecoveryOnly)}self.grant()?.ok_or(Error::Authorization)
    }
    pub fn role_request(&self,action:&str,data:Value,request_id:&str,now:i64)->Result<Vec<u8>> {
        self.role_writable(now)?;Uuid::parse_str(request_id).map_err(|_|Error::Invalid("request ID"))?;
        let payload=serde_json::to_string(&json!({"protocol":1,"action":action,"app_id":self.app.id(),"subject_id":self.subject,"device_id":self.device,"issued_ms":now,"request_id":request_id,"data":data}))?;
        if payload.len()>65536{return Err(Error::Invalid("role request too large"))}
        let raw:String=self.db.query_row("SELECT value FROM meta WHERE key='grant'",[],|r|r.get(0))?;let credential:Signed=serde_json::from_str(&raw)?;
        let proof=crypto::sign(&self.signer,"thinkgrow.role-request.v1",payload.as_bytes());Ok(serde_json::to_vec(&json!({"credential":credential,"payload":payload,"proof":proof}))?)
    }
    pub fn verify_role_reply(&self,signed:&Signed)->Result<RoleReply> {
        let reply:RoleReply=signed.verify(&self.trust.credentials,"thinkgrow.role-reply.v1")?;
        if reply.protocol!=1 || reply.app_id!=self.app.id() || reply.subject_id!=self.subject || reply.device_id!=self.device || reply.issued_ms<0 || reply.expires_ms<=reply.issued_ms || reply.expires_ms-reply.issued_ms>WEEK_MS{return Err(Error::Partition)}Ok(reply)
    }
    pub fn accept_role_reply(&self,signed:&Signed,action:&str,request_id:&str,now:i64)->Result<RoleReply> {
        let grant=self.role_writable(now)?;let reply=self.verify_role_reply(signed)?;
        if reply.action!=action || reply.request_id!=request_id || reply.credential_epoch!=grant.epoch || reply.issued_ms>now.saturating_add(300000) || reply.expires_ms<=now || reply.expires_ms>grant.expires_ms{return Err(Error::Authorization)}
        if action=="workspace" {
            let role=match self.app{App::Teacher=>"teacher",App::Parent=>"parent",App::Student=>"student"};
            if reply.data["role"]!=role{return Err(Error::Partition)}
            if self.role_workspace()?.is_some_and(|old|old.issued_ms>reply.issued_ms){return Err(Error::Conflict)}self.role_write("role","workspace",signed)?;
        }else if action=="install_game" {
            if self.app==App::Parent{return Err(Error::Authorization)}
            let bundle:GameBundle=serde_json::from_value(reply.data["game"].clone())?;bundle.validate()?;self.role_write("games",&format!("game/{}",bundle.id),signed)?;
        }Ok(reply)
    }
    pub fn role_workspace(&self)->Result<Option<RoleReply>>{self.role_read::<Signed>("role","workspace")?.map(|s|self.verify_role_reply(&s)).transpose()}
    pub fn role_drafts(&self)->Result<Vec<RoleDraft>> {
        let db=self.role_db("role")?;let mut q=db.prepare("SELECT id FROM objects WHERE id LIKE 'draft/%' ORDER BY id")?;
        let ids:Vec<String>=q.query_map([],|r|r.get(0))?.collect::<std::result::Result<_,_>>()?;ids.iter().map(|id|self.role_read("role",id)?.ok_or(Error::Schema)).collect()
    }
    pub fn save_role_draft(&self,kind:&str,data:Value,expected:u64,now:i64)->Result<RoleDraft> {
        let grant=self.role_writable(now)?;let workspace=self.role_workspace()?.ok_or(Error::Authorization)?;
        if workspace.expires_ms<=now || workspace.credential_epoch!=grant.epoch{return Err(Error::ReadRecoveryOnly)}
        let slot=if kind=="grade" && self.app==App::Teacher {
            exact(&data,&["answer_version_id","base_grading_revision","scores","feedback","level","confirm"])?;
            let version=data["answer_version_id"].as_str().ok_or(Error::Authorization)?;id(version)?;
            let selected=&workspace.data["selected"];let rows=selected["submissions"].as_array().ok_or(Error::Authorization)?;
            let row=rows.iter().find(|r|r["answer_version_id"]==version).ok_or(Error::Authorization)?;
            if data["base_grading_revision"]!=row["base_grading_revision"]{return Err(Error::Conflict)}
            let scores=data["scores"].as_object().ok_or(Error::Invalid("scores"))?;let rubrics=selected["rubrics"].as_array().ok_or(Error::Invalid("rubrics"))?;
            let confirm=data["confirm"].as_bool().ok_or(Error::Invalid("confirm"))?;
            if selected["evaluation_mode"]=="score" {
                if confirm && scores.len()!=rubrics.len(){return Err(Error::Invalid("all rubric scores required"))}
                for (key,value) in scores{let r=rubrics.iter().find(|r|r["id"]==key.as_str()).ok_or(Error::Invalid("rubric"))?;if value.as_u64().zip(r["max_score"].as_u64()).is_none_or(|(v,max)|v>max){return Err(Error::Invalid("score range"))}}
            }else if selected["evaluation_mode"]=="level" {
                if !scores.is_empty() || (confirm && !["매우 잘함","잘함","보통","노력 필요"].contains(&data["level"].as_str().unwrap_or(""))){return Err(Error::Invalid("achievement level"))}
            }else{return Err(Error::Invalid("evaluation mode"))}
            if data["feedback"].as_str().is_none_or(|s|s.len()>10000){return Err(Error::Invalid("feedback"))}format!("grade/{version}")
        }else if kind=="parent_reply" && self.app==App::Parent {
            exact(&data,&["link_id","body","link_revision"])?;let link=data["link_id"].as_str().ok_or(Error::Authorization)?;id(link)?;
            let rows=workspace.data["inbox"].as_array().ok_or(Error::Authorization)?;
            if !rows.iter().any(|r|r["id"]==link && r["link_revision"]==data["link_revision"]){return Err(Error::Authorization)}
            if data["body"].as_str().is_none_or(|s|s.trim().is_empty() || s.chars().count()>2000){return Err(Error::Invalid("message length"))}format!("reply/{link}")
        }else{return Err(Error::Authorization)};
        let key=format!("draft/{slot}");let old=self.role_read::<RoleDraft>("role",&key)?;
        if old.as_ref().map_or(0,|d|d.revision)!=expected{return Err(Error::Conflict)}
        let draft=RoleDraft{slot,revision:expected.checked_add(1).filter(|n|*n<=i64::MAX as u64).ok_or(Error::Conflict)?,operation_id:Uuid::new_v4().to_string(),kind:kind.into(),data,state:"local".into(),receipt:None};
        let mut db=self.role_db("role")?;let tx=db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let previous:Option<Vec<u8>>=tx.query_row("SELECT cipher FROM objects WHERE id=?1",[&key],|r|r.get(0)).optional()?;
        let previous=previous.map(|b|->Result<RoleDraft>{Ok(serde_json::from_slice(&self.decrypt(&format!("role/{key}"),&b)?)?)}).transpose()?;
        if previous.as_ref().map_or(0,|d|d.revision)!=expected{return Err(Error::Conflict)}
        for name in [format!("history/{}/{}",draft.slot,draft.revision),key] {let cipher=self.seal(&format!("role/{name}"),&serde_json::to_vec(&draft)?)?;tx.execute("INSERT INTO objects VALUES(?1,?2) ON CONFLICT(id) DO UPDATE SET cipher=excluded.cipher",params![name,cipher])?;}
        tx.commit()?;self.db.execute("UPDATE counters SET wall_floor=max(wall_floor,?1)",[now])?;Ok(draft)
    }
    pub fn role_draft(&self,slot:&str,revision:u64)->Result<RoleDraft>{let d:RoleDraft=self.role_read("role",&format!("draft/{slot}"))?.ok_or(Error::Authorization)?;if d.slot!=slot || d.revision!=revision{return Err(Error::Conflict)}Ok(d)}
    pub fn finish_role_draft(&self,mut draft:RoleDraft,receipt:Option<Signed>)->Result<RoleDraft>{self.role_draft(&draft.slot,draft.revision)?;draft.state=if receipt.is_some(){"synced"}else{"conflict"}.into();draft.receipt=receipt;self.role_write("role",&format!("draft/{}",draft.slot),&draft)?;Ok(draft)}
    pub fn installed_games(&self)->Result<Vec<GameInfo>> {
        let db=self.role_db("games")?;let mut q=db.prepare("SELECT id FROM objects WHERE id LIKE 'game/%' ORDER BY id")?;let ids:Vec<String>=q.query_map([],|r|r.get(0))?.collect::<std::result::Result<_,_>>()?;
        ids.iter().map(|id|Ok(self.game_bundle(id.strip_prefix("game/").ok_or(Error::Schema)?)?.info())).collect()
    }
    pub fn game_bundle(&self,id:&str)->Result<GameBundle>{if self.app==App::Parent{return Err(Error::Authorization)}let signed:Signed=self.role_read("games",&format!("game/{id}"))?.ok_or(Error::Invalid("game not installed"))?;let reply=self.verify_role_reply(&signed)?;if reply.action!="install_game"{return Err(Error::Signature)}let b:GameBundle=serde_json::from_value(reply.data["game"].clone())?;b.validate()?;if b.id!=id{return Err(Error::Partition)}Ok(b)}
    pub fn start_game(&self,id:&str,now:i64)->Result<(GameRun,GameBundle)> {
        self.start_game_for(id,None,now)
    }
    pub fn start_game_for(&self,id:&str,assignment:Option<&str>,now:i64)->Result<(GameRun,GameBundle)> {
        let grant=self.role_writable(now)?;let bundle=self.game_bundle(id)?;
        if bundle.network_mode.as_deref() == Some("online_session") {
            return Err(Error::Invalid("online game requires verified online session"));
        }
        if self.app==App::Student{let assignment=assignment.ok_or(Error::Authorization)?;let w=self.role_workspace()?.ok_or(Error::Authorization)?;if !grant.scopes.iter().any(|s|s.assignment_id==assignment) || w.credential_epoch!=grant.epoch || w.expires_ms<=now || !w.data["games"].as_array().is_some_and(|g|g.iter().any(|g|g["id"]==id && g["assignment_id"]==assignment && g["source_sha256"]==bundle.source_sha256)){return Err(Error::Authorization)}}
        if self.app==App::Teacher && assignment.is_some(){return Err(Error::Authorization)}
        if let Some(assignment)=assignment{crate::id(assignment)?;}
        let checkpoint=self.role_read::<Value>("role",&format!("checkpoint/{id}/{}/{}",bundle.source_sha256,assignment.unwrap_or("preview")))?.unwrap_or(Value::Null);
        let run=GameRun{id:Uuid::new_v4().to_string(),game:bundle.info(),assignment_id:assignment.map(str::to_owned),events:vec![],checkpoint};self.role_write("role",&format!("run/{}",run.id),&run)?;Ok((run,bundle))
    }
    pub fn start_online_game_for(&self,id:&str,assignment:Option<&str>,session_token:&str,now:i64)->Result<(GameRun,GameBundle,SessionTicket)> {
        let grant=self.role_writable(now)?;let bundle=self.game_bundle(id)?;
        if session_token.trim().is_empty() {
            return Err(Error::Invalid("valid online session token required"));
        }
        let signed:Signed=serde_json::from_str(session_token).map_err(|_|Error::Invalid("session token format"))?;
        let ticket:SessionTicket=signed.verify(&self.trust.credentials,"thinkgrow.game-session.v1")?;
        if ticket.protocol!=1 || ticket.game_id!=id || ticket.device_id!=self.device || ticket.subject_id!=self.subject || ticket.credential_epoch!=grant.epoch || ticket.issued_ms>now.saturating_add(300_000) || ticket.expires_ms<=now || ticket.allowed_origins.len()>10 || !ticket.allowed_origins.iter().all(|o|valid_online_origin(o)) {
            return Err(Error::Authorization);
        }
        if self.app==App::Student{let assignment=assignment.ok_or(Error::Authorization)?;let w=self.role_workspace()?.ok_or(Error::Authorization)?;if !grant.scopes.iter().any(|s|s.assignment_id==assignment) || w.credential_epoch!=grant.epoch || w.expires_ms<=now || !w.data["games"].as_array().is_some_and(|g|g.iter().any(|g|g["id"]==id && g["assignment_id"]==assignment && g["source_sha256"]==bundle.source_sha256)){return Err(Error::Authorization)}}
        if self.app==App::Teacher && assignment.is_some(){return Err(Error::Authorization)}
        if let Some(assignment)=assignment{crate::id(assignment)?;}
        let checkpoint=self.role_read::<Value>("role",&format!("checkpoint/{id}/{}/{}",bundle.source_sha256,assignment.unwrap_or("preview")))?.unwrap_or(Value::Null);
        let run=GameRun{id:Uuid::new_v4().to_string(),game:bundle.info(),assignment_id:assignment.map(str::to_owned),events:vec![],checkpoint};self.role_write("role",&format!("run/{}",run.id),&run)?;Ok((run,bundle,ticket))
    }
    pub fn game_event(&self,run_id:&str,event:Value,now:i64)->Result<Value> {
        self.role_writable(now)?;exact(&event,&["sequence","kind","data"])?;if serde_json::to_vec(&event)?.len()>16384{return Err(Error::Invalid("game event size"))}
        let mut run:GameRun=self.role_read("role",&format!("run/{run_id}"))?.ok_or(Error::Authorization)?;let seq=event["sequence"].as_u64().ok_or(Error::Invalid("event sequence"))?;
        if seq>0 && seq<=run.events.len() as u64{if run.events[seq as usize-1]==event{return Ok(json!({"accepted":seq}))}return Err(Error::Conflict)}
        if seq!=run.events.len() as u64+1 || run.events.len()>=4096 || !["progress","answer","complete","checkpoint"].contains(&event["kind"].as_str().unwrap_or("")){return Err(Error::Invalid("event sequence or kind"))}
        let checkpoint=event["kind"]=="checkpoint";if checkpoint{run.checkpoint=event["data"].clone();}run.events.push(event);
        let mut db=self.role_db("role")?;let tx=db.transaction()?;
        let mut entries=vec![(format!("run/{run_id}"),serde_json::to_vec(&run)?)];
        if checkpoint{entries.push((format!("checkpoint/{}/{}/{}",run.game.id,run.game.source_sha256,run.assignment_id.as_deref().unwrap_or("preview")),serde_json::to_vec(&run.checkpoint)?));}
        for (key,bytes) in entries{let cipher=self.seal(&format!("role/{key}"),&bytes)?;tx.execute("INSERT INTO objects VALUES(?1,?2) ON CONFLICT(id) DO UPDATE SET cipher=excluded.cipher",params![key,cipher])?;}tx.commit()?;Ok(json!({"accepted":seq}))
    }
    pub fn game_run(&self,id:&str,now:i64)->Result<GameRun>{self.role_writable(now)?;self.role_read("role",&format!("run/{id}"))?.ok_or(Error::Authorization)}
    pub(crate) fn start_material_run(&self,info:crate::MaterialInfo,bundle:GameBundle,now:i64)->Result<(GameRun,GameBundle)>{
        let grant=self.role_writable(now)?;
        if self.app==App::Student{
            let assignment=info.assignment_id.as_deref().ok_or(Error::Authorization)?;
            let w=self.role_workspace()?.ok_or(Error::Authorization)?;
            if w.credential_epoch!=grant.epoch || w.expires_ms<=now || !grant.scopes.iter().any(|s|s.assignment_id==assignment)
                || !w.data["materials"].as_array().is_some_and(|a|a.iter().any(|m|m["id"]==info.id && m["assignment_id"]==assignment && m["source_sha256"]==info.sha256)) {return Err(Error::Authorization)}
        }else if self.app!=App::Teacher{return Err(Error::Authorization)}
        let checkpoint=self.role_read::<Value>("role",&format!("checkpoint/{}/{}/{}",info.id,info.sha256,info.assignment_id.as_deref().unwrap_or("preview")))?.unwrap_or(Value::Null);
        let run=GameRun{id:Uuid::new_v4().to_string(),game:bundle.info(),assignment_id:info.assignment_id,events:vec![],checkpoint};
        self.role_write("role",&format!("run/{}",run.id),&run)?;Ok((run,bundle))
    }
    pub(crate) fn material_session_origins(&self,info:&crate::MaterialInfo,bundle:&GameBundle,token:Option<&str>,now:i64)->Result<Vec<String>>{
        if bundle.network_mode.as_deref()!=Some("online_session"){return Ok(vec![])}
        let signed:Signed=serde_json::from_str(token.ok_or(Error::Authorization)?)?;
        let ticket:SessionTicket=signed.verify(&self.trust.credentials,"thinkgrow.material-session.v1")?;
        let grant=self.role_writable(now)?;
        let manifest=&bundle.files["thinkgrow.app.json"];
        let manifest:Value=serde_json::from_slice(&STANDARD.decode(&manifest.data).map_err(|_|Error::Signature)?)?;
        if ticket.protocol!=1 || ticket.game_id!=info.id || ticket.subject_id!=self.subject || ticket.device_id!=self.device
            || ticket.credential_epoch!=grant.epoch || ticket.issued_ms>now+300_000 || ticket.expires_ms<=now
            || ticket.allowed_origins.len()>10 || !ticket.allowed_origins.iter().all(|o|valid_online_origin(o))
            || serde_json::to_value(&ticket.allowed_origins)?!=manifest["allowed_origins"]{return Err(Error::Authorization)}
        Ok(ticket.allowed_origins)
    }
    pub(crate) fn authorize_installer(&self,info:&crate::MaterialInfo,now:i64)->Result<()>{
        let grant=self.role_writable(now)?;let w=self.role_workspace()?.ok_or(Error::Authorization)?;
        if w.credential_epoch!=grant.epoch || w.expires_ms<=now || !w.data["materials"].as_array().is_some_and(|rows|rows.iter().any(|r|
            r["id"]==info.id && r["source_sha256"]==info.sha256 && r["security_policy"]==2
            && r["signature_status"]=="valid_signed" && r["scanner_verdict"]=="clean")){return Err(Error::Authorization)}
        Ok(())
    }
}
fn exact(value:&Value,fields:&[&str])->Result<()>{if !value.as_object().is_some_and(|v|v.len()==fields.len() && fields.iter().all(|f|v.contains_key(*f))){return Err(Error::Invalid("role action fields"))}Ok(())}
