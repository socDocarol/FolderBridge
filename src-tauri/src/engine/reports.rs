use super::*;

impl Engine {
    pub fn export_scan(&self, filter: EntryFilter, path: &Path) -> Result<u64> {
        let _guard = self.begin("export")?;
        let parent = path.parent().ok_or("Choose an output file")?;
        let mut temp = NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
        let mut written = 0;
        {
            let mut writer = csv::Writer::from_writer(&mut temp);
            writer
                .write_record([
                    "Relative path",
                    "Status",
                    "Extension",
                    "Source bytes",
                    "Destination bytes",
                    "Source modified (Unix ns)",
                    "Destination modified (Unix ns)",
                    "Source SHA-256",
                    "Destination SHA-256",
                    "Owner",
                    "Issue",
                    "Rule review",
                    "Rule reason",
                    "Migration state",
                    "Migration reason",
                ])
                .map_err(|e| e.to_string())?;
            let connection = self.store.connect()?;
            connection.execute_batch("BEGIN").map_err(db_error)?;
            let mut statement = connection
                .prepare(&format!(
                    "SELECT {} FROM entries WHERE {} ORDER BY relative_path COLLATE NOCASE",
                    crate::storage::COLUMNS,
                    crate::storage::FILTER
                ))
                .map_err(db_error)?;
            let rows = statement
                .query_map(
                    params![
                        filter.scan_id,
                        filter.search,
                        filter.status,
                        filter.extension,
                        filter.min_size.min(i64::MAX as u64) as i64,
                        filter.migration_state,
                        filter.source_only
                    ],
                    read_entry,
                )
                .map_err(db_error)?;
            for row in rows {
                self.check_cancel()?;
                let e = row.map_err(db_error)?;
                writer
                    .write_record([
                        csv_safe(&e.relative_path),
                        e.status,
                        csv_safe(&e.extension),
                        e.source_size.map(|v| v.to_string()).unwrap_or_default(),
                        e.destination_size
                            .map(|v| v.to_string())
                            .unwrap_or_default(),
                        e.source_modified.map(|v| v.to_string()).unwrap_or_default(),
                        e.destination_modified
                            .map(|v| v.to_string())
                            .unwrap_or_default(),
                        e.source_hash.unwrap_or_default(),
                        e.destination_hash.unwrap_or_default(),
                        csv_safe(&e.owner.unwrap_or_default()),
                        csv_safe(&e.issue.unwrap_or_default()),
                        e.rule_review.to_string(),
                        csv_safe(&e.rule_reason),
                        e.migration_state,
                        csv_safe(&e.migration_reason),
                    ])
                    .map_err(|e| e.to_string())?;
                written += 1;
            }
            writer.flush().map_err(|e| e.to_string())?;
        }
        temp.as_file().sync_all().map_err(|e| e.to_string())?;
        temp.persist_noclobber(path).map_err(|e| {
            format!(
                "Choose a new report filename; existing files are preserved: {}",
                e.error
            )
        })?;
        Ok(written)
    }

    pub fn export_operation(&self, id: i64, path: &Path) -> Result<()> {
        let mut temp = NamedTempFile::new_in(path.parent().ok_or("Choose a report file")?)
            .map_err(|e| e.to_string())?;
        {
            let mut writer = csv::Writer::from_writer(&mut temp);
            writer
                .write_record([
                    "Relative path",
                    "Original path",
                    "Target path",
                    "Outcome",
                    "Message",
                    "SHA-256",
                    "Cleanup method",
                    "Cleanup state",
                    "Holding path",
                    "Cleanup message",
                ])
                .map_err(|e| e.to_string())?;
            for item in self.store.operation_items(id)? {
                writer
                    .write_record([
                        csv_safe(&item.relative_path),
                        csv_safe(&item.original_path),
                        csv_safe(&item.target_path),
                        item.state,
                        csv_safe(&item.message),
                        item.content_hash.unwrap_or_default(),
                        item.cleanup
                            .as_ref()
                            .map(|c| c.method.clone())
                            .unwrap_or_default(),
                        item.cleanup
                            .as_ref()
                            .map(|c| c.state.clone())
                            .unwrap_or_default(),
                        csv_safe(
                            &item
                                .cleanup
                                .as_ref()
                                .map(|c| c.holding_path.clone())
                                .unwrap_or_default(),
                        ),
                        csv_safe(
                            &item
                                .cleanup
                                .as_ref()
                                .map(|c| c.message.clone())
                                .unwrap_or_default(),
                        ),
                    ])
                    .map_err(|e| e.to_string())?;
            }
            writer.flush().map_err(|e| e.to_string())?;
        }
        temp.persist_noclobber(path)
            .map_err(|e| e.error.to_string())?;
        Ok(())
    }

    pub fn export_statistics(&self, id: i64, path: &Path) -> Result<()> {
        let mut temp = NamedTempFile::new_in(path.parent().ok_or("Choose a report file")?)
            .map_err(|e| e.to_string())?;
        {
            let mut writer = csv::Writer::from_writer(&mut temp);
            writer
                .write_record([
                    "Extension",
                    "Files",
                    "Mean bytes",
                    "Median bytes",
                    "Lower quartile bytes",
                    "Upper quartile bytes",
                ])
                .map_err(|e| e.to_string())?;
            for row in self.store.size_statistics(id)? {
                writer
                    .write_record([
                        csv_safe(&row.extension),
                        row.count.to_string(),
                        row.mean.to_string(),
                        row.median.to_string(),
                        row.lower_quartile.to_string(),
                        row.upper_quartile.to_string(),
                    ])
                    .map_err(|e| e.to_string())?;
            }
            writer.flush().map_err(|e| e.to_string())?;
        }
        temp.persist_noclobber(path)
            .map_err(|e| e.error.to_string())?;
        Ok(())
    }
}
