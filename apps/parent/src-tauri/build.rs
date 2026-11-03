fn main(){
 let attrs=tauri_build::Attributes::new().app_manifest(tauri_build::AppManifest::new().commands(&["load_role_workspace","role_exchange","list_role_drafts","save_role_draft","sync_role_draft","open_registration_page", "account_status", "register_account", "unlock_account", "lock_account", "core_status", "change_account_pin", "recover_account_pin", "export_account_backup", "restore_account_backup"]));
 tauri_build::try_build(attrs).expect("Tauri build failed");
}
