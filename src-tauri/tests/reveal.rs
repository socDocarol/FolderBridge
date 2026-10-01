use folderbridge_lib::{
    engine::Engine,
    models::{EntryFilter, ScanOptions},
    reveal::entry_target,
};
use std::fs;
use tempfile::tempdir;

#[cfg(windows)]
#[test]
fn refuses_a_file_that_replaces_an_expected_folder() {
    use folderbridge_lib::reveal::{show, RevealTarget};
    let temp = tempdir().unwrap();
    let path = temp.path().join("was-a-folder.txt");
    fs::write(&path, "Do not launch this file.").unwrap();
    let result = show(&RevealTarget {
        path,
        select_file: false,
    });
    assert!(result.unwrap_err().contains("no longer a folder"));
}

#[test]
fn reveals_existing_file_and_nearest_missing_parent_from_the_same_scan() {
    let temp = tempdir().unwrap();
    let source = temp.path().join("source");
    let destination = temp.path().join("destination");
    fs::create_dir_all(source.join("shared/deeper")).unwrap();
    fs::create_dir_all(destination.join("shared")).unwrap();
    fs::write(source.join("shared/deeper/notes, café & plan.txt"), "data").unwrap();
    fs::write(destination.join("destination-only.txt"), "other").unwrap();
    let engine = Engine::open(&temp.path().join("app.sqlite")).unwrap();
    let scan = engine
        .scan(ScanOptions {
            source: source.to_string_lossy().into(),
            destination: Some(destination.to_string_lossy().into()),
            verify_contents: false,
            excluded_extensions: vec![],
            collect_owners: false,
        })
        .unwrap();
    let entries = engine
        .store
        .entries(&EntryFilter {
            scan_id: scan,
            ..Default::default()
        })
        .unwrap()
        .entries;
    let source_file = entries.iter().find(|e| e.status == "source_only").unwrap();
    let existing = entry_target(&engine.store, scan, source_file.id, "source").unwrap();
    assert!(existing.select_file);
    assert_eq!(
        existing.path,
        fs::canonicalize(source.join("shared/deeper/notes, café & plan.txt")).unwrap()
    );
    let missing = entry_target(&engine.store, scan, source_file.id, "destination").unwrap();
    assert!(!missing.select_file);
    assert_eq!(
        missing.path,
        fs::canonicalize(destination.join("shared")).unwrap()
    );
    let destination_file = entries
        .iter()
        .find(|e| e.status == "destination_only")
        .unwrap();
    assert!(
        entry_target(&engine.store, scan, destination_file.id, "destination")
            .unwrap()
            .select_file
    );
    assert_eq!(
        entry_target(&engine.store, scan, destination_file.id, "source")
            .unwrap()
            .path,
        fs::canonicalize(&source).unwrap()
    );
    assert!(entry_target(&engine.store, scan + 1, source_file.id, "source").is_err());
    assert!(entry_target(&engine.store, scan, source_file.id, "unknown").is_err());
    // A stale row must not open outside the root or open an unrelated fallback.
    engine
        .store
        .connect()
        .unwrap()
        .execute(
            "UPDATE entries SET source_relative='../source-other/escape.txt' WHERE id=?",
            [source_file.id],
        )
        .unwrap();
    assert!(entry_target(&engine.store, scan, source_file.id, "source").is_err());
    fs::rename(&destination, temp.path().join("moved-destination")).unwrap();
    assert!(entry_target(&engine.store, scan, destination_file.id, "destination").is_err());
}
