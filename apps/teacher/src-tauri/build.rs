fn main(){
 let attrs=tauri_build::Attributes::new().app_manifest(tauri_build::AppManifest::new().commands(&[
  "load_role_workspace","role_exchange","list_role_drafts","save_role_draft","sync_role_draft",
  "list_games","launch_game","stop_game","open_registration_page","account_status",
  "register_account","unlock_account","lock_account","core_status","change_account_pin",
  "recover_account_pin","export_account_backup","restore_account_backup",
  "launch_material_app","list_class_materials","open_material_file","save_material_chunk","delete_material","launch_online_game"
 ]));
 tauri_build::try_build(attrs).expect("Tauri build failed");
}
