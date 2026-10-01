mod commands;
mod exporter;
mod ingest;
mod store;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            commands::get_config,
            commands::save_config,
            commands::get_status,
            commands::get_software,
            commands::update_software,
            commands::batch_update,
            commands::delete_software,
            commands::batch_add,
            commands::merge_software,
            commands::llm_analyze,
            commands::scan_local,
            commands::export_markdown,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
