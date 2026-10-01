use crate::{
    engine::operation_item_path,
    paths::{contains, within_root},
    storage::{Result, Store},
};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RevealTarget {
    pub path: PathBuf,
    pub select_file: bool,
}

pub fn entry_target(
    store: &Store,
    scan_id: i64,
    entry_id: i64,
    side: &str,
) -> Result<RevealTarget> {
    let scan = store.scan(scan_id)?;
    let entry = Store::entry(&store.connect()?, scan_id, entry_id)?;
    let (root, relative) = match side {
        "source" => (scan.source.as_str(), entry.source_relative.as_deref()),
        "destination" => (
            scan.destination
                .as_deref()
                .ok_or("No destination selected.")?,
            entry.destination_relative.as_deref(),
        ),
        _ => return Err("Choose source or destination.".into()),
    };
    let root = fs::canonicalize(root).map_err(|e| format!("Folder unavailable: {e}"))?;
    if !root.is_dir() {
        return Err("Selected root is no longer a folder.".into());
    }
    let path = within_root(&root, relative.unwrap_or(&entry.relative_path))?;
    nearest_existing(&root, path)
}

pub fn operation_item_target(
    store: &Store,
    operation_id: i64,
    item_id: i64,
    location: &str,
) -> Result<RevealTarget> {
    let path = operation_item_path(store, operation_id, item_id, location)?;
    let root = store.scan(store.operation(operation_id)?.scan_id)?;
    let root = if location == "target" {
        let operation = store.operation(operation_id)?;
        if operation.action == "copy_to_source" {
            root.source
        } else {
            root.destination.ok_or("Missing destination")?
        }
    } else {
        let operation = store.operation(operation_id)?;
        if operation.action == "copy_to_source" {
            root.destination.ok_or("Missing destination")?
        } else {
            root.source
        }
    };
    let root = fs::canonicalize(root).map_err(|e| format!("Folder unavailable: {e}"))?;
    nearest_existing(&root, path)
}

fn nearest_existing(root: &std::path::Path, mut path: PathBuf) -> Result<RevealTarget> {
    loop {
        match fs::symlink_metadata(&path) {
            Ok(meta) => {
                if meta.file_type().is_symlink() {
                    return Err("Linked paths cannot be opened.".into());
                }
                let resolved = fs::canonicalize(&path).map_err(|e| e.to_string())?;
                if !contains(root, &resolved) {
                    return Err("Path leaves the selected folder.".into());
                }
                if !meta.is_file() && !meta.is_dir() {
                    return Err("Expected a file or folder.".into());
                }
                return Ok(RevealTarget {
                    path: resolved,
                    select_file: meta.is_file(),
                });
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                if path == root || !path.pop() || !contains(root, &path) {
                    return Err("Folder unavailable.".into());
                }
            }
            Err(e) => return Err(format!("Cannot open this location: {e}")),
        }
    }
}

#[cfg(windows)]
pub fn show(target: &RevealTarget) -> Result<()> {
    use std::{
        mem::size_of,
        ptr::{null, null_mut},
    };
    use windows_sys::Win32::{
        System::{
            Com::{
                CoInitializeEx, CoTaskMemFree, CoUninitialize, COINIT_APARTMENTTHREADED,
                COINIT_DISABLE_OLE1DDE,
            },
            SystemServices::{SFGAO_FILESYSTEM, SFGAO_FOLDER, SFGAO_LINK, SFGAO_STREAM},
        },
        UI::{
            Shell::{
                SHOpenFolderAndSelectItems, SHParseDisplayName, ShellExecuteExW,
                SEE_MASK_CLASSNAME, SEE_MASK_FLAG_NO_UI, SEE_MASK_IDLIST, SEE_MASK_NOASYNC,
                SHELLEXECUTEINFOW,
            },
            WindowsAndMessaging::SW_SHOWNORMAL,
        },
    };
    // Shell parsing expects ordinary drive/UNC syntax, not Rust's verbatim prefix.
    let raw = target.path.to_str().ok_or("Path is not valid Unicode.")?;
    let ordinary = if let Some(unc) = raw.strip_prefix("\\\\?\\UNC\\") {
        format!("\\\\{unc}")
    } else {
        raw.strip_prefix("\\\\?\\").unwrap_or(raw).to_owned()
    };
    let path: Vec<u16> = ordinary.encode_utf16().chain(Some(0)).collect();
    let verb: Vec<u16> = "open".encode_utf16().chain(Some(0)).collect();
    let folder_class: Vec<u16> = "Folder".encode_utf16().chain(Some(0)).collect();
    // Called on a dedicated blocking worker; balance COM and PIDL allocations on every path.
    unsafe {
        let initialized = CoInitializeEx(
            null(),
            (COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE) as u32,
        );
        if initialized < 0 {
            return Err(format!(
                "Explorer initialization failed (0x{:08X}).",
                initialized as u32
            ));
        }
        let mut pidl = null_mut();
        let mut attributes = 0;
        let parsed = SHParseDisplayName(
            path.as_ptr(),
            null_mut(),
            &mut pidl,
            SFGAO_FOLDER | SFGAO_FILESYSTEM | SFGAO_STREAM | SFGAO_LINK,
            &mut attributes,
        );
        let result = if parsed < 0 {
            Err(format!(
                "Explorer cannot find this location (0x{:08X}).",
                parsed as u32
            ))
        } else if !target.select_file
            && (attributes & (SFGAO_FOLDER | SFGAO_FILESYSTEM) != SFGAO_FOLDER | SFGAO_FILESYSTEM
                || attributes & (SFGAO_STREAM | SFGAO_LINK) != 0)
        {
            Err("This location is no longer a folder.".into())
        } else if target.select_file {
            let opened = SHOpenFolderAndSelectItems(pidl, 0, null(), 0);
            if opened < 0 {
                Err(format!(
                    "Explorer could not select the file (0x{:08X}).",
                    opened as u32
                ))
            } else {
                Ok(())
            }
        } else {
            let mut info: SHELLEXECUTEINFOW = std::mem::zeroed();
            info.cbSize = size_of::<SHELLEXECUTEINFOW>() as u32;
            // Fix the association to Folder so a replaced item cannot launch as a file.
            info.fMask =
                SEE_MASK_IDLIST | SEE_MASK_NOASYNC | SEE_MASK_FLAG_NO_UI | SEE_MASK_CLASSNAME;
            info.lpClass = folder_class.as_ptr();
            info.lpVerb = verb.as_ptr();
            info.lpIDList = pidl.cast();
            info.nShow = SW_SHOWNORMAL;
            if ShellExecuteExW(&mut info) == 0 {
                Err(format!(
                    "Explorer could not open the folder: {}",
                    std::io::Error::last_os_error()
                ))
            } else {
                Ok(())
            }
        };
        if !pidl.is_null() {
            CoTaskMemFree(pidl.cast());
        }
        CoUninitialize();
        result
    }
}

#[cfg(not(windows))]
pub fn show(_target: &RevealTarget) -> Result<()> {
    Err("Show in Explorer is available on Windows.".into())
}
