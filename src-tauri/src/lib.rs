mod commands;
mod exporter;
mod ingest;
pub mod mcp;
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
            // 后端心跳：与前端页面无关（切到子页面也不会停），每 1 分钟续租会话锁。
            use std::time::Duration;
            let handle = app.handle().clone();
            std::thread::spawn(move || loop {
                std::thread::sleep(Duration::from_secs(60));
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
            // 关软件：立刻隐藏窗口，把「推送最后一次改动 + 释放锁」丢后台。
            // 同步走网络、客户端总超时可到 900s，绝不能在关闭路径上阻塞。
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                use std::sync::atomic::Ordering;
                let state = window.state::<commands::SyncState>();
                if state.held.load(Ordering::SeqCst) {
                    api.prevent_close();
                    let owner = state.owner.clone();
                    // 先切断心跳续租，避免后台释放后又被续上。
                    state.held.store(false, Ordering::SeqCst);
                    // 本机不再持锁：本地落盘编辑权状态（MCP 是独立进程，靠这个文件判断）。
                    let enabled = store::get_webdav_config()
                        .get("enabled")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    store::write_session_state(enabled);
                    // 隐藏窗口：关闭手感瞬时。
                    let _ = window.hide();

                    let handle = window.app_handle().clone();
                    // 后台收尾：推送 + 释放锁，完成后退出。
                    let h1 = handle.clone();
                    std::thread::spawn(move || {
                        let _ = sync::run_sync(sync::SyncMode::Normal);
                        let _ = sync::release(&owner);
                        h1.exit(0);
                    });
                    // 看门狗：网络卡死也不拖住退出；未释放的锁靠租约过期回收。
                    let h2 = handle.clone();
                    std::thread::spawn(move || {
                        std::thread::sleep(std::time::Duration::from_secs(10));
                        h2.exit(0);
                    });
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_config,
            commands::save_config,
            commands::get_status,
            commands::get_version,
            commands::get_mcp_info,
            commands::get_ledger_revision,
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
