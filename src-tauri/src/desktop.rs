use crate::{engine::Engine, models::*};
use std::path::PathBuf;
use tauri::{Manager, State};

#[tauri::command]
async fn scan_folders(engine: State<'_, Engine>, options: ScanOptions) -> Result<i64, String> {
    let engine = engine.inner().clone();
    tauri::async_runtime::spawn_blocking(move || engine.scan(options))
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
fn get_progress(engine: State<'_, Engine>) -> Progress {
    engine.progress()
}
#[tauri::command]
fn cancel_job(engine: State<'_, Engine>) {
    engine.cancel();
}
#[tauri::command]
fn list_scans(engine: State<'_, Engine>, offset: Option<u64>) -> Result<Vec<Scan>, String> {
    engine.store.scans_page(offset.unwrap_or(0))
}
#[tauri::command]
async fn list_entries(engine: State<'_, Engine>, filter: EntryFilter) -> Result<EntryPage, String> {
    let engine = engine.inner().clone();
    tauri::async_runtime::spawn_blocking(move || engine.store.entries(&filter))
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
async fn matching_entry_ids(
    engine: State<'_, Engine>,
    filter: EntryFilter,
) -> Result<Vec<i64>, String> {
    let engine = engine.inner().clone();
    tauri::async_runtime::spawn_blocking(move || engine.store.matching_ids(&filter))
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
async fn get_analysis(engine: State<'_, Engine>, scan_id: i64) -> Result<Analysis, String> {
    let engine = engine.inner().clone();
    tauri::async_runtime::spawn_blocking(move || engine.store.analysis(scan_id))
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
async fn preview_operation(
    engine: State<'_, Engine>,
    request: OperationRequest,
) -> Result<Preview, String> {
    let engine = engine.inner().clone();
    tauri::async_runtime::spawn_blocking(move || engine.preview(&request))
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
async fn execute_operation(
    engine: State<'_, Engine>,
    request: OperationRequest,
) -> Result<i64, String> {
    let engine = engine.inner().clone();
    tauri::async_runtime::spawn_blocking(move || engine.execute(request))
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
fn list_operations(
    engine: State<'_, Engine>,
    offset: Option<u64>,
) -> Result<Vec<Operation>, String> {
    engine.store.operations_page(offset.unwrap_or(0))
}
#[tauri::command]
fn get_operation_items(
    engine: State<'_, Engine>,
    operation_id: i64,
) -> Result<Vec<OperationItem>, String> {
    engine.store.operation_items(operation_id)
}
#[tauri::command]
async fn restore_operation(engine: State<'_, Engine>, operation_id: i64) -> Result<(), String> {
    let engine = engine.inner().clone();
    tauri::async_runtime::spawn_blocking(move || engine.restore(operation_id))
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
async fn preview_cleanup(
    engine: State<'_, Engine>,
    operation_id: i64,
) -> Result<CleanupPreview, String> {
    let engine = engine.inner().clone();
    tauri::async_runtime::spawn_blocking(move || engine.preview_cleanup(operation_id))
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
async fn cleanup_originals(
    engine: State<'_, Engine>,
    request: CleanupRequest,
) -> Result<CleanupSummary, String> {
    let engine = engine.inner().clone();
    tauri::async_runtime::spawn_blocking(move || engine.cleanup_originals(request))
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
async fn restore_originals(
    engine: State<'_, Engine>,
    operation_id: i64,
    item_ids: Vec<i64>,
) -> Result<CleanupSummary, String> {
    let engine = engine.inner().clone();
    tauri::async_runtime::spawn_blocking(move || engine.restore_originals(operation_id, item_ids))
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
async fn reveal_operation_item(
    engine: State<'_, Engine>,
    operation_id: i64,
    item_id: i64,
    location: String,
) -> Result<crate::reveal::RevealTarget, String> {
    let engine = engine.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let target =
            crate::reveal::operation_item_target(&engine.store, operation_id, item_id, &location)?;
        crate::reveal::show(&target)?;
        Ok(target)
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
async fn export_scan(
    engine: State<'_, Engine>,
    filter: EntryFilter,
    path: String,
) -> Result<u64, String> {
    let engine = engine.inner().clone();
    tauri::async_runtime::spawn_blocking(move || engine.export_scan(filter, &PathBuf::from(path)))
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
async fn export_operation(
    engine: State<'_, Engine>,
    operation_id: i64,
    path: String,
) -> Result<(), String> {
    let engine = engine.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        engine.export_operation(operation_id, &PathBuf::from(path))
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
fn list_pairs(engine: State<'_, Engine>) -> Result<Vec<SavedPair>, String> {
    engine.store.pairs()
}

#[tauri::command]
async fn export_statistics(
    engine: State<'_, Engine>,
    scan_id: i64,
    path: String,
) -> Result<(), String> {
    let engine = engine.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        engine.export_statistics(scan_id, &PathBuf::from(path))
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
fn save_pair(engine: State<'_, Engine>, name: String, options: ScanOptions) -> Result<(), String> {
    engine.store.save_pair(&name, &options)
}
#[tauri::command]
fn delete_pair(engine: State<'_, Engine>, id: i64) -> Result<(), String> {
    engine.store.delete_pair(id)
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            // Optional isolated data directory for portable profiles and native QA.
            let directory = match std::env::var_os("FOLDERBRIDGE_DATA_DIR") {
                Some(path) => {
                    let path = PathBuf::from(path);
                    if !path.is_absolute() {
                        return Err(std::io::Error::other(
                            "FOLDERBRIDGE_DATA_DIR must be absolute.",
                        )
                        .into());
                    }
                    path
                }
                None => app.path().app_local_data_dir()?,
            };
            let engine = Engine::open(&directory.join("folderbridge.sqlite"))
                .map_err(std::io::Error::other)?;
            app.manage(engine);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            scan_folders,
            get_progress,
            cancel_job,
            list_scans,
            list_entries,
            matching_entry_ids,
            get_analysis,
            preview_operation,
            execute_operation,
            list_operations,
            get_operation_items,
            restore_operation,
            preview_cleanup,
            cleanup_originals,
            restore_originals,
            reveal_operation_item,
            export_scan,
            export_operation,
            export_statistics,
            list_pairs,
            save_pair,
            delete_pair,
            reveal_entry
        ])
        .run(tauri::generate_context!())
        .expect("Unable to start FolderBridge");
}

#[tauri::command]
async fn reveal_entry(
    engine: State<'_, Engine>,
    scan_id: i64,
    entry_id: i64,
    side: String,
) -> Result<crate::reveal::RevealTarget, String> {
    let engine = engine.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let target = crate::reveal::entry_target(&engine.store, scan_id, entry_id, &side)?;
        crate::reveal::show(&target)?;
        Ok(target)
    })
    .await
    .map_err(|e| e.to_string())?
}
