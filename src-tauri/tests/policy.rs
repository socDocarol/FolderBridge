use folderbridge_lib::{
    engine::Engine,
    models::{EntryFilter, FileRules, OperationRequest, ScanOptions},
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
        rules: FileRules::default(),
    }
}

#[test]
fn skip_rules_use_merged_source_size_and_folder_boundaries() {
    let t = tempdir().unwrap();
    let a = t.path().join("a");
    let b = t.path().join("b");
    fs::create_dir(&a).unwrap();
    fs::create_dir(&b).unwrap();
    fs::create_dir(a.join("private")).unwrap();
    fs::create_dir(a.join("privateish")).unwrap();
    for (name, size) in [
        ("private/x.txt", 5),
        ("privateish/x.txt", 5),
        ("small.txt", 2),
        ("min.txt", 3),
        ("max.txt", 7),
        ("large.txt", 8),
        ("skip.LOG", 5),
        ("exact.txt", 5),
    ] {
        fs::write(a.join(name), vec![b'x'; size]).unwrap();
    }
    fs::write(b.join("min.txt"), vec![b'y'; 10]).unwrap();
    let engine = Engine::open(&t.path().join("app.db")).unwrap();
    let mut o = options(&a, &b);
    o.excluded_extensions = vec!["LOG".into()];
    o.rules.excluded_paths = vec!["private\\".into(), "exact.txt".into()];
    o.rules.min_size = Some(3);
    o.rules.max_size = Some(7);
    let scan = engine.scan(o).unwrap();
    let rows = engine
        .store
        .entries(&EntryFilter {
            scan_id: scan,
            ..Default::default()
        })
        .unwrap()
        .entries;
    for name in [
        "private/x.txt",
        "small.txt",
        "large.txt",
        "skip.LOG",
        "exact.txt",
    ] {
        let e = rows.iter().find(|e| e.relative_path == name).unwrap();
        assert_eq!(e.status, "excluded", "{name}");
        assert_eq!(e.migration_state, "skipped");
        assert!(
            !e.rule_reason.is_empty(),
            "Missing exclusion reason for {name}"
        );
        assert!(e.source_hash.is_none());
    }
    for name in ["privateish/x.txt", "max.txt"] {
        assert_eq!(
            rows.iter()
                .find(|e| e.relative_path == name)
                .unwrap()
                .migration_state,
            "ready"
        );
    }
    assert_eq!(
        rows.iter()
            .find(|e| e.relative_path == "min.txt")
            .unwrap()
            .status,
        "different"
    );
}

#[test]
fn approval_is_atomic_and_explicit_approval_preserves_verified_copy_safety() {
    let t = tempdir().unwrap();
    let a = t.path().join("a");
    let b = t.path().join("b");
    fs::create_dir(&a).unwrap();
    fs::create_dir(&b).unwrap();
    fs::write(a.join("a.txt"), "ready").unwrap();
    fs::write(a.join("b.txt"), "review").unwrap();
    fs::write(a.join("conflict.txt"), "source").unwrap();
    fs::write(b.join("conflict.txt"), "target").unwrap();
    let engine = Engine::open(&t.path().join("app.db")).unwrap();
    let mut o = options(&a, &b);
    o.rules.review_extensions = vec![".txt".into()];
    let scan = engine.scan(o).unwrap();
    let rows = engine
        .store
        .entries(&EntryFilter {
            scan_id: scan,
            ..Default::default()
        })
        .unwrap()
        .entries;
    let ids: Vec<_> = rows
        .iter()
        .filter(|e| e.status == "source_only")
        .map(|e| e.id)
        .collect();
    let mut req = OperationRequest {
        scan_id: scan,
        entry_ids: ids.clone(),
        action: "copy_to_destination".into(),
        approved_entry_ids: vec![],
    };
    assert_eq!(engine.preview(&req).unwrap().review_entry_ids.len(), 2);
    let mut mixed = req.clone();
    mixed.entry_ids = rows.iter().map(|e| e.id).collect();
    let preview = engine.preview(&mixed).unwrap();
    assert_eq!(preview.eligible_entry_ids, ids);
    assert_eq!(preview.skipped, 1);
    assert!(engine.execute(req.clone()).is_err());
    assert!(!b.join("a.txt").exists());
    assert!(!b.join("b.txt").exists());
    req.approved_entry_ids = ids;
    engine.execute(req).unwrap();
    assert_eq!(
        fs::read(a.join("a.txt")).unwrap(),
        fs::read(b.join("a.txt")).unwrap()
    );
    let conflict = rows
        .iter()
        .find(|e| e.relative_path == "conflict.txt")
        .unwrap();
    let mut keep = OperationRequest {
        scan_id: scan,
        entry_ids: vec![conflict.id],
        action: "keep_both".into(),
        approved_entry_ids: vec![],
    };
    assert_eq!(
        engine.preview(&keep).unwrap().review_entry_ids,
        vec![conflict.id]
    );
    assert!(engine.execute(keep.clone()).is_err());
    assert_eq!(
        fs::read_to_string(b.join("conflict.txt")).unwrap(),
        "target"
    );
    keep.approved_entry_ids.push(conflict.id);
    engine.execute(keep).unwrap();
    assert_eq!(
        fs::read_to_string(b.join("conflict.txt")).unwrap(),
        "target"
    );
}

#[test]
fn rule_paths_reject_escaping_or_ambiguous_paths() {
    let mut o: ScanOptions =
        serde_json::from_str(r#"{"source":"source","destination":"target"}"#).unwrap();
    assert!(o.rules.excluded_paths.is_empty());
    for path in [
        "../escape",
        "/absolute",
        "C:/drive",
        "folder//",
        "folder//file",
        "./file",
        "folder/../file",
    ] {
        o.rules.excluded_paths = vec![path.into()];
        assert!(
            folderbridge_lib::policy::normalize_options(&mut o).is_err(),
            "accepted {path}"
        );
    }
    o.rules.excluded_paths = vec!["folder/".into(), "folder/file.txt".into()];
    folderbridge_lib::policy::normalize_options(&mut o).unwrap();
}

#[test]
fn review_thresholds_filtering_and_approvals_remain_bound_to_the_scan() {
    let t = tempdir().unwrap();
    let a = t.path().join("a");
    let b = t.path().join("b");
    fs::create_dir(&a).unwrap();
    fs::create_dir(&b).unwrap();
    for (name, size) in [
        ("ready.txt", 5),
        ("review.txt", 6),
        ("access.accdb", 4),
        ("skip.bak", 9),
    ] {
        fs::write(a.join(name), vec![b'x'; size]).unwrap();
    }
    let engine = Engine::open(&t.path().join("app.db")).unwrap();
    let mut o = options(&a, &b);
    o.rules.review_above = Some(5);
    o.rules.review_access_above = Some(3);
    o.excluded_extensions = vec![".bak".into()];
    let first = engine.scan(o.clone()).unwrap();
    engine.store.save_pair("pair", &o).unwrap();
    o.rules.review_above = None;
    engine.store.save_pair("pair", &o).unwrap();
    assert_eq!(
        engine
            .store
            .scan(first)
            .unwrap()
            .options
            .unwrap()
            .rules
            .review_above,
        Some(5)
    );
    let filter = EntryFilter {
        scan_id: first,
        migration_state: "review".into(),
        limit: 1,
        ..Default::default()
    };
    let page = engine.store.entries(&filter).unwrap();
    assert_eq!(page.total, 2);
    assert_eq!(page.entries.len(), 1);
    let approved = engine.store.matching_ids(&filter).unwrap();
    assert_eq!(approved.len(), 2);
    assert_eq!(
        engine
            .store
            .analysis(first)
            .unwrap()
            .migration_states
            .iter()
            .find(|g| g.label == "ready")
            .unwrap()
            .count,
        1
    );
    let csv = t.path().join("review.csv");
    assert_eq!(engine.export_scan(filter, &csv).unwrap(), 2);
    assert!(!fs::read_to_string(csv).unwrap().contains("ready.txt"));
    let all = engine
        .store
        .entries(&EntryFilter {
            scan_id: first,
            ..Default::default()
        })
        .unwrap()
        .entries;
    let skip = all
        .iter()
        .find(|e| e.relative_path == "skip.bak")
        .unwrap()
        .id;
    assert!(engine
        .execute(OperationRequest {
            scan_id: first,
            entry_ids: vec![skip],
            approved_entry_ids: vec![skip],
            action: "copy_to_destination".into()
        })
        .is_err());
    o.rules.review_above = Some(5);
    let second = engine.scan(o).unwrap();
    let second_ids = engine
        .store
        .matching_ids(&EntryFilter {
            scan_id: second,
            migration_state: "review".into(),
            ..Default::default()
        })
        .unwrap();
    assert!(engine
        .execute(OperationRequest {
            scan_id: second,
            entry_ids: second_ids,
            approved_entry_ids: approved,
            action: "copy_to_destination".into()
        })
        .is_err());
    assert_eq!(fs::read_dir(&b).unwrap().count(), 0);
}

#[test]
fn old_databases_and_saved_pairs_load_without_discarding_history() {
    let t = tempdir().unwrap();
    let db = t.path().join("old.db");
    let conn = rusqlite::Connection::open(&db).unwrap();
    conn.execute_batch(r#"CREATE TABLE scans (id INTEGER PRIMARY KEY, source TEXT NOT NULL, destination TEXT, started_at INTEGER NOT NULL, state TEXT NOT NULL, verified INTEGER NOT NULL, files INTEGER NOT NULL DEFAULT 0, errors INTEGER NOT NULL DEFAULT 0);
        CREATE TABLE entries (id INTEGER PRIMARY KEY, scan_id INTEGER NOT NULL, path_key TEXT NOT NULL, relative_path TEXT NOT NULL, source_relative TEXT, destination_relative TEXT, extension TEXT NOT NULL, status TEXT NOT NULL, source_size INTEGER, destination_size INTEGER, source_modified INTEGER, destination_modified INTEGER, source_hash TEXT, destination_hash TEXT, owner TEXT, issue TEXT, UNIQUE(scan_id,path_key));
        CREATE TABLE pairs (id INTEGER PRIMARY KEY, name TEXT NOT NULL, options TEXT NOT NULL);
        INSERT INTO scans VALUES(1,'source','target',1,'complete',1,1,0);
        INSERT INTO entries(id,scan_id,path_key,relative_path,extension,status,source_relative,source_size) VALUES(1,1,'a.txt','a.txt','.txt','source_only','a.txt',5);
        INSERT INTO pairs VALUES(1,'old','{"source":"source","destination":"target","excludedExtensions":[".bak"]}');"#).unwrap();
    drop(conn);
    let engine = Engine::open(&db).unwrap();
    assert!(engine.store.scan(1).unwrap().options.is_none());
    assert_eq!(
        engine.store.pairs().unwrap()[0].options.excluded_extensions,
        vec![".bak"]
    );
    assert!(engine.store.pairs().unwrap()[0]
        .options
        .rules
        .review_above
        .is_none());
    assert_eq!(
        engine
            .store
            .entries(&EntryFilter {
                scan_id: 1,
                ..Default::default()
            })
            .unwrap()
            .total,
        1
    );
    drop(engine);
    Engine::open(&db).unwrap();
}
