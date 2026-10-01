#![cfg(windows)]

use folderbridge_lib::{
    engine::Engine,
    models::{CleanupRequest, EntryFilter, OperationRequest, ScanOptions},
};
use std::{fs, path::Path};
use tempfile::TempDir;

struct Fixture {
    _dir: TempDir,
    engine: Engine,
    source: std::path::PathBuf,
    target: std::path::PathBuf,
    operation: i64,
    item: i64,
}

fn fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source");
    let target = dir.path().join("target");
    fs::create_dir(&source).unwrap();
    fs::create_dir(&target).unwrap();
    fs::write(source.join("file.txt"), "verified copy").unwrap();
    let engine = Engine::open(&dir.path().join("store.sqlite")).unwrap();
    let scan = engine
        .scan(ScanOptions {
            source: source.to_string_lossy().into(),
            destination: Some(target.to_string_lossy().into()),
            verify_contents: true,
            excluded_extensions: vec![],
            collect_owners: false,
        })
        .unwrap();
    let id = engine
        .store
        .entries(&EntryFilter {
            scan_id: scan,
            ..Default::default()
        })
        .unwrap()
        .entries[0]
        .id;
    let operation = engine
        .execute(OperationRequest {
            scan_id: scan,
            entry_ids: vec![id],
            action: "copy_to_destination".into(),
        })
        .unwrap();
    let item = engine.store.operation_items(operation).unwrap()[0].id;
    Fixture {
        _dir: dir,
        engine,
        source,
        target,
        operation,
        item,
    }
}

fn request(f: &Fixture) -> CleanupRequest {
    CleanupRequest {
        operation_id: f.operation,
        item_ids: vec![f.item],
        handling: "network".into(),
        confirmation: "REMOVE 1".into(),
        sharepoint_confirmed: false,
    }
}

#[test]
fn verified_copy_has_proof_and_network_holding_restores_without_overwrite() {
    let f = fixture();
    let preview = f.engine.preview_cleanup(f.operation).unwrap();
    assert!(!preview.is_network);
    assert!(preview.items[0].eligible, "{}", preview.items[0].reason);
    let result = f.engine.cleanup_originals(request(&f)).unwrap();
    assert_eq!(
        result.completed,
        1,
        "{}; progress: {:?}; item: {:?}",
        result.message,
        f.engine.progress(),
        f.engine.store.operation_items(f.operation).unwrap()[0].cleanup
    );
    assert!(!f.source.join("file.txt").exists());
    assert_eq!(
        fs::read_to_string(f.target.join("file.txt")).unwrap(),
        "verified copy"
    );
    let held = f.source.join(format!("_ToDelete/{}/file.txt", f.operation));
    assert_eq!(fs::read_to_string(&held).unwrap(), "verified copy");
    assert_eq!(
        f.engine.store.operation_items(f.operation).unwrap()[0]
            .cleanup
            .as_ref()
            .unwrap()
            .state,
        "held"
    );
    let restored = f
        .engine
        .restore_originals(f.operation, vec![f.item])
        .unwrap();
    assert_eq!(restored.completed, 1);
    assert_eq!(
        fs::read_to_string(f.source.join("file.txt")).unwrap(),
        "verified copy"
    );
    assert!(!held.exists());
}

#[test]
fn changed_source_or_target_is_skipped() {
    for change_source in [true, false] {
        let f = fixture();
        let changed = if change_source {
            f.source.join("file.txt")
        } else {
            f.target.join("file.txt")
        };
        fs::write(&changed, "changed").unwrap();
        let preview = f.engine.preview_cleanup(f.operation).unwrap();
        assert!(!preview.items[0].eligible);
        let result = f.engine.cleanup_originals(request(&f)).unwrap();
        assert_eq!(result.completed, 0);
        assert!(f.source.join("file.txt").exists());
    }
}

#[test]
fn same_content_source_replacement_is_not_the_copied_original() {
    let f = fixture();
    fs::remove_file(f.source.join("file.txt")).unwrap();
    fs::write(f.source.join("file.txt"), "verified copy").unwrap();
    let preview = f.engine.preview_cleanup(f.operation).unwrap();
    assert!(!preview.items[0].eligible);
    assert!(preview.items[0].reason.contains("replaced"));
    assert_eq!(
        f.engine.cleanup_originals(request(&f)).unwrap().completed,
        0
    );
    assert!(f.source.join("file.txt").exists());
}

#[test]
fn legacy_copy_without_proof_is_ineligible() {
    let f = fixture();
    f.engine
        .store
        .connect()
        .unwrap()
        .execute("DELETE FROM copy_proofs WHERE item_id=?", [f.item])
        .unwrap();
    let preview = f.engine.preview_cleanup(f.operation).unwrap();
    assert!(!preview.items[0].eligible);
    assert!(preview.items[0].reason.contains("Legacy"));
    assert_eq!(
        f.engine.cleanup_originals(request(&f)).unwrap().completed,
        0
    );
}

#[test]
fn invalid_selection_action_and_confirmation_do_not_touch_files() {
    let f = fixture();
    let mut wrong = request(&f);
    wrong.item_ids = vec![f.item + 9999];
    assert!(f.engine.cleanup_originals(wrong).is_err());
    let mut wrong = request(&f);
    wrong.confirmation = "REMOVE 2".into();
    assert!(f.engine.cleanup_originals(wrong).is_err());
    let mut wrong = request(&f);
    wrong.handling = "sharepoint".into();
    assert!(f.engine.cleanup_originals(wrong).is_err());
    let mut wrong = request(&f);
    wrong.handling = "bogus".into();
    assert!(f.engine.cleanup_originals(wrong).is_err());
    f.engine
        .store
        .connect()
        .unwrap()
        .execute(
            "UPDATE operations SET action='quarantine_destination' WHERE id=?",
            [f.operation],
        )
        .unwrap();
    assert!(f.engine.cleanup_originals(request(&f)).is_err());
    assert!(f.source.join("file.txt").exists());
}

#[test]
fn occupied_holding_path_preserves_original_and_colliding_file() {
    let f = fixture();
    let held = f.source.join(format!("_ToDelete/{}/file.txt", f.operation));
    fs::create_dir_all(held.parent().unwrap()).unwrap();
    fs::write(&held, "existing").unwrap();
    assert_eq!(
        f.engine.cleanup_originals(request(&f)).unwrap().completed,
        0
    );
    assert_eq!(fs::read_to_string(&held).unwrap(), "existing");
    assert_eq!(
        fs::read_to_string(f.source.join("file.txt")).unwrap(),
        "verified copy"
    );
}

#[test]
fn restore_refuses_to_overwrite_a_new_original() {
    let f = fixture();
    assert_eq!(
        f.engine.cleanup_originals(request(&f)).unwrap().completed,
        1
    );
    fs::write(f.source.join("file.txt"), "new file").unwrap();
    assert_eq!(
        f.engine
            .restore_originals(f.operation, vec![f.item])
            .unwrap()
            .completed,
        0
    );
    assert_eq!(
        fs::read_to_string(f.source.join("file.txt")).unwrap(),
        "new file"
    );
    assert!(f
        .source
        .join(format!("_ToDelete/{}/file.txt", f.operation))
        .exists());
}

#[test]
fn held_network_files_are_excluded_from_subsequent_scans() {
    let f = fixture();
    assert_eq!(
        f.engine.cleanup_originals(request(&f)).unwrap().completed,
        1
    );
    let scan = f
        .engine
        .scan(ScanOptions {
            source: f.source.to_string_lossy().into(),
            destination: Some(f.target.to_string_lossy().into()),
            verify_contents: true,
            excluded_extensions: vec![],
            collect_owners: false,
        })
        .unwrap();
    let entries = f
        .engine
        .store
        .entries(&EntryFilter {
            scan_id: scan,
            ..Default::default()
        })
        .unwrap()
        .entries;
    assert!(entries
        .iter()
        .all(|entry| !entry.relative_path.contains("_ToDelete")));
}

#[test]
fn interrupted_journal_never_retries_cleanup_and_restore_waits_for_holding_file() {
    let f = fixture();
    let holding = f.source.join(format!("_ToDelete/{}/file.txt", f.operation));
    f.engine.store.connect().unwrap().execute(
        "INSERT INTO migration_cleanup(item_id,method,state,holding_path,message) VALUES(?,'network','planned',?,'pending')",
        rusqlite::params![f.item, holding.to_string_lossy()],
    ).unwrap();
    assert!(!f.engine.preview_cleanup(f.operation).unwrap().items[0].eligible);
    assert_eq!(
        f.engine.cleanup_originals(request(&f)).unwrap().completed,
        0
    );
    assert_eq!(
        f.engine
            .restore_originals(f.operation, vec![f.item])
            .unwrap()
            .completed,
        0
    );
    assert!(f.source.join("file.txt").exists());
}

#[test]
fn alternate_stream_copy_succeeds_but_cleanup_proof_is_ineligible() {
    let f = fixture();
    // Recreate a second fixture with ADS before the copy: the main data stream
    // remains copyable, but cleanup cannot assert the entire file was backed up.
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source");
    let target = root.path().join("target");
    fs::create_dir(&source).unwrap();
    fs::create_dir(&target).unwrap();
    let original = source.join("ads.txt");
    fs::write(&original, "main data").unwrap();
    fs::write(format!("{}:extra", original.display()), "hidden data").unwrap();
    let engine = Engine::open(&root.path().join("store.sqlite")).unwrap();
    let scan = engine
        .scan(ScanOptions {
            source: source.to_string_lossy().into(),
            destination: Some(target.to_string_lossy().into()),
            verify_contents: true,
            excluded_extensions: vec![],
            collect_owners: false,
        })
        .unwrap();
    let id = engine
        .store
        .entries(&EntryFilter {
            scan_id: scan,
            ..Default::default()
        })
        .unwrap()
        .entries[0]
        .id;
    let operation = engine
        .execute(OperationRequest {
            scan_id: scan,
            entry_ids: vec![id],
            action: "copy_to_destination".into(),
        })
        .unwrap();
    assert!(target.join("ads.txt").exists());
    let preview = engine.preview_cleanup(operation).unwrap();
    assert!(!preview.items[0].eligible);
    assert!(preview.items[0].reason.contains("stream"));
    assert!(original.exists());
    assert!(Path::new(&f.source.join("file.txt")).exists());
}

#[test]
fn failed_cleanup_is_recorded_and_can_retry_after_copy_is_repaired() {
    let f = fixture();
    fs::write(f.target.join("file.txt"), "changed target").unwrap();
    assert_eq!(
        f.engine.cleanup_originals(request(&f)).unwrap().completed,
        0
    );
    let item = f
        .engine
        .store
        .operation_items(f.operation)
        .unwrap()
        .remove(0);
    let cleanup = item.cleanup.unwrap();
    assert_eq!(cleanup.state, "failed");
    assert!(cleanup.message.contains("changed"));
    fs::write(f.target.join("file.txt"), "verified copy").unwrap();
    assert_eq!(
        f.engine.cleanup_originals(request(&f)).unwrap().completed,
        1
    );
    fs::rename(&f.target, f.target.with_file_name("offline")).unwrap();
    assert_eq!(
        f.engine
            .restore_originals(f.operation, vec![f.item])
            .unwrap()
            .completed,
        1
    );
}

#[test]
fn preview_does_not_read_contents_and_execution_still_rejects_same_size_changes() {
    let f = fixture();
    fs::write(f.target.join("file.txt"), "different txt").unwrap();
    assert_eq!(
        fs::metadata(f.source.join("file.txt")).unwrap().len(),
        fs::metadata(f.target.join("file.txt")).unwrap().len()
    );
    f.engine.preview_cleanup(f.operation).unwrap();
    assert_eq!(f.engine.progress().bytes, 0);
    assert_eq!(
        f.engine.cleanup_originals(request(&f)).unwrap().completed,
        0
    );
    assert!(f.source.join("file.txt").exists());
}

#[test]
fn journal_failures_preserve_recovery_and_stop_cleanup() {
    for before_move in [true, false] {
        let f = fixture();
        let sql = if before_move {
            "CREATE TRIGGER deny_cleanup BEFORE INSERT ON migration_cleanup BEGIN SELECT RAISE(ABORT,'test journal failure'); END"
        } else {
            "CREATE TRIGGER deny_cleanup BEFORE UPDATE ON migration_cleanup BEGIN SELECT RAISE(ABORT,'test journal failure'); END"
        };
        f.engine
            .store
            .connect()
            .unwrap()
            .execute_batch(sql)
            .unwrap();
        assert!(f
            .engine
            .cleanup_originals(request(&f))
            .unwrap_err()
            .starts_with("Journal"));
        assert!(f.target.join("file.txt").exists());
        if before_move {
            assert!(f.source.join("file.txt").exists());
        } else {
            let item = f
                .engine
                .store
                .operation_items(f.operation)
                .unwrap()
                .remove(0);
            let cleanup = item.cleanup.unwrap();
            assert_eq!(cleanup.state, "planned");
            assert_eq!(
                fs::read_to_string(cleanup.holding_path).unwrap(),
                "verified copy"
            );
        }
    }
}
