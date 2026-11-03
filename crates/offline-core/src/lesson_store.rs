//! Separate schema: existing core database/checksum and outbox are unchanged.
use super::*;
use crate::lesson::Lesson;
impl Store {
 fn lesson_db(&self)->Result<Connection> {
  let file=self.home.join("lessons.sqlite3");let existing=file.exists();let mut db=Connection::open(file)?;
  db.busy_timeout(Duration::from_secs(3))?;db.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL;")?;
  let version:u32=db.query_row("PRAGMA user_version",[],|r|r.get(0))?;
  if existing && version!=1 {return Err(Error::Schema)}
  if !existing {
   let tx=db.transaction()?;
   tx.execute_batch("CREATE TABLE lessons(assignment TEXT NOT NULL,revision TEXT NOT NULL,cipher BLOB NOT NULL,issued INTEGER NOT NULL,PRIMARY KEY(assignment,revision)); CREATE TABLE binding(value BLOB NOT NULL); PRAGMA user_version=1;")?;
   tx.execute("INSERT INTO binding(value) VALUES(?1)",[self.seal("lessons/key-check",b"prepared-lessons-schema-1")?])?;tx.commit()?;
  }
  let cipher:Vec<u8>=db.query_row("SELECT value FROM binding",[],|r|r.get(0))?;
  if self.decrypt("lessons/key-check",&cipher)?!=b"prepared-lessons-schema-1" {return Err(Error::Partition)}
  Ok(db)
 }
 fn verified_lesson(&self,signed:&Signed)->Result<Lesson> {
  let lesson:Lesson=signed.verify(&self.trust.credentials,"thinkgrow.lesson.v1")?;lesson.validate()?;
  if self.app!=App::Student || lesson.subject_id!=self.subject || lesson.device_id!=self.device {return Err(Error::Partition)}
  Ok(lesson)
 }
 pub fn install_lessons(&self,signed:&[Signed],now:i64)->Result<()> {
  if self.access(now)?!=Access::ReadWrite{return Err(Error::ReadRecoveryOnly)}
  if signed.is_empty() || signed.len()>16{return Err(Error::Invalid("lesson count"))}
  let grant=self.grant()?.ok_or(Error::Authorization)?;
  let mut rows=Vec::new();let mut assignments=std::collections::BTreeSet::new();
  for s in signed {
   let l=self.verified_lesson(s)?;
   if l.credential_epoch!=grant.epoch || l.issued_ms>now.saturating_add(300000) || l.expires_ms<=now || l.expires_ms>grant.expires_ms || !grant.scopes.iter().any(|scope|scope.school_id==l.school_id && scope.class_id==l.class_id && scope.assignment_id==l.assignment_id) || !assignments.insert(l.assignment_id.clone()) {return Err(Error::Authorization)}
   let cipher=self.seal(&format!("lesson/{}/{}",l.assignment_id,l.package_revision),&serde_json::to_vec(s)?)?;
   rows.push((l,cipher));
  }
  let mut db=self.lesson_db()?;let tx=db.transaction()?;
  for (l,cipher) in rows {tx.execute("INSERT INTO lessons VALUES(?1,?2,?3,?4) ON CONFLICT(assignment,revision) DO UPDATE SET cipher=excluded.cipher,issued=excluded.issued WHERE excluded.issued>=lessons.issued",params![l.assignment_id,l.package_revision,cipher,l.issued_ms])?;}
  tx.commit()?;Ok(())
 }
 pub fn lesson(&self,assignment:&str,revision:Option<&str>)->Result<Option<Lesson>> {
  if self.app!=App::Student{return Err(Error::Authorization)}
  let db=self.lesson_db()?;
  let row:Option<(String,Vec<u8>)>=if let Some(revision)=revision {db.query_row("SELECT revision,cipher FROM lessons WHERE assignment=?1 AND revision=?2",params![assignment,revision],|r|Ok((r.get(0)?,r.get(1)?))).optional()?}
  else {db.query_row("SELECT revision,cipher FROM lessons WHERE assignment=?1 ORDER BY issued DESC,revision LIMIT 1",[assignment],|r|Ok((r.get(0)?,r.get(1)?))).optional()?};
  row.map(|(revision,cipher)|{let s:Signed=serde_json::from_slice(&self.decrypt(&format!("lesson/{assignment}/{revision}"),&cipher)?)?;let l=self.verified_lesson(&s)?;if l.assignment_id!=assignment || l.package_revision!=revision{return Err(Error::Partition)}Ok(l)}).transpose()
 }
 pub fn lessons(&self)->Result<Vec<Lesson>> {
  if self.app!=App::Student{return Err(Error::Authorization)}
  let db=self.lesson_db()?;let mut query=db.prepare("SELECT DISTINCT assignment FROM lessons ORDER BY assignment")?;
  let ids:Vec<String>=query.query_map([],|r|r.get(0))?.collect::<std::result::Result<_,_>>()?;
  ids.into_iter().map(|id|{let draft=self.load_draft(&id)?;self.lesson(&id,draft.as_ref().map(|d|d.answer.package_revision.as_str()))?.ok_or(Error::Invalid("pinned lesson missing"))}).collect()
 }
 pub fn save_prepared_draft(&mut self,answer:&Answer,expected:u64,now:i64)->Result<u64> {
  let lesson=self.lesson(&answer.assignment_id,Some(&answer.package_revision))?.ok_or(Error::Authorization)?;
  if now<lesson.issued_ms || now>=lesson.expires_ms || lesson.starts_ms.is_some_and(|start|now<start){return Err(Error::ReadRecoveryOnly)}
  lesson.validate_answer(answer)?;
  if let Some(old)=self.load_draft(&answer.assignment_id)? {if old.answer.package_revision!=answer.package_revision{return Err(Error::Conflict)}}
  self.save_draft(answer,expected,now)
 }
 pub fn submit_prepared(&mut self,assignment:&str,revision:u64,intent:Uuid,now:i64)->Result<Envelope> {
  let answer=self.load_draft(assignment)?.ok_or(Error::Invalid("draft missing"))?.answer;
  let lesson=self.lesson(assignment,Some(&answer.package_revision))?.ok_or(Error::Authorization)?;
  if now<lesson.issued_ms || now>=lesson.expires_ms{return Err(Error::ReadRecoveryOnly)}
  lesson.validate_answer(&answer)?;self.submit(assignment,revision,intent,now)
 }
 pub fn lesson_request(&self,now:i64)->Result<serde_json::Value> {
  if self.app!=App::Student || self.access(now)?!=Access::ReadWrite{return Err(Error::Authorization)}
  let grant=self.grant()?.ok_or(Error::Authorization)?;
  if grant.scopes.is_empty() || grant.scopes.len()>16{return Err(Error::Invalid("select one to sixteen lessons when registering"))}
  let raw:String=self.db.query_row("SELECT value FROM meta WHERE key='grant'",[],|r|r.get(0))?;let credential:Signed=serde_json::from_str(&raw)?;
  let payload=serde_json::to_string(&serde_json::json!({"protocol":1,"action":"prepare_lessons","app_id":self.app.id(),"subject_id":self.subject,"device_id":self.device,"issued_ms":now,"request_id":Uuid::new_v4().to_string(),"assignment_ids":grant.scopes.iter().map(|s|&s.assignment_id).collect::<Vec<_>>()}))?;
  let proof=crypto::sign(&self.signer,"thinkgrow.lesson-request.v1",payload.as_bytes());
  Ok(serde_json::json!({"credential":credential,"payload":payload,"proof":proof}))
 }
}
