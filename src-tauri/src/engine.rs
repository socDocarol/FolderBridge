mod cleanup;
mod operations;
pub(crate) use cleanup::operation_item_path;
mod reports;
mod scan;

use crate::{
    models::*,
    paths::*,
    storage::{db_error, now, read_entry, Result, Store},
};
use rusqlite::{params, Connection, OptionalExtension};
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    fs::{self, File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};
use tempfile::NamedTempFile;
use walkdir::WalkDir;

#[derive(Clone)]
pub struct Engine {
    pub store: Store,
    progress: Arc<Mutex<Progress>>,
    busy: Arc<AtomicBool>,
    cancel: Arc<AtomicBool>,
}

struct JobGuard(Engine);

// Fields drop in declaration order. Close the Windows handle before deleting its
// path, since this file intentionally denies other delete/rename handles.
struct StagedFile {
    file: File,
    path: tempfile::TempPath,
}

struct CopyProof {
    hash: String,
    identity: Option<String>,
    issue: Option<String>,
}
impl Drop for JobGuard {
    fn drop(&mut self) {
        self.0.update(|p| p.running = false);
        self.0.busy.store(false, Ordering::SeqCst);
    }
}

impl Engine {
    pub fn open(path: &Path) -> Result<Self> {
        Ok(Self {
            store: Store::open(path)?,
            progress: Arc::new(Mutex::new(Progress::default())),
            busy: Arc::new(AtomicBool::new(false)),
            cancel: Arc::new(AtomicBool::new(false)),
        })
    }

    fn begin(&self, kind: &str) -> Result<JobGuard> {
        if self
            .busy
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return Err("Another job is running. Wait for it or cancel it first.".into());
        }
        self.cancel.store(false, Ordering::SeqCst);
        self.update(|p| {
            *p = Progress {
                running: true,
                kind: kind.into(),
                phase: "starting".into(),
                ..Default::default()
            }
        });
        Ok(JobGuard(self.clone()))
    }

    pub fn progress(&self) -> Progress {
        self.progress
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }
    fn update(&self, change: impl FnOnce(&mut Progress)) {
        change(&mut self.progress.lock().unwrap_or_else(|e| e.into_inner()));
    }
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::SeqCst);
    }
    fn check_cancel(&self) -> Result<()> {
        if self.cancel.load(Ordering::SeqCst) {
            Err("Cancelled".into())
        } else {
            Ok(())
        }
    }

    fn hash(&self, path: &Path) -> Result<String> {
        let mut file = locked_read(path)?;
        self.hash_open(&mut file)
    }

    fn hash_open(&self, file: &mut File) -> Result<String> {
        file.seek(SeekFrom::Start(0)).map_err(|e| e.to_string())?;
        let before = metadata_signature(&file.metadata().map_err(|e| e.to_string())?)?;
        let mut hash = Sha256::new();
        let mut buffer = vec![0u8; 1024 * 1024];
        loop {
            self.check_cancel()?;
            let read = file.read(&mut buffer).map_err(|e| e.to_string())?;
            if read == 0 {
                break;
            }
            hash.update(&buffer[..read]);
            self.update(|p| p.bytes += read as u64);
        }
        if before != metadata_signature(&file.metadata().map_err(|e| e.to_string())?)? {
            return Err("File changed while being read; scan again.".into());
        }
        Ok(format!("{:x}", hash.finalize()))
    }

    fn copy_verified(
        &self,
        source: &Path,
        target: &Path,
        target_root: &Path,
        operation: i64,
        size: Option<u64>,
        modified: Option<i64>,
    ) -> Result<CopyProof> {
        let mut input = locked_read(source)?;
        check_snapshot(source, size, modified)?;
        let identity = crate::cleanup_native::identity(&input);
        let content_only = crate::cleanup_native::validate_content_only(&input);
        let (identity, issue) = match (identity, content_only) {
            (Ok(identity), Ok(())) => (Some(identity), None),
            (Err(error), _) | (_, Err(error)) => (None, Some(error)),
        };
        let staging_relative = format!("{STAGING}/{operation}");
        let staging = within_root(target_root, &staging_relative)?;
        fs::create_dir_all(&staging).map_err(|e| e.to_string())?;
        let staging = within_root(target_root, &staging_relative)?;
        let temporary = tempfile::Builder::new()
            .prefix("copy-")
            .make_in(&staging, |path| {
                let mut options = OpenOptions::new();
                options.read(true).write(true).create_new(true);
                #[cfg(windows)]
                {
                    use std::os::windows::fs::OpenOptionsExt;
                    options
                        .share_mode(1)
                        .access_mode(0x80000000 | 0x40000000 | 0x10000);
                }
                options.open(path)
            })
            .map_err(|e| e.to_string())?;
        let (file, path) = temporary.into_parts();
        let mut temporary = StagedFile { file, path };
        let mut hash = Sha256::new();
        let mut buffer = vec![0u8; 1024 * 1024];
        loop {
            self.check_cancel()?;
            let n = input.read(&mut buffer).map_err(|e| e.to_string())?;
            if n == 0 {
                break;
            }
            temporary
                .file
                .write_all(&buffer[..n])
                .map_err(|e| e.to_string())?;
            hash.update(&buffer[..n]);
            self.update(|p| p.bytes += n as u64);
        }
        temporary.file.sync_all().map_err(|e| e.to_string())?;
        check_snapshot(source, size, modified)?;
        let source_hash = format!("{:x}", hash.finalize());
        temporary
            .file
            .seek(SeekFrom::Start(0))
            .map_err(|e| e.to_string())?;
        let mut verification = Sha256::new();
        loop {
            self.check_cancel()?;
            let n = temporary
                .file
                .read(&mut buffer)
                .map_err(|e| e.to_string())?;
            if n == 0 {
                break;
            }
            verification.update(&buffer[..n]);
        }
        let target_hash = format!("{:x}", verification.finalize());
        if source_hash != target_hash {
            return Err("Copy verification failed; source preserved.".into());
        }
        let source_modified = input
            .metadata()
            .and_then(|m| m.modified())
            .map_err(|e| e.to_string())?;
        temporary
            .file
            .set_times(fs::FileTimes::new().set_modified(source_modified))
            .map_err(|e| e.to_string())?;
        temporary.file.sync_all().map_err(|e| e.to_string())?;
        self.check_cancel()?;
        #[cfg(windows)]
        rename_open_file(&temporary.file, &temporary.path, target)?;
        #[cfg(not(windows))]
        temporary.path.persist_noclobber(target).map_err(|e| {
            format!(
                "Could not finalize copy without replacing a file: {}",
                e.error
            )
        })?;
        Ok(CopyProof {
            hash: source_hash,
            identity,
            issue,
        })
    }
}

fn eligible(entry: &Entry, action: &str) -> bool {
    match action {
        "copy_to_destination" => entry.status == "source_only",
        "copy_to_source" => entry.status == "destination_only",
        "keep_both" => entry.status == "different",
        "quarantine_destination" => {
            entry.status == "identical"
                && entry.source_hash.is_some()
                && entry.destination_hash.is_some()
        }
        _ => false,
    }
}

fn check_snapshot(path: &Path, size: Option<u64>, modified: Option<i64>) -> Result<()> {
    let meta = fs::symlink_metadata(path).map_err(|e| format!("File unavailable: {e}"))?;
    if !meta.is_file() || meta.file_type().is_symlink() {
        return Err("Expected a regular file; links are not processed.".into());
    }
    let actual = metadata_signature(&meta)?;
    if Some(actual.0) != size || Some(actual.1) != modified {
        return Err("File changed since the comparison. Scan again.".into());
    }
    Ok(())
}

fn locked_read(path: &Path) -> Result<File> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.share_mode(1);
    }
    options
        .open(path)
        .map_err(|e| format!("Cannot read or file is in use: {e}"))
}

pub fn movable_read(path: &Path) -> Result<File> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.share_mode(1).access_mode(0x80000000 | 0x10000);
    }
    options.open(path).map_err(|e| {
        format!("Cannot lock file for a verified move. Close applications using it: {e}")
    })
}

fn reject_open_database(path: &Path) -> Result<()> {
    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
        .to_lowercase();
    if extension == "accdb" || extension == "mdb" {
        let lock = path.with_extension(if extension == "accdb" {
            "laccdb"
        } else {
            "ldb"
        });
        if fs::symlink_metadata(lock).is_ok() {
            return Err("Access database has a lock file. Close it and scan again.".into());
        }
    }
    Ok(())
}

pub fn csv_safe(value: &str) -> String {
    if value.starts_with(['\t', '\r', '\n']) || value.trim_start().starts_with(['=', '+', '-', '@'])
    {
        format!("'{value}")
    } else {
        value.into()
    }
}

#[cfg(windows)]
fn file_owner(path: &Path) -> Result<String> {
    use std::{os::windows::ffi::OsStrExt, ptr};
    use windows_sys::Win32::{
        Foundation::LocalFree,
        Security::{
            Authorization::{ConvertSidToStringSidW, GetNamedSecurityInfoW, SE_FILE_OBJECT},
            OWNER_SECURITY_INFORMATION,
        },
    };
    let name: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut sid = ptr::null_mut();
    let mut descriptor = ptr::null_mut();
    let status = unsafe {
        GetNamedSecurityInfoW(
            name.as_ptr(),
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION,
            &mut sid,
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
            &mut descriptor,
        )
    };
    if status != 0 {
        return Err(std::io::Error::from_raw_os_error(status as i32).to_string());
    }
    let mut text: *mut u16 = ptr::null_mut();
    let converted = unsafe { ConvertSidToStringSidW(sid, &mut text) };
    let result = if converted == 0 {
        Err(std::io::Error::last_os_error().to_string())
    } else {
        let mut len = 0;
        unsafe {
            while *text.add(len) != 0 {
                len += 1;
            }
            let result = String::from_utf16_lossy(std::slice::from_raw_parts(text, len));
            LocalFree(text.cast());
            Ok(result)
        }
    };
    unsafe {
        LocalFree(descriptor.cast());
    }
    result
}

#[cfg(unix)]
fn file_owner(path: &Path) -> Result<String> {
    use std::os::unix::fs::MetadataExt;
    fs::metadata(path)
        .map(|m| format!("uid:{}", m.uid()))
        .map_err(|e| e.to_string())
}

#[cfg(not(any(unix, windows)))]
fn file_owner(_path: &Path) -> Result<String> {
    Err("Owner lookup is unavailable on this platform".into())
}
