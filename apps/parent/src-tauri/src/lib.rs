use tauri::Manager;
use tauri_plugin_opener::OpenerExt;
use std::sync::Arc;
use thinkgrow_offline_core::registration::{Registration,AccountStatus};
use thinkgrow_offline_core::{App,runtime::{NativeContext,CoreStatus}};

#[tauri::command]
fn load_role_workspace(state:tauri::State<'_,Arc<NativeContext>>)->Result<Option<thinkgrow_offline_core::workspace::RoleReply>,String>{state.role_workspace().map_err(|e|e.to_string())}
#[tauri::command]
async fn role_exchange(state:tauri::State<'_,Arc<Registration>>,action:String,data:serde_json::Value)->Result<thinkgrow_offline_core::workspace::RoleReply,String>{
 let registration=state.inner().clone();tauri::async_runtime::spawn_blocking(move||registration.role_exchange(&action,data).map_err(|e|e.to_string())).await.map_err(|_|"role operation interrupted; local data preserved".to_string())?
}

#[tauri::command]
fn list_role_drafts(state:tauri::State<'_,Arc<NativeContext>>)->Result<Vec<thinkgrow_offline_core::workspace::RoleDraft>,String>{state.role_drafts().map_err(|e|e.to_string())}
#[tauri::command]
fn save_role_draft(state:tauri::State<'_,Arc<NativeContext>>,kind:String,data:serde_json::Value,expected:u64)->Result<thinkgrow_offline_core::workspace::RoleDraft,String>{state.save_role_draft(&kind,data,expected).map_err(|e|e.to_string())}
#[tauri::command]
async fn sync_role_draft(state:tauri::State<'_,Arc<Registration>>,slot:String,revision:u64)->Result<thinkgrow_offline_core::workspace::RoleDraft,String>{
 let registration=state.inner().clone();tauri::async_runtime::spawn_blocking(move||registration.sync_role_draft(&slot,revision).map_err(|e|e.to_string())).await.map_err(|_|"send interrupted; local draft preserved".to_string())?
}
#[tauri::command]
fn core_status(state:tauri::State<'_,Arc<NativeContext>>)->Result<CoreStatus,String>{state.status().map_err(|e|e.to_string())}
#[tauri::command]
fn open_registration_page(app:tauri::AppHandle)->Result<(),String>{
 app.opener().open_url("https://schoolclassscorecheck.web.app/offline-register.html?role=parent",None::<&str>)
  .map_err(|_|"could not open registration page".to_string())
}
#[tauri::command]
async fn account_status(state:tauri::State<'_,Arc<Registration>>)->Result<AccountStatus,String>{
 let registration=state.inner().clone();
 tauri::async_runtime::spawn_blocking(move||registration.status().map_err(|e|e.to_string()))
  .await.map_err(|_|"account operation interrupted".to_string())?
}
#[tauri::command]
async fn register_account(state:tauri::State<'_,Arc<Registration>>,code:String,pin:String,label:String,pending_handle:Option<String>)->Result<CoreStatus,String>{
 let registration=state.inner().clone();
 tauri::async_runtime::spawn_blocking(move||registration.register(code,pin,label,pending_handle).map_err(|e|e.to_string()))
  .await.map_err(|_|"account operation interrupted; data preserved".to_string())?
}
#[tauri::command]
async fn unlock_account(state:tauri::State<'_,Arc<Registration>>,handle:String,pin:String)->Result<CoreStatus,String>{
 let registration=state.inner().clone();
 tauri::async_runtime::spawn_blocking(move||registration.unlock(&handle,pin).map_err(|e|e.to_string()))
  .await.map_err(|_|"account operation interrupted; data preserved".to_string())?
}
#[tauri::command]
async fn change_account_pin(state:tauri::State<'_,Arc<Registration>>,handle:String,current_pin:String,new_pin:String)->Result<CoreStatus,String>{
 let registration=state.inner().clone();
 tauri::async_runtime::spawn_blocking(move||registration.change_pin(&handle,current_pin,new_pin).map_err(|e|e.to_string()))
  .await.map_err(|_|"account operation interrupted; data preserved".to_string())?
}
#[tauri::command]
async fn recover_account_pin(state:tauri::State<'_,Arc<Registration>>,handle:String,passphrase:String,new_pin:String)->Result<CoreStatus,String>{
 let registration=state.inner().clone();
 tauri::async_runtime::spawn_blocking(move||registration.recover_pin(&handle,passphrase,new_pin).map_err(|e|e.to_string()))
  .await.map_err(|_|"account operation interrupted; data preserved".to_string())?
}
#[tauri::command]
async fn export_account_backup(state:tauri::State<'_,Arc<Registration>>,handle:String,pin:String,passphrase:String)->Result<String,String>{
 let registration=state.inner().clone();
 tauri::async_runtime::spawn_blocking(move||registration.export_backup(&handle,pin,passphrase).map_err(|e|e.to_string()))
  .await.map_err(|_|"backup operation interrupted; data preserved".to_string())?
}
#[tauri::command]
async fn restore_account_backup(state:tauri::State<'_,Arc<Registration>>,backup_data:String,passphrase:String,new_pin:String)->Result<String,String>{
 let registration=state.inner().clone();
 tauri::async_runtime::spawn_blocking(move||registration.restore_backup(backup_data,passphrase,new_pin).map_err(|e|e.to_string()))
  .await.map_err(|_|"backup recovery interrupted; source file remains unchanged".to_string())?
}
#[tauri::command]
async fn lock_account(state:tauri::State<'_,Arc<Registration>>)->Result<CoreStatus,String>{
 let registration=state.inner().clone();
 tauri::async_runtime::spawn_blocking(move||registration.lock().map_err(|e|e.to_string()))
  .await.map_err(|_|"account operation interrupted; data preserved".to_string())?
}
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run(){
 tauri::Builder::default().plugin(tauri_plugin_opener::init()).plugin(tauri_plugin_thinkgrow_vault::init()).setup(|app|{
   let base=app.path().app_local_data_dir()?;
   let context=Arc::new(NativeContext::new(base.clone(),App::Parent));
   let (protector,transport)=tauri_plugin_thinkgrow_vault::adapters(app.handle());
   app.manage(Arc::new(Registration::with_adapters(&base,App::Parent,context.clone(),protector,transport)?));
   app.manage(context);
   Ok(())
 }).invoke_handler(tauri::generate_handler![load_role_workspace,role_exchange,list_role_drafts,save_role_draft,sync_role_draft,core_status,open_registration_page,account_status,register_account,unlock_account,change_account_pin,recover_account_pin,export_account_backup,restore_account_backup,lock_account])
 .run(tauri::generate_context!()).expect("failed to run ThinkGrow offline app");
}
