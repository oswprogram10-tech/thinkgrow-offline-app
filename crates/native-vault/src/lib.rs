//! Internal native bridge; no WebView commands or filesystem capability.
use std::sync::Arc;
#[cfg(not(any(target_os="android",target_os="ios")))]
use thinkgrow_offline_core::vault::SystemProtector;
use tauri::{plugin::{Builder,TauriPlugin},Runtime};
use thinkgrow_offline_core::{vault::OsProtector,registration::EnrollmentClient};
#[cfg(any(target_os="android",target_os="ios"))]
use tauri::Manager;
#[cfg(target_os="ios")]
tauri::ios_plugin_binding!(init_plugin_thinkgrow_vault);

pub fn init<R:Runtime>()->TauriPlugin<R> {
    Builder::new("thinkgrow-vault").setup(|_app,_api| {
        #[cfg(target_os="android")]
        let handle=_api.register_android_plugin("com.thinkgrow.offline.vault","VaultPlugin")?;
        #[cfg(target_os="ios")]
        let handle=_api.register_ios_plugin(init_plugin_thinkgrow_vault)?;
        #[cfg(any(target_os="android",target_os="ios"))]
        _app.manage(Mobile(Arc::new(Bridge(handle))));
        Ok(())
    }).build()
}
pub fn adapters<R:Runtime>(_app:&tauri::AppHandle<R>)->(Arc<dyn OsProtector>,Option<Arc<dyn EnrollmentClient>>) {
    #[cfg(any(target_os="android",target_os="ios"))] {
        let bridge=_app.state::<Mobile<R>>().0.clone(); (bridge.clone(),Some(bridge))
    }
    #[cfg(not(any(target_os="android",target_os="ios")))] { (Arc::new(SystemProtector),None) }
}
#[cfg(any(target_os="android",target_os="ios"))]
struct Mobile<R:Runtime>(Arc<Bridge<R>>);
#[cfg(any(target_os="android",target_os="ios"))]
struct Bridge<R:Runtime>(tauri::plugin::PluginHandle<R>);
#[cfg(any(target_os="android",target_os="ios"))]
#[derive(serde::Serialize)]
struct Request {data:String,entropy:String}
#[cfg(any(target_os="android",target_os="ios"))]
#[derive(serde::Deserialize)]
struct Response {data:String}
#[cfg(any(target_os="android",target_os="ios"))]
impl<R:Runtime> Bridge<R> {
    fn call(&self,method:&str,bytes:&[u8],entropy:&[u8])->thinkgrow_offline_core::Result<Vec<u8>> {
        use base64::Engine;
        if bytes.len()>1024*1024+8192 {return Err(thinkgrow_offline_core::Error::VaultUnavailable)}
        let payload=Request{data:base64::engine::general_purpose::STANDARD.encode(bytes),entropy:base64::engine::general_purpose::STANDARD.encode(entropy)};
        let response:Response=self.0.run_mobile_plugin(method,payload).map_err(|_|thinkgrow_offline_core::Error::VaultUnavailable)?;
        if response.data.len()>3*1024*1024 {return Err(thinkgrow_offline_core::Error::VaultUnavailable)}
        base64::engine::general_purpose::STANDARD.decode(response.data).map_err(|_|thinkgrow_offline_core::Error::VaultUnavailable)
    }
}
#[cfg(any(target_os="android",target_os="ios"))]
impl<R:Runtime> OsProtector for Bridge<R> {
    fn available(&self)->bool {true}
    fn protect(&self,bytes:&[u8],entropy:&[u8])->thinkgrow_offline_core::Result<Vec<u8>> {self.call("seal",bytes,entropy)}
    fn unprotect(&self,bytes:&[u8],entropy:&[u8])->thinkgrow_offline_core::Result<Vec<u8>> {self.call("unseal",bytes,entropy)}
}
#[cfg(any(target_os="android",target_os="ios"))]
impl<R:Runtime> EnrollmentClient for Bridge<R> {
    fn role_exchange(&self,request_json:&[u8])->thinkgrow_offline_core::Result<thinkgrow_offline_core::Signed> {
        #[derive(serde::Deserialize)] #[serde(deny_unknown_fields)] struct Reply{reply:thinkgrow_offline_core::Signed}
        let bytes=self.call("roleExchange",request_json,b"").map_err(|_|thinkgrow_offline_core::Error::RegistrationFailed)?;
        if bytes==b"{\"conflict\":true}"{return Err(thinkgrow_offline_core::Error::Conflict)}
        Ok(serde_json::from_slice::<Reply>(&bytes)?.reply)
    }
    fn redeem(&self,request_json:&[u8])->thinkgrow_offline_core::Result<thinkgrow_offline_core::Signed> {
        #[derive(serde::Deserialize)] struct Reply {credential:thinkgrow_offline_core::Signed}
        let bytes=self.call("redeem",request_json,b"").map_err(|_|thinkgrow_offline_core::Error::RegistrationFailed)?;
        let reply:Reply=serde_json::from_slice(&bytes).map_err(|_|thinkgrow_offline_core::Error::RegistrationFailed)?;
        Ok(reply.credential)
    }
    fn lessons(&self,request_json:&[u8])->thinkgrow_offline_core::Result<Vec<thinkgrow_offline_core::Signed>> {
        #[derive(serde::Deserialize)] #[serde(deny_unknown_fields)] struct Reply {lessons:Vec<thinkgrow_offline_core::Signed>}
        let bytes=self.call("lessons",request_json,b"").map_err(|_|thinkgrow_offline_core::Error::RegistrationFailed)?;
        Ok(serde_json::from_slice::<Reply>(&bytes).map_err(|_|thinkgrow_offline_core::Error::RegistrationFailed)?.lessons)
    }
}
