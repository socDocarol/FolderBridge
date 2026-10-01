use folderbridge_lib::paths::{validate_roots, within_root};
use folderbridge_lib::{
    engine::Engine,
    models::{EntryFilter, OperationRequest, ScanOptions},
};
use std::fs;
use tempfile::tempdir;

fn options(source: &std::path::Path, destination: &std::path::Path) -> ScanOptions {
    ScanOptions {
        source: source.to_string_lossy().into(),
        destination: Some(destination.to_string_lossy().into()),
        verify_contents: true,
        excluded_extensions: vec![],
        collect_owners: false,
        rules: Default::default(),
    }
}

#[test]
fn comparisons_distinguish_equal_size_conflicts_and_verified_duplicates() {
    let t = tempdir().unwrap();
    let (a, b) = (t.path().join("a"), t.path().join("b"));
    fs::create_dir(&a).unwrap();
    fs::create_dir(&b).unwrap();
    fs::write(a.join("conflict.txt"), "AAAA").unwrap();
    fs::write(b.join("conflict.txt"), "BBBB").unwrap();
    fs::write(a.join("same.txt"), "same").unwrap();
    fs::write(b.join("same.txt"), "same").unwrap();
    fs::write(a.join("missing.txt"), "copy me").unwrap();
    let engine = Engine::open(&t.path().join("app.db")).unwrap();
    let id = engine.scan(options(&a, &b)).unwrap();
    let rows = engine
        .store
        .entries(&EntryFilter {
            scan_id: id,
            ..Default::default()
        })
        .unwrap()
        .entries;
    assert_eq!(
        rows.iter()
            .find(|e| e.relative_path == "conflict.txt")
            .unwrap()
            .status,
        "different"
    );
    assert_eq!(
        rows.iter()
            .find(|e| e.relative_path == "same.txt")
            .unwrap()
            .status,
        "identical"
    );
    assert_eq!(
        rows.iter()
            .find(|e| e.relative_path == "missing.txt")
            .unwrap()
            .status,
        "source_only"
    );
}

#[test]
fn copy_never_overwrites_a_destination_created_after_scan() {
    let t = tempdir().unwrap();
    let (a, b) = (t.path().join("a"), t.path().join("b"));
    fs::create_dir(&a).unwrap();
    fs::create_dir(&b).unwrap();
    fs::write(a.join("file.txt"), "source").unwrap();
    let engine = Engine::open(&t.path().join("app.db")).unwrap();
    let id = engine.scan(options(&a, &b)).unwrap();
    let entry = engine
        .store
        .entries(&EntryFilter {
            scan_id: id,
            ..Default::default()
        })
        .unwrap()
        .entries
        .remove(0);
    fs::write(b.join("file.txt"), "keep destination").unwrap();
    let op = engine
        .execute(OperationRequest {
            scan_id: id,
            entry_ids: vec![entry.id],
            action: "copy_to_destination".into(),
            approved_entry_ids: vec![],
        })
        .unwrap();
    assert_eq!(
        fs::read_to_string(b.join("file.txt")).unwrap(),
        "keep destination"
    );
    assert_eq!(
        engine.store.operation_items(op).unwrap()[0].state,
        "skipped"
    );
}

#[test]
fn quarantine_is_verified_and_restorable_without_overwrites() {
    let t = tempdir().unwrap();
    let (a, b) = (t.path().join("a"), t.path().join("b"));
    fs::create_dir(&a).unwrap();
    fs::create_dir(&b).unwrap();
    fs::write(a.join("same.txt"), "safe").unwrap();
    fs::write(b.join("same.txt"), "safe").unwrap();
    let engine = Engine::open(&t.path().join("app.db")).unwrap();
    let id = engine.scan(options(&a, &b)).unwrap();
    let entry = engine
        .store
        .entries(&EntryFilter {
            scan_id: id,
            ..Default::default()
        })
        .unwrap()
        .entries
        .remove(0);
    let op = engine
        .execute(OperationRequest {
            scan_id: id,
            entry_ids: vec![entry.id],
            action: "quarantine_destination".into(),
            approved_entry_ids: vec![],
        })
        .unwrap();
    assert!(a.join("same.txt").exists());
    assert!(!b.join("same.txt").exists());
    engine.restore(op).unwrap();
    assert_eq!(fs::read_to_string(b.join("same.txt")).unwrap(), "safe");
}

#[test]
fn changed_backup_blocks_cleanup() {
    let t = tempdir().unwrap();
    let (a, b) = (t.path().join("a"), t.path().join("b"));
    fs::create_dir(&a).unwrap();
    fs::create_dir(&b).unwrap();
    fs::write(a.join("same.txt"), "safe").unwrap();
    fs::write(b.join("same.txt"), "safe").unwrap();
    let engine = Engine::open(&t.path().join("app.db")).unwrap();
    let id = engine.scan(options(&a, &b)).unwrap();
    let entry = engine
        .store
        .entries(&EntryFilter {
            scan_id: id,
            ..Default::default()
        })
        .unwrap()
        .entries
        .remove(0);
    fs::write(a.join("same.txt"), "DIFF").unwrap();
    engine
        .execute(OperationRequest {
            scan_id: id,
            entry_ids: vec![entry.id],
            action: "quarantine_destination".into(),
            approved_entry_ids: vec![],
        })
        .unwrap();
    assert_eq!(fs::read_to_string(b.join("same.txt")).unwrap(), "safe");
}

#[test]
fn overlapping_roots_are_rejected() {
    let temp = tempdir().unwrap();
    let nested = temp.path().join("child");
    fs::create_dir(&nested).unwrap();
    assert!(validate_roots(temp.path(), Some(&nested)).is_err());
    assert!(validate_roots(&nested, Some(temp.path())).is_err());
    assert!(validate_roots(temp.path(), Some(temp.path())).is_err());
}

#[test]
fn relative_paths_cannot_escape_the_selected_root() {
    let temp = tempdir().unwrap();
    assert!(within_root(temp.path(), "../escape.txt").is_err());
    assert!(within_root(temp.path(), "..\\escape.txt").is_err());
    assert!(within_root(temp.path(), "C:\\elsewhere\\escape.txt").is_err());
    assert!(within_root(temp.path(), "normal.txt").is_ok());
}

#[test]
fn successful_copy_is_verified_and_preserves_both_original_files() {
    let t = tempdir().unwrap();
    let (a, b) = (t.path().join("a"), t.path().join("b"));
    fs::create_dir(&a).unwrap();
    fs::create_dir(&b).unwrap();
    fs::write(a.join("copy.txt"), "verified content").unwrap();
    let engine = Engine::open(&t.path().join("app.db")).unwrap();
    let scan = engine.scan(options(&a, &b)).unwrap();
    let entry = engine
        .store
        .entries(&EntryFilter {
            scan_id: scan,
            ..Default::default()
        })
        .unwrap()
        .entries
        .remove(0);
    let operation = engine
        .execute(OperationRequest {
            scan_id: scan,
            entry_ids: vec![entry.id],
            action: "copy_to_destination".into(),
            approved_entry_ids: vec![],
        })
        .unwrap();
    assert_eq!(
        fs::read(a.join("copy.txt")).unwrap(),
        fs::read(b.join("copy.txt")).unwrap()
    );
    let items = engine.store.operation_items(operation).unwrap();
    assert_eq!(items[0].state, "copied");
    assert_eq!(items[0].content_hash.as_ref().unwrap().len(), 64);
    assert_eq!(
        fs::metadata(a.join("copy.txt"))
            .unwrap()
            .modified()
            .unwrap(),
        fs::metadata(b.join("copy.txt"))
            .unwrap()
            .modified()
            .unwrap()
    );
}

#[test]
fn cancelled_copy_preserves_source_and_publishes_no_partial_destination() {
    use std::{
        thread,
        time::{Duration, Instant},
    };
    let t = tempdir().unwrap();
    let (a, b) = (t.path().join("a"), t.path().join("b"));
    fs::create_dir(&a).unwrap();
    fs::create_dir(&b).unwrap();
    fs::File::create(a.join("large.bin"))
        .unwrap()
        .set_len(128 * 1024 * 1024)
        .unwrap();
    let engine = Engine::open(&t.path().join("app.db")).unwrap();
    let scan = engine.scan(options(&a, &b)).unwrap();
    let ids = engine
        .store
        .matching_ids(&EntryFilter {
            scan_id: scan,
            ..Default::default()
        })
        .unwrap();
    let worker = engine.clone();
    let job = thread::spawn(move || {
        worker.execute(OperationRequest {
            scan_id: scan,
            entry_ids: ids,
            action: "copy_to_destination".into(),
            approved_entry_ids: vec![],
        })
    });
    let start = Instant::now();
    while engine.progress().kind != "operation" || engine.progress().bytes == 0 {
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "Copy did not start"
        );
        thread::sleep(Duration::from_millis(1));
    }
    engine.cancel();
    let operation = job.join().unwrap().unwrap();
    assert_eq!(
        engine.store.operation(operation).unwrap().state,
        "cancelled"
    );
    assert!(a.join("large.bin").exists());
    assert!(!b.join("large.bin").exists());
    assert!(!engine.progress().running);
    // Normal cancellation cleans its temporary file, allowing the next comparison.
    let next = engine.scan(options(&a, &b)).unwrap();
    assert_eq!(engine.store.scan(next).unwrap().state, "complete");
}

#[test]
fn restore_preserves_a_new_file_at_the_original_location() {
    let t = tempdir().unwrap();
    let (a, b) = (t.path().join("a"), t.path().join("b"));
    fs::create_dir(&a).unwrap();
    fs::create_dir(&b).unwrap();
    fs::write(a.join("same.txt"), "same").unwrap();
    fs::write(b.join("same.txt"), "same").unwrap();
    let engine = Engine::open(&t.path().join("app.db")).unwrap();
    let scan = engine.scan(options(&a, &b)).unwrap();
    let entry = engine
        .store
        .entries(&EntryFilter {
            scan_id: scan,
            ..Default::default()
        })
        .unwrap()
        .entries
        .remove(0);
    let operation = engine
        .execute(OperationRequest {
            scan_id: scan,
            entry_ids: vec![entry.id],
            action: "quarantine_destination".into(),
            approved_entry_ids: vec![],
        })
        .unwrap();
    fs::write(b.join("same.txt"), "new version").unwrap();
    engine.restore(operation).unwrap();
    assert_eq!(
        fs::read_to_string(b.join("same.txt")).unwrap(),
        "new version"
    );
    let item = engine.store.operation_items(operation).unwrap().remove(0);
    assert_eq!(item.state, "quarantined");
    assert!(std::path::Path::new(&item.target_path).exists());
}

#[test]
fn quick_comparison_never_enables_cleanup() {
    let t = tempdir().unwrap();
    let (a, b) = (t.path().join("a"), t.path().join("b"));
    fs::create_dir(&a).unwrap();
    fs::create_dir(&b).unwrap();
    fs::write(a.join("same.txt"), "same").unwrap();
    fs::write(b.join("same.txt"), "same").unwrap();
    let engine = Engine::open(&t.path().join("app.db")).unwrap();
    let mut opts = options(&a, &b);
    opts.verify_contents = false;
    let scan = engine.scan(opts).unwrap();
    let entry = engine
        .store
        .entries(&EntryFilter {
            scan_id: scan,
            ..Default::default()
        })
        .unwrap()
        .entries
        .remove(0);
    assert_eq!(entry.status, "unverified");
    assert!(engine
        .execute(OperationRequest {
            scan_id: scan,
            entry_ids: vec![entry.id],
            action: "quarantine_destination".into(),
            approved_entry_ids: vec![],
        })
        .is_err());
    assert!(b.join("same.txt").exists());
}

#[test]
fn file_owners_and_spreadsheet_safe_export_work() {
    let t = tempdir().unwrap();
    let (a, b) = (t.path().join("a"), t.path().join("b"));
    fs::create_dir(&a).unwrap();
    fs::create_dir(&b).unwrap();
    fs::write(a.join("=report.txt"), "data").unwrap();
    let engine = Engine::open(&t.path().join("app.db")).unwrap();
    let mut opts = options(&a, &b);
    opts.collect_owners = true;
    let scan = engine.scan(opts).unwrap();
    let filter = EntryFilter {
        scan_id: scan,
        ..Default::default()
    };
    let entry = engine.store.entries(&filter).unwrap().entries.remove(0);
    assert!(entry.owner.is_some());
    let report = t.path().join("report.csv");
    assert_eq!(engine.export_scan(filter, &report).unwrap(), 1);
    let text = fs::read_to_string(report).unwrap();
    assert!(text.contains("'=report.txt"));
}

#[test]
fn stale_partial_copy_files_are_never_recovery_candidates() {
    let t = tempdir().unwrap();
    let (a, b) = (t.path().join("a"), t.path().join("b"));
    fs::create_dir(&a).unwrap();
    fs::create_dir(&b).unwrap();
    fs::write(b.join(".folderbridge-copy-interrupted"), "partial").unwrap();
    let engine = Engine::open(&t.path().join("app.db")).unwrap();
    let scan = engine.scan(options(&a, &b)).unwrap();
    let entry = engine
        .store
        .entries(&EntryFilter {
            scan_id: scan,
            ..Default::default()
        })
        .unwrap()
        .entries
        .remove(0);
    assert_eq!(entry.status, "error");
    assert_eq!(engine.store.scan(scan).unwrap().state, "incomplete");
}

#[test]
fn old_quarantine_operations_remain_restorable() {
    let t = tempdir().unwrap();
    let (a, b) = (t.path().join("a"), t.path().join("b"));
    fs::create_dir(&a).unwrap();
    fs::create_dir(&b).unwrap();
    fs::write(a.join("same"), "same").unwrap();
    fs::write(b.join("same"), "same").unwrap();
    let engine = Engine::open(&t.path().join("app.db")).unwrap();
    let scan = engine.scan(options(&a, &b)).unwrap();
    let entry = engine
        .store
        .entries(&EntryFilter {
            scan_id: scan,
            ..Default::default()
        })
        .unwrap()
        .entries
        .remove(0);
    let operation = engine
        .execute(OperationRequest {
            scan_id: scan,
            entry_ids: vec![entry.id],
            action: "quarantine_destination".into(),
            approved_entry_ids: vec![],
        })
        .unwrap();
    let conn = engine.store.connect().unwrap();
    for _ in 0..110 {
        conn.execute("INSERT INTO operations(scan_id,action,started_at,state) VALUES(?,'copy_to_destination',0,'complete')",[scan]).unwrap();
    }
    engine.restore(operation).unwrap();
    assert!(b.join("same").exists());
}

#[test]
fn statistics_match_interpolated_quartiles() {
    let t = tempdir().unwrap();
    let (a, b) = (t.path().join("a"), t.path().join("b"));
    fs::create_dir(&a).unwrap();
    fs::create_dir(&b).unwrap();
    for (i, size) in [10, 20, 30, 40].into_iter().enumerate() {
        fs::write(a.join(format!("{i}.txt")), vec![b'x'; size]).unwrap();
    }
    let engine = Engine::open(&t.path().join("app.db")).unwrap();
    let scan = engine.scan(options(&a, &b)).unwrap();
    let rows = engine.store.size_statistics(scan).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].mean, 25.0);
    assert_eq!(rows[0].median, 25.0);
    assert_eq!(rows[0].lower_quartile, 17.5);
    assert_eq!(rows[0].upper_quartile, 32.5);
}

#[cfg(windows)]
#[test]
fn verified_move_handle_blocks_external_replacement_and_overwrite() {
    use folderbridge_lib::{engine::movable_read, paths::rename_open_file};
    let t = tempdir().unwrap();
    let original = t.path().join("original");
    let other = t.path().join("other");
    let target = t.path().join("target");
    fs::write(&original, "safe").unwrap();
    fs::write(&other, "bad").unwrap();
    let guard = movable_read(&original).unwrap();
    assert!(fs::write(&original, "bad").is_err());
    assert!(fs::rename(&original, &target).is_err());
    assert!(fs::remove_file(&original).is_err());
    fs::write(&target, "existing").unwrap();
    assert!(rename_open_file(&guard, &original, &target).is_err());
    assert_eq!(fs::read_to_string(&target).unwrap(), "existing");
    let final_path = t.path().join("final");
    rename_open_file(&guard, &original, &final_path).unwrap();
    drop(guard);
    assert_eq!(fs::read_to_string(final_path).unwrap(), "safe");
}
