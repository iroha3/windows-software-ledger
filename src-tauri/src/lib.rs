mod commands;
mod exporter;
mod ingest;
mod store;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            commands::get_config,
            commands::save_config,
            commands::get_status,
            commands::get_software,
            commands::get_dev_env,
            commands::get_browser_extensions,
            commands::update_extension,
            commands::vault_list,
            commands::vault_add,
            commands::vault_delete,
            commands::vault_export,
            commands::update_software,
            commands::batch_update,
            commands::delete_software,
            commands::batch_add,
            commands::merge_software,
            commands::llm_analyze,
            commands::scan_preview,
            commands::scan_commit,
            commands::export_markdown,
            commands::export_save,
            commands::open_url,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
