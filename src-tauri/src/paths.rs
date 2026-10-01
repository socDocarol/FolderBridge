use std::path::{Path, PathBuf};

pub const QUARANTINE: &str = ".folderbridge-quarantine";
pub const STAGING: &str = ".folderbridge-staging";

pub fn path_key(path: &Path) -> String {
    let value = path.to_string_lossy().replace('\\', "/");
    if cfg!(windows) {
        value.to_lowercase()
    } else {
        value
    }
}

pub fn contains(root: &Path, candidate: &Path) -> bool {
    let root = path_key(root).trim_end_matches('/').to_string();
    let candidate = path_key(candidate);
    candidate == root || candidate.starts_with(&(root + "/"))
}

pub fn validate_roots(
    source: &Path,
    destination: Option<&Path>,
) -> Result<(PathBuf, Option<PathBuf>), String> {
    let source =
        std::fs::canonicalize(source).map_err(|e| format!("Cannot open source folder: {e}"))?;
    let destination = destination
        .map(std::fs::canonicalize)
        .transpose()
        .map_err(|e| format!("Cannot open destination folder: {e}"))?;
    if !source.is_dir() || destination.as_ref().is_some_and(|p| !p.is_dir()) {
        return Err("Choose folders, not files.".into());
    }
    if destination
        .as_ref()
        .is_some_and(|p| contains(&source, p) || contains(p, &source))
    {
        return Err("Choose separate folders. Neither folder can be inside the other.".into());
    }
    Ok((source, destination))
}

pub fn within_root(root: &Path, relative: &str) -> Result<PathBuf, String> {
    if relative.is_empty() || relative.starts_with(['/', '\\']) || relative.contains(':') {
        return Err("Expected a relative file path inside the selected folder.".into());
    }
    let components: Vec<_> = relative.split(['/', '\\']).collect();
    if components
        .iter()
        .any(|p| p.is_empty() || *p == "." || *p == "..")
    {
        return Err("Relative path cannot contain empty, dot, or parent components.".into());
    }
    let canonical_root =
        std::fs::canonicalize(root).map_err(|e| format!("Folder is unavailable: {e}"))?;
    let mut result = canonical_root.clone();
    for component in components {
        result.push(component);
        match std::fs::symlink_metadata(&result) {
            Ok(meta) => {
                if meta.file_type().is_symlink() {
                    return Err("Links and junctions are not followed.".into());
                }
                let actual = std::fs::canonicalize(&result).map_err(|e| e.to_string())?;
                if !contains(&canonical_root, &actual) {
                    return Err("Path leaves the selected folder.".into());
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(format!("Cannot verify path: {error}")),
        }
    }
    Ok(result)
}

pub fn relative_text(root: &Path, path: &Path) -> Result<String, String> {
    let relative = path.strip_prefix(root).map_err(|e| e.to_string())?;
    relative
        .to_str()
        .map(|s| s.replace('\\', "/"))
        .ok_or_else(|| "Filename is not valid Unicode; skipped.".into())
}

pub fn metadata_signature(meta: &std::fs::Metadata) -> Result<(u64, i64), String> {
    let modified = meta
        .modified()
        .map_err(|e| e.to_string())?
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| "Modification time predates 1970; skipped.".to_string())?;
    Ok((
        meta.len(),
        i64::try_from(modified.as_nanos()).map_err(|e| e.to_string())?,
    ))
}

pub fn move_no_replace(source: &Path, destination: &Path) -> Result<(), String> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        let from: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
        let to: Vec<u16> = destination
            .as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect();
        // With flags=0, MoveFileExW fails if the destination already exists.
        let result = unsafe {
            windows_sys::Win32::Storage::FileSystem::MoveFileExW(from.as_ptr(), to.as_ptr(), 0)
        };
        if result == 0 {
            return Err(std::io::Error::last_os_error().to_string());
        }
        Ok(())
    }
    #[cfg(not(windows))]
    {
        // Files only; link creation is exclusive. If unlink fails, both copies remain.
        std::fs::hard_link(source, destination).map_err(|e| e.to_string())?;
        std::fs::remove_file(source).map_err(|e| e.to_string())
    }
}

/// Rename the verified, still-open file. Windows denies other writers and deleters
/// for the lifetime of this handle; a sync client cannot swap the pathname.
#[cfg(windows)]
pub fn rename_open_file(
    file: &std::fs::File,
    _source: &Path,
    destination: &Path,
) -> Result<(), String> {
    use std::{
        mem::{offset_of, size_of},
        os::windows::{ffi::OsStrExt, io::AsRawHandle},
    };
    use windows_sys::Win32::Storage::FileSystem::{
        FileRenameInfo, SetFileInformationByHandle, FILE_RENAME_INFO,
    };
    let name: Vec<u16> = destination.as_os_str().encode_wide().collect();
    let byte_length = name.len().checked_mul(2).ok_or("Target name is too long")?;
    let size = (offset_of!(FILE_RENAME_INFO, FileName) + byte_length + 2)
        .max(size_of::<FILE_RENAME_INFO>());
    // usize backing provides alignment for FILE_RENAME_INFO's HANDLE member.
    let mut storage = vec![0usize; size.div_ceil(size_of::<usize>())];
    let info = storage.as_mut_ptr().cast::<FILE_RENAME_INFO>();
    unsafe {
        (*info).Anonymous.ReplaceIfExists = false;
        (*info).FileNameLength = u32::try_from(byte_length).map_err(|e| e.to_string())?;
        std::ptr::copy_nonoverlapping(
            name.as_ptr(),
            std::ptr::addr_of_mut!((*info).FileName).cast::<u16>(),
            name.len(),
        );
        if SetFileInformationByHandle(
            file.as_raw_handle(),
            FileRenameInfo,
            info.cast(),
            size as u32,
        ) == 0
        {
            return Err(std::io::Error::last_os_error().to_string());
        }
    }
    Ok(())
}

#[cfg(not(windows))]
pub fn rename_open_file(
    _file: &std::fs::File,
    source: &Path,
    destination: &Path,
) -> Result<(), String> {
    move_no_replace(source, destination)
}
