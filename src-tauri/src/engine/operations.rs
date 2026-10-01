use super::*;

impl Engine {
    pub fn preview(&self, request: &OperationRequest) -> Result<Preview> {
        let scan = self.store.scan(request.scan_id)?;
        if scan.state != "complete" {
            return Err("Only complete scans can be used for file operations. Resolve issues and scan again.".into());
        }
        if scan.destination.is_none() {
            return Err("Choose a destination and compare both folders first.".into());
        }
        if request.entry_ids.is_empty() || request.entry_ids.len() > 10000 {
            return Err("Select between 1 and 10,000 files per operation.".into());
        }
        let description=match request.action.as_str() {
            "copy_to_destination"=>"Copy missing source files to the destination. Existing files are preserved.",
            "copy_to_source"=>"Recover destination-only files to the source. Existing files are preserved.",
            "keep_both"=>"Copy source versions with a new name beside conflicting destination files. Both versions remain.",
            "quarantine_destination"=>"Move verified destination duplicates into its .folderbridge-quarantine folder. Source files remain. Restore is available in History. This does not free disk space.",
            _=>return Err("Unknown file operation.".into())
        };
        let mut preview = Preview {
            action: request.action.clone(),
            eligible: 0,
            skipped: 0,
            bytes: 0,
            description: description.into(),
            review_entry_ids: Vec::new(),
            eligible_entry_ids: Vec::new(),
        };
        let mut seen = HashSet::new();
        let conn = self.store.connect()?;
        for id in &request.entry_ids {
            if !seen.insert(id) {
                continue;
            }
            let entry = Store::entry(&conn, request.scan_id, *id)?;
            if eligible(&entry, &request.action) {
                preview.eligible += 1;
                preview.eligible_entry_ids.push(entry.id);
                if entry.rule_review || request.action == "keep_both" {
                    preview.review_entry_ids.push(entry.id);
                }
                preview.bytes += if request.action == "copy_to_source"
                    || request.action == "quarantine_destination"
                {
                    entry.destination_size.unwrap_or(0)
                } else {
                    entry.source_size.unwrap_or(0)
                };
            } else {
                preview.skipped += 1;
            }
        }
        Ok(preview)
    }

    pub fn execute(&self, request: OperationRequest) -> Result<i64> {
        let _guard = self.begin("operation")?;
        let preview = self.preview(&request)?;
        if preview.eligible == 0 {
            return Err("None of the selected files are eligible for this action.".into());
        }
        let approved: HashSet<i64> = request.approved_entry_ids.iter().copied().collect();
        if preview
            .review_entry_ids
            .iter()
            .any(|id| !approved.contains(id))
        {
            return Err("Some selected files require review. Approve their entry IDs before starting this operation.".into());
        }
        let scan = self.store.scan(request.scan_id)?;
        let (source, destination) = validate_roots(
            Path::new(&scan.source),
            scan.destination.as_deref().map(Path::new),
        )?;
        let destination = destination.ok_or("Missing destination")?;
        let conn = self.store.connect()?;
        conn.execute(
            "INSERT INTO operations(scan_id,action,started_at,state) VALUES(?,?,?,'running')",
            params![scan.id, request.action, now()],
        )
        .map_err(db_error)?;
        let operation = conn.last_insert_rowid();
        self.update(|p| {
            p.operation_id = Some(operation);
            p.scan_id = Some(scan.id);
            p.phase = "applying".into();
            p.total = preview.eligible + preview.skipped;
        });
        let mut seen = HashSet::new();
        let mut completed = 0u64;
        let mut skipped = 0u64;
        let mut errors = 0u64;
        for id in request.entry_ids {
            if self.check_cancel().is_err() {
                break;
            }
            if !seen.insert(id) {
                continue;
            }
            let entry = Store::entry(&conn, scan.id, id)?;
            self.update(|p| p.current_path = entry.relative_path.clone());
            if !eligible(&entry, &request.action) {
                skipped += 1;
                self.journal(
                    &conn,
                    operation,
                    &entry.relative_path,
                    "",
                    "",
                    "skipped",
                    "Not eligible for this action",
                    None,
                )?;
                self.update(|p| p.processed += 1);
                continue;
            }
            let result = self.apply_entry(
                &conn,
                operation,
                &entry,
                &request.action,
                &source,
                &destination,
            );
            match result {
                Ok(()) => completed += 1,
                Err(error) => {
                    if error == "Cancelled" {
                        break;
                    }
                    // Unsafe or changed items are retained and recorded, never forced.
                    skipped += 1;
                    self.journal(
                        &conn,
                        operation,
                        &entry.relative_path,
                        "",
                        "",
                        "skipped",
                        &error,
                        None,
                    )?;
                    if error.starts_with("Journal") {
                        errors += 1;
                        break;
                    }
                }
            }
            conn.execute(
                "UPDATE operations SET completed=?,skipped=?,errors=? WHERE id=?",
                params![completed as i64, skipped as i64, errors as i64, operation],
            )
            .map_err(db_error)?;
            self.update(|p| {
                p.processed += 1;
                p.errors = errors;
            });
        }
        let state = if self.cancel.load(Ordering::SeqCst) {
            "cancelled"
        } else if errors > 0 || skipped > 0 {
            "completed_with_issues"
        } else {
            "complete"
        };
        conn.execute(
            "UPDATE operations SET state=?,completed=?,skipped=?,errors=? WHERE id=?",
            params![
                state,
                completed as i64,
                skipped as i64,
                errors as i64,
                operation
            ],
        )
        .map_err(db_error)?;
        self.update(|p| {
            p.phase = state.into();
            p.message = format!(
                "{completed} completed, {skipped} skipped. Rescan to refresh the comparison."
            );
        });
        Ok(operation)
    }

    // Keep the database insert fields explicit at each mutation boundary.
    #[allow(clippy::too_many_arguments)]
    fn journal(
        &self,
        conn: &Connection,
        operation: i64,
        relative: &str,
        original: &str,
        target: &str,
        state: &str,
        message: &str,
        hash: Option<&str>,
    ) -> Result<i64> {
        conn.execute("INSERT INTO operation_items(operation_id,relative_path,original_path,target_path,state,message,content_hash) VALUES(?,?,?,?,?,?,?)",params![operation,relative,original,target,state,message,hash]).map_err(|e|format!("Journal could not be saved: {e}"))?;
        Ok(conn.last_insert_rowid())
    }

    fn apply_entry(
        &self,
        conn: &Connection,
        operation: i64,
        entry: &Entry,
        action: &str,
        source: &Path,
        destination: &Path,
    ) -> Result<()> {
        let from_source = action != "copy_to_source" && action != "quarantine_destination";
        let (from_root, to_root, relative, size, modified) = if from_source {
            (
                source,
                destination,
                entry.source_relative.as_deref(),
                entry.source_size,
                entry.source_modified,
            )
        } else {
            (
                destination,
                source,
                entry.destination_relative.as_deref(),
                entry.destination_size,
                entry.destination_modified,
            )
        };
        let relative = relative.ok_or("Missing file path")?;
        let from = within_root(from_root, relative)?;
        check_snapshot(&from, size, modified)?;
        if action == "quarantine_destination" {
            let reference = within_root(
                source,
                entry
                    .source_relative
                    .as_deref()
                    .ok_or("Missing reference")?,
            )?;
            check_snapshot(&reference, entry.source_size, entry.source_modified)?;
            reject_open_database(&from)?;
            reject_open_database(&reference)?;
            let mut reference_guard = locked_read(&reference)?;
            let mut destination_guard = movable_read(&from)?;
            check_snapshot(&reference, entry.source_size, entry.source_modified)?;
            check_snapshot(&from, size, modified)?;
            let source_hash = self.hash_open(&mut reference_guard)?;
            let hash = self.hash_open(&mut destination_guard)?;
            if source_hash != hash
                || entry.source_hash.as_deref() != Some(&hash)
                || entry.destination_hash.as_deref() != Some(&hash)
            {
                return Err("Contents changed or backup differs. Both files were retained.".into());
            }
            let quarantine_relative = format!("{QUARANTINE}/{operation}/{relative}");
            let target = within_root(destination, &quarantine_relative)?;
            fs::create_dir_all(target.parent().ok_or("Missing parent")?)
                .map_err(|e| e.to_string())?;
            let target = within_root(destination, &quarantine_relative)?;
            let journal = self.journal(
                conn,
                operation,
                relative,
                &from.to_string_lossy(),
                &target.to_string_lossy(),
                "planned",
                "Verified duplicate; quarantine pending",
                Some(&hash),
            )?;
            self.check_cancel()?;
            // Keep handles open while renaming on Windows to exclude concurrent writers.

            check_snapshot(&reference, entry.source_size, entry.source_modified)?;
            check_snapshot(&from, size, modified)?;
            rename_open_file(&destination_guard, &from, &target)?;
            conn.execute("UPDATE operation_items SET state='quarantined',message='Duplicate retained in quarantine; restore available' WHERE id=?",[journal]).map_err(|e|format!("Journal update failed after quarantine: {e}"))?;
            return Ok(());
        }
        let target_relative = if action == "keep_both" {
            let p = Path::new(relative);
            let name = p.file_stem().and_then(|s| s.to_str()).unwrap_or("file");
            let extension = p
                .extension()
                .and_then(|s| s.to_str())
                .map(|s| format!(".{s}"))
                .unwrap_or_default();
            p.with_file_name(format!("{name} (source copy {operation}){extension}"))
                .to_string_lossy()
                .replace('\\', "/")
        } else {
            relative.into()
        };
        let target = within_root(to_root, &target_relative)?;
        if fs::symlink_metadata(&target).is_ok() {
            return Err("Destination already exists; it was preserved.".into());
        }
        fs::create_dir_all(target.parent().ok_or("Missing parent")?).map_err(|e| e.to_string())?;
        let target = within_root(to_root, &target_relative)?;
        let journal = self.journal(
            conn,
            operation,
            relative,
            &from.to_string_lossy(),
            &target.to_string_lossy(),
            "planned",
            "Verified copy pending",
            None,
        )?;
        let proof = self.copy_verified(&from, &target, to_root, operation, size, modified)?;
        let transaction = conn
            .unchecked_transaction()
            .map_err(|e| format!("Journal transaction failed after copy: {e}"))?;
        transaction
            .execute(
                "INSERT INTO copy_proofs(item_id,identity,issue) VALUES(?,?,?)",
                params![journal, proof.identity, proof.issue],
            )
            .map_err(|e| format!("Proof journal failed after copy: {e}"))?;
        transaction.execute("UPDATE operation_items SET state='copied',message='Copied and SHA-256 verified; source preserved',content_hash=? WHERE id=?",params![proof.hash,journal]).map_err(|e|format!("Journal update failed after copy: {e}"))?;
        transaction
            .commit()
            .map_err(|e| format!("Journal commit failed after copy: {e}"))?;
        Ok(())
    }

    pub fn restore(&self, operation: i64) -> Result<()> {
        let _guard = self.begin("restore")?;
        let original = self.store.operation(operation)?;
        if original.action != "quarantine_destination" {
            return Err("This operation has no quarantine to restore.".into());
        }
        let scan = self.store.scan(original.scan_id)?;
        let root = Path::new(scan.destination.as_deref().ok_or("Missing destination")?);
        let rows = self.store.operation_items(operation)?;
        let conn = self.store.connect()?;
        self.update(|p| {
            p.phase = "restoring".into();
            p.operation_id = Some(operation);
            p.total = rows.len() as u64;
        });
        for item in rows {
            if self.check_cancel().is_err() {
                break;
            }
            if item.state != "quarantined" && item.state != "planned" {
                continue;
            }
            let restored = (|| -> Result<()> {
                let from = within_root(
                    root,
                    &format!("{QUARANTINE}/{operation}/{}", item.relative_path),
                )?;
                let to = within_root(root, &item.relative_path)?;
                if fs::symlink_metadata(&to).is_ok() {
                    return Err("Original location is occupied; both versions retained.".into());
                }
                let expected = item
                    .content_hash
                    .as_deref()
                    .ok_or("No verification record; inspect this interrupted item manually")?;
                let mut guard = movable_read(&from)?;
                if self.hash_open(&mut guard)? != expected {
                    return Err("Quarantined contents changed; retained for manual review.".into());
                }
                fs::create_dir_all(to.parent().ok_or("Missing parent")?)
                    .map_err(|e| e.to_string())?;
                let to = within_root(root, &item.relative_path)?;
                rename_open_file(&guard, &from, &to)?;
                conn.execute("UPDATE operation_items SET state='restored',message='Restored without overwriting' WHERE id=?",[item.id]).map_err(db_error)?;
                Ok(())
            })();
            if let Err(error) = restored {
                self.update(|p| p.errors += 1);
                conn.execute(
                    "UPDATE operation_items SET message=? WHERE id=?",
                    params![error, item.id],
                )
                .map_err(db_error)?;
            }
            self.update(|p| {
                p.processed += 1;
                p.current_path = item.relative_path;
            });
        }
        self.update(|p| {
            p.phase = if self.cancel.load(Ordering::SeqCst) {
                "cancelled"
            } else if p.errors > 0 {
                "completed_with_issues"
            } else {
                "complete"
            }
            .into();
            p.message = "Restore finished. Review item outcomes and rescan the folders.".into();
        });
        Ok(())
    }
}
