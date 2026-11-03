//! Student projection: no answer keys, grading material or arbitrary metadata.
use crate::{contract::*, Error, Result};
use serde::{Serialize,Deserialize};
use std::collections::BTreeSet;
#[derive(Clone,Debug,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Item {pub id:String,pub text:String}
#[derive(Clone,Debug,Serialize,Deserialize)]
#[serde(tag="kind",rename_all="snake_case",deny_unknown_fields)]
pub enum Content {
 Essay{min_length:u32,max_length:u32}, TrueFalse{}, MultipleChoice{options:Vec<Item>},
 Matching{left:Vec<Item>,right:Vec<Item>}, Ordering{items:Vec<Item>},
 ShortAnswer{max_length:u32}, FillInBlank{blanks:Vec<String>},
}
#[derive(Clone,Debug,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Lesson {
 pub protocol:u32,pub app_id:String,pub subject_id:String,pub device_id:String,pub credential_epoch:u64,
 pub issued_ms:i64,pub expires_ms:i64,pub package_revision:String,
 pub school_id:String,pub class_id:String,pub assignment_id:String,pub title:String,pub prompt:String,pub hint:String,
 pub content:Content,pub starts_ms:Option<i64>,pub due_ms:Option<i64>,pub allow_late:bool,
}
#[derive(Clone,Debug,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Response {pub format:u32,pub body:String,pub structured_answer:serde_json::Value}
impl Lesson {
 pub fn validate(&self)->Result<()> {
  if self.protocol!=PROTOCOL || self.app_id!=App::Student.id() || self.credential_epoch==0 || self.issued_ms<0 || self.expires_ms<=self.issued_ms || self.expires_ms-self.issued_ms>WEEK_MS || !digest(&self.package_revision) {return Err(Error::Invalid("lesson binding"))}
  for v in [&self.subject_id,&self.device_id,&self.school_id,&self.class_id,&self.assignment_id] {id(v)?;}
  if self.title.len()>512 || self.prompt.len()>32768 || self.hint.len()>8192 {return Err(Error::Invalid("lesson text"))}
  match &self.content {
   Content::Essay{min_length,max_length} if min_length<=max_length && *max_length<=100000=>(),
   Content::TrueFalse{}=>(), Content::MultipleChoice{options}=>items(options)?,
   Content::Matching{left,right}=>{items(left)?;items(right)?;},Content::Ordering{items:list}=>items(list)?,
   Content::ShortAnswer{max_length} if *max_length<=10000=>(),
   Content::FillInBlank{blanks}=>{if blanks.is_empty() || blanks.len()>64 || blanks.iter().collect::<BTreeSet<_>>().len()!=blanks.len(){return Err(Error::Invalid("lesson blanks"))}for s in blanks{id(s)?;}},
   _=>return Err(Error::Invalid("lesson content")),
  }
  Ok(())
 }
 pub fn validate_answer(&self,answer:&Answer)->Result<()> {
  if answer.school_id!=self.school_id || answer.class_id!=self.class_id || answer.assignment_id!=self.assignment_id || answer.package_revision!=self.package_revision || !answer.blobs.is_empty() {return Err(Error::Authorization)}
  let response:Response=serde_json::from_str(&answer.body)?;
  if response.format!=1 || !response.structured_answer.is_object() {return Err(Error::Invalid("lesson response"))}
  let a=response.structured_answer.as_object().unwrap();
  let valid=match &self.content {
   Content::Essay{max_length,..}=>a.is_empty() && response.body.chars().count()<=*max_length as usize,
   Content::TrueFalse{}=>a.len()<=1 && a.get("selected").is_none_or(|v|v.is_boolean() || v.is_null()) && a.keys().all(|k|k=="selected"),
   Content::MultipleChoice{options}=>a.len()<=1 && a.keys().all(|k|k=="selected_option_id") && a.get("selected_option_id").is_none_or(|v|v.is_null() || v.as_str().is_some_and(|s|options.iter().any(|i|i.id==s))),
   Content::ShortAnswer{max_length}=>a.len()<=1 && a.keys().all(|k|k=="text") && a.get("text").is_none_or(|v|v.as_str().is_some_and(|s|s.chars().count()<=*max_length as usize)),
   Content::Ordering{items}=>a.len()<=1 && a.keys().all(|k|k=="ordered_ids") && a.get("ordered_ids").is_none_or(|v|v.as_array().is_some_and(|ids|ids.len()<=items.len() && ids.iter().all(|v|v.as_str().is_some_and(|s|items.iter().any(|i|i.id==s))) && ids.iter().filter_map(|v|v.as_str()).collect::<BTreeSet<_>>().len()==ids.len())),
   Content::Matching{left,right}=>a.len()<=1 && a.keys().all(|k|k=="pairs") && a.get("pairs").is_none_or(|v|v.as_array().is_some_and(|pairs|pairs.len()<=left.len() && pairs.iter().all(|p|p.as_object().is_some_and(|o|o.len()==2 && o.get("left_id").and_then(|v|v.as_str()).is_some_and(|s|left.iter().any(|i|i.id==s)) && o.get("right_id").and_then(|v|v.as_str()).is_some_and(|s|right.iter().any(|i|i.id==s)))) && pairs.iter().filter_map(|p|p.get("left_id").and_then(|v|v.as_str())).collect::<BTreeSet<_>>().len()==pairs.len() && pairs.iter().filter_map(|p|p.get("right_id").and_then(|v|v.as_str())).collect::<BTreeSet<_>>().len()==pairs.len())),
   Content::FillInBlank{blanks}=>a.len()<=1 && a.keys().all(|k|k=="blanks") && a.get("blanks").is_none_or(|v|v.as_object().is_some_and(|values|values.iter().all(|(key,v)|blanks.contains(key)&&v.as_str().is_some_and(|s|s.len()<=10000)))),
  };
  if valid {Ok(())}else{Err(Error::Invalid("lesson response shape"))}
 }
}
fn items(items:&[Item])->Result<()> {
 if items.is_empty() || items.len()>64 || items.iter().map(|i|&i.id).collect::<BTreeSet<_>>().len()!=items.len(){return Err(Error::Invalid("lesson items"))}
 for item in items{id(&item.id)?;if item.text.len()>8192{return Err(Error::Invalid("lesson item text"))}}
 Ok(())
}
