mod agent;
mod commands;
mod db;

use db::Db;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let dir = app
                .path()
                .app_data_dir()
                .expect("resolve app data dir");
            std::fs::create_dir_all(&dir).ok();
            let db_path = dir.join("manch.sqlite3");
            let db = Db::open(db_path.to_str().expect("utf-8 db path")).expect("open database");
            app.manage(db);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::save_api_key,
            commands::list_configured_providers,
            commands::send_prompt,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
