fn main(){
 let attrs=tauri_build::Attributes::new().app_manifest(tauri_build::AppManifest::new().commands(&["load_role_workspace","role_exchange","list_games","launch_game","stop_game","open_registration_page", "account_status", "register_account", "unlock_account", "lock_account", "core_status", "change_account_pin", "recover_account_pin", "export_account_backup", "restore_account_backup", "list_lessons","prepare_lessons", "load_draft", "save_draft", "queue_submission"]));
 tauri_build::try_build(attrs).expect("Tauri build failed");
}
