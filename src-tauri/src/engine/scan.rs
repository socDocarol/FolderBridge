use super::*;

impl Engine {
    pub fn scan(&self, options: ScanOptions) -> Result<i64> {
        let _guard = self.begin("scan")?;
        let result = self.scan_inner(options);
        if let Err(error) = &result {
            self.update(|p| {
                p.phase = "failed".into();
                p.message = error.clone();
            });
        }
        result
    }

    fn scan_inner(&self, mut options: ScanOptions) -> Result<i64> {
        let (source, destination) = validate_roots(
            Path::new(&options.source),
            options
                .destination
                .as_deref()
                .filter(|p| !p.trim().is_empty())
                .map(Path::new),
        )?;
        options.source = source.to_string_lossy().into_owned();
        options.destination = destination
            .as_ref()
            .map(|p| p.to_string_lossy().into_owned());
        crate::policy::normalize_options(&mut options)?;
        let snapshot = serde_json::to_string(&options).map_err(|e| e.to_string())?;
        let conn = self.store.connect()?;
        conn.execute("INSERT INTO scans(source,destination,started_at,state,verified,options) VALUES(?,?,?,'running',?,?)",params![options.source,options.destination,now(),options.verify_contents,snapshot]).map_err(db_error)?;
        let id = conn.last_insert_rowid();
        self.update(|p| {
            p.scan_id = Some(id);
            p.phase = "inventory".into();
        });
        let work = (|| {
            self.inventory(&conn, id, &source, true, &options)?;
            if let Some(target) = &destination {
                self.inventory(&conn, id, target, false, &options)?;
            }
            self.compare(
                &conn,
                id,
                &source,
                destination.as_deref(),
                options.verify_contents,
                &options,
            )
        })();
        // inventory/compare may have been interrupted inside a batch transaction.
        if !conn.is_autocommit() {
            conn.execute_batch("COMMIT").map_err(db_error)?;
        }
        let progress = self.progress();
        let state = match &work {
            Err(e) if e == "Cancelled" => "cancelled",
            Err(_) => "incomplete",
            Ok(_) if progress.errors > 0 => "incomplete",
            _ => "complete",
        };
        let files: i64 = conn
            .query_row("SELECT count(*) FROM entries WHERE scan_id=?", [id], |r| {
                r.get(0)
            })
            .map_err(db_error)?;
        let errors = progress.errors + u64::from(work.as_ref().is_err_and(|e| e != "Cancelled"));
        conn.execute(
            "UPDATE scans SET state=?,files=?,errors=? WHERE id=?",
            params![state, files, errors as i64, id],
        )
        .map_err(db_error)?;
        self.update(|p| {
            p.phase = state.into();
            p.message = work.err().unwrap_or_else(|| {
                if errors > 0 {
                    "Some paths could not be read. Review issues before trying again.".into()
                } else {
                    "Comparison ready. No files were changed.".into()
                }
            });
        });
        Ok(id)
    }

    fn inventory(
        &self,
        conn: &Connection,
        id: i64,
        root: &Path,
        source: bool,
        options: &ScanOptions,
    ) -> Result<()> {
        let staging = root.join(STAGING);
        if staging.exists()
            && WalkDir::new(&staging)
                .follow_links(false)
                .into_iter()
                .any(|e| {
                    e.map(|v| v.file_type().is_file() || v.file_type().is_symlink())
                        .unwrap_or(true)
                })
        {
            self.scan_issue(conn,id,STAGING,"Interrupted staging files were retained here. Inspect this folder before using this scan for file operations.")?;
        }
        conn.execute_batch("BEGIN").map_err(db_error)?;
        let mut count = 0u64;
        let walk = WalkDir::new(root)
            .follow_links(false)
            .into_iter()
            .filter_entry(|e| {
                e.depth() == 0
                    || ![QUARANTINE, STAGING, "_ToDelete"]
                        .iter()
                        .any(|name| e.file_name().to_string_lossy().eq_ignore_ascii_case(name))
            });
        for item in walk {
            self.check_cancel()?;
            let entry = match item {
                Ok(e) => e,
                Err(error) => {
                    let path = error
                        .path()
                        .and_then(|p| relative_text(root, p).ok())
                        .unwrap_or_else(|| "(unreadable folder)".into());
                    self.scan_issue(conn, id, &path, &error.to_string())?;
                    continue;
                }
            };
            if entry.depth() == 0 {
                continue;
            }
            let relative = relative_text(root, entry.path())?;
            if entry.file_type().is_symlink() {
                self.scan_issue(
                    conn,
                    id,
                    &relative,
                    "Link or junction skipped; targets are outside this scan's coverage.",
                )?;
                continue;
            }
            if !entry.file_type().is_file() {
                continue;
            }
            if entry
                .file_name()
                .to_string_lossy()
                .starts_with(".folderbridge-copy-")
            {
                self.scan_issue(conn,id,&relative,"Interrupted staging artifact. Retained for manual inspection; not eligible for file actions.")?;
                continue;
            }
            let path = match within_root(root, &relative) {
                Ok(p) => p,
                Err(e) => {
                    self.scan_issue(conn, id, &relative, &e)?;
                    continue;
                }
            };
            let (size, modified) = match fs::metadata(&path)
                .map_err(|e| e.to_string())
                .and_then(|m| metadata_signature(&m))
            {
                Ok(m) => m,
                Err(e) => {
                    self.scan_issue(conn, id, &relative, &e)?;
                    continue;
                }
            };
            let extension = path
                .extension()
                .map(|s| format!(".{}", s.to_string_lossy().to_lowercase()))
                .unwrap_or_default();
            let key = if cfg!(windows) {
                relative.to_lowercase()
            } else {
                relative.clone()
            };
            let status = "pending";
            let owner = if options.collect_owners {
                Some(file_owner(&path).unwrap_or_else(|e| format!("Unavailable: {e}")))
            } else {
                None
            };
            let side = if source { "source" } else { "destination" };
            let existing: Option<String> = conn
                .query_row(
                    &format!("SELECT {side}_relative FROM entries WHERE scan_id=? AND path_key=?"),
                    params![id, key],
                    |r| r.get(0),
                )
                .optional()
                .map_err(db_error)?
                .flatten();
            if existing.is_some() {
                self.scan_issue(
                    conn,
                    id,
                    &relative,
                    "Ambiguous filename after normalization; actions are blocked.",
                )?;
                continue;
            }
            conn.execute(&format!("INSERT INTO entries(scan_id,path_key,relative_path,{side}_relative,extension,status,{side}_size,{side}_modified,owner) VALUES(?1,?2,?3,?3,?4,?5,?6,?7,?8) ON CONFLICT(scan_id,path_key) DO UPDATE SET {side}_relative=?3,{side}_size=?6,{side}_modified=?7,owner=coalesce(entries.owner,?8)"),params![id,key,relative,extension,status,size as i64,modified,owner]).map_err(db_error)?;
            count += 1;
            self.update(|p| {
                p.processed += 1;
                p.current_path = relative;
                p.bytes += size;
            });
            if count.is_multiple_of(256) {
                conn.execute_batch("COMMIT; BEGIN").map_err(db_error)?;
            }
        }
        conn.execute_batch("COMMIT").map_err(db_error)?;
        Ok(())
    }

    fn scan_issue(&self, conn: &Connection, id: i64, relative: &str, message: &str) -> Result<()> {
        let key = if cfg!(windows) {
            relative.to_lowercase()
        } else {
            relative.into()
        };
        conn.execute("INSERT INTO entries(scan_id,path_key,relative_path,extension,status,issue) VALUES(?,?,?,'','error',?) ON CONFLICT(scan_id,path_key) DO UPDATE SET status='error',issue=excluded.issue",params![id,key,relative,message]).map_err(db_error)?;
        self.update(|p| p.errors += 1);
        Ok(())
    }

    fn compare(
        &self,
        conn: &Connection,
        id: i64,
        source: &Path,
        destination: Option<&Path>,
        verify: bool,
        options: &ScanOptions,
    ) -> Result<()> {
        let total: i64 = conn
            .query_row("SELECT count(*) FROM entries WHERE scan_id=?", [id], |r| {
                r.get(0)
            })
            .map_err(db_error)?;
        self.update(|p| {
            p.phase = if verify { "verifying" } else { "comparing" }.into();
            p.processed = 0;
            p.total = total as u64;
            p.bytes = 0;
        });
        let mut after = 0;
        loop {
            let rows = self.store.batch(id, after)?;
            if rows.is_empty() {
                break;
            }
            conn.execute_batch("BEGIN").map_err(db_error)?;
            for entry in rows {
                after = entry.id;
                self.check_cancel()?;
                self.update(|p| p.current_path = entry.relative_path.clone());
                if entry.status != "pending" {
                    self.update(|p| p.processed += 1);
                    continue;
                }
                let policy = crate::policy::classify(&entry, options);
                if let Some(reason) = policy.skip_reason {
                    conn.execute("UPDATE entries SET status='excluded',issue=?1,rule_review=0,rule_reason=?1,migration_state='skipped',migration_reason=?1 WHERE id=?2",params![reason,entry.id]).map_err(db_error)?;
                    self.update(|p| p.processed += 1);
                    continue;
                }
                let rule_review = !policy.review_reason.is_empty();
                let rule_reason = policy.review_reason;
                let compared = (|| -> Result<(String, Option<String>, Option<String>)> {
                    match (entry.source_size, entry.destination_size) {
                        (Some(_), None) => Ok((
                            if destination.is_none() {
                                "inventory"
                            } else {
                                "source_only"
                            }
                            .into(),
                            None,
                            None,
                        )),
                        (None, Some(_)) => Ok(("destination_only".into(), None, None)),
                        (Some(a), Some(b)) if a != b => Ok(("different".into(), None, None)),
                        (Some(_), Some(_)) if !verify => Ok(("unverified".into(), None, None)),
                        (Some(_), Some(_)) => {
                            let a = within_root(
                                source,
                                entry
                                    .source_relative
                                    .as_deref()
                                    .ok_or("Missing source path")?,
                            )?;
                            let b = within_root(
                                destination.ok_or("Missing destination")?,
                                entry
                                    .destination_relative
                                    .as_deref()
                                    .ok_or("Missing destination path")?,
                            )?;
                            check_snapshot(&a, entry.source_size, entry.source_modified)?;
                            check_snapshot(&b, entry.destination_size, entry.destination_modified)?;
                            let ah = self.hash(&a)?;
                            let bh = self.hash(&b)?;
                            Ok((
                                if ah == bh { "identical" } else { "different" }.into(),
                                Some(ah),
                                Some(bh),
                            ))
                        }
                        _ => Err("File metadata is unavailable.".into()),
                    }
                })();
                match compared {
                    Ok((status, ah, bh)) => {
                        let (migration_state, migration_reason) =
                            crate::policy::migration(&status, rule_review, &rule_reason);
                        conn.execute("UPDATE entries SET status=?,source_hash=?,destination_hash=?,rule_review=?,rule_reason=?,migration_state=?,migration_reason=? WHERE id=?",params![status,ah,bh,rule_review,rule_reason,migration_state,migration_reason,entry.id]).map_err(db_error)?;
                    }
                    Err(e) if e == "Cancelled" => return Err(e),
                    Err(e) => {
                        conn.execute(
                            "UPDATE entries SET status='error',issue=? WHERE id=?",
                            params![e, entry.id],
                        )
                        .map_err(db_error)?;
                        self.update(|p| p.errors += 1);
                    }
                }
                self.update(|p| p.processed += 1);
            }
            conn.execute_batch("COMMIT").map_err(db_error)?;
        }
        Ok(())
    }
}
