// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // `--mcp`：以 stdio MCP 服务端模式运行，把软件台账作为只读信息源递给 AI agent。
    // 其余情况正常开 GUI。
    if std::env::args().skip(1).any(|a| a == "--mcp") {
        windows_software_ledger_lib::mcp::serve_stdio();
        return;
    }
    windows_software_ledger_lib::run()
}
