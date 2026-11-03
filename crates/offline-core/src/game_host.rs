//! Ephemeral loopback origin. No Tauri IPC, cookies, shell or account credentials.
use crate::{Error,Result,runtime::NativeContext,workspace::{GameBundle,GameRun}};
use base64::{engine::general_purpose::STANDARD,Engine};
use serde::Serialize;
use std::{io::{Read,Write},net::{TcpListener,TcpStream},sync::{Arc,Weak,atomic::{AtomicBool,Ordering}},time::Duration};

pub struct GameHost {stop:Arc<AtomicBool>,thread:Option<std::thread::JoinHandle<()>>}
impl Drop for GameHost {fn drop(&mut self){self.stop.store(true,Ordering::Release);if let Some(thread)=self.thread.take(){if thread.thread().id()!=std::thread::current().id(){let _=thread.join();}}}}
#[derive(Serialize)]
pub struct Launch {pub url:String,pub run_id:String}
impl GameHost {
    pub fn start(context:Weak<NativeContext>,app:crate::App,run:GameRun,bundle:GameBundle,session_token:Option<String>,allowed_origins:Vec<String>)->Result<(Self,Launch)> {
        if allowed_origins.len()>10 || !allowed_origins.iter().all(|o|crate::workspace::valid_online_origin(o)){return Err(Error::Authorization)}
        // Fixed, distinct origins let the enclosing WebView CSP reject navigation
        // to every other localhost service. Port conflicts fail closed.
        let port=match app{crate::App::Teacher=>41731,crate::App::Student=>41732,_=>return Err(Error::Authorization)};
        let listener=TcpListener::bind(("127.0.0.1",port))?;listener.set_nonblocking(true)?;
        let host=format!("127.0.0.1:{}",listener.local_addr()?.port());let token=uuid::Uuid::new_v4().simple().to_string();
        let prefix=format!("/{token}/");let url=format!("http://{host}{prefix}{}",bundle.entry);
        let stop=Arc::new(AtomicBool::new(false));let stopped=stop.clone();let run_id=run.id.clone();
        let thread=std::thread::spawn(move|| {
            while !stopped.load(Ordering::Acquire) {
                match listener.accept() {
                    Ok((mut stream,_))=>{let _=stream.set_read_timeout(Some(Duration::from_secs(3)));let _=stream.set_write_timeout(Some(Duration::from_secs(3)));
                        let result=serve(&mut stream,&host,&prefix,&run_id,&bundle,&context,&stopped,session_token.as_deref(),&allowed_origins);
                        if result.is_err(){let _=response(&mut stream,403,"text/plain",b"Game session unavailable","");}
                    },
                    Err(e) if e.kind()==std::io::ErrorKind::WouldBlock=>std::thread::sleep(Duration::from_millis(25)),
                    Err(_)=>break,
                }
            }
        });Ok((Self{stop,thread:Some(thread)},Launch{url,run_id:run.id}))
    }
}
fn serve(stream:&mut TcpStream,host:&str,prefix:&str,run:&str,bundle:&GameBundle,context:&Weak<NativeContext>,stop:&AtomicBool,session_token:Option<&str>,allowed_origins:&[String])->Result<()> {
    let mut header=Vec::new();let mut byte=[0];
    while !header.ends_with(b"\r\n\r\n"){if header.len()>=8192 || stream.read(&mut byte)?!=1{return Err(Error::Authorization)}header.push(byte[0]);}
    let raw=std::str::from_utf8(&header).map_err(|_|Error::Authorization)?;let mut lines=raw.split("\r\n");
    let parts:Vec<_>=lines.next().unwrap_or("").split_whitespace().collect();
    if parts.len()!=3 || parts[2]!="HTTP/1.1"{return Err(Error::Authorization)}
    let method=parts[0];let target=parts[1];let mut headers=std::collections::BTreeMap::new();
    for line in lines.filter(|line|!line.is_empty()){let (k,v)=line.split_once(':').ok_or(Error::Authorization)?;if headers.insert(k.to_ascii_lowercase(),v.trim()).is_some(){return Err(Error::Authorization)}}
    if headers.get("host")!=Some(&host) || headers.contains_key("transfer-encoding") || stop.load(Ordering::Acquire){return Err(Error::Authorization)}
    if bundle.network_mode.as_deref()==Some("online_session"){
        let signed:crate::Signed=serde_json::from_str(session_token.ok_or(Error::Authorization)?)?;
        let ticket:crate::workspace::SessionTicket=serde_json::from_str(&signed.payload)?;
        let context=context.upgrade().ok_or(Error::Authorization)?;
        if ticket.expires_ms<=context.checked_now(){return Err(Error::Authorization)}
    }
    let path=target.strip_prefix(prefix).ok_or(Error::Authorization)?;
    let context=context.upgrade().ok_or(Error::Authorization)?;let current=context.game_run(run)?;
    let connect_policy=if bundle.network_mode.as_deref()==Some("online_session"){
        if allowed_origins.is_empty(){
            "connect-src 'self';".to_string()
        }else{
            let joined=allowed_origins.join(" ");
            format!("connect-src 'self' {joined};")
        }
    }else{"connect-src 'self';".to_string()};
    if method=="GET" && path=="__host/context"{return response(stream,200,"application/json",&serde_json::to_vec(&serde_json::json!({"run_id":run,"game":current.game,"assignment_id":current.assignment_id,"checkpoint":current.checkpoint,"last_sequence":current.events.len(),"session_token":session_token,"allowed_origins":allowed_origins}))?,&connect_policy)}
    if method=="GET" && path=="__host/sdk.js"{
        let script=SDK.replace("__PREFIX__",prefix);return response(stream,200,"application/javascript",script.as_bytes(),&connect_policy)
    }
    if method=="POST" && path=="__host/events" {
        let origin=format!("http://{host}");
        let len=headers.get("content-length").and_then(|s|s.parse::<usize>().ok()).filter(|n|*n>0 && *n<=16384).ok_or(Error::Authorization)?;
        let mut data=vec![0;len];stream.read_exact(&mut data)?;
        if headers.get("origin")!=Some(&origin.as_str()) || headers.get("content-type")!=Some(&"application/json"){return Err(Error::Authorization)}
        let event=serde_json::from_slice(&data)?;let result=context.record_game_event(run,event)?;
        return response(stream,200,"application/json",&serde_json::to_vec(&result)?,&connect_policy)
    }
    if method!="GET" || !crate::workspace::safe_game_path(path){return Err(Error::Authorization)}
    let file=bundle.files.get(path).ok_or(Error::Authorization)?;let bytes=STANDARD.decode(&file.data).map_err(|_|Error::Signature)?;
    let extension=path.rsplit('.').next().unwrap_or("");
    let mime=match extension{"html"=>"text/html; charset=utf-8","js"|"mjs"=>"application/javascript","css"=>"text/css","json"=>"application/json","svg"=>"image/svg+xml","png"=>"image/png","jpg"|"jpeg"=>"image/jpeg","webp"=>"image/webp","gif"=>"image/gif","wav"=>"audio/wav","mp3"=>"audio/mpeg","mp4"=>"video/mp4","wasm"=>"application/wasm",_=>"application/octet-stream"};
    if extension=="html" {
        let mut html=format!("<!doctype html><meta charset=\"utf-8\"><script src=\"{prefix}__host/sdk.js\"></script>").into_bytes();html.extend(bytes);
        response(stream,200,mime,&html,&connect_policy)
    }else{response(stream,200,mime,&bytes,&connect_policy)}
}
fn response(stream:&mut TcpStream,status:u16,mime:&str,body:&[u8],connect_policy:&str)->Result<()> {
    let frames=if connect_policy.contains("https://"){connect_policy.strip_prefix("connect-src 'self' ").unwrap_or("").trim_end_matches(';')}else{"'none'"};
    let policy=format!("default-src 'none'; script-src 'self' 'unsafe-inline'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; media-src 'self' blob:; font-src 'self'; {connect_policy} object-src 'none'; base-uri 'none'; form-action 'none'; frame-src {frames}; sandbox allow-scripts allow-same-origin allow-forms");
    write!(stream,"HTTP/1.1 {status} Response\r\nContent-Type: {mime}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nReferrer-Policy: no-referrer\r\nContent-Security-Policy: {policy}\r\nPermissions-Policy: camera=(), microphone=(), geolocation=(), payment=()\r\nConnection: close\r\n\r\n",body.len())?;stream.write_all(body)?;Ok(())
}
const SDK:&str=r#"(()=>{'use strict';const root='__PREFIX__';let seq=0,queue=Promise.resolve();const ready=fetch(root+'__host/context').then(r=>{if(!r.ok)throw Error('Session closed');return r.json()}).then(c=>{seq=c.last_sequence;return c});function emitEvent(kind,data){const op=queue.then(async()=>{await ready;const body=JSON.stringify({sequence:seq+1,kind,data});let error;for(let n=0;n<2;n++){try{const r=await fetch(root+'__host/events',{method:'POST',headers:{'Content-Type':'application/json'},body});if(!r.ok)throw Error('Event not saved');const result=await r.json();seq=result.accepted;return result}catch(e){error=e}}throw error});queue=op.catch(()=>{});return op}window.ThinkGrowOffline=Object.freeze({ready,emitEvent,saveCheckpoint:data=>emitEvent('checkpoint',data)});})();"#;
