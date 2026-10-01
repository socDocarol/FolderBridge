use super::*;
use crate::cleanup_native;
use std::path::PathBuf;

struct ItemPaths {
    original_root: PathBuf,
    target_root: PathBuf,
    original: PathBuf,
    target: PathBuf,
}

struct VerifiedItem {
    paths: ItemPaths,
    original: File,
    _target: File,
    _parents: Vec<File>,
}

fn copy_action(action: &str) -> bool {
    matches!(
        action,
        "copy_to_destination" | "copy_to_source" | "keep_both"
    )
}

fn roots_for(store: &Store, operation: &Operation) -> Result<(PathBuf, PathBuf)> {
    let scan = store.scan(operation.scan_id)?;
    let (source, destination) = validate_roots(
        Path::new(&scan.source),
        scan.destination.as_deref().map(Path::new),
    )?;
    let destination = destination.ok_or("Copy destination is unavailable")?;
    Ok(if operation.action == "copy_to_source" {
        (destination, source)
    } else {
        (source, destination)
    })
}

fn paths_for(store: &Store, operation: &Operation, item: &OperationItem) -> Result<ItemPaths> {
    if !copy_action(&operation.action) || item.operation_id != operation.id {
        return Err("This item is not part of a copy operation.".into());
    }
    let (original_root, target_root) = roots_for(store, operation)?;
    let original = within_root(&original_root, &item.relative_path)?;
    let target_relative = if operation.action == "keep_both" {
        let p = Path::new(&item.relative_path);
        let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("file");
        let extension = p
            .extension()
            .and_then(|s| s.to_str())
            .map(|s| format!(".{s}"))
            .unwrap_or_default();
        p.with_file_name(format!("{stem} (source copy {}){extension}", operation.id))
            .to_string_lossy()
            .replace('\\', "/")
    } else {
        item.relative_path.clone()
    };
    let target = within_root(&target_root, &target_relative)?;
    if Path::new(&item.original_path) != original || Path::new(&item.target_path) != target {
        return Err("Stored copy paths disagree with the scan and operation.".into());
    }
    Ok(ItemPaths {
        original_root,
        target_root,
        original,
        target,
    })
}

pub fn operation_item_path(
    store: &Store,
    operation_id: i64,
    item_id: i64,
    location: &str,
) -> Result<PathBuf> {
    let operation = store.operation(operation_id)?;
    let item = store
        .operation_items(operation_id)?
        .into_iter()
        .find(|item| item.id == item_id)
        .ok_or("Operation item not found")?;
    match location {
        "original" => Ok(original_for_restore(store, &operation, &item)?.1),
        "target" => Ok(paths_for(store, &operation, &item)?.target),
        "holding" => {
            let cleanup = item
                .cleanup
                .as_ref()
                .ok_or("This item has no holding location")?;
            let (root, _) = original_for_restore(store, &operation, &item)?;
            let expected =
                holding_under(&root, operation_id, &item.relative_path, &cleanup.method)?;
            if Path::new(&cleanup.holding_path) != expected {
                return Err("Holding path does not match this operation.".into());
            }
            Ok(expected)
        }
        _ => Err("Choose original, target, or holding.".into()),
    }
}

fn holding_path(
    paths: &ItemPaths,
    operation_id: i64,
    relative: &str,
    method: &str,
) -> Result<PathBuf> {
    holding_under(&paths.original_root, operation_id, relative, method)
}

fn holding_under(root: &Path, operation_id: i64, relative: &str, method: &str) -> Result<PathBuf> {
    let folder = match method {
        "network" => format!("_ToDelete/{operation_id}/{relative}"),
        "local" | "sharepoint" => format!("{STAGING}/cleanup-{operation_id}/{relative}"),
        _ => return Err("Unknown cleanup method.".into()),
    };
    within_root(root, &folder)
}

fn original_for_restore(
    store: &Store,
    operation: &Operation,
    item: &OperationItem,
) -> Result<(PathBuf, PathBuf)> {
    if !copy_action(&operation.action) || item.operation_id != operation.id {
        return Err("This item is not part of a copy operation.".into());
    }
    let scan = store.scan(operation.scan_id)?;
    let raw = if operation.action == "copy_to_source" {
        scan.destination
            .as_deref()
            .ok_or("Original root is unavailable")?
    } else {
        &scan.source
    };
    let root = fs::canonicalize(raw).map_err(|e| format!("Original root is unavailable: {e}"))?;
    let original = within_root(&root, &item.relative_path)?;
    if Path::new(&item.original_path) != original {
        return Err("Stored original path disagrees with scan.".into());
    }
    Ok((root, original))
}

fn proof(conn: &Connection, id: i64) -> Result<(String, Option<String>)> {
    let record: Option<(Option<String>, Option<String>)> = conn
        .query_row(
            "SELECT identity,issue FROM copy_proofs WHERE item_id=?",
            [id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(db_error)?;
    match record {
        None => Err("Legacy copy has no cleanup proof; original retained.".into()),
        Some((None, issue)) => Err(issue.unwrap_or_else(|| "Cleanup proof is incomplete.".into())),
        Some((Some(identity), issue)) => Ok((identity, issue)),
    }
}

#[cfg(windows)]
fn pin_parents(root: &Path, path: &Path) -> Result<Vec<File>> {
    use std::os::windows::fs::OpenOptionsExt;
    let parent = path.parent().ok_or("Missing file parent")?;
    let relative = parent
        .strip_prefix(root)
        .map_err(|_| "File escaped its root")?;
    let mut chain: Vec<PathBuf> = root
        .ancestors()
        .filter(|p| !p.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .collect();
    chain.reverse();
    let mut next = root.to_path_buf();
    for part in relative.components() {
        next.push(part);
        chain.push(next.clone());
    }
    chain
        .into_iter()
        .map(|folder| {
            let mut options = OpenOptions::new();
            // Child creation/renames need directory write sharing. Deny delete sharing
            // so the directory itself cannot be renamed out from under the path.
            options
                .read(true)
                .share_mode(1 | 2)
                .custom_flags(0x02000000);
            options
                .open(&folder)
                .map_err(|e| format!("Cannot lock folder {}: {e}", folder.display()))
        })
        .collect()
}

#[cfg(not(windows))]
fn pin_parents(_root: &Path, _path: &Path) -> Result<Vec<File>> {
    Err("Original cleanup requires Windows directory locks.".into())
}

impl Engine {
    fn verify_cleanup_item(
        &self,
        operation: &Operation,
        item: &OperationItem,
        conn: &Connection,
        verify_contents: bool,
    ) -> Result<VerifiedItem> {
        if item.state != "copied"
            || item
                .cleanup
                .as_ref()
                .is_some_and(|c| !matches!(c.state.as_str(), "failed" | "restored"))
        {
            return Err("Only copied originals without pending cleanup are eligible.".into());
        }
        let (expected_identity, issue) = proof(conn, item.id)?;
        if let Some(issue) = issue {
            return Err(issue);
        }
        let expected_hash = item
            .content_hash
            .as_deref()
            .ok_or("Copy has no verification hash")?;
        let paths = paths_for(&self.store, operation, item)?;
        if let Some(previous) = &item.cleanup {
            let old_holding =
                holding_path(&paths, operation.id, &item.relative_path, &previous.method)?;
            match fs::symlink_metadata(old_holding) {
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                _ => return Err(
                    "Previous holding location exists or is inaccessible; review before retrying."
                        .into(),
                ),
            }
        }
        reject_open_database(&paths.original)?;
        reject_open_database(&paths.target)?;
        let mut parents = pin_parents(&paths.original_root, &paths.original)?;
        parents.extend(pin_parents(&paths.target_root, &paths.target)?);
        // Recheck the full path while all ancestors are pinned.
        if within_root(&paths.original_root, &item.relative_path)? != paths.original {
            return Err("Original path changed while locking folders.".into());
        }
        within_root(
            &paths.target_root,
            &relative_text(&paths.target_root, &paths.target)?,
        )?;
        let mut original = movable_read(&paths.original)?;
        let mut target = locked_read(&paths.target)?;
        cleanup_native::validate_content_only(&original)?;
        cleanup_native::validate_content_only(&target)?;
        if cleanup_native::identity(&original)? != expected_identity {
            return Err("Original file was replaced after copying; retained.".into());
        }
        if original.metadata().map_err(|e| e.to_string())?.len()
            != target.metadata().map_err(|e| e.to_string())?.len()
            || (verify_contents
                && (self.hash_open(&mut original)? != expected_hash
                    || self.hash_open(&mut target)? != expected_hash))
        {
            return Err("Original or copied file changed; original retained.".into());
        }
        Ok(VerifiedItem {
            paths,
            original,
            _target: target,
            _parents: parents,
        })
    }

    pub fn preview_cleanup(&self, operation_id: i64) -> Result<CleanupPreview> {
        let _guard = self.begin("cleanup_preview")?;
        let operation = self.store.operation(operation_id)?;
        if !copy_action(&operation.action) || operation.state == "running" {
            return Err("Choose a finished copy operation for original cleanup.".into());
        }
        let (original_root, target_root) = roots_for(&self.store, &operation)?;
        let is_network = cleanup_native::is_network(&original_root)?;
        let conn = self.store.connect()?;
        let mut items = Vec::new();
        for item in self.store.operation_items(operation_id)? {
            self.check_cancel()?;
            let reason = match self.verify_cleanup_item(&operation, &item, &conn, false) {
                Ok(_) => String::new(),
                Err(error) => error,
            };
            items.push(CleanupPreviewItem {
                eligible: reason.is_empty(),
                reason,
                item,
            });
        }
        Ok(CleanupPreview {
            original_root: original_root.to_string_lossy().into(),
            target_root: target_root.to_string_lossy().into(),
            is_network,
            items,
        })
    }

    pub fn cleanup_originals(&self, request: CleanupRequest) -> Result<CleanupSummary> {
        if request.item_ids.is_empty() || request.item_ids.len() > 10000 {
            return Err("Select between 1 and 10,000 copied files.".into());
        }
        let ids: HashSet<i64> = request.item_ids.iter().copied().collect();
        if ids.len() != request.item_ids.len() {
            return Err("Select each file only once.".into());
        }
        if request.confirmation != format!("REMOVE {}", ids.len()) {
            return Err(format!("Type REMOVE {} to confirm cleanup.", ids.len()));
        }
        if !matches!(
            request.handling.as_str(),
            "local" | "sharepoint" | "network"
        ) {
            return Err("Choose local, SharePoint, or network handling.".into());
        }
        if request.handling == "sharepoint" && !request.sharepoint_confirmed {
            return Err("Acknowledge that OneDrive must sync the deletion to SharePoint.".into());
        }
        let _guard = self.begin("cleanup")?;
        let operation = self.store.operation(request.operation_id)?;
        if !copy_action(&operation.action) || operation.state == "running" {
            return Err("Choose a finished copy operation for original cleanup.".into());
        }
        let rows = self.store.operation_items(operation.id)?;
        if ids.iter().any(|id| !rows.iter().any(|row| row.id == *id)) {
            return Err("A selected item does not belong to this operation.".into());
        }
        let (original_root, _) = roots_for(&self.store, &operation)?;
        let is_network = cleanup_native::is_network(&original_root)?;
        if is_network && request.handling != "network" {
            return Err("Network originals must use the _ToDelete holding folder.".into());
        }
        let conn = self.store.connect()?;
        let mut completed = 0;
        let mut skipped = 0;
        self.update(|p| {
            p.operation_id = Some(operation.id);
            p.total = ids.len() as u64;
            p.phase = "checking".into();
        });
        for item in rows.into_iter().filter(|item| ids.contains(&item.id)) {
            if self.check_cancel().is_err() {
                break;
            }
            self.update(|p| p.current_path = item.relative_path.clone());
            let result = self.cleanup_one(&conn, &operation, &item, &request.handling);
            match result {
                Ok(()) => completed += 1,
                Err(error) => {
                    skipped += 1;
                    self.update(|p| {
                        p.errors += 1;
                        p.message = error.clone();
                    });
                    if error.starts_with("Journal") {
                        return Err(error);
                    }
                    // Record failures only if no mutation was claimed. Never overwrite
                    // a pending/held record owned by another process.
                    let holding = holding_under(
                        &original_root,
                        operation.id,
                        &item.relative_path,
                        &request.handling,
                    )
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_default();
                    conn.execute("INSERT INTO migration_cleanup(item_id,method,state,holding_path,message) VALUES(?,?,'failed',?,?) ON CONFLICT(item_id) DO UPDATE SET message=excluded.message WHERE migration_cleanup.state IN ('failed','restored')",
                        params![item.id, request.handling, holding, error]).map_err(|e| format!("Journal failure recording retained original: {e}"))?;
                }
            }
            self.update(|p| p.processed += 1);
        }
        let unprocessed = ids.len() as u64 - completed - skipped;
        let mut message = if request.handling == "sharepoint" {
            format!("{completed} local originals sent to the Recycle Bin, {skipped} skipped. SharePoint deletion awaits OneDrive sync; cloud deletion was not verified.")
        } else if request.handling == "network" {
            format!("{completed} originals moved to _ToDelete, {skipped} skipped. Files remain recoverable there.")
        } else {
            format!("{completed} originals sent to the Recycle Bin, {skipped} skipped.")
        };
        if unprocessed > 0 {
            message.push_str(&format!(" {unprocessed} not processed."));
        }
        self.update(|p| {
            p.phase = if self.cancel.load(Ordering::SeqCst) {
                "cancelled"
            } else if skipped > 0 {
                "needs_review"
            } else {
                "complete"
            }
            .into();
            p.message = message.clone();
        });
        Ok(CleanupSummary {
            completed,
            skipped,
            message,
        })
    }

    fn cleanup_one(
        &self,
        conn: &Connection,
        operation: &Operation,
        item: &OperationItem,
        method: &str,
    ) -> Result<()> {
        let verified = self.verify_cleanup_item(operation, item, conn, true)?;
        let holding = holding_path(&verified.paths, operation.id, &item.relative_path, method)?;
        fs::create_dir_all(holding.parent().ok_or("Missing holding parent")?)
            .map_err(|e| e.to_string())?;
        let holding = holding_path(&verified.paths, operation.id, &item.relative_path, method)?;
        let _holding_parents = pin_parents(&verified.paths.original_root, &holding)?;
        if fs::symlink_metadata(&holding).is_ok() {
            return Err("Holding location is occupied; original retained.".into());
        }
        let claimed = conn.execute("INSERT INTO migration_cleanup(item_id,method,state,holding_path,message) VALUES(?,?,'planned',?,'Verified; move pending') ON CONFLICT(item_id) DO UPDATE SET method=excluded.method,state='planned',holding_path=excluded.holding_path,message=excluded.message WHERE migration_cleanup.state IN ('failed','restored')",
            params![item.id, method, holding.to_string_lossy()]).map_err(|e| format!("Journal could not be saved: {e}"))?;
        if claimed != 1 {
            return Err("Cleanup is already recorded or pending; inspect History.".into());
        }
        if let Err(error) = self.check_cancel() {
            self.set_cleanup_state(
                conn,
                item.id,
                "failed",
                "Cancelled before removal; original retained",
            )?;
            return Err(error);
        }
        if let Err(error) = rename_open_file(&verified.original, &verified.paths.original, &holding)
        {
            self.set_cleanup_state(
                conn,
                item.id,
                "needs_review",
                &format!("Move failed; inspect original and holding locations: {error}"),
            )?;
            return Err(error);
        }
        if method == "network" {
            self.set_cleanup_state(
                conn,
                item.id,
                "held",
                "Original held in _ToDelete; restore is available",
            )?;
            return Ok(());
        }
        // Shell deletion needs a closed file handle. Confirm the staged pathname still
        // names the same held file before releasing it; parent folders stay pinned.
        let expected_identity = cleanup_native::identity(&verified.original)?;
        drop(verified.original);
        let mut staged = movable_read(&holding)?;
        if cleanup_native::identity(&staged)? != expected_identity {
            self.set_cleanup_state(
                conn,
                item.id,
                "needs_review",
                "Staged file identity changed; manual review required",
            )?;
            return Err("Staged file identity changed; manual review required.".into());
        }
        cleanup_native::validate_content_only(&staged)?;
        if self.hash_open(&mut staged)? != item.content_hash.as_deref().unwrap_or_default() {
            self.set_cleanup_state(
                conn,
                item.id,
                "needs_review",
                "Staged file content changed; manual review required",
            )?;
            return Err("Staged file content changed; manual review required.".into());
        }
        drop(staged);
        match cleanup_native::recycle(&holding) {
            Ok(()) => {
                let message = if method == "sharepoint" {
                    "Sent to local Recycle Bin; SharePoint deletion awaits OneDrive sync"
                } else {
                    "Sent to Windows Recycle Bin; restore there to staging before app restore"
                };
                self.set_cleanup_state(conn, item.id, "recycled", message)
            }
            Err(error) => {
                // A shell failure can leave the file staged or in the Bin. Attempt
                // recovery only for the same unchanged file, without overwriting.
                let recovered = (|| -> Result<()> {
                    let mut file = movable_read(&holding)?;
                    if cleanup_native::identity(&file)? != expected_identity
                        || self.hash_open(&mut file)?
                            != item.content_hash.as_deref().unwrap_or_default()
                    {
                        return Err("Staged original changed".into());
                    }
                    rename_open_file(&file, &holding, &verified.paths.original)
                })()
                .is_ok();
                if recovered {
                    self.set_cleanup_state(
                        conn,
                        item.id,
                        "failed",
                        &format!("Recycling refused; original restored: {error}"),
                    )?;
                    return Err(format!("Recycling refused; original restored: {error}"));
                }
                self.set_cleanup_state(
                    conn,
                    item.id,
                    "needs_review",
                    &format!(
                        "Recycle not confirmed: {error}. Inspect holding path and Recycle Bin."
                    ),
                )?;
                Err(error)
            }
        }
    }

    fn set_cleanup_state(
        &self,
        conn: &Connection,
        id: i64,
        state: &str,
        message: &str,
    ) -> Result<()> {
        conn.execute(
            "UPDATE migration_cleanup SET state=?,message=? WHERE item_id=?",
            params![state, message, id],
        )
        .map_err(|e| format!("Journal update failed after cleanup mutation: {e}"))?;
        Ok(())
    }

    pub fn restore_originals(
        &self,
        operation_id: i64,
        item_ids: Vec<i64>,
    ) -> Result<CleanupSummary> {
        if item_ids.is_empty() || item_ids.len() > 10000 {
            return Err("Select between 1 and 10,000 items.".into());
        }
        let ids: HashSet<i64> = item_ids.iter().copied().collect();
        if ids.len() != item_ids.len() {
            return Err("Select each file only once.".into());
        }
        let _guard = self.begin("cleanup_restore")?;
        let operation = self.store.operation(operation_id)?;
        if !copy_action(&operation.action) {
            return Err("Only copied originals can be restored.".into());
        }
        let rows = self.store.operation_items(operation_id)?;
        if ids.iter().any(|id| !rows.iter().any(|row| row.id == *id)) {
            return Err("A selected item does not belong to this operation.".into());
        }
        let conn = self.store.connect()?;
        let mut completed = 0;
        let mut skipped = 0;
        self.update(|p| {
            p.operation_id = Some(operation_id);
            p.total = ids.len() as u64;
            p.phase = "restoring".into();
        });
        for item in rows.into_iter().filter(|row| ids.contains(&row.id)) {
            if self.check_cancel().is_err() {
                break;
            }
            self.update(|p| p.current_path = item.relative_path.clone());
            match self.restore_original_one(&conn, &operation, &item) {
                Ok(()) => completed += 1,
                Err(error) => {
                    skipped += 1;
                    self.update(|p| {
                        p.message = error.clone();
                        p.errors += 1;
                    });
                    if error.starts_with("Journal") {
                        return Err(error);
                    }
                    conn.execute(
                        "UPDATE migration_cleanup SET message=? WHERE item_id=?",
                        params![format!("Restore skipped: {error}"), item.id],
                    )
                    .map_err(|e| format!("Journal error recording restore: {e}"))?;
                }
            }
            self.update(|p| p.processed += 1);
        }
        let unprocessed = ids.len() as u64 - completed - skipped;
        let message = format!("{completed} originals restored, {skipped} skipped, {unprocessed} not processed. Check History for details.");
        self.update(|p| {
            p.phase = if self.cancel.load(Ordering::SeqCst) {
                "cancelled"
            } else {
                "complete"
            }
            .into();
            p.message = message.clone();
        });
        Ok(CleanupSummary {
            completed,
            skipped,
            message,
        })
    }

    fn restore_original_one(
        &self,
        conn: &Connection,
        operation: &Operation,
        item: &OperationItem,
    ) -> Result<()> {
        let cleanup = item
            .cleanup
            .as_ref()
            .ok_or("No cleanup record for this item")?;
        if cleanup.state == "restored" {
            return Err("Original was already restored.".into());
        }
        let (original_root, original) = original_for_restore(&self.store, operation, item)?;
        let holding = holding_under(
            &original_root,
            operation.id,
            &item.relative_path,
            &cleanup.method,
        )?;
        if Path::new(&cleanup.holding_path) != holding {
            return Err("Holding path does not match this operation.".into());
        }
        let (expected_identity, _) = proof(conn, item.id)?;
        fs::create_dir_all(original.parent().ok_or("Missing original parent")?)
            .map_err(|e| e.to_string())?;
        within_root(&original_root, &item.relative_path)?;
        let mut parents = pin_parents(&original_root, &original)?;
        parents.extend(pin_parents(&original_root, &holding)?);
        if fs::symlink_metadata(&original).is_ok() {
            return Err("Original location is occupied; holding file retained.".into());
        }
        let mut file = movable_read(&holding).map_err(|_| "Holding file unavailable. Restore it from the Recycle Bin to the staging path first.".to_string())?;
        if cleanup_native::identity(&file)? != expected_identity {
            return Err("Holding file identity changed; retained for review.".into());
        }
        cleanup_native::validate_content_only(&file)?;
        if self.hash_open(&mut file)? != item.content_hash.as_deref().unwrap_or_default() {
            return Err("Holding file contents changed; retained for review.".into());
        }
        conn.execute("UPDATE migration_cleanup SET state='restoring',message='Restore pending' WHERE item_id=?", [item.id])
            .map_err(|e| format!("Journal could not be saved: {e}"))?;
        rename_open_file(&file, &holding, &original)?;
        self.set_cleanup_state(
            conn,
            item.id,
            "restored",
            "Original restored without overwriting",
        )?;
        Ok(())
    }
}
