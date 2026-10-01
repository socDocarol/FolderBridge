//! Windows cleanup primitives. Recycling has no permanent-delete fallback.
use crate::storage::Result;
use std::{fs::File, path::Path};

#[cfg(windows)]
pub use platform::{identity, is_network, recycle, validate_content_only};

#[cfg(not(windows))]
pub fn identity(_file: &File) -> Result<String> {
    Err("Verified source cleanup requires Windows file identity support.".into())
}

#[cfg(not(windows))]
pub fn validate_content_only(_file: &File) -> Result<()> {
    Err("Verified source cleanup requires Windows stream validation.".into())
}

#[cfg(not(windows))]
pub fn is_network(_path: &Path) -> Result<bool> {
    Err("Source cleanup requires Windows drive classification.".into())
}

#[cfg(not(windows))]
pub fn recycle(_path: &Path) -> Result<()> {
    Err("Source cleanup requires the Windows Recycle Bin.".into())
}

#[cfg(windows)]
mod platform {
    use super::*;
    use std::{
        mem::{offset_of, size_of},
        os::windows::{ffi::OsStrExt, io::AsRawHandle},
        path::{Component, Prefix},
        sync::{Arc, Mutex},
    };
    use windows::{
        core::{implement, Error, Ref, HRESULT, PCWSTR},
        Win32::{
            Foundation::{ERROR_INSUFFICIENT_BUFFER, ERROR_MORE_DATA, E_ABORT, HANDLE},
            Storage::FileSystem::{
                FileBasicInfo, FileIdInfo, FileStreamInfo, GetDriveTypeW,
                GetFileInformationByHandleEx, GetFileType, GetVolumePathNameW,
                FILE_ATTRIBUTE_DEVICE, FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_ENCRYPTED,
                FILE_BASIC_INFO, FILE_ID_INFO, FILE_INFO_BY_HANDLE_CLASS, FILE_STREAM_INFO,
                FILE_TYPE_DISK,
            },
            System::Com::{
                CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER,
                COINIT_APARTMENTTHREADED,
            },
            UI::Shell::{
                FileOperation, IFileOperation, IFileOperationProgressSink,
                IFileOperationProgressSink_Impl, IShellItem, SHCreateItemFromParsingName,
                FOFX_EARLYFAILURE, FOFX_RECYCLEONDELETE, FOF_NOCONFIRMATION, FOF_NOERRORUI,
                FOF_NO_CONNECTED_ELEMENTS, FOF_SILENT, TSF_DELETE_RECYCLE_IF_POSSIBLE,
            },
        },
    };

    fn query<T: Default>(file: &File, class: FILE_INFO_BY_HANDLE_CLASS) -> Result<T> {
        let mut info = T::default();
        unsafe {
            GetFileInformationByHandleEx(
                HANDLE(file.as_raw_handle()),
                class,
                (&mut info as *mut T).cast(),
                size_of::<T>() as u32,
            )
        }
        .map_err(|e| format!("Cannot verify cleanup file metadata: {e}"))?;
        Ok(info)
    }

    /// Identity is queried from the held handle, never by reopening its pathname.
    pub fn identity(file: &File) -> Result<String> {
        let id: FILE_ID_INFO = query(file, FileIdInfo)?;
        let basic: FILE_BASIC_INFO = query(file, FileBasicInfo)?;
        if id.FileId.Identifier.iter().all(|b| *b == 0) {
            return Err("Filesystem did not provide a stable file ID; cleanup refused.".into());
        }
        let file_id: String = id
            .FileId
            .Identifier
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        Ok(format!(
            "{:016x}:{file_id}:{:016x}",
            id.VolumeSerialNumber, basic.CreationTime
        ))
    }

    pub fn validate_content_only(file: &File) -> Result<()> {
        if unsafe { GetFileType(HANDLE(file.as_raw_handle())) } != FILE_TYPE_DISK {
            return Err("Cleanup supports regular disk files only.".into());
        }
        let basic: FILE_BASIC_INFO = query(file, FileBasicInfo)?;
        if basic.FileAttributes
            & (FILE_ATTRIBUTE_DIRECTORY.0 | FILE_ATTRIBUTE_DEVICE.0 | FILE_ATTRIBUTE_ENCRYPTED.0)
            != 0
        {
            return Err(
                "Cleanup does not support directories, device files, or EFS-encrypted files."
                    .into(),
            );
        }
        // Cloud placeholders, compression, and sparseness can preserve ordinary
        // logical file contents. Link/name-surrogate rejection belongs to paths.
        // Query streams through this same locked handle, including zero-byte ADS.
        let mut bytes = 4096usize;
        loop {
            // FILE_STREAM_INFO requires eight-byte alignment.
            let mut buffer = vec![0u64; bytes / size_of::<u64>()];
            let result = unsafe {
                GetFileInformationByHandleEx(
                    HANDLE(file.as_raw_handle()),
                    FileStreamInfo,
                    buffer.as_mut_ptr().cast(),
                    bytes as u32,
                )
            };
            match result {
                Ok(()) => {
                    let data =
                        unsafe { std::slice::from_raw_parts(buffer.as_ptr().cast::<u8>(), bytes) };
                    return validate_stream_records(data);
                }
                Err(error)
                    if (error.code() == HRESULT::from_win32(ERROR_MORE_DATA.0)
                        || error.code() == HRESULT::from_win32(ERROR_INSUFFICIENT_BUFFER.0))
                        && bytes < 16 * 1024 * 1024 =>
                {
                    bytes *= 2;
                }
                Err(error) => {
                    return Err(format!(
                        "Cannot enumerate all file streams; cleanup refused: {error}"
                    ));
                }
            }
        }
    }

    fn validate_stream_records(data: &[u8]) -> Result<()> {
        let header = offset_of!(FILE_STREAM_INFO, StreamName);
        let unnamed: Vec<u16> = "::$DATA".encode_utf16().collect();
        let mut position = 0usize;
        let mut unnamed_seen = false;
        loop {
            let record = data
                .get(position..)
                .ok_or("Invalid stream metadata; cleanup refused.")?;
            if record.len() < header {
                return Err("Incomplete stream metadata; cleanup refused.".into());
            }
            let next = u32::from_ne_bytes(record[0..4].try_into().unwrap()) as usize;
            let name_len = u32::from_ne_bytes(record[4..8].try_into().unwrap()) as usize;
            let end = header
                .checked_add(name_len)
                .ok_or("Invalid stream metadata length.")?;
            if !name_len.is_multiple_of(2) || end > record.len() {
                return Err("Invalid stream metadata length; cleanup refused.".into());
            }
            let name: Vec<u16> = record[header..end]
                .chunks_exact(2)
                .map(|pair| u16::from_ne_bytes([pair[0], pair[1]]))
                .collect();
            if name != unnamed {
                return Err(
                    "File has an alternate or unsupported data stream; cleanup refused.".into(),
                );
            }
            if unnamed_seen {
                return Err("Unexpected duplicate stream metadata; cleanup refused.".into());
            }
            unnamed_seen = true;
            if next == 0 {
                return Ok(());
            }
            if next < end || !next.is_multiple_of(8) || next >= record.len() {
                return Err("Invalid stream metadata offset; cleanup refused.".into());
            }
            position = position
                .checked_add(next)
                .ok_or("Invalid stream metadata offset.")?;
        }
    }

    fn wide_path(path: &Path) -> Result<Vec<u16>> {
        if !path.is_absolute() {
            return Err("Cleanup requires an absolute path.".into());
        }
        let mut wide: Vec<u16> = path.as_os_str().encode_wide().collect();
        if wide.contains(&0) || wide.len() >= 32767 {
            return Err("Cleanup path contains a null character or is too long.".into());
        }
        wide.push(0);
        Ok(wide)
    }

    fn shell_path(path: &Path) -> Result<Vec<u16>> {
        let raw = path.to_str().ok_or("Cleanup path is not valid Unicode.")?;
        let ordinary = if let Some(unc) = raw.strip_prefix("\\\\?\\UNC\\") {
            format!("\\\\{unc}")
        } else {
            raw.strip_prefix("\\\\?\\").unwrap_or(raw).to_owned()
        };
        wide_path(Path::new(&ordinary))
    }

    pub fn is_network(path: &Path) -> Result<bool> {
        let wide = wide_path(path)?;
        if matches!(
            path.components().next(),
            Some(Component::Prefix(prefix))
                if matches!(prefix.kind(), Prefix::UNC(_, _) | Prefix::VerbatimUNC(_, _))
        ) {
            return Ok(true);
        }
        let mut volume = vec![0u16; 32768];
        unsafe { GetVolumePathNameW(PCWSTR(wide.as_ptr()), &mut volume) }
            .map_err(|e| format!("Cannot determine source volume; cleanup refused: {e}"))?;
        match unsafe { GetDriveTypeW(PCWSTR(volume.as_ptr())) } {
            4 => Ok(true),              // DRIVE_REMOTE includes mapped network drives.
            2 | 3 | 5 | 6 => Ok(false), // Removable, fixed, CD-ROM, RAM disk.
            _ => Err("Cannot classify source drive; cleanup refused.".into()),
        }
    }

    #[derive(Default)]
    struct RecycleOutcome {
        pre_checked: bool,
        refused: bool,
        posts: usize,
        recycled: bool,
        item_result: Option<HRESULT>,
        finish_result: Option<HRESULT>,
    }

    #[implement(IFileOperationProgressSink)]
    struct RecycleSink {
        outcome: Arc<Mutex<RecycleOutcome>>,
    }

    #[allow(non_snake_case)]
    impl IFileOperationProgressSink_Impl for RecycleSink_Impl {
        fn StartOperations(&self) -> windows::core::Result<()> {
            Ok(())
        }
        fn FinishOperations(&self, result: HRESULT) -> windows::core::Result<()> {
            self.outcome
                .lock()
                .map_err(|_| Error::from_hresult(E_ABORT))?
                .finish_result = Some(result);
            Ok(())
        }
        fn PreDeleteItem(&self, flags: u32, item: Ref<IShellItem>) -> windows::core::Result<()> {
            let mut outcome = self
                .outcome
                .lock()
                .map_err(|_| Error::from_hresult(E_ABORT))?;
            if flags & TSF_DELETE_RECYCLE_IF_POSSIBLE.0 as u32 == 0 || item.is_null() {
                outcome.refused = true;
                return Err(Error::from_hresult(E_ABORT));
            }
            outcome.pre_checked = true;
            Ok(())
        }
        fn PostDeleteItem(
            &self,
            flags: u32,
            _item: Ref<IShellItem>,
            result: HRESULT,
            recycled: Ref<IShellItem>,
        ) -> windows::core::Result<()> {
            let mut outcome = self
                .outcome
                .lock()
                .map_err(|_| Error::from_hresult(E_ABORT))?;
            outcome.posts += 1;
            outcome.item_result = Some(result);
            outcome.recycled = result.is_ok()
                && !recycled.is_null()
                && flags & TSF_DELETE_RECYCLE_IF_POSSIBLE.0 as u32 != 0;
            if !outcome.recycled {
                return Err(Error::from_hresult(E_ABORT));
            }
            Ok(())
        }
        fn PreRenameItem(
            &self,
            _flags: u32,
            _item: Ref<IShellItem>,
            _name: &PCWSTR,
        ) -> windows::core::Result<()> {
            Ok(())
        }
        fn PostRenameItem(
            &self,
            _flags: u32,
            _item: Ref<IShellItem>,
            _name: &PCWSTR,
            _result: HRESULT,
            _new: Ref<IShellItem>,
        ) -> windows::core::Result<()> {
            Ok(())
        }
        fn PreMoveItem(
            &self,
            _flags: u32,
            _item: Ref<IShellItem>,
            _folder: Ref<IShellItem>,
            _name: &PCWSTR,
        ) -> windows::core::Result<()> {
            Ok(())
        }
        fn PostMoveItem(
            &self,
            _flags: u32,
            _item: Ref<IShellItem>,
            _folder: Ref<IShellItem>,
            _name: &PCWSTR,
            _result: HRESULT,
            _new: Ref<IShellItem>,
        ) -> windows::core::Result<()> {
            Ok(())
        }
        fn PreCopyItem(
            &self,
            _flags: u32,
            _item: Ref<IShellItem>,
            _folder: Ref<IShellItem>,
            _name: &PCWSTR,
        ) -> windows::core::Result<()> {
            Ok(())
        }
        fn PostCopyItem(
            &self,
            _flags: u32,
            _item: Ref<IShellItem>,
            _folder: Ref<IShellItem>,
            _name: &PCWSTR,
            _result: HRESULT,
            _new: Ref<IShellItem>,
        ) -> windows::core::Result<()> {
            Ok(())
        }
        fn PreNewItem(
            &self,
            _flags: u32,
            _folder: Ref<IShellItem>,
            _name: &PCWSTR,
        ) -> windows::core::Result<()> {
            Ok(())
        }
        fn PostNewItem(
            &self,
            _flags: u32,
            _folder: Ref<IShellItem>,
            _name: &PCWSTR,
            _template: &PCWSTR,
            _attributes: u32,
            _result: HRESULT,
            _new: Ref<IShellItem>,
        ) -> windows::core::Result<()> {
            Ok(())
        }
        fn UpdateProgress(&self, _total: u32, _so_far: u32) -> windows::core::Result<()> {
            Ok(())
        }
        fn ResetTimer(&self) -> windows::core::Result<()> {
            Ok(())
        }
        fn PauseTimer(&self) -> windows::core::Result<()> {
            Ok(())
        }
        fn ResumeTimer(&self) -> windows::core::Result<()> {
            Ok(())
        }
    }

    struct ComApartment;
    impl Drop for ComApartment {
        fn drop(&mut self) {
            unsafe { CoUninitialize() }
        }
    }

    /// Caller must first journal and stage the verified file, release its source
    /// delete-denying handle, and retain the verified destination lock. A failure
    /// never authorizes deleting the staged file; caller restores or retains it.
    pub fn recycle(path: &Path) -> Result<()> {
        if is_network(path)? {
            return Err("Network files cannot use the Windows Recycle Bin.".into());
        }
        let wide = shell_path(path)?;
        std::thread::Builder::new()
            .name("folderbridge-recycle".into())
            .spawn(move || recycle_sta(&wide))
            .map_err(|e| format!("Cannot start Recycle Bin operation: {e}"))?
            .join()
            .map_err(|_| {
                "Recycle Bin operation stopped unexpectedly; inspect the cleanup journal."
                    .to_string()
            })?
    }

    fn recycle_sta(wide: &[u16]) -> Result<()> {
        unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }
            .ok()
            .map_err(|e| format!("Cannot initialize Windows Recycle Bin: {e}"))?;
        let _apartment = ComApartment;
        let operation: IFileOperation =
            unsafe { CoCreateInstance(&FileOperation, None, CLSCTX_INPROC_SERVER) }
                .map_err(|e| format!("Cannot open Windows Recycle Bin operation: {e}"))?;
        unsafe {
            operation.SetOperationFlags(
                FOFX_RECYCLEONDELETE
                    | FOFX_EARLYFAILURE
                    | FOF_NOERRORUI
                    | FOF_SILENT
                    | FOF_NOCONFIRMATION
                    | FOF_NO_CONNECTED_ELEMENTS,
            )
        }
        .map_err(|e| format!("Cannot enforce recycle-only operation: {e}"))?;
        let item: IShellItem = unsafe { SHCreateItemFromParsingName(PCWSTR(wide.as_ptr()), None) }
            .map_err(|e| format!("Cannot locate staged file for recycling: {e}"))?;
        let outcome = Arc::new(Mutex::new(RecycleOutcome::default()));
        let sink: IFileOperationProgressSink = RecycleSink {
            outcome: outcome.clone(),
        }
        .into();
        unsafe { operation.DeleteItem(&item, &sink) }
            .map_err(|e| format!("Cannot queue recycle-only operation: {e}"))?;
        let performed = unsafe { operation.PerformOperations() };
        // Windows can return success when an operation was silently aborted.
        // Query this even when PerformOperations failed.
        let aborted = unsafe { operation.GetAnyOperationsAborted() };
        let outcome = outcome
            .lock()
            .map_err(|_| "Cannot verify Recycle Bin outcome.".to_string())?;
        if outcome.refused {
            return Err("Windows cannot recycle this file. Permanent deletion was refused.".into());
        }
        performed.map_err(|e| format!("Windows Recycle Bin operation failed: {e}"))?;
        if aborted
            .map_err(|e| format!("Cannot verify Recycle Bin completion: {e}"))?
            .as_bool()
        {
            return Err("Windows Recycle Bin operation was aborted.".into());
        }
        if !outcome.pre_checked
            || outcome.posts != 1
            || !outcome.recycled
            || !outcome.item_result.is_some_and(|result| result.is_ok())
            || outcome.finish_result.is_some_and(|result| result.is_err())
        {
            return Err("Windows did not confirm that the staged file entered the Recycle Bin. Inspect the cleanup journal.".into());
        }
        Ok(())
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use std::fs;

        #[test]
        fn shell_paths_remove_verbatim_prefix() {
            let drive = shell_path(Path::new(r"\\?\E:\Folder\file.txt")).unwrap();
            let unc = shell_path(Path::new(r"\\?\UNC\server\share\file.txt")).unwrap();
            let decode = |wide: Vec<u16>| String::from_utf16(&wide[..wide.len() - 1]).unwrap();
            assert_eq!(decode(drive), r"E:\Folder\file.txt");
            assert_eq!(decode(unc), r"\\server\share\file.txt");
        }

        #[test]
        fn vetoes_delete_without_recycle_flag() {
            let outcome = Arc::new(Mutex::new(RecycleOutcome::default()));
            let sink: IFileOperationProgressSink = RecycleSink {
                outcome: outcome.clone(),
            }
            .into();
            let error = unsafe { sink.PreDeleteItem(0, None::<&IShellItem>) }.unwrap_err();
            assert_eq!(error.code(), E_ABORT);
            assert!(outcome.lock().unwrap().refused);
        }

        #[test]
        fn identity_survives_rename_and_distinguishes_replacement() {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("original.txt");
            fs::write(&path, b"same contents").unwrap();
            let before = identity(&File::open(&path).unwrap()).unwrap();
            let moved = dir.path().join("retained.txt");
            fs::rename(&path, &moved).unwrap();
            assert_eq!(before, identity(&File::open(&moved).unwrap()).unwrap());
            fs::write(&path, b"same contents").unwrap();
            assert_ne!(before, identity(&File::open(&path).unwrap()).unwrap());
        }

        #[test]
        fn accepts_plain_files_including_empty() {
            let dir = tempfile::tempdir().unwrap();
            for (name, content) in [("empty.txt", &b""[..]), ("plain.txt", &b"plain"[..])] {
                let path = dir.path().join(name);
                fs::write(&path, content).unwrap();
                validate_content_only(&File::open(&path).unwrap()).unwrap();
            }
        }

        #[test]
        fn rejects_named_streams_including_empty() {
            let dir = tempfile::tempdir().unwrap();
            for (name, content) in [("empty.txt", &b""[..]), ("full.txt", &b"hidden"[..])] {
                let path = dir.path().join(name);
                fs::write(&path, b"main data").unwrap();
                let mut stream = path.as_os_str().to_os_string();
                stream.push(":extra");
                fs::write(&stream, content).unwrap();
                assert!(validate_content_only(&File::open(&path).unwrap()).is_err());
            }
        }

        #[test]
        fn malformed_stream_metadata_fails_closed() {
            assert!(validate_stream_records(&[]).is_err());
            assert!(validate_stream_records(&[0; 32]).is_err());
            let mut bad = vec![0; 64];
            bad[4..8].copy_from_slice(&u32::MAX.to_ne_bytes());
            assert!(validate_stream_records(&bad).is_err());
        }

        #[test]
        fn unc_paths_are_network_without_connecting() {
            assert!(is_network(Path::new(r"\\server\share\folder")).unwrap());
            assert!(is_network(Path::new(r"\\?\UNC\server\share\folder")).unwrap());
            assert!(is_network(Path::new("relative/path")).is_err());
        }
    }
}
