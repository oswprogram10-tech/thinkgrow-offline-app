use thinkgrow_offline_core::{*,crypto::{self,DataKey},transport::*};
use ed25519_dalek::SigningKey;
use tempfile::TempDir;
use uuid::Uuid;
use rusqlite::Connection;
const NOW:i64=20_000;
struct Fixture {dir:TempDir,issuer:SigningKey,device:SigningKey,classroom:SigningKey,online:SigningKey}
impl Fixture {
 fn new()->Self{Self{dir:tempfile::tempdir().unwrap(),issuer:SigningKey::from_bytes(&[7;32]),
 device:SigningKey::from_bytes(&[8;32]),classroom:SigningKey::from_bytes(&[9;32]),online:SigningKey::from_bytes(&[10;32])}}
 fn trust(&self)->Trust{Trust{credentials:[("issuer".into(),self.issuer.verifying_key().to_bytes())].into(),
 classroom_receipts:[("class".into(),self.classroom.verifying_key().to_bytes())].into(),
 online_receipts:[("online".into(),self.online.verifying_key().to_bytes())].into()}}
 fn open(&self,app:App)->Store{Store::open(self.dir.path(),app,"actor","device",DataKey::new([2;32]),self.device.clone(),self.trust()).unwrap()}
 fn grant(&self,app:App,epoch:u64)->Grant{Grant{protocol:1,app_id:app.id().into(),subject_id:"actor".into(),device_id:"device".into(),
 device_public_key:crypto::b64(&self.device.verifying_key().to_bytes()),epoch,issued_ms:1000,expires_ms:1000+WEEK_MS,
 scopes:vec![Scope{school_id:"school".into(),class_id:"class".into(),assignment_id:"assignment".into()}]}}
 fn signed(&self,g:&Grant)->Signed{let payload=serde_json::to_string(g).unwrap();Signed{key_id:"issuer".into(),
 signature:crypto::sign(&self.issuer,"thinkgrow.credential.v1",payload.as_bytes()),payload}}
 fn store(&self)->Store{let mut s=self.open(App::Student);s.install_grant(&self.signed(&self.grant(App::Student,1)),NOW).unwrap();s}
 fn receipt(&self,e:&Envelope,d:Destination)->Signed{let payload=serde_json::to_string(&Receipt{protocol:1,
 operation_id:e.operation.operation_id.clone(),origin_device_id:"device".into(),subject_id:"actor".into(),
 payload_hash:e.operation.payload_hash.clone(),destination:d,receipt_id:format!("receipt-{}",d.text()),server_received_ms:NOW}).unwrap();
 let (id,key)=match d{Destination::Classroom=>("class",&self.classroom),Destination::Online=>("online",&self.online)};
 Signed{key_id:id.into(),signature:crypto::sign(key,"thinkgrow.receipt.v1",payload.as_bytes()),payload}}
}
fn answer(body:&str)->Answer{Answer{school_id:"school".into(),class_id:"class".into(),assignment_id:"assignment".into(),
 package_revision:"immutable-package-1".into(),base_answer_revision:0,body:body.into(),blobs:vec![]}}
fn submit(s:&mut Store)->Envelope{s.save_draft(&answer("original frozen answer"),0,NOW).unwrap();s.submit("assignment",1,Uuid::new_v4(),NOW).unwrap()}
#[test]fn encrypted_reopen_and_expiry_preserve_data(){
 let f=Fixture::new();let mut s=f.store();s.save_draft(&answer("SECRET-ANSWER-MARKER"),0,NOW).unwrap();let home=s.storage_path().to_owned();
 drop(s);let mut s=f.open(App::Student);assert_eq!(s.load_draft("assignment").unwrap().unwrap().answer.body,"SECRET-ANSWER-MARKER");
 for file in ["store.sqlite3","store.sqlite3-wal"] {if let Ok(bytes)=std::fs::read(home.join(file)){
 assert!(!bytes.windows(20).any(|w|w==b"SECRET-ANSWER-MARKER"));}}
 let expired=WEEK_MS+1001;assert_eq!(s.status(expired).unwrap().access,Access::ReadRecoveryOnly);
 assert!(matches!(s.save_draft(&answer("discard"),1,expired),Err(Error::ReadRecoveryOnly)));
 assert_eq!(s.load_draft("assignment").unwrap().unwrap().revision,1);
}
#[test]fn draft_cas_keeps_old_revisions(){
 let f=Fixture::new();let mut s=f.store();s.save_draft(&answer("one"),0,NOW).unwrap();
 assert!(matches!(s.save_draft(&answer("stale"),0,NOW),Err(Error::Conflict)));
 s.save_draft(&answer("two"),1,NOW).unwrap();
 let db=Connection::open(s.storage_path().join("store.sqlite3")).unwrap();
 assert_eq!(db.query_row("SELECT count(*) FROM draft_revisions",[],|r|r.get::<_,i64>(0)).unwrap(),2);
}
#[test]fn submission_snapshot_and_intent_are_immutable(){
 let f=Fixture::new();let mut s=f.store();s.save_draft(&answer("frozen"),0,NOW).unwrap();
 let intent=Uuid::new_v4();let a=s.submit("assignment",1,intent,NOW).unwrap();let b=s.submit("assignment",1,intent,NOW).unwrap();
 assert_eq!(serde_json::to_vec(&a).unwrap(),serde_json::to_vec(&b).unwrap());a.verify(&f.grant(App::Student,1)).unwrap();
 s.save_draft(&answer("new draft"),1,NOW).unwrap();
 let l=s.lease(Destination::Online,NOW).unwrap().unwrap();assert_eq!(l.envelope.operation.answer.body,"frozen");
 assert_eq!(s.pending(Destination::Online).unwrap(),1);
}
#[test]fn classroom_receipt_does_not_complete_online(){
 let f=Fixture::new();let mut s=f.store();let e=submit(&mut s);let c=f.receipt(&e,Destination::Classroom);
 assert!(s.accept_receipt(&c,Destination::Classroom).unwrap());assert!(!s.accept_receipt(&c,Destination::Classroom).unwrap());
 assert_eq!(s.pending(Destination::Classroom).unwrap(),0);assert_eq!(s.pending(Destination::Online).unwrap(),1);
 assert!(s.accept_receipt(&c,Destination::Online).is_err());assert_eq!(s.pending(Destination::Online).unwrap(),1);
 s.accept_receipt(&f.receipt(&e,Destination::Online),Destination::Online).unwrap();assert_eq!(s.pending(Destination::Online).unwrap(),0);
}
#[test]fn forged_and_misbound_receipts_never_complete(){
 let f=Fixture::new();let mut s=f.store();let e=submit(&mut s);let mut receipt=f.receipt(&e,Destination::Online);
 receipt.payload.push(' ');assert!(s.accept_receipt(&receipt,Destination::Online).is_err());
 let mut wrong=e.clone();wrong.operation.payload_hash="a".repeat(64);
 assert!(s.accept_receipt(&f.receipt(&wrong,Destination::Online),Destination::Online).is_err());
 assert_eq!(s.pending(Destination::Online).unwrap(),1);
}
#[test]fn rollback_clock_blocks_writes_without_deleting(){
 let f=Fixture::new();let mut s=f.store();s.save_draft(&answer("safe"),0,NOW+10000).unwrap();
 assert_eq!(s.access(NOW).unwrap(),Access::ReadRecoveryOnly);assert!(s.submit("assignment",1,Uuid::new_v4(),NOW).is_err());
 assert_eq!(s.load_draft("assignment").unwrap().unwrap().answer.body,"safe");
}
#[test]fn grants_bind_account_device_role_and_key(){
 let f=Fixture::new();let mut s=f.open(App::Student);
 for field in 0..5 {let mut g=f.grant(App::Student,1);match field{0=>g.subject_id="other".into(),1=>g.device_id="other".into(),
 2=>g.app_id=App::Teacher.id().into(),3=>g.device_public_key=crypto::b64(&[0;32]),_=>g.expires_ms+=1}
 assert!(s.install_grant(&f.signed(&g),NOW).is_err());}
 assert_eq!(s.access(NOW).unwrap(),Access::ProvisioningRequired);
}
#[test]fn unknown_and_duplicate_signed_fields_are_rejected(){
 let f=Fixture::new();let mut s=f.open(App::Student);
 let raw=serde_json::to_string(&f.grant(App::Student,1)).unwrap();
 for raw in [raw.replacen("{","{\"unrecognized\":true,",1),raw.replacen("{","{\"protocol\":1,",1)]{
 let signed=Signed{key_id:"issuer".into(),signature:crypto::sign(&f.issuer,"thinkgrow.credential.v1",raw.as_bytes()),payload:raw};
 assert!(s.install_grant(&signed,NOW).is_err());}
}
#[test]fn wrong_key_cannot_silently_replace_existing_store(){
 let f=Fixture::new();let mut s=f.store();s.save_draft(&answer("retained"),0,NOW).unwrap();drop(s);
 assert!(matches!(Store::open(f.dir.path(),App::Student,"actor","device",DataKey::new([3;32]),f.device.clone(),f.trust()),Err(Error::Encryption)));
 assert_eq!(f.open(App::Student).load_draft("assignment").unwrap().unwrap().answer.body,"retained");
}
#[test]fn account_partitions_do_not_fall_back(){
 let f=Fixture::new();let mut s=f.store();s.save_draft(&answer("actor-only"),0,NOW).unwrap();
 let other=Store::open(f.dir.path(),App::Student,"another","device",DataKey::new([2;32]),f.device.clone(),f.trust()).unwrap();
 assert!(other.load_draft("assignment").unwrap().is_none());assert_ne!(s.storage_path(),other.storage_path());
}
#[test]fn teacher_and_parent_cannot_impersonate_student_submission(){
 let f=Fixture::new();for app in [App::Teacher,App::Parent]{let mut s=f.open(app);
 s.install_grant(&f.signed(&f.grant(app,1)),NOW).unwrap();assert!(matches!(s.save_draft(&answer("no"),0,NOW),Err(Error::Authorization)));}
}
#[test]fn attachment_corruption_is_detected_and_submission_blocked(){
 let f=Fixture::new();let mut s=f.store();let hash=s.put_blob(b"private attachment",NOW).unwrap();let file=s.storage_path().join("blobs").join(&hash);
 assert_ne!(std::fs::read(&file).unwrap(),b"private attachment");
 let mut a=answer("draft preserved");a.blobs.push(hash.clone());s.save_draft(&a,0,NOW).unwrap();
 std::fs::write(&file,b"corrupt").unwrap();assert_eq!(s.recover_blobs().unwrap(),vec![hash]);
 assert!(s.submit("assignment",1,Uuid::new_v4(),NOW).is_err());assert_eq!(s.pending(Destination::Online).unwrap(),0);
}
#[test]fn uncommitted_outbox_failure_rolls_back_sequence_and_submission(){
 let f=Fixture::new();let mut s=f.store();s.save_draft(&answer("must survive"),0,NOW).unwrap();
 let db=Connection::open(s.storage_path().join("store.sqlite3")).unwrap();
 db.execute_batch("CREATE TRIGGER fail_delivery BEFORE INSERT ON deliveries BEGIN SELECT RAISE(ABORT,'injected'); END;").unwrap();
 assert!(s.submit("assignment",1,Uuid::new_v4(),NOW).is_err());
 assert_eq!(db.query_row("SELECT count(*) FROM outbox",[],|r|r.get::<_,i64>(0)).unwrap(),0);
 assert_eq!(db.query_row("SELECT seq FROM counters",[],|r|r.get::<_,i64>(0)).unwrap(),0);
 assert_eq!(s.load_draft("assignment").unwrap().unwrap().answer.body,"must survive");
}
#[test]fn interrupted_lease_replays_original_proof(){
 let f=Fixture::new();let mut s=f.store();let e=submit(&mut s);
 let l=s.lease(Destination::Online,NOW).unwrap().unwrap();drop(s);let mut s=f.open(App::Student);
 assert!(s.lease(Destination::Online,NOW+1).unwrap().is_none());
 let retry=s.lease(Destination::Online,NOW+60001).unwrap().unwrap();assert_ne!(l.token,retry.token);
 assert_eq!(serde_json::to_vec(&e).unwrap(),serde_json::to_vec(&retry.envelope).unwrap());
}
#[test]fn renewal_retains_operation_identity_and_allows_pending_replay(){
 let f=Fixture::new();let mut s=f.store();let e=submit(&mut s);
 assert!(s.lease(Destination::Online,WEEK_MS+1001).is_err());
 let mut renewed=f.grant(App::Student,2);renewed.issued_ms=WEEK_MS+1000;renewed.expires_ms=renewed.issued_ms+WEEK_MS;
 s.install_grant(&f.signed(&renewed),WEEK_MS+2000).unwrap();
 let l=s.lease(Destination::Online,WEEK_MS+2000).unwrap().unwrap();
 assert_eq!(e.operation.operation_id,l.operation_id);l.envelope.verify(&renewed).unwrap();
 assert!(s.install_grant(&f.signed(&f.grant(App::Student,1)),WEEK_MS+2000).is_err());
}
struct Offline;impl Transport for Offline {fn send(&mut self,_:Destination,_:&Envelope)->Result<Reply>{Ok(Reply::Retry{retry_after_ms:Some(10000)})}}
#[test]fn retry_after_and_lease_cas_are_durable(){
 let f=Fixture::new();let mut s=f.store();submit(&mut s);
 assert_eq!(tick(&mut s,&mut Offline,Destination::Online,NOW).unwrap(),Tick::Deferred);
 assert_eq!(tick(&mut s,&mut Offline,Destination::Online,NOW+9999).unwrap(),Tick::Idle);
 drop(s);let mut s=f.open(App::Student);let lease=s.lease(Destination::Online,NOW+12000).unwrap().unwrap();
 s.delivery_failed(&lease,"pending",NOW+14000,None).unwrap();
 assert!(matches!(s.delivery_failed(&lease,"pending",0,None),Err(Error::Lease)));
}
#[test]fn remote_conflict_does_not_overwrite_local_draft(){
 let f=Fixture::new();let mut s=f.store();submit(&mut s);let l=s.lease(Destination::Online,NOW).unwrap().unwrap();
 s.delivery_failed(&l,"conflict",0,Some("remote version retained")).unwrap();
 assert_eq!(s.load_draft("assignment").unwrap().unwrap().answer.body,"original frozen answer");
 let db=Connection::open(s.storage_path().join("store.sqlite3")).unwrap();
 assert_eq!(db.query_row("SELECT count(*) FROM conflicts",[],|r|r.get::<_,i64>(0)).unwrap(),1);
 assert_eq!(s.pending(Destination::Online).unwrap(),1);
}
#[test]fn newer_schema_is_preserved_not_downgraded(){
 let f=Fixture::new();let mut s=f.store();s.save_draft(&answer("keep"),0,NOW).unwrap();let home=s.storage_path().to_owned();drop(s);
 let db=Connection::open(home.join("store.sqlite3")).unwrap();db.pragma_update(None,"user_version",2).unwrap();drop(db);
 assert!(matches!(Store::open(f.dir.path(),App::Student,"actor","device",DataKey::new([2;32]),f.device.clone(),f.trust()),Err(Error::Schema)));
 let db=Connection::open(home.join("store.sqlite3")).unwrap();
 assert_eq!(db.query_row("SELECT count(*) FROM drafts",[],|r|r.get::<_,i64>(0)).unwrap(),1);
}

#[test]fn native_context_fails_closed_without_registration(){
 let f=Fixture::new();let context=thinkgrow_offline_core::runtime::NativeContext::new(f.dir.path().into(),App::Student);
 let state=context.status().unwrap();assert!(state.registration_required);assert!(state.local.is_none());
 assert!(matches!(context.save(&answer("not registered"),0),Err(Error::Authorization)));
}
#[test]fn encrypted_revisions_cannot_be_transplanted_across_accounts(){
 let f=Fixture::new();let mut a=f.store();a.save_draft(&answer("account A secret"),0,NOW).unwrap();
 let mut b=Store::open(f.dir.path(),App::Student,"another","device",DataKey::new([2;32]),f.device.clone(),f.trust()).unwrap();
 let mut grant=f.grant(App::Student,1);grant.subject_id="another".into();b.install_grant(&f.signed(&grant),NOW).unwrap();
 b.save_draft(&answer("account B"),0,NOW).unwrap();
 let da=Connection::open(a.storage_path().join("store.sqlite3")).unwrap();
 let cipher:Vec<u8>=da.query_row("SELECT cipher FROM draft_revisions",[],|r|r.get(0)).unwrap();
 let db=Connection::open(b.storage_path().join("store.sqlite3")).unwrap();
 db.execute("UPDATE draft_revisions SET cipher=?1",[cipher]).unwrap();
 assert!(matches!(b.load_draft("assignment"),Err(Error::Encryption)));
}

#[test]fn python_signed_wire_fixture_matches_rust(){
 let fixture:serde_json::Value=serde_json::from_str(include_str!("../../../contracts/fixtures/python-signed-v1.json")).unwrap();
 let issuer:[u8;32]=crypto::decode(fixture["issuer_public_key"].as_str().unwrap()).unwrap().try_into().unwrap();
 let server:[u8;32]=crypto::decode(fixture["server_public_key"].as_str().unwrap()).unwrap().try_into().unwrap();
 let signed:Signed=serde_json::from_value(fixture["credential"].clone()).unwrap();
 let grant:Grant=signed.verify(&[("fixture-issuer".into(),issuer)].into(),"thinkgrow.credential.v1").unwrap();
 let e:Envelope=serde_json::from_value(fixture["envelope"].clone()).unwrap();e.verify(&grant).unwrap();
 let receipt:Signed=serde_json::from_value(fixture["receipt"].clone()).unwrap();
 let r:Receipt=receipt.verify(&[("fixture-online".into(),server)].into(),"thinkgrow.receipt.v1").unwrap();
 assert_eq!(r.payload_hash,e.operation.payload_hash);
}

#[test]fn persisted_signed_grant_cannot_be_transplanted_to_another_account(){
 let f=Fixture::new();let a=f.store();
 let mut b=Store::open(f.dir.path(),App::Student,"another","device",DataKey::new([2;32]),f.device.clone(),f.trust()).unwrap();
 let mut g=f.grant(App::Student,1);g.subject_id="another".into();b.install_grant(&f.signed(&g),NOW).unwrap();
 let da=Connection::open(a.storage_path().join("store.sqlite3")).unwrap();
 let signed:String=da.query_row("SELECT value FROM meta WHERE key='grant'",[],|r|r.get(0)).unwrap();
 let db=Connection::open(b.storage_path().join("store.sqlite3")).unwrap();
 db.execute("UPDATE meta SET value=?1 WHERE key='grant'",[signed]).unwrap();
 assert!(matches!(b.save_draft(&answer("unauthorized"),0,NOW),Err(Error::Authorization)));
}

#[test]fn invalid_wire_revision_never_reaches_the_outbox(){
 let f=Fixture::new();let mut s=f.store();let mut bad=answer("preserve a valid draft");
 bad.base_answer_revision=u64::MAX;assert!(s.save_draft(&bad,0,NOW).is_err());
 assert_eq!(s.pending(Destination::Online).unwrap(),0);assert!(s.load_draft("assignment").unwrap().is_none());
}

#[test]fn startup_reconciles_interrupted_attachment_journal_without_deletion(){
 let f=Fixture::new();let mut s=f.store();let hash=s.put_blob(b"durable blob",NOW).unwrap();let home=s.storage_path().to_owned();
 let db=Connection::open(home.join("store.sqlite3")).unwrap();
 db.execute("UPDATE blobs SET state='pending' WHERE hash=?1",[&hash]).unwrap();
 let absent="a".repeat(64);db.execute("INSERT INTO blobs VALUES(?1,'pending')",[&absent]).unwrap();drop(db);drop(s);
 let s=f.open(App::Student);assert_eq!(s.read_blob(&hash).unwrap(),b"durable blob");
 let db=Connection::open(home.join("store.sqlite3")).unwrap();
 assert_eq!(db.query_row("SELECT state FROM blobs WHERE hash=?1",[&hash],|r|r.get::<_,String>(0)).unwrap(),"ready");
 assert_eq!(db.query_row("SELECT state FROM blobs WHERE hash=?1",[&absent],|r|r.get::<_,String>(0)).unwrap(),"missing");
 assert!(home.join("blobs").join(hash).exists());
}

fn lesson_fixture(f:&Fixture,revision:char)->(lesson::Lesson,Signed){
 let lesson=lesson::Lesson{protocol:1,app_id:App::Student.id().into(),subject_id:"actor".into(),device_id:"device".into(),credential_epoch:1,
 issued_ms:NOW-1000,expires_ms:1000+WEEK_MS,package_revision:revision.to_string().repeat(64),school_id:"school".into(),class_id:"class".into(),assignment_id:"assignment".into(),title:"Private lesson title".into(),prompt:"LESSON-CONTENT-SECRET-MARKER".into(),hint:"".into(),
 content:lesson::Content::Essay{min_length:0,max_length:100000},starts_ms:None,due_ms:None,allow_late:false};
 let signed=sign_lesson(f,&lesson);(lesson,signed)
}
fn sign_lesson(f:&Fixture,lesson:&lesson::Lesson)->Signed{let payload=serde_json::to_string(lesson).unwrap();Signed{key_id:"issuer".into(),signature:crypto::sign(&f.issuer,"thinkgrow.lesson.v1",payload.as_bytes()),payload}}
fn prepared_answer(l:&lesson::Lesson)->Answer{Answer{school_id:l.school_id.clone(),class_id:l.class_id.clone(),assignment_id:l.assignment_id.clone(),package_revision:l.package_revision.clone(),base_answer_revision:0,body:serde_json::json!({"format":1,"body":"PREPARED-ANSWER-SECRET","structured_answer":{}}).to_string(),blobs:vec![]}}
#[test]fn prepared_lesson_and_answer_reopen_encrypted_and_submit_frozen(){
 let f=Fixture::new();let mut s=f.store();let (l,signed)=lesson_fixture(&f,'a');s.install_lessons(&[signed],NOW).unwrap();s.save_prepared_draft(&prepared_answer(&l),0,NOW).unwrap();let e=s.submit_prepared("assignment",1,Uuid::new_v4(),NOW).unwrap();assert!(e.operation.answer.body.contains("PREPARED-ANSWER-SECRET"));let home=s.storage_path().to_owned();drop(s);
 let s=f.open(App::Student);assert_eq!(s.lessons().unwrap()[0].prompt,l.prompt);assert_eq!(s.load_draft("assignment").unwrap().unwrap().revision,1);
 for file in ["lessons.sqlite3","lessons.sqlite3-wal","store.sqlite3","store.sqlite3-wal"] {if let Ok(bytes)=std::fs::read(home.join(file)){for marker in ["LESSON-CONTENT-SECRET-MARKER","PREPARED-ANSWER-SECRET"]{assert!(!bytes.windows(marker.len()).any(|w|w==marker.as_bytes()));}}}
}
#[test]fn prepared_lessons_require_signature_subject_device_and_scope(){
 let f=Fixture::new();let s=f.store();let (l,_)=lesson_fixture(&f,'a');
 for field in ["subject","device","scope","epoch","signature"] {let mut altered=l.clone();match field{"subject"=>altered.subject_id="other".into(),"device"=>altered.device_id="other".into(),"scope"=>altered.class_id="other".into(),"epoch"=>altered.credential_epoch=2,_=>()};let mut signed=sign_lesson(&f,&altered);if field=="signature"{signed.signature=crypto::b64(&[0;64]);}assert!(s.install_lessons(&[signed],NOW).is_err());}
 assert!(s.lessons().unwrap().is_empty());
}
#[test]fn prepared_lesson_batch_is_atomic_on_one_bad_signature(){
 let f=Fixture::new();let s=f.store();let (_,good)=lesson_fixture(&f,'a');let mut bad=good.clone();bad.signature=crypto::b64(&[0;64]);assert!(s.install_lessons(&[good,bad],NOW).is_err());assert!(s.lessons().unwrap().is_empty());
}
#[test]fn new_package_keeps_old_draft_pinned_and_does_not_replace_it(){
 let f=Fixture::new();let mut s=f.store();let (old,signed)=lesson_fixture(&f,'a');s.install_lessons(&[signed],NOW).unwrap();s.save_prepared_draft(&prepared_answer(&old),0,NOW).unwrap();let (mut new,_)=lesson_fixture(&f,'b');new.issued_ms=NOW;new.prompt="New public content".into();s.install_lessons(&[sign_lesson(&f,&new)],NOW).unwrap();
 assert_eq!(s.lessons().unwrap()[0].package_revision,old.package_revision);assert!(matches!(s.save_prepared_draft(&prepared_answer(&new),1,NOW),Err(Error::Conflict)));assert_eq!(s.load_draft("assignment").unwrap().unwrap().revision,1);
}
#[test]fn expired_prepared_lesson_preserves_reading_but_blocks_edit_and_submit(){
 let f=Fixture::new();let mut s=f.store();let (mut l,_)=lesson_fixture(&f,'a');l.expires_ms=NOW+100;s.install_lessons(&[sign_lesson(&f,&l)],NOW).unwrap();s.save_prepared_draft(&prepared_answer(&l),0,NOW).unwrap();assert!(matches!(s.save_prepared_draft(&prepared_answer(&l),1,NOW+101),Err(Error::ReadRecoveryOnly)));assert!(s.submit_prepared("assignment",1,Uuid::new_v4(),NOW+101).is_err());assert_eq!(s.lessons().unwrap().len(),1);
}
#[test]fn prepared_question_unknown_fields_are_rejected_even_if_signed(){
 let f=Fixture::new();let s=f.store();let (_,mut signed)=lesson_fixture(&f,'a');let mut data:serde_json::Value=serde_json::from_str(&signed.payload).unwrap();data["content"]["correct_answer"]=serde_json::json!("PRIVATE");signed.payload=serde_json::to_string(&data).unwrap();signed.signature=crypto::sign(&f.issuer,"thinkgrow.lesson.v1",signed.payload.as_bytes());assert!(s.install_lessons(&[signed],NOW).is_err());
}
#[test]fn prepared_response_rejects_foreign_option_and_missing_package(){
 let f=Fixture::new();let mut s=f.store();let (mut l,_)=lesson_fixture(&f,'a');assert!(s.save_prepared_draft(&prepared_answer(&l),0,NOW).is_err());l.content=lesson::Content::MultipleChoice{options:vec![lesson::Item{id:"allowed".into(),text:"A".into()}]};s.install_lessons(&[sign_lesson(&f,&l)],NOW).unwrap();let mut a=prepared_answer(&l);a.body=serde_json::json!({"format":1,"body":"","structured_answer":{"selected_option_id":"foreign"}}).to_string();assert!(s.save_prepared_draft(&a,0,NOW).is_err());a.body=serde_json::json!({"format":1,"body":"","structured_answer":{"selected_option_id":"allowed"}}).to_string();s.save_prepared_draft(&a,0,NOW).unwrap();
}
#[test]fn prepared_schema_and_cipher_tamper_do_not_silently_replace_data(){
 let f=Fixture::new();let s=f.store();let (_,signed)=lesson_fixture(&f,'a');s.install_lessons(&[signed],NOW).unwrap();let db=Connection::open(s.storage_path().join("lessons.sqlite3")).unwrap();db.execute("UPDATE lessons SET cipher=?1",[vec![1u8;40]]).unwrap();assert!(s.lessons().is_err());db.execute_batch("PRAGMA user_version=99").unwrap();assert!(matches!(s.lessons(),Err(Error::Schema)));assert_eq!(db.query_row("SELECT count(*) FROM lessons",[],|r|r.get::<_,i64>(0)).unwrap(),1);
}
#[test]fn lesson_prepare_request_is_device_signed_and_separate_from_operations(){
 let f=Fixture::new();let s=f.store();let request=s.lesson_request(NOW).unwrap();let payload=request["payload"].as_str().unwrap();crypto::verify(&f.device.verifying_key().to_bytes(),"thinkgrow.lesson-request.v1",payload.as_bytes(),request["proof"].as_str().unwrap()).unwrap();assert!(crypto::verify(&f.device.verifying_key().to_bytes(),"thinkgrow.operation.v1",payload.as_bytes(),request["proof"].as_str().unwrap()).is_err());assert!(payload.contains("assignment"));
}
#[test]fn prepared_question_structures_cover_all_seven_types(){
 let f=Fixture::new();let (mut l,_)=lesson_fixture(&f,'a');let item=lesson::Item{id:"a".into(),text:"A".into()};
 for content in [lesson::Content::Essay{min_length:0,max_length:100000},lesson::Content::TrueFalse{},lesson::Content::MultipleChoice{options:vec![item.clone()]},lesson::Content::Matching{left:vec![item.clone()],right:vec![item.clone()]},lesson::Content::Ordering{items:vec![item.clone()]},lesson::Content::ShortAnswer{max_length:10000},lesson::Content::FillInBlank{blanks:vec!["a".into()]}]{l.content=content;l.validate().unwrap();let mut answer=prepared_answer(&l);answer.body=serde_json::json!({"format":1,"body":"","structured_answer":{}}).to_string();l.validate_answer(&answer).unwrap();}
}

#[test]fn python_prepared_lesson_signature_and_typed_fields_match_rust(){
 let fixture:serde_json::Value=serde_json::from_str(include_str!("../../../contracts/fixtures/prepared-lesson-v1.json")).unwrap();
 assert_eq!(fixture["synthetic_only"],true);let signed:Signed=serde_json::from_value(fixture["signed_lesson"].clone()).unwrap();
 let public:[u8;32]=crypto::decode(fixture["issuer_public_key"].as_str().unwrap()).unwrap().try_into().unwrap();
 let lesson:lesson::Lesson=signed.verify(&[(signed.key_id.clone(),public)].into(),"thinkgrow.lesson.v1").unwrap();lesson.validate().unwrap();assert_eq!(lesson.subject_id,"student-a");assert_eq!(lesson.prompt,"PUBLIC-LESSON-MARKER");assert!(!signed.payload.contains("PRIVATE-ANSWER"));
}
