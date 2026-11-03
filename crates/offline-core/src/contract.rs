use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use crate::{crypto, Error, Result};
pub const PROTOCOL: u32=1;
pub const SCHEMA: u32=1;
pub const MAX_TEXT: usize=2*1024*1024;
pub const MAX_BLOB: usize=15*1024*1024;
pub const WEEK_MS: i64=7*24*60*60*1000;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all="snake_case")]
pub enum App { Teacher, Student, Parent }
impl App {
    pub fn id(self)-> &'static str { match self {
        Self::Teacher=>"com.thinkgrow.offline.teacher", Self::Student=>"com.thinkgrow.offline.student",
        Self::Parent=>"com.thinkgrow.offline.parent" } }
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all="snake_case")]
pub enum Destination { Classroom, Online }
impl Destination {
    pub fn text(self)-> &'static str { match self { Self::Classroom=>"classroom", Self::Online=>"online" } }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Signed { pub key_id:String, pub payload:String, pub signature:String }
impl Signed {
    pub fn verify<T:serde::de::DeserializeOwned>(&self, keys:&BTreeMap<String,[u8;32]>, domain:&str)->Result<T> {
        if self.payload.len()>MAX_TEXT { return Err(Error::Invalid("signed payload too large")); }
        let key=keys.get(&self.key_id).ok_or(Error::Signature)?;
        crypto::verify(key,domain,self.payload.as_bytes(),&self.signature)?;
        Ok(serde_json::from_str(&self.payload)?)
    }
}
#[derive(Clone, Default)]
pub struct Trust {
    pub credentials:BTreeMap<String,[u8;32]>,
    pub classroom_receipts:BTreeMap<String,[u8;32]>,
    pub online_receipts:BTreeMap<String,[u8;32]>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Scope { pub school_id:String, pub class_id:String, pub assignment_id:String }
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Grant {
    pub protocol:u32, pub app_id:String, pub subject_id:String, pub device_id:String,
    pub device_public_key:String, pub epoch:u64, pub issued_ms:i64, pub expires_ms:i64,
    pub scopes:Vec<Scope>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Answer {
    pub school_id:String, pub class_id:String, pub assignment_id:String,
    pub package_revision:String, pub base_answer_revision:u64, pub body:String,
    pub blobs:Vec<String>,
}
impl Answer {
    pub fn validate(&self)->Result<()> {
        for value in [&self.school_id,&self.class_id,&self.assignment_id,&self.package_revision] { id(value)?; }
        if self.base_answer_revision>i64::MAX as u64 || self.body.len()>MAX_TEXT || self.blobs.len()>64 || self.blobs.iter().any(|h| !digest(h)) {
            return Err(Error::Invalid("answer size or blob hash")); }
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Operation {
    pub protocol:u32, pub operation_id:String, pub origin_device_id:String, pub device_seq:u64,
    pub subject_id:String, pub app_id:String, pub credential_epoch:u64,
    pub kind:String, pub answer:Answer, pub draft_revision:u64, pub payload_hash:String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Envelope { pub operation:Operation, pub proof:String }
impl Envelope {
    pub fn verify(&self, grant:&Grant)->Result<()> {
        let o=&self.operation;
        if grant.app_id!=App::Student.id() || o.device_seq==0 || o.device_seq>i64::MAX as u64 ||
            o.draft_revision==0 || o.draft_revision>i64::MAX as u64 || o.protocol!=PROTOCOL || o.origin_device_id!=grant.device_id || o.subject_id!=grant.subject_id ||
            o.app_id!=grant.app_id || o.credential_epoch>grant.epoch || o.kind!="submission" ||
            o.payload_hash!=crypto::hash(&serde_json::to_vec(&o.answer)?) {
            return Err(Error::Invalid("operation binding")); }
        uuid::Uuid::parse_str(&o.operation_id).map_err(|_|Error::Invalid("operation id"))?;
        o.answer.validate()?;
        if !grant.scopes.iter().any(|s| s.school_id==o.answer.school_id && s.class_id==o.answer.class_id &&
            s.assignment_id==o.answer.assignment_id) { return Err(Error::Authorization); }
        let bytes:[u8;32]=crypto::decode(&grant.device_public_key)?.try_into().map_err(|_|Error::Signature)?;
        crypto::verify(&bytes,"thinkgrow.operation.v1",&serde_json::to_vec(o)?,&self.proof)
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Receipt {
    pub protocol:u32, pub operation_id:String, pub origin_device_id:String, pub subject_id:String,
    pub payload_hash:String, pub destination:Destination, pub receipt_id:String,
    pub server_received_ms:i64,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all="snake_case")]
pub enum Access { ProvisioningRequired, ReadRecoveryOnly, ReadWrite }
#[derive(Debug, Serialize)]
pub struct Status { pub protocol:u32, pub schema:u32, pub access:Access, pub drafts:u64,
    pub pending_classroom:u64, pub pending_online:u64, pub installed_games:u64 }
#[derive(Debug, Serialize, Deserialize)]
pub struct Draft { pub revision:u64, pub answer:Answer }
#[derive(Debug)]
pub struct Lease { pub operation_id:String, pub token:String, pub attempts:u32, pub destination:Destination, pub envelope:Envelope }
pub fn id(s:&str)->Result<()> {
    if s.is_empty() || s.len()>160 || s.chars().any(|c| c.is_control()) {
        Err(Error::Invalid("identifier")) } else { Ok(()) }
}
pub fn digest(s:&str)->bool { s.len()==64 && s.bytes().all(|b|b.is_ascii_digit() || (b'a'..=b'f').contains(&b)) }
