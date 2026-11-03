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
fn launch_material_app(state:tauri::State<'_,Arc<NativeContext>>,id:String,session_token:Option<String>)->Result<thinkgrow_offline_core::game_host::Launch,String>{state.inner().launch_material_app(&id,session_token.as_deref()).map_err(|e|e.to_string())}
#[tauri::command]
fn list_class_materials(state:tauri::State<'_,Arc<NativeContext>>)->Result<Vec<thinkgrow_offline_core::MaterialInfo>,String>{state.list_materials().map_err(|e|e.to_string())}
#[tauri::command]
fn open_material_file(app:tauri::AppHandle,state:tauri::State<'_,Arc<NativeContext>>,id:String)->Result<String,String>{
    let info = state.get_material(&id).map_err(|e|e.to_string())?.ok_or_else(||"material not found".to_string())?;
    if info.subtype == "app" && info.app_kind.as_deref() == Some("web_app") {
        return Err("웹 앱 패키지는 보안 격리 정책에 따라 운영체제 외부 연결 앱으로 열 수 없습니다.".to_string());
    }
    let path = state.open_material_file(&id).map_err(|e|e.to_string())?;
    let path_str = path.to_string_lossy().to_string();
    app.opener().open_path(&path_str, None::<&str>).map_err(|_|"could not open file with OS".to_string())?;
    Ok(path_str)
}
#[tauri::command]
fn save_material_chunk(state:tauri::State<'_,Arc<NativeContext>>,chunk:thinkgrow_offline_core::MaterialChunk,info:thinkgrow_offline_core::MaterialInfo)->Result<bool,String>{
    state.save_material_chunk(&chunk,&info).map_err(|e|e.to_string())
}
#[tauri::command]
fn delete_material(state:tauri::State<'_,Arc<NativeContext>>,id:String)->Result<(),String>{
    state.delete_material(&id).map_err(|e|e.to_string())
}
#[tauri::command]
fn launch_online_game(state:tauri::State<'_,Arc<NativeContext>>,game_id:String,assignment_id:Option<String>,session_token:String)->Result<thinkgrow_offline_core::game_host::Launch,String>{
    state.inner().launch_online_game_for(&game_id,assignment_id.as_deref(),&session_token).map_err(|e|e.to_string())
}

#[tauri::command]
fn list_games(state:tauri::State<'_,Arc<NativeContext>>)->Result<Vec<thinkgrow_offline_core::workspace::GameInfo>,String>{state.installed_games().map_err(|e|e.to_string())}
#[tauri::command]
fn launch_game(state:tauri::State<'_,Arc<NativeContext>>,game_id:String,assignment_id:Option<String>)->Result<thinkgrow_offline_core::game_host::Launch,String>{state.inner().launch_game_for(&game_id,assignment_id.as_deref()).map_err(|e|e.to_string())}
#[tauri::command]
fn stop_game(state:tauri::State<'_,Arc<NativeContext>>)->Result<(),String>{state.stop_game().map_err(|e|e.to_string())}
#[tauri::command]
fn core_status(state:tauri::State<'_,Arc<NativeContext>>)->Result<CoreStatus,String>{state.status().map_err(|e|e.to_string())}

#[tauri::command]
fn load_draft(state:tauri::State<'_,Arc<NativeContext>>,assignment:String)->Result<Option<thinkgrow_offline_core::Draft>,String>{
 state.load(&assignment).map_err(|e|e.to_string())
}
#[tauri::command]
fn save_draft(state:tauri::State<'_,Arc<NativeContext>>,answer:thinkgrow_offline_core::Answer,expected_revision:u64)->Result<u64,String>{
 state.save_prepared(&answer,expected_revision).map_err(|e|e.to_string())
}
#[tauri::command]
fn queue_submission(state:tauri::State<'_,Arc<NativeContext>>,assignment:String,revision:u64,intent:String)->Result<thinkgrow_offline_core::Envelope,String>{
 state.submit_prepared(&assignment,revision,&intent).map_err(|e|e.to_string())
}
#[tauri::command]
fn list_lessons(state:tauri::State<'_,Arc<NativeContext>>)->Result<Vec<thinkgrow_offline_core::lesson::Lesson>,String>{state.lessons().map_err(|e|e.to_string())}
#[tauri::command]
async fn prepare_lessons(state:tauri::State<'_,Arc<Registration>>)->Result<Vec<thinkgrow_offline_core::lesson::Lesson>,String>{
 let registration=state.inner().clone();
 tauri::async_runtime::spawn_blocking(move||registration.prepare_lessons().map_err(|e|e.to_string())).await.map_err(|_|"lesson preparation interrupted; data preserved".to_string())?
}
#[tauri::command]
fn open_registration_page(app:tauri::AppHandle)->Result<(),String>{
 app.opener().open_url("https://schoolclassscorecheck.web.app/offline-register.html?role=student",None::<&str>)
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
   let context=Arc::new(NativeContext::new(base.clone(),App::Student));
   let (protector,transport)=tauri_plugin_thinkgrow_vault::adapters(app.handle());
   app.manage(Arc::new(Registration::with_adapters(&base,App::Student,context.clone(),protector,transport)?));
   app.manage(context);
   Ok(())
 }).invoke_handler(tauri::generate_handler![load_role_workspace,role_exchange,list_games,launch_game,stop_game,list_class_materials,launch_material_app,open_material_file,save_material_chunk,delete_material,launch_online_game,core_status,list_lessons,prepare_lessons,load_draft,save_draft,queue_submission,open_registration_page,account_status,register_account,unlock_account,change_account_pin,recover_account_pin,export_account_backup,restore_account_backup,lock_account])
 .run(tauri::generate_context!()).expect("failed to run ThinkGrow offline app");
}
