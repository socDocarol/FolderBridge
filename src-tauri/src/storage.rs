use crate::models::*;
use rusqlite::{params, Connection, OptionalExtension, Row};
use std::{
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

pub type Result<T> = std::result::Result<T, String>;
pub fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}
pub fn db_error(error: rusqlite::Error) -> String {
    error.to_string()
}

#[derive(Clone)]
pub struct Store {
    pub path: PathBuf,
}

const COLUMNS: &str = "id,scan_id,relative_path,source_relative,destination_relative,extension,status,source_size,destination_size,source_modified,destination_modified,source_hash,destination_hash,owner,issue";

fn number(row: &Row<'_>, index: usize) -> rusqlite::Result<u64> {
    Ok(row.get::<_, i64>(index)?.max(0) as u64)
}
fn optional_number(row: &Row<'_>, index: usize) -> rusqlite::Result<Option<u64>> {
    Ok(row.get::<_, Option<i64>>(index)?.map(|n| n.max(0) as u64))
}

pub fn read_entry(row: &Row<'_>) -> rusqlite::Result<Entry> {
    Ok(Entry {
        id: row.get(0)?,
        scan_id: row.get(1)?,
        relative_path: row.get(2)?,
        source_relative: row.get(3)?,
        destination_relative: row.get(4)?,
        extension: row.get(5)?,
        status: row.get(6)?,
        source_size: optional_number(row, 7)?,
        destination_size: optional_number(row, 8)?,
        source_modified: row.get(9)?,
        destination_modified: row.get(10)?,
        source_hash: row.get(11)?,
        destination_hash: row.get(12)?,
        owner: row.get(13)?,
        issue: row.get(14)?,
    })
}

impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let store = Self {
            path: path.to_owned(),
        };
        let connection = store.connect()?;
        connection.execute_batch("
            PRAGMA journal_mode=WAL;
            CREATE TABLE IF NOT EXISTS scans (
                id INTEGER PRIMARY KEY, source TEXT NOT NULL, destination TEXT, started_at INTEGER NOT NULL,
                state TEXT NOT NULL, verified INTEGER NOT NULL, files INTEGER NOT NULL DEFAULT 0, errors INTEGER NOT NULL DEFAULT 0);
            CREATE TABLE IF NOT EXISTS entries (
                id INTEGER PRIMARY KEY, scan_id INTEGER NOT NULL, path_key TEXT NOT NULL, relative_path TEXT NOT NULL,
                source_relative TEXT, destination_relative TEXT, extension TEXT NOT NULL, status TEXT NOT NULL,
                source_size INTEGER, destination_size INTEGER, source_modified INTEGER, destination_modified INTEGER,
                source_hash TEXT, destination_hash TEXT, owner TEXT, issue TEXT,
                UNIQUE(scan_id,path_key));
            CREATE INDEX IF NOT EXISTS entries_scan_status ON entries(scan_id,status);
            CREATE INDEX IF NOT EXISTS entries_scan_path ON entries(scan_id,relative_path COLLATE NOCASE);
            CREATE TABLE IF NOT EXISTS operations (
                id INTEGER PRIMARY KEY, scan_id INTEGER NOT NULL, action TEXT NOT NULL, started_at INTEGER NOT NULL,
                state TEXT NOT NULL, completed INTEGER NOT NULL DEFAULT 0, skipped INTEGER NOT NULL DEFAULT 0, errors INTEGER NOT NULL DEFAULT 0);
            CREATE TABLE IF NOT EXISTS operation_items (
                id INTEGER PRIMARY KEY, operation_id INTEGER NOT NULL, relative_path TEXT NOT NULL,
                original_path TEXT NOT NULL, target_path TEXT NOT NULL, state TEXT NOT NULL, message TEXT NOT NULL, content_hash TEXT);
            CREATE TABLE IF NOT EXISTS copy_proofs (
                item_id INTEGER PRIMARY KEY, identity TEXT, issue TEXT);
            CREATE TABLE IF NOT EXISTS migration_cleanup (
                item_id INTEGER PRIMARY KEY, method TEXT NOT NULL, state TEXT NOT NULL,
                holding_path TEXT NOT NULL, message TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS pairs (id INTEGER PRIMARY KEY, name TEXT NOT NULL, options TEXT NOT NULL);
            UPDATE scans SET state='interrupted' WHERE state='running';
            UPDATE operations SET state='interrupted' WHERE state='running';
        ").map_err(db_error)?;
        Ok(store)
    }

    pub fn connect(&self) -> Result<Connection> {
        let conn = Connection::open(&self.path).map_err(db_error)?;
        conn.busy_timeout(Duration::from_secs(10))
            .map_err(db_error)?;
        Ok(conn)
    }

    pub fn scans(&self) -> Result<Vec<Scan>> {
        self.scans_page(0)
    }

    pub fn scans_page(&self, offset: u64) -> Result<Vec<Scan>> {
        let conn = self.connect()?;
        let mut statement = conn.prepare("SELECT id,source,destination,started_at,state,verified,files,errors FROM scans ORDER BY id DESC LIMIT 100 OFFSET ?").map_err(db_error)?;
        let rows = statement
            .query_map([offset.min(i64::MAX as u64) as i64], |r| {
                Ok(Scan {
                    id: r.get(0)?,
                    source: r.get(1)?,
                    destination: r.get(2)?,
                    started_at: r.get(3)?,
                    state: r.get(4)?,
                    verified: r.get(5)?,
                    files: number(r, 6)?,
                    errors: number(r, 7)?,
                })
            })
            .map_err(db_error)?;
        rows.collect::<rusqlite::Result<Vec<_>>>().map_err(db_error)
    }

    pub fn scan(&self, id: i64) -> Result<Scan> {
        let conn = self.connect()?;
        conn.query_row("SELECT id,source,destination,started_at,state,verified,files,errors FROM scans WHERE id=?", [id], |r| Ok(Scan {id:r.get(0)?,source:r.get(1)?,destination:r.get(2)?,started_at:r.get(3)?,state:r.get(4)?,verified:r.get(5)?,files:number(r,6)?,errors:number(r,7)?})).map_err(db_error)
    }

    pub fn entry(conn: &Connection, scan_id: i64, id: i64) -> Result<Entry> {
        conn.prepare_cached(&format!(
            "SELECT {COLUMNS} FROM entries WHERE scan_id=? AND id=?"
        ))
        .map_err(db_error)?
        .query_row(params![scan_id, id], read_entry)
        .map_err(db_error)
    }

    pub fn batch(&self, scan_id: i64, after: i64) -> Result<Vec<Entry>> {
        let conn = self.connect()?;
        let mut stmt = conn
            .prepare(&format!(
                "SELECT {COLUMNS} FROM entries WHERE scan_id=? AND id>? ORDER BY id LIMIT 128"
            ))
            .map_err(db_error)?;
        let rows = stmt
            .query_map(params![scan_id, after], read_entry)
            .map_err(db_error)?;
        rows.collect::<rusqlite::Result<Vec<_>>>().map_err(db_error)
    }

    pub fn entries(&self, filter: &EntryFilter) -> Result<EntryPage> {
        let conn = self.connect()?;
        let conditions = "scan_id=?1 AND (?2='' OR instr(lower(relative_path),lower(?2))>0) AND (?3='' OR status=?3) AND (?4='' OR extension=?4) AND max(coalesce(source_size,0),coalesce(destination_size,0))>=?5";
        let total = conn
            .query_row(
                &format!("SELECT count(*) FROM entries WHERE {conditions}"),
                params![
                    filter.scan_id,
                    filter.search,
                    filter.status,
                    filter.extension,
                    filter.min_size.min(i64::MAX as u64) as i64
                ],
                |r| number(r, 0),
            )
            .map_err(db_error)?;
        let mut stmt = conn.prepare(&format!("SELECT {COLUMNS} FROM entries WHERE {conditions} ORDER BY relative_path COLLATE NOCASE LIMIT ?6 OFFSET ?7")).map_err(db_error)?;
        let limit = if filter.limit == 0 {
            100
        } else {
            filter.limit.min(500)
        };
        let rows = stmt
            .query_map(
                params![
                    filter.scan_id,
                    filter.search,
                    filter.status,
                    filter.extension,
                    filter.min_size.min(i64::MAX as u64) as i64,
                    limit as i64,
                    filter.offset.min(i64::MAX as u64) as i64
                ],
                read_entry,
            )
            .map_err(db_error)?;
        Ok(EntryPage {
            entries: rows
                .collect::<rusqlite::Result<Vec<_>>>()
                .map_err(db_error)?,
            total,
        })
    }

    pub fn matching_ids(&self, filter: &EntryFilter) -> Result<Vec<i64>> {
        let conn = self.connect()?;
        let mut stmt=conn.prepare("SELECT id FROM entries WHERE scan_id=?1 AND (?2='' OR instr(lower(relative_path),lower(?2))>0) AND (?3='' OR status=?3) AND (?4='' OR extension=?4) AND max(coalesce(source_size,0),coalesce(destination_size,0))>=?5 ORDER BY id LIMIT 10001").map_err(db_error)?;
        let rows = stmt
            .query_map(
                params![
                    filter.scan_id,
                    filter.search,
                    filter.status,
                    filter.extension,
                    filter.min_size.min(i64::MAX as u64) as i64
                ],
                |r| r.get(0),
            )
            .map_err(db_error)?;
        let ids = rows
            .collect::<rusqlite::Result<Vec<i64>>>()
            .map_err(db_error)?;
        if ids.len() > 10000 {
            return Err("More than 10,000 files match. Narrow the folder, status, or file-type filter before selecting all.".into());
        }
        Ok(ids)
    }

    pub fn analysis(&self, id: i64) -> Result<Analysis> {
        let conn = self.connect()?;
        let group = |expression: &str| -> Result<Vec<GroupTotal>> {
            let mut stmt = conn.prepare(&format!("SELECT {expression} AS label,count(*),sum(coalesce(source_size,destination_size,0)) FROM entries WHERE scan_id=? GROUP BY label ORDER BY 3 DESC")).map_err(db_error)?;
            let rows = stmt
                .query_map([id], |r| {
                    Ok(GroupTotal {
                        label: r.get(0)?,
                        count: number(r, 1)?,
                        bytes: number(r, 2)?,
                    })
                })
                .map_err(db_error)?;
            rows.collect::<rusqlite::Result<Vec<_>>>().map_err(db_error)
        };
        let totals = conn.query_row("SELECT coalesce(sum(source_size),0),coalesce(sum(destination_size),0),coalesce(sum(CASE WHEN status='identical' THEN destination_size ELSE 0 END),0) FROM entries WHERE scan_id=?", [id], |r| Ok((number(r,0)?,number(r,1)?,number(r,2)?))).map_err(db_error)?;
        Ok(Analysis { statuses:group("status")?, extensions:group("CASE WHEN extension='' THEN '(no extension)' ELSE extension END")?,
            folders:group("CASE WHEN instr(relative_path,'/')>0 THEN substr(relative_path,1,instr(relative_path,'/')-1) ELSE '(root)' END")?,
            source_bytes:totals.0,destination_bytes:totals.1,duplicate_bytes:totals.2,size_statistics:self.size_statistics(id)? })
    }

    pub fn size_statistics(&self, id: i64) -> Result<Vec<SizeStatistics>> {
        // SQLite orders the inventory; only aggregate rows cross into application memory.
        let quantile = |p: &str| {
            format!("sum(CASE WHEN idx=CAST((n-1)*{p} AS INTEGER) THEN size*(1-((n-1)*{p}-CAST((n-1)*{p} AS INTEGER))) WHEN idx=CAST((n-1)*{p} AS INTEGER)+1 THEN size*((n-1)*{p}-CAST((n-1)*{p} AS INTEGER)) ELSE 0 END)")
        };
        let sql=format!("WITH ranked AS (SELECT extension,coalesce(source_size,destination_size) AS size,row_number() OVER(PARTITION BY extension ORDER BY coalesce(source_size,destination_size))-1 AS idx,count(*) OVER(PARTITION BY extension) AS n FROM entries WHERE scan_id=? AND coalesce(source_size,destination_size) IS NOT NULL) SELECT extension,count(*),avg(size),{},{},{} FROM ranked GROUP BY extension ORDER BY avg(size) DESC",quantile("0.5"),quantile("0.25"),quantile("0.75"));
        let conn = self.connect()?;
        let mut statement = conn.prepare(&sql).map_err(db_error)?;
        let rows = statement
            .query_map([id], |r| {
                Ok(SizeStatistics {
                    extension: r.get(0)?,
                    count: number(r, 1)?,
                    mean: r.get(2)?,
                    median: r.get(3)?,
                    lower_quartile: r.get(4)?,
                    upper_quartile: r.get(5)?,
                })
            })
            .map_err(db_error)?;
        rows.collect::<rusqlite::Result<Vec<_>>>().map_err(db_error)
    }

    pub fn operations(&self) -> Result<Vec<Operation>> {
        self.operations_page(0)
    }

    pub fn operations_page(&self, offset: u64) -> Result<Vec<Operation>> {
        let conn = self.connect()?;
        let mut stmt = conn.prepare("SELECT id,scan_id,action,started_at,state,completed,skipped,errors FROM operations ORDER BY id DESC LIMIT 100 OFFSET ?").map_err(db_error)?;
        let rows = stmt
            .query_map([offset.min(i64::MAX as u64) as i64], |r| {
                Ok(Operation {
                    id: r.get(0)?,
                    scan_id: r.get(1)?,
                    action: r.get(2)?,
                    started_at: r.get(3)?,
                    state: r.get(4)?,
                    completed: number(r, 5)?,
                    skipped: number(r, 6)?,
                    errors: number(r, 7)?,
                })
            })
            .map_err(db_error)?;
        rows.collect::<rusqlite::Result<Vec<_>>>().map_err(db_error)
    }

    pub fn operation(&self, id: i64) -> Result<Operation> {
        self.connect()?.query_row("SELECT id,scan_id,action,started_at,state,completed,skipped,errors FROM operations WHERE id=?",[id],|r|Ok(Operation{id:r.get(0)?,scan_id:r.get(1)?,action:r.get(2)?,started_at:r.get(3)?,state:r.get(4)?,completed:number(r,5)?,skipped:number(r,6)?,errors:number(r,7)?})).map_err(db_error)
    }

    pub fn operation_items(&self, id: i64) -> Result<Vec<OperationItem>> {
        let conn = self.connect()?;
        let mut stmt = conn.prepare("SELECT i.id,i.operation_id,i.relative_path,i.original_path,i.target_path,i.state,i.message,i.content_hash,c.method,c.state,c.holding_path,c.message FROM operation_items i LEFT JOIN migration_cleanup c ON c.item_id=i.id WHERE i.operation_id=? ORDER BY i.id").map_err(db_error)?;
        let rows = stmt
            .query_map([id], |r| {
                Ok(OperationItem {
                    id: r.get(0)?,
                    operation_id: r.get(1)?,
                    relative_path: r.get(2)?,
                    original_path: r.get(3)?,
                    target_path: r.get(4)?,
                    state: r.get(5)?,
                    message: r.get(6)?,
                    content_hash: r.get(7)?,
                    cleanup: match r.get::<_, Option<String>>(8)? {
                        Some(method) => Some(CleanupRecord {
                            method,
                            state: r.get(9)?,
                            holding_path: r.get(10)?,
                            message: r.get(11)?,
                        }),
                        None => None,
                    },
                })
            })
            .map_err(db_error)?;
        rows.collect::<rusqlite::Result<Vec<_>>>().map_err(db_error)
    }

    pub fn pairs(&self) -> Result<Vec<SavedPair>> {
        let conn = self.connect()?;
        let mut stmt = conn
            .prepare("SELECT id,name,options FROM pairs ORDER BY name COLLATE NOCASE")
            .map_err(db_error)?;
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })
            .map_err(db_error)?;
        rows.map(|r| {
            let (id, name, options) = r.map_err(db_error)?;
            Ok(SavedPair {
                id,
                name,
                options: serde_json::from_str(&options).map_err(|e| e.to_string())?,
            })
        })
        .collect()
    }

    pub fn save_pair(&self, name: &str, options: &ScanOptions) -> Result<()> {
        if name.trim().is_empty() {
            return Err("Enter a name for this folder pair.".into());
        }
        let conn = self.connect()?;
        let existing: Option<i64> = conn
            .query_row("SELECT id FROM pairs WHERE name=?", [name.trim()], |r| {
                r.get(0)
            })
            .optional()
            .map_err(db_error)?;
        let json = serde_json::to_string(options).map_err(|e| e.to_string())?;
        if let Some(id) = existing {
            conn.execute("UPDATE pairs SET options=? WHERE id=?", params![json, id])
                .map_err(db_error)?;
        } else {
            conn.execute(
                "INSERT INTO pairs(name,options) VALUES(?,?)",
                params![name.trim(), json],
            )
            .map_err(db_error)?;
        }
        Ok(())
    }

    pub fn delete_pair(&self, id: i64) -> Result<()> {
        self.connect()?
            .execute("DELETE FROM pairs WHERE id=?", [id])
            .map_err(db_error)?;
        Ok(())
    }
}
