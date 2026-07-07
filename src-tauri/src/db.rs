use crate::model::{Document, Folder};
use rusqlite::{params, Connection};
use std::sync::Mutex;

/// App-wide database handle, stored in Tauri state.
pub struct Db(pub Mutex<Connection>);

impl Db {
    pub fn open(path: &std::path::Path) -> rusqlite::Result<Self> {
        let conn = Connection::open(path)?;
        configure(&conn)?;
        migrate(&conn)?;
        backfill_relative_dirs(&conn)?;
        Ok(Db(Mutex::new(conn)))
    }
}

/// Relative directory of `path` under `folder_path`, POSIX-style ("" at root).
/// Mirrors `commands::relative_dir_of`, kept here so the one-time backfill
/// doesn't need to reach into the commands module.
fn rel_dir(folder_path: &str, path: &str) -> String {
    let p = std::path::Path::new(path);
    let root = std::path::Path::new(folder_path);
    let rel = p.strip_prefix(root).unwrap_or(p);
    match rel.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent
            .components()
            .map(|c| c.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/"),
        _ => String::new(),
    }
}

/// Populates `relative_dir` for documents indexed before subfolder support
/// existed, computing it from each row's already-stored path. Runs once.
fn backfill_relative_dirs(conn: &Connection) -> rusqlite::Result<()> {
    if get_setting(conn, "reldir_backfill_v1")?.is_some() {
        return Ok(());
    }
    let rows: Vec<(String, String, String)> = {
        let mut stmt = conn.prepare("SELECT id, folder_path, path FROM documents")?;
        let mapped = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
        mapped.collect::<rusqlite::Result<_>>()?
    };
    for (id, folder_path, path) in rows {
        let rd = rel_dir(&folder_path, &path);
        conn.execute(
            "UPDATE documents SET relative_dir = ?2 WHERE id = ?1",
            params![id, rd],
        )?;
    }
    set_setting(conn, "reldir_backfill_v1", "done")?;
    Ok(())
}

/// Shared connection pragmas. WAL + a generous busy timeout let the main
/// connection and the background thumbnail connection write concurrently
/// without hitting `SQLITE_BUSY`.
pub fn configure(conn: &Connection) -> rusqlite::Result<()> {
    conn.busy_timeout(std::time::Duration::from_secs(30))?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    Ok(())
}

pub(crate) fn migrate(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS folders (
            path       TEXT PRIMARY KEY,
            name       TEXT NOT NULL,
            added_at   TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS documents (
            id            TEXT PRIMARY KEY,
            path          TEXT UNIQUE NOT NULL,
            folder_path   TEXT NOT NULL,
            file_name     TEXT NOT NULL,
            title         TEXT NOT NULL,
            authors       TEXT NOT NULL DEFAULT '[]',
            subject       TEXT,
            keywords      TEXT NOT NULL DEFAULT '[]',
            page_count    INTEGER NOT NULL DEFAULT 0,
            size_bytes    INTEGER NOT NULL DEFAULT 0,
            year          INTEGER,
            thumbnail_path TEXT,
            category      TEXT NOT NULL DEFAULT '',
            tags          TEXT NOT NULL DEFAULT '[]',
            modified_at   TEXT,
            added_at      TEXT NOT NULL,
            status        TEXT NOT NULL DEFAULT 'unread',
            current_page  INTEGER NOT NULL DEFAULT 0,
            FOREIGN KEY (folder_path) REFERENCES folders(path) ON DELETE CASCADE
        );

        CREATE INDEX IF NOT EXISTS idx_documents_folder ON documents(folder_path);

        CREATE TABLE IF NOT EXISTS settings (
            key   TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );
        "#,
    )?;

    // Additive column migrations for databases created before BibTeX support.
    // Duplicate-column errors are expected on already-migrated DBs and ignored.
    for stmt in [
        "ALTER TABLE documents ADD COLUMN doi TEXT",
        "ALTER TABLE documents ADD COLUMN citation_key TEXT",
        "ALTER TABLE documents ADD COLUMN venue TEXT",
        "ALTER TABLE documents ADD COLUMN abstract_text TEXT",
        "ALTER TABLE documents ADD COLUMN metadata_source TEXT NOT NULL DEFAULT 'pdf'",
        "ALTER TABLE documents ADD COLUMN relative_dir TEXT NOT NULL DEFAULT ''",
        "ALTER TABLE documents ADD COLUMN started_at TEXT",
        "ALTER TABLE documents ADD COLUMN finished_at TEXT",
        "ALTER TABLE documents ADD COLUMN last_read_at TEXT",
        "ALTER TABLE documents ADD COLUMN thumb_attempts INTEGER NOT NULL DEFAULT 0",
    ] {
        let _ = conn.execute(stmt, []);
    }

    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS reading_events (
            id      INTEGER PRIMARY KEY AUTOINCREMENT,
            doc_id  TEXT NOT NULL,
            kind    TEXT NOT NULL,
            page    INTEGER,
            at      TEXT NOT NULL,
            FOREIGN KEY (doc_id) REFERENCES documents(id) ON DELETE CASCADE
        );
        CREATE INDEX IF NOT EXISTS idx_reading_events_doc ON reading_events(doc_id);
        CREATE INDEX IF NOT EXISTS idx_reading_events_at ON reading_events(at);
        "#,
    )?;
    Ok(())
}

pub fn get_setting(conn: &Connection, key: &str) -> rusqlite::Result<Option<String>> {
    let mut stmt = conn.prepare("SELECT value FROM settings WHERE key = ?1")?;
    let mut rows = stmt.query_map(params![key], |r| r.get::<_, String>(0))?;
    match rows.next() {
        Some(v) => Ok(Some(v?)),
        None => Ok(None),
    }
}

pub fn set_setting(conn: &Connection, key: &str, value: &str) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )?;
    Ok(())
}

pub fn delete_setting(conn: &Connection, key: &str) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM settings WHERE key = ?1", params![key])?;
    Ok(())
}

fn json_array(values: &[String]) -> String {
    serde_json::to_string(values).unwrap_or_else(|_| "[]".to_string())
}

fn parse_json_array(raw: &str) -> Vec<String> {
    serde_json::from_str(raw).unwrap_or_default()
}

pub fn add_folder(conn: &Connection, path: &str, name: &str, added_at: &str) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO folders (path, name, added_at) VALUES (?1, ?2, ?3)
         ON CONFLICT(path) DO UPDATE SET name = excluded.name",
        params![path, name, added_at],
    )?;
    Ok(())
}

pub fn remove_folder(conn: &Connection, path: &str) -> rusqlite::Result<()> {
    // documents cascade-delete via the foreign key
    conn.execute("DELETE FROM folders WHERE path = ?1", params![path])?;
    Ok(())
}

pub fn list_folders(conn: &Connection) -> rusqlite::Result<Vec<Folder>> {
    let mut stmt = conn.prepare(
        "SELECT f.path, f.name, f.added_at,
                (SELECT COUNT(*) FROM documents d WHERE d.folder_path = f.path)
         FROM folders f ORDER BY f.name COLLATE NOCASE",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(Folder {
            path: r.get(0)?,
            name: r.get(1)?,
            added_at: r.get(2)?,
            document_count: r.get::<_, i64>(3)? as u32,
        })
    })?;
    rows.collect()
}

pub fn upsert_document(conn: &Connection, d: &Document) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO documents
            (id, path, folder_path, file_name, title, authors, subject, keywords,
             page_count, size_bytes, year, thumbnail_path, category, tags,
             modified_at, added_at, status, current_page,
             doi, citation_key, venue, abstract_text, metadata_source, relative_dir,
             started_at, finished_at, last_read_at, thumb_attempts)
         VALUES
            (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18,
             ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27, ?28)
         ON CONFLICT(id) DO UPDATE SET
            folder_path     = excluded.folder_path,
            file_name       = excluded.file_name,
            title           = excluded.title,
            authors         = excluded.authors,
            subject         = excluded.subject,
            keywords        = excluded.keywords,
            page_count      = excluded.page_count,
            size_bytes      = excluded.size_bytes,
            year            = excluded.year,
            category        = excluded.category,
            modified_at     = excluded.modified_at,
            doi             = excluded.doi,
            citation_key    = excluded.citation_key,
            venue           = excluded.venue,
            abstract_text   = excluded.abstract_text,
            metadata_source = excluded.metadata_source,
            relative_dir    = excluded.relative_dir",
        params![
            d.id,
            d.path,
            d.folder_path,
            d.file_name,
            d.title,
            json_array(&d.authors),
            d.subject,
            json_array(&d.keywords),
            d.page_count,
            d.size_bytes as i64,
            d.year,
            d.thumbnail_path,
            d.category,
            json_array(&d.tags),
            d.modified_at,
            d.added_at,
            d.status,
            d.current_page,
            d.doi,
            d.citation_key,
            d.venue,
            d.abstract_text,
            d.metadata_source,
            d.relative_dir,
            d.started_at,
            d.finished_at,
            d.last_read_at,
            d.thumb_attempts,
        ],
    )?;
    Ok(())
}

/// Appends a reading-activity event.
pub fn log_reading_event(
    conn: &Connection,
    doc_id: &str,
    kind: &str,
    page: Option<u32>,
    at: &str,
) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO reading_events (doc_id, kind, page, at) VALUES (?1, ?2, ?3, ?4)",
        params![doc_id, kind, page, at],
    )?;
    Ok(())
}

/// Moves reading events to a new document id (used when a rename changes the
/// path-derived id, so history isn't lost).
pub fn repoint_reading_events(conn: &Connection, from: &str, to: &str) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE reading_events SET doc_id = ?2 WHERE doc_id = ?1",
        params![from, to],
    )?;
    Ok(())
}

pub fn list_reading_events(conn: &Connection) -> rusqlite::Result<Vec<crate::model::ReadingEvent>> {
    let mut stmt = conn.prepare("SELECT doc_id, kind, page, at FROM reading_events ORDER BY at")?;
    let rows = stmt.query_map([], |r| {
        Ok(crate::model::ReadingEvent {
            doc_id: r.get(0)?,
            kind: r.get(1)?,
            page: r.get::<_, Option<i64>>(2)?.map(|p| p as u32),
            at: r.get(3)?,
        })
    })?;
    rows.collect()
}

/// Overwrites a document's reading state (status / progress / timestamps).
pub fn set_reading_state(
    conn: &Connection,
    id: &str,
    status: &str,
    current_page: u32,
    started_at: Option<&str>,
    finished_at: Option<&str>,
    last_read_at: Option<&str>,
) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE documents SET status = ?2, current_page = ?3, started_at = ?4,
                finished_at = ?5, last_read_at = ?6 WHERE id = ?1",
        params![id, status, current_page, started_at, finished_at, last_read_at],
    )?;
    Ok(())
}

/// After this many failed render attempts a document is considered
/// un-renderable and stops being offered for automatic retry.
pub const MAX_THUMB_ATTEMPTS: u32 = 3;

/// Records a freshly generated thumbnail path and clears the failure counter.
pub fn set_thumbnail(conn: &Connection, id: &str, thumb: &str) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE documents SET thumbnail_path = ?2, thumb_attempts = 0 WHERE id = ?1",
        params![id, thumb],
    )?;
    Ok(())
}

/// Bumps the failure counter after a render attempt didn't produce a thumbnail.
pub fn increment_thumb_attempt(conn: &Connection, id: &str) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE documents SET thumb_attempts = thumb_attempts + 1 WHERE id = ?1",
        params![id],
    )?;
    Ok(())
}

/// Immediately retires a document from retry (used when qlmanage hangs — the
/// PDF is unsupported, so there's no point trying again).
pub fn retire_thumbnail(conn: &Connection, id: &str) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE documents SET thumb_attempts = ?2 WHERE id = ?1",
        params![id, MAX_THUMB_ATTEMPTS],
    )?;
    Ok(())
}

/// Clears the failure counter for still-missing thumbnails in a folder so they
/// become eligible for retry again (used by a forced retry).
pub fn reset_thumb_attempts(conn: &Connection, folder_path: &str) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE documents SET thumb_attempts = 0
         WHERE folder_path = ?1 AND (thumbnail_path IS NULL OR thumbnail_path = '')",
        params![folder_path],
    )?;
    Ok(())
}

fn row_to_document(r: &rusqlite::Row) -> rusqlite::Result<Document> {
    Ok(Document {
        id: r.get(0)?,
        path: r.get(1)?,
        folder_path: r.get(2)?,
        file_name: r.get(3)?,
        title: r.get(4)?,
        authors: parse_json_array(&r.get::<_, String>(5)?),
        subject: r.get(6)?,
        keywords: parse_json_array(&r.get::<_, String>(7)?),
        page_count: r.get::<_, i64>(8)? as u32,
        size_bytes: r.get::<_, i64>(9)? as u64,
        year: r.get(10)?,
        thumbnail_path: r.get(11)?,
        category: r.get(12)?,
        tags: parse_json_array(&r.get::<_, String>(13)?),
        modified_at: r.get(14)?,
        added_at: r.get(15)?,
        status: r.get(16)?,
        current_page: r.get::<_, i64>(17)? as u32,
        doi: r.get(18)?,
        citation_key: r.get(19)?,
        venue: r.get(20)?,
        abstract_text: r.get(21)?,
        metadata_source: r.get(22)?,
        relative_dir: r.get(23)?,
        started_at: r.get(24)?,
        finished_at: r.get(25)?,
        last_read_at: r.get(26)?,
        thumb_attempts: r.get::<_, i64>(27)? as u32,
    })
}

const DOC_COLUMNS: &str = "id, path, folder_path, file_name, title, authors, subject, keywords, \
     page_count, size_bytes, year, thumbnail_path, category, tags, modified_at, added_at, \
     status, current_page, doi, citation_key, venue, abstract_text, metadata_source, relative_dir, \
     started_at, finished_at, last_read_at, thumb_attempts";

pub fn list_documents(conn: &Connection) -> rusqlite::Result<Vec<Document>> {
    let sql = format!("SELECT {DOC_COLUMNS} FROM documents ORDER BY title COLLATE NOCASE");
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([], row_to_document)?;
    rows.collect()
}

pub fn get_document(conn: &Connection, id: &str) -> rusqlite::Result<Option<Document>> {
    let sql = format!("SELECT {DOC_COLUMNS} FROM documents WHERE id = ?1");
    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query_map(params![id], row_to_document)?;
    match rows.next() {
        Some(doc) => Ok(Some(doc?)),
        None => Ok(None),
    }
}

pub fn get_document_by_path(conn: &Connection, path: &str) -> rusqlite::Result<Option<Document>> {
    let sql = format!("SELECT {DOC_COLUMNS} FROM documents WHERE path = ?1");
    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query_map(params![path], row_to_document)?;
    match rows.next() {
        Some(doc) => Ok(Some(doc?)),
        None => Ok(None),
    }
}

pub fn delete_document(conn: &Connection, id: &str) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM documents WHERE id = ?1", params![id])?;
    Ok(())
}

/// Documents in a folder that still need a thumbnail and haven't exhausted
/// their retry budget (so permanently un-renderable files are left alone).
pub fn documents_missing_thumbnails(
    conn: &Connection,
    folder_path: &str,
) -> rusqlite::Result<Vec<(String, String)>> {
    let mut stmt = conn.prepare(
        "SELECT id, path FROM documents
         WHERE folder_path = ?1 AND (thumbnail_path IS NULL OR thumbnail_path = '')
           AND thumb_attempts < ?2",
    )?;
    let rows = stmt.query_map(params![folder_path, MAX_THUMB_ATTEMPTS], |r| {
        Ok((r.get(0)?, r.get(1)?))
    })?;
    rows.collect()
}

/// (id, path) for every document currently tracked under a folder — used to
/// diff against a fresh directory walk and find documents whose file has
/// disappeared (deleted, moved elsewhere, or trashed).
pub fn document_paths_in_folder(conn: &Connection, folder_path: &str) -> rusqlite::Result<Vec<(String, String)>> {
    let mut stmt = conn.prepare("SELECT id, path FROM documents WHERE folder_path = ?1")?;
    let rows = stmt.query_map(params![folder_path], |r| Ok((r.get(0)?, r.get(1)?)))?;
    rows.collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Document;

    fn sample_doc(id: &str, folder: &str, title: &str) -> Document {
        Document {
            id: id.to_string(),
            path: format!("/tmp/{id}.pdf"),
            folder_path: folder.to_string(),
            file_name: format!("{id}.pdf"),
            title: title.to_string(),
            authors: vec!["Ada Lovelace".to_string()],
            subject: Some("Notes".to_string()),
            keywords: vec!["a".to_string(), "b".to_string()],
            page_count: 12,
            size_bytes: 3456,
            year: Some(1843),
            doi: None,
            citation_key: None,
            venue: None,
            abstract_text: None,
            metadata_source: "pdf".to_string(),
            thumbnail_path: None,
            category: "Papers".to_string(),
            relative_dir: String::new(),
            tags: vec![],
            modified_at: None,
            added_at: "2026-01-01T00:00:00Z".to_string(),
            status: "unread".to_string(),
            current_page: 0,
            started_at: None,
            finished_at: None,
            last_read_at: None,
            thumb_attempts: 0,
        }
    }

    #[test]
    fn upsert_list_and_cascade() {
        let conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "foreign_keys", "ON").unwrap();
        migrate(&conn).unwrap();

        add_folder(&conn, "/lib", "lib", "2026-01-01T00:00:00Z").unwrap();
        upsert_document(&conn, &sample_doc("d1", "/lib", "First")).unwrap();
        upsert_document(&conn, &sample_doc("d2", "/lib", "Second")).unwrap();

        let docs = list_documents(&conn).unwrap();
        assert_eq!(docs.len(), 2);
        // Ordered by title: "First" then "Second".
        assert_eq!(docs[0].title, "First");
        assert_eq!(docs[0].authors, vec!["Ada Lovelace"]);
        assert_eq!(docs[0].keywords, vec!["a", "b"]);

        // Thumbnails start missing, then get recorded.
        let missing = documents_missing_thumbnails(&conn, "/lib").unwrap();
        assert_eq!(missing.len(), 2);
        set_thumbnail(&conn, "d1", "/cache/d1.png").unwrap();
        assert_eq!(documents_missing_thumbnails(&conn, "/lib").unwrap().len(), 1);

        let folders = list_folders(&conn).unwrap();
        assert_eq!(folders[0].document_count, 2);

        // Removing the folder cascades to its documents.
        remove_folder(&conn, "/lib").unwrap();
        assert_eq!(list_documents(&conn).unwrap().len(), 0);
    }

    #[test]
    fn backfill_computes_relative_dir_from_paths() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        add_folder(&conn, "/lib/Thesis", "Thesis", "2026-01-01T00:00:00Z").unwrap();

        // Simulate pre-feature rows: correct paths, empty relative_dir.
        let mut root_doc = sample_doc("r1", "/lib/Thesis", "Root Doc");
        root_doc.path = "/lib/Thesis/root.pdf".to_string();
        let mut nested_doc = sample_doc("n1", "/lib/Thesis", "Nested Doc");
        nested_doc.path = "/lib/Thesis/Aristotle/2ndary/paper.pdf".to_string();
        upsert_document(&conn, &root_doc).unwrap();
        upsert_document(&conn, &nested_doc).unwrap();

        backfill_relative_dirs(&conn).unwrap();

        let docs = list_documents(&conn).unwrap();
        let root = docs.iter().find(|d| d.id == "r1").unwrap();
        let nested = docs.iter().find(|d| d.id == "n1").unwrap();
        assert_eq!(root.relative_dir, "");
        assert_eq!(nested.relative_dir, "Aristotle/2ndary");

        // Marked done, so it won't clobber values on the next open.
        assert_eq!(get_setting(&conn, "reldir_backfill_v1").unwrap().as_deref(), Some("done"));
    }

    #[test]
    fn reading_events_log_repoint_and_cascade() {
        let conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "foreign_keys", "ON").unwrap();
        migrate(&conn).unwrap();

        add_folder(&conn, "/lib", "lib", "2026-01-01T00:00:00Z").unwrap();
        upsert_document(&conn, &sample_doc("d1", "/lib", "First")).unwrap();

        log_reading_event(&conn, "d1", "open", None, "2026-07-01T10:00:00Z").unwrap();
        log_reading_event(&conn, "d1", "progress", Some(12), "2026-07-01T10:30:00Z").unwrap();

        let events = list_reading_events(&conn).unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].kind, "open");
        assert_eq!(events[1].page, Some(12));

        // Reading state persists.
        set_reading_state(&conn, "d1", "in_progress", 12, Some("2026-07-01T10:00:00Z"), None, Some("2026-07-01T10:30:00Z")).unwrap();
        let d = get_document(&conn, "d1").unwrap().unwrap();
        assert_eq!(d.status, "in_progress");
        assert_eq!(d.current_page, 12);
        assert_eq!(d.last_read_at.as_deref(), Some("2026-07-01T10:30:00Z"));

        // Rename: history carries over to the new id.
        upsert_document(&conn, &sample_doc("d2", "/lib", "First Renamed")).unwrap();
        repoint_reading_events(&conn, "d1", "d2").unwrap();
        delete_document(&conn, "d1").unwrap();
        let events = list_reading_events(&conn).unwrap();
        assert_eq!(events.len(), 2);
        assert!(events.iter().all(|e| e.doc_id == "d2"));

        // Removing the folder cascades documents AND their reading events.
        remove_folder(&conn, "/lib").unwrap();
        assert_eq!(list_reading_events(&conn).unwrap().len(), 0);
    }

    #[test]
    fn thumb_attempts_gate_and_reset_retry_eligibility() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        add_folder(&conn, "/lib", "lib", "2026-01-01T00:00:00Z").unwrap();
        upsert_document(&conn, &sample_doc("d1", "/lib", "Doc")).unwrap();

        // Missing thumbnail, no attempts yet -> eligible for retry.
        assert_eq!(documents_missing_thumbnails(&conn, "/lib").unwrap().len(), 1);

        // Fail up to the ceiling -> no longer offered.
        for _ in 0..MAX_THUMB_ATTEMPTS {
            increment_thumb_attempt(&conn, "d1").unwrap();
        }
        assert_eq!(
            documents_missing_thumbnails(&conn, "/lib").unwrap().len(),
            0,
            "exhausted docs should not be retried"
        );
        assert_eq!(get_document(&conn, "d1").unwrap().unwrap().thumb_attempts, MAX_THUMB_ATTEMPTS);

        // A forced reset makes it eligible again.
        reset_thumb_attempts(&conn, "/lib").unwrap();
        assert_eq!(documents_missing_thumbnails(&conn, "/lib").unwrap().len(), 1);

        // Retire short-circuits straight to the ceiling (used on a hang).
        retire_thumbnail(&conn, "d1").unwrap();
        assert_eq!(documents_missing_thumbnails(&conn, "/lib").unwrap().len(), 0);

        // A successful render clears the counter and (trivially) removes it
        // from the missing set.
        set_thumbnail(&conn, "d1", "/cache/d1.png").unwrap();
        assert_eq!(get_document(&conn, "d1").unwrap().unwrap().thumb_attempts, 0);
        assert_eq!(documents_missing_thumbnails(&conn, "/lib").unwrap().len(), 0);
    }
}
