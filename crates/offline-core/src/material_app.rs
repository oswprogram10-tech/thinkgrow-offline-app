//! Read packages in memory; never extract uploaded files into the host filesystem.
use crate::{*, workspace::{GameBundle,GameFile,GameRun}};
use base64::{engine::general_purpose::STANDARD,Engine};
use std::{collections::BTreeMap,io::{Cursor,Read}};

pub fn bundle(info:&MaterialInfo, bytes:&[u8])->Result<GameBundle>{
    if bytes.len()>64*1024*1024 || crypto::hash(bytes)!=info.sha256{return Err(Error::Signature)}
    let mut archive=zip::ZipArchive::new(Cursor::new(bytes)).map_err(|_|Error::Invalid("app ZIP"))?;
    if archive.len()>512{return Err(Error::Invalid("app file count"))}
    let mut files=BTreeMap::new();let mut total=0usize;
    for i in 0..archive.len(){
        let mut file=archive.by_index(i).map_err(|_|Error::Invalid("app ZIP entry"))?;
        if file.is_dir(){continue}
        let name=file.name().to_string();
        if !workspace::safe_game_path(&name) || files.contains_key(&name) || file.size()>16*1024*1024
            || file.unix_mode().is_some_and(|m|m&0o170000==0o120000){return Err(Error::Invalid("app path or size"))}
        let mut data=Vec::new();(&mut file).take(16*1024*1024+1).read_to_end(&mut data)?;
        total+=data.len();if data.len()>16*1024*1024 || total>64*1024*1024{return Err(Error::Invalid("app expanded size"))}
        let ext=name.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
        if !["html","htm","js","mjs","css","json","svg","png","jpg","jpeg","webp","gif","ico","wav","mp3","mp4","ogg","webm","woff","woff2","ttf","wasm","txt","map"].contains(&ext.as_str())
            || data.starts_with(b"MZ") || data.starts_with(b"\x7fELF"){return Err(Error::Invalid("app executable"))}
        files.insert(name,GameFile{sha256:crypto::hash(&data),data:STANDARD.encode(data)});
    }
    let manifest=files.get("thinkgrow.app.json").ok_or(Error::Invalid("app manifest"))?;
    let manifest:serde_json::Value=serde_json::from_slice(&STANDARD.decode(&manifest.data).map_err(|_|Error::Signature)?)?;
    let entry=manifest["entry"].as_str().ok_or(Error::Invalid("app entry"))?.to_owned();
    if manifest["host_api"]!=1 || !entry.ends_with(".html") || !files.contains_key(&entry){return Err(Error::Invalid("app contract"))}
    let network=manifest["network"].as_str().unwrap_or("offline");
    if !["offline","isolated","online"].contains(&network){return Err(Error::Invalid("app network"))}
    if let Some(hashes)=manifest.get("files"){
        let expected=hashes.as_object().ok_or(Error::Invalid("app hashes"))?;
        if expected.len()+1!=files.len() || files.iter().any(|(n,f)|n!="thinkgrow.app.json" && expected.get(n).and_then(|h|h.as_str())!=Some(f.sha256.as_str())){return Err(Error::Signature)}
    }
    Ok(GameBundle{id:info.id.clone(),title:info.title.clone(),version:info.version.clone(),runtime:"static".into(),host_api:1,entry,source_sha256:info.sha256.clone(),files,network_mode:Some(if network=="online"{"online_session"}else{"offline"}.into())})
}

impl Store{
    pub fn start_material_app(&self,id:&str,token:Option<&str>,now:i64)->Result<(GameRun,GameBundle,Vec<String>)>{
        if self.access(now)?!=Access::ReadWrite{return Err(Error::Authorization)}
        let (info,bytes)=self.decrypt_material(id)?;
        if info.subtype!="app" || !matches!(info.app_kind.as_deref(),Some("web_app"|"web")){return Err(Error::Authorization)}
        let bundle=bundle(&info,&bytes)?;
        let origins=self.material_session_origins(&info,&bundle,token,now)?;
        let (run,bundle)=self.start_material_run(info,bundle,now)?;Ok((run,bundle,origins))
    }
}

#[cfg(test)]
mod tests{
    use super::*;
    use std::io::Write;
    fn fixture(entry:&str,network:&str)->(MaterialInfo,Vec<u8>){
        let mut out=Cursor::new(Vec::new());
        {let mut zip=zip::ZipWriter::new(&mut out);let opts=zip::write::SimpleFileOptions::default();
        zip.start_file("thinkgrow.app.json",opts).unwrap();zip.write_all(serde_json::json!({"name":"App","version":"1.0.0","host_api":1,"entry":entry,"network":network,"allowed_origins":[]}).to_string().as_bytes()).unwrap();
        zip.start_file(entry,opts).unwrap();zip.write_all(b"<h1>Lesson</h1>").unwrap();zip.finish().unwrap();}
        let bytes=out.into_inner();
        let info=MaterialInfo{id:"mat-app".into(),version:"1.0.0".into(),subtype:"app".into(),app_kind:Some("web_app".into()),title:"Lesson".into(),filename:"app.zip".into(),size:bytes.len() as u64,sha256:crypto::hash(&bytes),assignment_id:None,teacher_id:None,sync_at:0,offline_available:true};
        (info,bytes)
    }
    #[test]fn package_is_hash_bound_and_never_extracted(){
        let (info,mut bytes)=fixture("index.html","offline");
        let app=bundle(&info,&bytes).unwrap();assert_eq!(app.entry,"index.html");assert_eq!(app.files.len(),2);
        bytes[0]^=1;assert!(bundle(&info,&bytes).is_err());
    }
    #[test]fn unsafe_paths_and_executable_entries_are_rejected(){
        for path in ["../index.html","/index.html","nested/../index.html","start.exe"]{
            let (info,bytes)=fixture(path,"offline");assert!(bundle(&info,&bytes).is_err());
        }
    }
    #[test]fn network_required_package_cannot_be_mislabeled_offline(){
        let (info,bytes)=fixture("index.html","online");assert_eq!(bundle(&info,&bytes).unwrap().network_mode.as_deref(),Some("online_session"));
        let (info,bytes)=fixture("index.html","unknown");assert!(bundle(&info,&bytes).is_err());
    }
    #[test]fn csp_sources_require_exact_secure_origins(){
        assert!(workspace::valid_online_origin("https://example.com"));assert!(workspace::valid_online_origin("wss://example.com"));
        for origin in ["https://example.com; script-src *","https://example.com/a","https://u:p@example.com","https://example.com\r\nX:y","http://example.com"]{assert!(!workspace::valid_online_origin(origin));}
    }
}
