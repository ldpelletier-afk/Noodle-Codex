mod bib;
mod commands;
mod db;
mod model;
mod pdf;
mod thumb;
mod watch;

use commands::{
    attach_bibtex, detach_bibtex, export_bibtex, export_bibtex_batch, get_bibtex_status,
    get_document, list_documents, list_folders, list_reading_events, open_document,
    rematch_bibtex, remove_folder, retry_thumbnails, reveal_in_finder, scan_folder,
    set_reading_progress, set_reading_status, update_document, write_text_file,
};
use db::Db;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .setup(|app| {
            let paths = commands::app_paths(&app.handle())
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
            let db = Db::open(&paths.db_file)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;
            app.manage(db);

            // Start live-tracking every already-added folder so new PDFs
            // dropped in while Codex is running get picked up automatically.
            let state = app.state::<Db>();
            if let Ok(conn) = state.0.lock() {
                watch::start_all(app.handle().clone(), &conn);
            }

            // Heal any thumbnail records left dangling by an external cache
            // wipe (e.g. a "clear caches" tool), then regenerate them.
            commands::spawn_startup_thumbnail_recovery(app.handle().clone());

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            scan_folder,
            list_documents,
            get_document,
            list_folders,
            remove_folder,
            retry_thumbnails,
            open_document,
            reveal_in_finder,
            attach_bibtex,
            detach_bibtex,
            get_bibtex_status,
            rematch_bibtex,
            export_bibtex,
            export_bibtex_batch,
            write_text_file,
            update_document,
            set_reading_progress,
            set_reading_status,
            list_reading_events,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
