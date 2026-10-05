mod commands;
mod exporter;
mod ingest;
mod store;
mod sync;
mod webdav;
mod xlsx;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(commands::SyncState::new())
        .setup(|app| {
            // 后端心跳：与前端页面无关（切到子页面也不会停），每 5 分钟续租会话锁。
            use std::time::Duration;
            let handle = app.handle().clone();
            std::thread::spawn(move || loop {
                std::thread::sleep(Duration::from_secs(300));
                let (owner, held) = {
                    let state = handle.state::<commands::SyncState>();
                    (state.owner.clone(), state.held.load(std::sync::atomic::Ordering::SeqCst))
                };
                if held {
                    let _ = sync::renew(&owner, &commands::current_machine());
                }
            });
            Ok(())
        })
        .on_window_event(|window, event| {
            // 关软件：推送最后一次改动并释放会话锁，保证「关了就释放」。
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                use std::sync::atomic::Ordering;
                let state = window.state::<commands::SyncState>();
                if state.held.load(Ordering::SeqCst) {
                    api.prevent_close();
                    let owner = state.owner.clone();
                    // 关闭路径上允许短暂阻塞（网络超时已由客户端限制）。
                    let _ = sync::run_sync(sync::SyncMode::Normal);
                    let _ = sync::release(&owner);
                    state.held.store(false, Ordering::SeqCst);
                    let _ = window.close();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_config,
            commands::save_config,
            commands::get_status,
            commands::get_version,
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
            commands::export_xlsx,
            commands::export_save,
            commands::open_url,
            commands::get_webdav_config,
            commands::save_webdav_config,
            commands::webdav_test,
            commands::sync_session_start,
            commands::sync_session_end,
            commands::sync_heartbeat,
            commands::sync_now,
            commands::sync_unlock,
            commands::sync_status,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
