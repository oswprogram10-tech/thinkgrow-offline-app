use thinkgrow_offline_core::{App,crypto,update::Ledger};
use std::{env,fs,path::Path};
fn run()->Result<(),Box<dyn std::error::Error>>{
 let a:Vec<String>=env::args().skip(1).collect();
 if a.len()!=12 && a.len()!=13{return Err("Usage: release-check catalog sig pub ledger-root role platform channel now component current-version core-version artifact-file".into())}
 let raw=fs::read(&a[0])?;
 let sig=String::from_utf8(crypto::decode(fs::read_to_string(&a[1])?.trim())?)?;
 let key=String::from_utf8(crypto::decode(fs::read_to_string(&a[2])?.trim())?)?;
 let app=match a[4].as_str(){"teacher"=>App::Teacher,"student"=>App::Student,"parent"=>App::Parent,_=>return Err("role".into())};
 let now=a[7].parse::<i64>()?;
 let mut ledger=Ledger::open(Path::new(&a[3]),app,&a[5],&a[6])?;
 let catalog=ledger.accept_catalog(&raw,&sig,&key,now)?;
 let staged=ledger.stage(&catalog,&a[8],&a[9],&a[10],Path::new(&a[11]),now)?;
 let policy=a.get(12).map(String::as_str).unwrap_or("confirmed");
 if !["confirmed","in-class","no-consent"].contains(&policy){return Err("installation policy".into())}
 ledger.installation_gate(&staged,policy=="in-class",policy!="no-consent",now)?;
 println!("{}",serde_json::to_string(&staged)?);Ok(())
}
fn main(){if let Err(e)=run(){eprintln!("{e}");std::process::exit(1)}}
