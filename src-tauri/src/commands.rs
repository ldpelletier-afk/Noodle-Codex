use crate::bib::BibIndex;
use crate::db::{self, Db};
use crate::model::{Document, Folder, ScanProgress};
use crate::pdf;
use crate::thumb;
use chrono::{DateTime, Local, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use tauri::{AppHandle, Emitter, Manager, State};
use walkdir::WalkDir;

const THUMB_SIZE: u32 = 640;
const BIBTEX_PATH_KEY: &str = "bibtex_path";

/// Folders whose thumbnails are being rendered right now. Guards against
/// spawning a second, colliding render job for a folder that's already in
/// flight (e.g. when the user re-adds a folder mid-scan).
fn active_thumbnail_jobs() -> &'static Mutex<HashSet<String>> {
    static JOBS: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    JOBS.get_or_init(|| Mutex::new(HashSet::new()))
}

/// Resolves the SQLite file path and thumbnail cache root for this app.
pub struct AppPaths {
    pub db_file: PathBuf,
    pub cache_root: PathBuf,
}

pub fn app_paths(app: &AppHandle) -> Result<AppPaths, String> {
    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("no app data dir: {e}"))?;
    let cache_root = app
        .path()
        .app_cache_dir()
        .map_err(|e| format!("no app cache dir: {e}"))?;
    std::fs::create_dir_all(&data_dir).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&cache_root).map_err(|e| e.to_string())?;
    Ok(AppPaths {
        db_file: data_dir.join("codex.db"),
        cache_root,
    })
}

fn iso_from_systemtime(t: std::time::SystemTime) -> String {
    let dt: DateTime<Utc> = t.into();
    dt.to_rfc3339()
}

fn is_pdf(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("pdf"))
        .unwrap_or(false)
}

fn is_hidden(entry: &walkdir::DirEntry) -> bool {
    entry
        .file_name()
        .to_str()
        .map(|s| s.starts_with('.'))
        .unwrap_or(false)
}

fn collect_pdfs(root: &Path) -> Vec<PathBuf> {
    WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| !is_hidden(e))
        .filter_map(|e| e.ok())
        .map(|e| e.into_path())
        .filter(|p| p.is_file() && is_pdf(p))
        .collect()
}

/// The document's containing directory relative to the scanned root, in
/// POSIX form ("" when the file sits directly in the root). This is what
/// lets Codex mirror the folder's own subfolder structure.
fn relative_dir_of(root: &Path, pdf_path: &Path) -> String {
    let rel = pdf_path.strip_prefix(root).unwrap_or(pdf_path);
    match rel.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => {
            parent
                .components()
                .map(|c| c.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join("/")
        }
        _ => String::new(),
    }
}

fn build_document(path: &Path, folder_path: &str, category: &str) -> Document {
    let meta = pdf::extract_meta(path);
    let title = pdf::readable_title(&meta, path);
    let fs_meta = std::fs::metadata(path).ok();
    let size_bytes = fs_meta.as_ref().map(|m| m.len()).unwrap_or(0);
    let modified_at = fs_meta
        .as_ref()
        .and_then(|m| m.modified().ok())
        .map(iso_from_systemtime);

    Document {
        id: pdf::doc_id(path),
        path: path.to_string_lossy().to_string(),
        folder_path: folder_path.to_string(),
        file_name: path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default(),
        title,
        authors: meta.authors,
        subject: meta.subject,
        keywords: meta.keywords,
        page_count: meta.page_count,
        size_bytes,
        year: meta.year,
        doi: None,
        citation_key: None,
        venue: None,
        abstract_text: None,
        metadata_source: "pdf".to_string(),
        thumbnail_path: None,
        category: category.to_string(),
        relative_dir: relative_dir_of(Path::new(folder_path), path),
        tags: Vec::new(),
        modified_at,
        added_at: Local::now().to_rfc3339(),
        status: "unread".to_string(),
        current_page: 0,
        started_at: None,
        finished_at: None,
        last_read_at: None,
        thumb_attempts: 0,
    }
}

/// Overwrites a document's bibliographic fields from a matched BibTeX entry.
/// PDF-derived title/authors are only replaced when the entry actually has
/// them, so a sparse entry never blanks out good PDF metadata.
fn enrich_from_bib(doc: &mut Document, entry: &crate::bib::BibEntry) {
    if let Some(t) = &entry.title {
        doc.title = t.clone();
    }
    if !entry.authors.is_empty() {
        doc.authors = entry.authors.clone();
    }
    if entry.year.is_some() {
        doc.year = entry.year;
    }
    if !entry.keywords.is_empty() {
        doc.keywords = entry.keywords.clone();
    }
    doc.doi = entry.doi.clone();
    doc.citation_key = Some(entry.key.clone());
    doc.venue = entry.venue.clone();
    doc.abstract_text = entry.abstract_text.clone();
    doc.metadata_source = "bibtex".to_string();
}

/// Loads the central BibTeX index (if one is attached) and any `.bib` files
/// found directly inside the given folder, merged into a single index. Later
/// entries never override earlier keys, so the central file takes precedence.
fn load_bib_for_folder(conn: &rusqlite::Connection, folder: &Path) -> Option<BibIndex> {
    let mut sources: Vec<String> = Vec::new();

    if let Ok(Some(central)) = db::get_setting(conn, BIBTEX_PATH_KEY) {
        if let Ok(src) = std::fs::read_to_string(&central) {
            sources.push(src);
        }
    }
    if let Ok(entries) = std::fs::read_dir(folder) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.extension().and_then(|e| e.to_str()).map(|e| e.eq_ignore_ascii_case("bib")) == Some(true) {
                if let Ok(src) = std::fs::read_to_string(&p) {
                    sources.push(src);
                }
            }
        }
    }

    if sources.is_empty() {
        return None;
    }
    BibIndex::parse(&sources.join("\n")).ok()
}

fn folder_leaf(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string_lossy().to_string())
}

/// Whether a file needs its metadata (re-)extracted on this scan. False for
/// files whose on-disk modification time hasn't changed since they were last
/// indexed (an unchanged file costs only a cheap stat, not a full PDF parse +
/// bib re-match), and for documents the user hand-edited — those are never
/// silently overwritten by a rescan.
fn needs_rescan(existing: Option<&Document>, pdf_path: &Path) -> bool {
    let Some(doc) = existing else {
        return true; // never indexed before
    };
    if doc.metadata_source == "manual" {
        return false;
    }
    let current_mtime = std::fs::metadata(pdf_path)
        .ok()
        .and_then(|m| m.modified().ok())
        .map(iso_from_systemtime);
    current_mtime.as_deref() != doc.modified_at.as_deref()
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct LibraryChanged {
    folder_path: String,
    folder_name: String,
    added: u32,
    removed: u32,
}

/// Removes documents under `folder_path` whose file is no longer present.
/// Guarded by the caller checking the folder root itself still exists —
/// pruning is about a file genuinely leaving the tracked folder (deleted,
/// trashed, or moved elsewhere), not about the whole root being temporarily
/// unreachable (e.g. an external drive unplugged), which must never wipe out
/// a collection. Returns how many documents were removed.
fn prune_missing_documents(
    conn: &rusqlite::Connection,
    folder_path: &str,
    still_present: &HashSet<String>,
) -> u32 {
    let Ok(tracked) = db::document_paths_in_folder(conn, folder_path) else {
        return 0;
    };
    let mut removed = 0u32;
    for (id, path) in tracked {
        if !still_present.contains(&path) && db::delete_document(conn, &id).is_ok() {
            removed += 1;
        }
    }
    removed
}

/// The DB-only half of ingesting newly-detected PDFs: extracts metadata,
/// matches against any `.bib`, and upserts each one that's new or changed —
/// the same logic `scan_folder` uses per file, just for a small incremental
/// batch. Deliberately takes no `AppHandle` so it's testable without a live
/// Tauri runtime; `ingest_new_pdfs` wraps this with the emit/thumbnail side
/// effects. Returns how many files were added/changed.
fn ingest_new_pdfs_core(
    conn: &rusqlite::Connection,
    folder_path: &str,
    category: &str,
    paths: &[PathBuf],
) -> u32 {
    let bib = load_bib_for_folder(conn, Path::new(folder_path));
    let mut added = 0u32;

    for pdf_path in paths {
        if !pdf_path.is_file() {
            continue;
        }
        let existing = db::get_document_by_path(conn, &pdf_path.to_string_lossy())
            .ok()
            .flatten();
        if !needs_rescan(existing.as_ref(), pdf_path) {
            continue;
        }

        let mut doc = build_document(pdf_path, folder_path, category);
        if let Some(index) = &bib {
            if let Some(entry) = index.match_document(&doc.file_name, doc.doi.as_deref(), &doc.title) {
                enrich_from_bib(&mut doc, entry);
            }
        }
        if db::upsert_document(conn, &doc).is_ok() {
            added += 1;
        }
    }

    added
}

/// The DB-only half of reconciling a tracked folder against what's actually
/// on disk: ingests anything new/changed, and prunes documents whose file is
/// gone. No `AppHandle`, so it's testable without a live Tauri runtime;
/// `reconcile_folder` wraps this with the emit/thumbnail side effects.
/// Returns (added, removed).
///
/// Skips entirely if the folder root itself doesn't currently exist — that's
/// "temporarily unreachable" (an external drive unplugged, a cloud-synced
/// folder not yet remounted), not "everything in it was deleted", and must
/// never be treated as license to wipe out a whole collection.
fn reconcile_folder_core(conn: &rusqlite::Connection, folder_path: &str, category: &str) -> (u32, u32) {
    let root = Path::new(folder_path);
    if !root.is_dir() {
        return (0, 0);
    }

    let pdfs = collect_pdfs(root);
    let added = ingest_new_pdfs_core(conn, folder_path, category, &pdfs);

    let still_present: HashSet<String> = pdfs.iter().map(|p| p.to_string_lossy().to_string()).collect();
    let removed = prune_missing_documents(conn, folder_path, &still_present);

    (added, removed)
}

/// Reconciles a tracked folder against what's actually on disk right now —
/// the live-watcher's response to any filesystem activity under a folder. A
/// single event (of any kind, add or remove, file or directory) is enough to
/// trigger this; it doesn't try to interpret exactly which paths changed,
/// since a bulk directory delete may not report events for every contained
/// file in every case. If anything changed, emits `library-changed` and kicks
/// off thumbnail rendering for newly-added files. Returns (added, removed).
pub(crate) fn reconcile_folder(
    conn: &rusqlite::Connection,
    app: &AppHandle,
    folder_path: &str,
    category: &str,
) -> (u32, u32) {
    let (added, removed) = reconcile_folder_core(conn, folder_path, category);

    if added > 0 || removed > 0 {
        let _ = app.emit(
            "library-changed",
            LibraryChanged {
                folder_path: folder_path.to_string(),
                folder_name: category.to_string(),
                added,
                removed,
            },
        );
        if added > 0 {
            if let Ok(missing) = db::documents_missing_thumbnails(conn, folder_path) {
                spawn_thumbnail_job(app.clone(), folder_path.to_string(), missing.len() as u32);
            }
        }
    }

    (added, removed)
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FolderOverlap {
    /// "nested_under" if the candidate folder sits inside an existing one,
    /// "contains" if the candidate would swallow an existing one.
    pub kind: String,
    pub other_path: String,
    pub other_name: String,
}

/// Detects whether `candidate` overlaps with an already-tracked folder (one is
/// an ancestor of the other). Because a document's id is derived from its
/// absolute path, scanning two overlapping roots would silently reassign the
/// shared files' `folder_path` to whichever was scanned most recently — so
/// overlaps are rejected rather than allowed to fight over the same files. An
/// exact match isn't an overlap; it's a rescan of the same collection.
fn detect_folder_overlap(existing: &[Folder], candidate: &Path) -> Option<FolderOverlap> {
    for f in existing {
        let existing_path = Path::new(&f.path);
        if existing_path == candidate {
            continue;
        }
        if candidate.starts_with(existing_path) {
            return Some(FolderOverlap {
                kind: "nested_under".to_string(),
                other_path: f.path.clone(),
                other_name: f.name.clone(),
            });
        }
        if existing_path.starts_with(candidate) {
            return Some(FolderOverlap {
                kind: "contains".to_string(),
                other_path: f.path.clone(),
                other_name: f.name.clone(),
            });
        }
    }
    None
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanResult {
    pub folder: Folder,
    /// Total PDFs found under the folder.
    pub indexed: u32,
    /// Of those, how many were new or had changed since the last scan (and so
    /// actually had their metadata re-extracted).
    pub changed: u32,
    /// Previously-tracked documents whose file is no longer there (deleted,
    /// trashed, or moved out of the folder since the last scan).
    pub removed: u32,
}

/// Indexes every PDF under `path`: extracts metadata and upserts each into the
/// store (fast), then kicks off a background thread that renders first-page
/// thumbnails and streams progress + `thumbnail-ready` events back to the UI.
#[tauri::command]
pub async fn scan_folder(
    app: AppHandle,
    db: State<'_, Db>,
    path: String,
) -> Result<ScanResult, String> {
    let root = PathBuf::from(&path);
    if !root.is_dir() {
        return Err(format!("Not a folder: {path}"));
    }
    let category = folder_leaf(&root);
    let added_at = Local::now().to_rfc3339();

    let pdfs = collect_pdfs(&root);
    let total = pdfs.len() as u32;
    let mut changed = 0u32;
    let removed;

    {
        let conn = db.0.lock().map_err(|e| e.to_string())?;

        // Adding an overlapping folder (an ancestor or descendant of one
        // already tracked) would fight over the same files, since a
        // document's id is derived from its absolute path regardless of
        // which root it's scanned from. An exact re-add of the same path
        // isn't an overlap — that's just a rescan, handled below.
        let existing_folders = db::list_folders(&conn).map_err(|e| e.to_string())?;
        if let Some(overlap) = detect_folder_overlap(&existing_folders, &root) {
            let relation = if overlap.kind == "nested_under" { "is inside" } else { "already contains" };
            return Err(format!(
                "This folder {relation} your existing collection \"{}\" ({}). Remove that collection first if you want to reorganize your folders.",
                overlap.other_name, overlap.other_path
            ));
        }

        db::add_folder(&conn, &path, &category, &added_at).map_err(|e| e.to_string())?;
        // Live-track this folder from now on, so PDFs dropped in later while
        // Codex is running get picked up automatically.
        crate::watch::start_folder(app.clone(), path.clone());

        // Match against a central .bib (if attached) plus any .bib in this folder.
        let bib = load_bib_for_folder(&conn, &root);

        for (i, pdf_path) in pdfs.iter().enumerate() {
            let existing = db::get_document_by_path(&conn, &pdf_path.to_string_lossy())
                .map_err(|e| e.to_string())?;

            let file_name = if !needs_rescan(existing.as_ref(), pdf_path) {
                pdf_path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default()
            } else {
                let mut doc = build_document(pdf_path, &path, &category);
                if let Some(index) = &bib {
                    if let Some(entry) = index.match_document(&doc.file_name, doc.doi.as_deref(), &doc.title) {
                        enrich_from_bib(&mut doc, entry);
                    }
                }
                let name = doc.file_name.clone();
                db::upsert_document(&conn, &doc).map_err(|e| e.to_string())?;
                changed += 1;
                name
            };

            let _ = app.emit(
                "scan-progress",
                ScanProgress {
                    folder_path: path.clone(),
                    processed: (i + 1) as u32,
                    total,
                    current_file: file_name,
                    phase: "indexing".to_string(),
                    done: false,
                },
            );
        }

        // Anything previously tracked under this folder that's no longer on
        // disk (deleted, trashed, or moved elsewhere) gets removed too, so a
        // manual rescan also catches up on deletions that happened while
        // Codex was closed (the live watcher only sees this while running).
        let still_present: HashSet<String> =
            pdfs.iter().map(|p| p.to_string_lossy().to_string()).collect();
        removed = prune_missing_documents(&conn, &path, &still_present);
    }

    // Thumbnails are slow (~0.1s each), so render them off the command thread.
    // If a render for this folder is already running (e.g. the user re-added
    // it mid-scan), don't start a second one — but still emit `done` so this
    // call's progress UI doesn't hang waiting on a job it didn't start.
    if !spawn_thumbnail_job(app.clone(), path.clone(), total) {
        emit_done(&app, &path, total);
    }

    let folder = {
        let conn = db.0.lock().map_err(|e| e.to_string())?;
        db::list_folders(&conn)
            .map_err(|e| e.to_string())?
            .into_iter()
            .find(|f| f.path == path)
            .unwrap_or(Folder {
                path: path.clone(),
                name: category,
                document_count: total,
                added_at,
            })
    };

    Ok(ScanResult {
        folder,
        indexed: total,
        changed,
        removed,
    })
}

fn emit_done(app: &AppHandle, folder_path: &str, total: u32) {
    let _ = app.emit(
        "scan-progress",
        ScanProgress {
            folder_path: folder_path.to_string(),
            processed: total,
            total,
            current_file: String::new(),
            phase: "done".to_string(),
            done: true,
        },
    );
}

/// Releases the folder's in-flight guard and emits `done` when the job ends —
/// on a normal finish, an early return, OR a panic (Drop runs during unwind),
/// so the folder never gets stuck "scanning" or permanently guarded.
struct ThumbnailJobGuard {
    app: AppHandle,
    folder_path: String,
    total: u32,
}

impl Drop for ThumbnailJobGuard {
    fn drop(&mut self) {
        active_thumbnail_jobs()
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .remove(&self.folder_path);
        emit_done(&self.app, &self.folder_path, self.total);
    }
}

/// Spawns the background thumbnail renderer for a folder, unless one is already
/// running for it. Returns true if a new job was started. When it returns
/// false, the caller is responsible for emitting `done` so the UI unsticks.
fn spawn_thumbnail_job(app: AppHandle, folder_path: String, total: u32) -> bool {
    {
        let mut jobs = active_thumbnail_jobs()
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        if !jobs.insert(folder_path.clone()) {
            // A render for this folder is already in flight; don't double it up.
            return false;
        }
    }

    std::thread::spawn(move || {
        let guard = ThumbnailJobGuard {
            app: app.clone(),
            folder_path: folder_path.clone(),
            total,
        };
        run_thumbnail_pass(&guard.app, &guard.folder_path, guard.total);
        // guard drops here → releases the folder guard and emits `done`.
    });
    true
}

/// One pass over a folder's documents that still lack thumbnails. Opens its own
/// SQLite connection (WAL allows concurrent access) so it never blocks the
/// command connection. Best-effort: any early return is fine because the
/// caller always emits `done` afterwards.
fn run_thumbnail_pass(app: &AppHandle, folder_path: &str, total: u32) {
    let paths = match app_paths(app) {
        Ok(p) => p,
        Err(_) => return,
    };
    let conn = match rusqlite::Connection::open(&paths.db_file) {
        Ok(c) => c,
        Err(_) => return,
    };
    let _ = db::configure(&conn);

    let pending = match db::documents_missing_thumbnails(&conn, folder_path) {
        Ok(p) => p,
        Err(_) => return,
    };

    for (i, (id, pdf_path)) in pending.iter().enumerate() {
        let pdf = PathBuf::from(pdf_path);
        match thumb::generate(&paths.cache_root, id, &pdf, THUMB_SIZE) {
            thumb::ThumbOutcome::Rendered(thumb) => {
                let thumb_str = thumb.to_string_lossy().to_string();
                let _ = db::set_thumbnail(&conn, id, &thumb_str);
                let _ = app.emit(
                    "thumbnail-ready",
                    ThumbnailReady {
                        id: id.clone(),
                        thumbnail_path: thumb_str,
                    },
                );
            }
            // A hang means the PDF is unsupported — retire it immediately so we
            // never freeze on it again.
            thumb::ThumbOutcome::TimedOut => {
                let _ = db::retire_thumbnail(&conn, id);
            }
            // A plain failure may be transient (Quick Look overloaded); count it
            // and let it be retried until it hits the ceiling.
            thumb::ThumbOutcome::Failed => {
                let _ = db::increment_thumb_attempt(&conn, id);
            }
        }

        let _ = app.emit(
            "scan-progress",
            ScanProgress {
                folder_path: folder_path.to_string(),
                processed: (i + 1) as u32,
                total,
                current_file: pdf
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default(),
                phase: "thumbnails".to_string(),
                done: false,
            },
        );
    }
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ThumbnailReady {
    id: String,
    thumbnail_path: String,
}

#[tauri::command]
pub fn list_documents(db: State<'_, Db>) -> Result<Vec<Document>, String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    db::list_documents(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_document(db: State<'_, Db>, id: String) -> Result<Option<Document>, String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    db::get_document(&conn, &id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_folders(db: State<'_, Db>) -> Result<Vec<Folder>, String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    db::list_folders(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn remove_folder(db: State<'_, Db>, path: String) -> Result<(), String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    db::remove_folder(&conn, &path).map_err(|e| e.to_string())?;
    crate::watch::stop_folder(&path);
    Ok(())
}

/// Re-renders thumbnails for documents in a folder that are still missing one
/// (a clean recovery for Quick Look failures, so the user never has to re-add
/// a whole folder). With `force`, previously-retired (un-renderable) documents
/// are given another chance. Returns how many documents will be retried. Emits
/// the same `scan-progress`/`thumbnail-ready` events as a scan.
#[tauri::command]
pub fn retry_thumbnails(
    app: AppHandle,
    db: State<'_, Db>,
    folder_path: String,
    force: Option<bool>,
) -> Result<u32, String> {
    let missing = {
        let conn = db.0.lock().map_err(|e| e.to_string())?;
        if force.unwrap_or(false) {
            db::reset_thumb_attempts(&conn, &folder_path).map_err(|e| e.to_string())?;
        }
        db::documents_missing_thumbnails(&conn, &folder_path)
            .map_err(|e| e.to_string())?
            .len() as u32
    };
    if !spawn_thumbnail_job(app.clone(), folder_path.clone(), missing) {
        // Already running — nothing new to start, but unstick any UI waiting.
        emit_done(&app, &folder_path, missing);
    }
    Ok(missing)
}

fn now_iso() -> String {
    Local::now().to_rfc3339()
}

type ReadingState = (String, u32, Option<String>, Option<String>);

/// Pure reading-state transition for a page update: returns
/// (status, current_page, started_at, finished_at). `started_at` is preserved
/// if already set, otherwise stamped `now` once reading begins.
fn progress_state(page_count: u32, started_at: &Option<String>, page: u32, now: &str) -> ReadingState {
    let has_total = page_count > 0;
    let page = if has_total { page.min(page_count) } else { page };
    let start = || started_at.clone().or_else(|| Some(now.to_string()));

    if has_total && page >= page_count {
        ("completed".to_string(), page_count, start(), Some(now.to_string()))
    } else if page > 0 {
        ("in_progress".to_string(), page, start(), None)
    } else {
        ("unread".to_string(), 0, None, None)
    }
}

/// Pure reading-state transition for a direct status change.
fn status_state(
    status: &str,
    page_count: u32,
    current_page: u32,
    started_at: &Option<String>,
    now: &str,
) -> Result<ReadingState, String> {
    let start = || started_at.clone().or_else(|| Some(now.to_string()));
    match status {
        "completed" => Ok((
            "completed".to_string(),
            if page_count > 0 { page_count } else { current_page },
            start(),
            Some(now.to_string()),
        )),
        "in_progress" => Ok(("in_progress".to_string(), current_page, start(), None)),
        "unread" => Ok(("unread".to_string(), 0, None, None)),
        other => Err(format!("Unknown status: {other}")),
    }
}

/// Opens the PDF in Skim, if it's installed — a consistent reading
/// experience Codex can eventually hook into for in-app tracking. Falls
/// back to the system default viewer when Skim isn't available. Logs the
/// open as reading activity and, for an unread document, marks it as started.
#[tauri::command]
pub fn open_document(app: AppHandle, db: State<'_, Db>, id: String) -> Result<Document, String> {
    use tauri_plugin_opener::OpenerExt;
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    let doc = db::get_document(&conn, &id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Document not found".to_string())?;

    let opener = app.opener();
    if opener.open_path(&doc.path, Some("Skim")).is_err() {
        opener
            .open_path(&doc.path, None::<&str>)
            .map_err(|e| e.to_string())?;
    }

    let now = now_iso();
    db::log_reading_event(&conn, &id, "open", None, &now).map_err(|e| e.to_string())?;

    let status = if doc.status == "unread" { "in_progress" } else { doc.status.as_str() };
    let started_at = doc.started_at.clone().or_else(|| Some(now.clone()));
    db::set_reading_state(
        &conn,
        &id,
        status,
        doc.current_page,
        started_at.as_deref(),
        doc.finished_at.as_deref(),
        Some(&now),
    )
    .map_err(|e| e.to_string())?;

    db::get_document(&conn, &id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Document not found".to_string())
}

/// Sets the current reading page, deriving status (unread / in_progress /
/// completed) and started/finished timestamps, and logs a progress event.
#[tauri::command]
pub fn set_reading_progress(db: State<'_, Db>, id: String, page: u32) -> Result<Document, String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    let doc = db::get_document(&conn, &id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Document not found".to_string())?;

    let now = now_iso();
    let (status, current_page, started_at, finished_at) =
        progress_state(doc.page_count, &doc.started_at, page, &now);

    db::set_reading_state(
        &conn,
        &id,
        &status,
        current_page,
        started_at.as_deref(),
        finished_at.as_deref(),
        Some(&now),
    )
    .map_err(|e| e.to_string())?;
    db::log_reading_event(&conn, &id, "progress", Some(current_page), &now)
        .map_err(|e| e.to_string())?;

    db::get_document(&conn, &id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Document not found".to_string())
}

/// Sets reading status directly (Unread / Reading / Finished), adjusting
/// progress and timestamps to match.
#[tauri::command]
pub fn set_reading_status(db: State<'_, Db>, id: String, status: String) -> Result<Document, String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    let doc = db::get_document(&conn, &id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Document not found".to_string())?;
    let now = now_iso();
    let (new_status, current_page, started_at, finished_at) =
        status_state(&status, doc.page_count, doc.current_page, &doc.started_at, &now)?;
    // Clearing back to Unread also clears the last-read timestamp.
    let last_read_at = if new_status == "unread" { None } else { Some(now.as_str()) };

    db::set_reading_state(
        &conn,
        &id,
        &new_status,
        current_page,
        started_at.as_deref(),
        finished_at.as_deref(),
        last_read_at,
    )
    .map_err(|e| e.to_string())?;

    db::get_document(&conn, &id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Document not found".to_string())
}

#[tauri::command]
pub fn list_reading_events(db: State<'_, Db>) -> Result<Vec<crate::model::ReadingEvent>, String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    db::list_reading_events(&conn).map_err(|e| e.to_string())
}

/// Reveals the PDF in Finder.
#[tauri::command]
pub fn reveal_in_finder(app: AppHandle, path: String) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    app.opener()
        .reveal_item_in_dir(std::path::PathBuf::from(path))
        .map_err(|e| e.to_string())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BibtexStatus {
    /// Path of the attached central .bib file, if any.
    pub path: Option<String>,
    /// Documents whose displayed metadata currently comes from BibTeX.
    pub matched: u32,
    /// Total documents in the library.
    pub total: u32,
}

fn bibtex_status(conn: &rusqlite::Connection) -> Result<BibtexStatus, String> {
    let path = db::get_setting(conn, BIBTEX_PATH_KEY).map_err(|e| e.to_string())?;
    let docs = db::list_documents(conn).map_err(|e| e.to_string())?;
    let matched = docs.iter().filter(|d| d.metadata_source == "bibtex").count() as u32;
    Ok(BibtexStatus {
        path,
        matched,
        total: docs.len() as u32,
    })
}

/// Re-runs BibTeX matching across the whole library: for each folder, loads the
/// central .bib plus any .bib inside it, then re-derives each document's
/// metadata. Documents that no longer match fall back to their PDF metadata.
fn rematch_library(conn: &rusqlite::Connection) -> Result<(), String> {
    let folders = db::list_folders(conn).map_err(|e| e.to_string())?;
    let all_docs = db::list_documents(conn).map_err(|e| e.to_string())?;

    for folder in &folders {
        let index = load_bib_for_folder(conn, Path::new(&folder.path));
        for doc in all_docs
            .iter()
            .filter(|d| d.folder_path == folder.path && d.metadata_source != "manual")
        {
            // Start from freshly extracted PDF metadata so unmatched docs reset.
            let mut fresh = build_document(Path::new(&doc.path), &doc.folder_path, &doc.category);
            // Preserve user/reading state and the already-rendered thumbnail.
            fresh.thumbnail_path = doc.thumbnail_path.clone();
            fresh.tags = doc.tags.clone();
            fresh.status = doc.status.clone();
            fresh.current_page = doc.current_page;
            fresh.added_at = doc.added_at.clone();

            if let Some(idx) = &index {
                if let Some(entry) =
                    idx.match_document(&fresh.file_name, fresh.doi.as_deref(), &fresh.title)
                {
                    enrich_from_bib(&mut fresh, entry);
                }
            }
            db::upsert_document(conn, &fresh).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// Attaches a central BibTeX file and re-matches the whole library against it.
#[tauri::command]
pub fn attach_bibtex(db: State<'_, Db>, path: String) -> Result<BibtexStatus, String> {
    // Validate that the file parses before storing it.
    BibIndex::load(Path::new(&path))?;
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    db::set_setting(&conn, BIBTEX_PATH_KEY, &path).map_err(|e| e.to_string())?;
    rematch_library(&conn)?;
    bibtex_status(&conn)
}

/// Detaches the central BibTeX file (per-folder .bib files still apply).
#[tauri::command]
pub fn detach_bibtex(db: State<'_, Db>) -> Result<BibtexStatus, String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    db::delete_setting(&conn, BIBTEX_PATH_KEY).map_err(|e| e.to_string())?;
    rematch_library(&conn)?;
    bibtex_status(&conn)
}

#[tauri::command]
pub fn get_bibtex_status(db: State<'_, Db>) -> Result<BibtexStatus, String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    bibtex_status(&conn)
}

#[tauri::command]
pub fn rematch_bibtex(db: State<'_, Db>) -> Result<BibtexStatus, String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    rematch_library(&conn)?;
    bibtex_status(&conn)
}

fn bibtex_escape(s: &str) -> String {
    s.replace('{', "\\{").replace('}', "\\}")
}

/// The citation key a document would use in isolation — its stored key if
/// matched from BibTeX, otherwise `<lastauthorname><year>` derived the same
/// way BibTeX conventionally does. Not guaranteed unique across a batch; see
/// `dedupe_citation_key`.
fn bibtex_base_key(doc: &Document) -> String {
    doc.citation_key.clone().unwrap_or_else(|| {
        let author = doc
            .authors
            .first()
            .and_then(|a| a.split_whitespace().last())
            .unwrap_or("anon")
            .to_lowercase();
        let year = doc.year.map(|y| y.to_string()).unwrap_or_default();
        format!("{author}{year}")
    })
}

/// Disambiguates a citation key against keys already used in this export by
/// appending a/b/c/... (matching the convention BibTeX tools use for
/// same-author-same-year collisions), and reserves the result in `used`.
fn dedupe_citation_key(base: &str, used: &mut std::collections::HashSet<String>) -> String {
    if used.insert(base.to_string()) {
        return base.to_string();
    }
    for suffix in 'a'..='z' {
        let candidate = format!("{base}{suffix}");
        if used.insert(candidate.clone()) {
            return candidate;
        }
    }
    // Astronomically unlikely (27+ collisions on one key), but stay unique.
    let mut n = 2;
    loop {
        let candidate = format!("{base}-{n}");
        if used.insert(candidate.clone()) {
            return candidate;
        }
        n += 1;
    }
}

/// Regenerates a BibTeX entry body (everything after `@type{key,`) from a
/// document's stored fields, so any document can be cited even if it was
/// never matched to an entry.
fn document_to_bibtex(doc: &Document, key: &str) -> String {
    let entry_type = if doc.venue.is_some() { "article" } else { "misc" };

    let mut out = format!("@{entry_type}{{{key},\n");
    out.push_str(&format!("  title = {{{}}},\n", bibtex_escape(&doc.title)));
    if !doc.authors.is_empty() {
        out.push_str(&format!(
            "  author = {{{}}},\n",
            bibtex_escape(&doc.authors.join(" and "))
        ));
    }
    if let Some(y) = doc.year {
        out.push_str(&format!("  year = {{{y}}},\n"));
    }
    if let Some(v) = &doc.venue {
        out.push_str(&format!("  journal = {{{}}},\n", bibtex_escape(v)));
    }
    if let Some(d) = &doc.doi {
        out.push_str(&format!("  doi = {{{}}},\n", bibtex_escape(d)));
    }
    if !doc.keywords.is_empty() {
        out.push_str(&format!(
            "  keywords = {{{}}},\n",
            bibtex_escape(&doc.keywords.join(", "))
        ));
    }
    if let Some(a) = &doc.abstract_text {
        out.push_str(&format!("  abstract = {{{}}},\n", bibtex_escape(a)));
    }
    out.push_str("}\n");
    out
}

/// Regenerates a single document's BibTeX entry.
#[tauri::command]
pub fn export_bibtex(db: State<'_, Db>, id: String) -> Result<String, String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    let doc = db::get_document(&conn, &id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Document not found".to_string())?;
    let key = bibtex_base_key(&doc);
    Ok(document_to_bibtex(&doc, &key))
}

/// Exports several documents as one .bib file, disambiguating any citation
/// keys that would otherwise collide (e.g. two docs by the same first author
/// in the same year). Order follows the given id list.
#[tauri::command]
pub fn export_bibtex_batch(db: State<'_, Db>, ids: Vec<String>) -> Result<String, String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    let mut used = std::collections::HashSet::new();
    let mut out = String::new();

    for id in ids {
        let doc = db::get_document(&conn, &id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("Document not found: {id}"))?;
        let key = dedupe_citation_key(&bibtex_base_key(&doc), &mut used);
        out.push_str(&document_to_bibtex(&doc, &key));
        out.push('\n');
    }

    Ok(out)
}

/// Writes text to an arbitrary path chosen via the save dialog. A plain Rust
/// command rather than a webview fs write, so the frontend doesn't need a
/// broad filesystem-write capability just to save one export file.
#[tauri::command]
pub fn write_text_file(path: String, contents: String) -> Result<(), String> {
    std::fs::write(&path, contents).map_err(|e| format!("Couldn't write {path}: {e}"))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateDocumentInput {
    pub id: String,
    pub title: String,
    pub authors: Vec<String>,
    pub year: Option<i32>,
    pub venue: Option<String>,
    pub doi: Option<String>,
    pub keywords: Vec<String>,
    pub subject: Option<String>,
}

/// Does the actual work of a hand edit: writes Title/Author/Subject/Keywords
/// into the PDF itself (verified before replacing the file — see
/// `pdf::write_info_fields`), renames the file to match the new title, and
/// carries the cached thumbnail over to the new id. Pure filesystem logic,
/// no DB/Tauri context, so it's directly unit-testable.
fn perform_document_edit(
    cache_root: &Path,
    existing: &Document,
    input: UpdateDocumentInput,
) -> Result<(Document, Option<String>), String> {
    let title = input.title.trim();
    if title.is_empty() {
        return Err("Title can't be empty".to_string());
    }

    let old_path = PathBuf::from(&existing.path);
    let author_field = input.authors.join("; ");
    let keywords_field = input.keywords.join(", ");
    let subject_field = input.subject.clone().unwrap_or_default();

    // Write into the PDF first. Some real-world files (encrypted, or with an
    // exotic/malformed structure lopdf can't safely round-trip) will fail
    // here — that's expected for a minority of PDFs and isn't fatal to the
    // rest of the edit: the file itself is left untouched either way (the
    // writer never replaces the original unless its own reload verifies), so
    // we still rename and save everything to Codex's own catalog, just
    // without updating this file's embedded metadata. The caller surfaces
    // this as a warning rather than blocking the save.
    let pdf_warning = pdf::write_info_fields(&old_path, title, &author_field, &subject_field, &keywords_field)
        .err()
        .map(|e| format!("Saved to Codex, but couldn't update the PDF's own metadata: {e}"));

    // Rename to match the new title, staying in the same folder.
    let dir = old_path.parent().ok_or("PDF has no parent directory")?;
    let stem = pdf::sanitize_filename(title);
    let final_path = pdf::unique_pdf_path(dir, &stem, &old_path);
    if final_path != old_path {
        std::fs::rename(&old_path, &final_path)
            .map_err(|e| format!("metadata saved, but couldn't rename file: {e}"))?;
    }

    let new_id = pdf::doc_id(&final_path);

    // Carry the cached thumbnail over to the new id rather than re-rendering
    // it — the page content didn't change, only the file name/metadata did.
    let mut thumbnail_path = existing.thumbnail_path.clone();
    if new_id != existing.id {
        thumbnail_path = match &existing.thumbnail_path {
            Some(old_thumb) => {
                let old_thumb_path = PathBuf::from(old_thumb);
                let new_thumb_path = thumb::thumbnails_dir(cache_root).join(format!("{new_id}.png"));
                if std::fs::rename(&old_thumb_path, &new_thumb_path).is_ok() {
                    Some(new_thumb_path.to_string_lossy().to_string())
                } else {
                    thumb::generate(cache_root, &new_id, &final_path, THUMB_SIZE)
                        .rendered_path()
                        .map(|p| p.to_string_lossy().to_string())
                }
            }
            None => thumb::generate(cache_root, &new_id, &final_path, THUMB_SIZE)
                .rendered_path()
                .map(|p| p.to_string_lossy().to_string()),
        };
    }

    let fs_meta = std::fs::metadata(&final_path).ok();
    let doc = Document {
        id: new_id,
        path: final_path.to_string_lossy().to_string(),
        folder_path: existing.folder_path.clone(),
        file_name: final_path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default(),
        title: title.to_string(),
        authors: input.authors,
        subject: input.subject,
        keywords: input.keywords,
        page_count: existing.page_count,
        size_bytes: fs_meta.as_ref().map(|m| m.len()).unwrap_or(existing.size_bytes),
        year: input.year,
        doi: input.doi,
        citation_key: existing.citation_key.clone(),
        venue: input.venue,
        abstract_text: existing.abstract_text.clone(),
        metadata_source: "manual".to_string(),
        thumbnail_path,
        category: existing.category.clone(),
        relative_dir: existing.relative_dir.clone(),
        tags: existing.tags.clone(),
        modified_at: fs_meta.and_then(|m| m.modified().ok()).map(iso_from_systemtime),
        added_at: existing.added_at.clone(),
        status: existing.status.clone(),
        current_page: existing.current_page,
        started_at: existing.started_at.clone(),
        finished_at: existing.finished_at.clone(),
        last_read_at: existing.last_read_at.clone(),
        thumb_attempts: existing.thumb_attempts,
    };
    Ok((doc, pdf_warning))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateDocumentResult {
    pub document: Document,
    /// Set when the edit was saved to Codex's catalog (and the file renamed)
    /// but the PDF's own embedded metadata couldn't be updated — e.g. an
    /// encrypted or unusually-structured file that can't be safely rewritten.
    /// `None` means the PDF's Info dictionary was updated too.
    pub pdf_warning: Option<String>,
}

/// Applies a hand edit to a document and persists it, marking
/// `metadata_source: "manual"` so later re-scans / BibTeX re-matches never
/// silently overwrite it. A PDF that can't have its own metadata rewritten
/// (see `perform_document_edit`) still gets saved to Codex's catalog — the
/// command only fails outright for a genuine validation error (blank title)
/// or a filesystem/DB problem.
#[tauri::command]
pub fn update_document(
    app: AppHandle,
    db: State<'_, Db>,
    input: UpdateDocumentInput,
) -> Result<UpdateDocumentResult, String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    let existing = db::get_document(&conn, &input.id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Document not found".to_string())?;

    let paths = app_paths(&app)?;
    let (updated, pdf_warning) = perform_document_edit(&paths.cache_root, &existing, input)?;

    if updated.id != existing.id {
        // Insert the new row first, then carry reading history over to it, then
        // drop the old row — keeping the events' foreign key valid throughout.
        db::upsert_document(&conn, &updated).map_err(|e| e.to_string())?;
        db::repoint_reading_events(&conn, &existing.id, &updated.id).map_err(|e| e.to_string())?;
        db::delete_document(&conn, &existing.id).map_err(|e| e.to_string())?;
    } else {
        db::upsert_document(&conn, &updated).map_err(|e| e.to_string())?;
    }

    Ok(UpdateDocumentResult { document: updated, pdf_warning })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bib::BibIndex;

    /// Exercises the "add folder" core: discover PDFs, extract metadata, match
    /// them to a folder .bib, and enrich. This is everything scan_folder does
    /// apart from the Tauri/thumbnail plumbing.
    #[test]
    fn scan_core_indexes_and_enriches() {
        let base = std::env::temp_dir().join(format!("codex_scan_test_{}", std::process::id()));
        std::fs::create_dir_all(&base).unwrap();

        // Two PDFs; the .bib references only the first, by file name.
        let one = base.join("smith2020.pdf");
        let two = base.join("loose_note.pdf");
        crate::pdf::build_test_pdf(&one, "Weak PDF Title", "", "", "", "D:20200101000000Z");
        crate::pdf::build_test_pdf(&two, "Loose Note", "", "", "", "D:20190101000000Z");

        std::fs::write(
            base.join("refs.bib"),
            r#"@article{smith2020,
                title = {The Real, Enriched Title},
                author = {Smith, Alice and Jones, Bob},
                journaltitle = {Nature},
                year = {2020},
                doi = {10.1000/xyz},
                file = {Smith:smith2020.pdf:application/pdf}
            }"#,
        )
        .unwrap();

        let category = folder_leaf(&base);
        let pdfs = collect_pdfs(&base);
        assert_eq!(pdfs.len(), 2, "should discover both PDFs");

        let bib = BibIndex::load(&base.join("refs.bib")).unwrap();

        let mut enriched = None;
        let mut untouched = None;
        for p in &pdfs {
            let mut doc = build_document(p, &base.to_string_lossy(), &category);
            if let Some(entry) = bib.match_document(&doc.file_name, doc.doi.as_deref(), &doc.title) {
                enrich_from_bib(&mut doc, entry);
            }
            if doc.file_name == "smith2020.pdf" {
                enriched = Some(doc);
            } else {
                untouched = Some(doc);
            }
        }

        let enriched = enriched.unwrap();
        assert_eq!(enriched.title, "The Real, Enriched Title");
        assert_eq!(enriched.authors, vec!["Alice Smith", "Bob Jones"]);
        assert_eq!(enriched.year, Some(2020));
        assert_eq!(enriched.venue.as_deref(), Some("Nature"));
        assert_eq!(enriched.doi.as_deref(), Some("10.1000/xyz"));
        assert_eq!(enriched.metadata_source, "bibtex");
        assert_eq!(enriched.citation_key.as_deref(), Some("smith2020"));

        let untouched = untouched.unwrap();
        assert_eq!(untouched.title, "Loose Note");
        assert_eq!(untouched.metadata_source, "pdf");

        std::fs::remove_dir_all(&base).ok();
    }

    /// Runs one scan pass over `pdfs` against `conn`, mirroring scan_folder's
    /// per-file loop (skip unchanged via needs_rescan, else re-extract +
    /// upsert), and returns how many files were actually (re)processed.
    fn scan_pass(conn: &rusqlite::Connection, pdfs: &[PathBuf], folder: &str, category: &str) -> u32 {
        let mut changed = 0;
        for p in pdfs {
            let existing = db::get_document_by_path(conn, &p.to_string_lossy()).unwrap();
            if needs_rescan(existing.as_ref(), p) {
                let doc = build_document(p, folder, category);
                db::upsert_document(conn, &doc).unwrap();
                changed += 1;
            }
        }
        changed
    }

    #[test]
    fn incremental_rescan_only_touches_new_or_changed_files() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        db::migrate(&conn).unwrap();

        let base = std::env::temp_dir().join(format!("codex_incremental_test_{}", std::process::id()));
        std::fs::create_dir_all(&base).unwrap();
        db::add_folder(&conn, &base.to_string_lossy(), "lib", "2026-01-01T00:00:00Z").unwrap();

        let a = base.join("a.pdf");
        let b = base.join("b.pdf");
        crate::pdf::build_test_pdf(&a, "A", "", "", "", "D:20200101000000Z");
        crate::pdf::build_test_pdf(&b, "B", "", "", "", "D:20200101000000Z");
        let pdfs = vec![a.clone(), b.clone()];
        let folder = base.to_string_lossy().to_string();

        // First pass: both files are new.
        assert_eq!(scan_pass(&conn, &pdfs, &folder, "lib"), 2);

        // Second pass, nothing on disk changed: neither file needs work.
        assert_eq!(scan_pass(&conn, &pdfs, &folder, "lib"), 0, "unchanged rescan should touch nothing");

        // Modify only `a`; a third pass should re-process just that one file.
        std::thread::sleep(std::time::Duration::from_millis(20));
        crate::pdf::build_test_pdf(&a, "A Revised", "", "", "", "D:20210101000000Z");
        assert_eq!(scan_pass(&conn, &pdfs, &folder, "lib"), 1, "only the edited file should be reprocessed");

        let updated_a = db::get_document_by_path(&conn, &a.to_string_lossy()).unwrap().unwrap();
        assert_eq!(updated_a.title, "A Revised");

        // A newly added third file also counts as changed.
        let c = base.join("c.pdf");
        crate::pdf::build_test_pdf(&c, "C", "", "", "", "D:20200101000000Z");
        let pdfs_with_c = vec![a, b, c];
        assert_eq!(scan_pass(&conn, &pdfs_with_c, &folder, "lib"), 1, "only the new file should be processed");

        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn ingest_new_pdfs_core_adds_new_files_and_matches_bib() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        db::migrate(&conn).unwrap();

        let base = std::env::temp_dir().join(format!("codex_ingest_test_{}", std::process::id()));
        std::fs::create_dir_all(&base).unwrap();
        let folder = base.to_string_lossy().to_string();
        db::add_folder(&conn, &folder, "lib", "2026-01-01T00:00:00Z").unwrap();

        let dropped = base.join("smith2020.pdf");
        crate::pdf::build_test_pdf(&dropped, "Weak Title", "", "", "", "D:20200101000000Z");
        std::fs::write(
            base.join("refs.bib"),
            r#"@article{smith2020,
                title = {A Watcher-Detected Paper},
                author = {Smith, Alice},
                journaltitle = {Nature},
                year = {2020},
                file = {Smith:smith2020.pdf:application/pdf}
            }"#,
        )
        .unwrap();

        let added = ingest_new_pdfs_core(&conn, &folder, "lib", &[dropped.clone()]);
        assert_eq!(added, 1);

        let doc = db::get_document_by_path(&conn, &dropped.to_string_lossy()).unwrap().unwrap();
        assert_eq!(doc.title, "A Watcher-Detected Paper", "the watcher path should still pick up bib enrichment");
        assert_eq!(doc.metadata_source, "bibtex");

        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn ingest_new_pdfs_core_skips_unchanged_and_manually_edited_files() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        db::migrate(&conn).unwrap();

        let base = std::env::temp_dir().join(format!("codex_ingest_skip_test_{}", std::process::id()));
        std::fs::create_dir_all(&base).unwrap();
        let folder = base.to_string_lossy().to_string();
        db::add_folder(&conn, &folder, "lib", "2026-01-01T00:00:00Z").unwrap();

        let pdf = base.join("doc.pdf");
        crate::pdf::build_test_pdf(&pdf, "Doc", "", "", "", "D:20200101000000Z");

        // First sighting: added.
        assert_eq!(ingest_new_pdfs_core(&conn, &folder, "lib", &[pdf.clone()]), 1);
        // Re-detected with no change on disk (e.g. a duplicate fs event): skipped.
        assert_eq!(ingest_new_pdfs_core(&conn, &folder, "lib", &[pdf.clone()]), 0);

        // A hand-edited document must never be silently overwritten, even if
        // the watcher re-detects it (e.g. Finder touching its mtime).
        let mut doc = db::get_document_by_path(&conn, &pdf.to_string_lossy()).unwrap().unwrap();
        doc.metadata_source = "manual".to_string();
        doc.title = "User's Title".to_string();
        db::upsert_document(&conn, &doc).unwrap();
        assert_eq!(ingest_new_pdfs_core(&conn, &folder, "lib", &[pdf.clone()]), 0);
        let after = db::get_document_by_path(&conn, &pdf.to_string_lossy()).unwrap().unwrap();
        assert_eq!(after.title, "User's Title");

        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn prune_missing_documents_removes_only_vanished_paths() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        db::migrate(&conn).unwrap();
        db::add_folder(&conn, "/lib", "lib", "2026-01-01T00:00:00Z").unwrap();

        let mut kept = build_document(Path::new("/lib/kept.pdf"), "/lib", "lib");
        kept.id = "kept".to_string();
        let mut gone = build_document(Path::new("/lib/Subfolder/gone.pdf"), "/lib", "lib");
        gone.id = "gone".to_string();
        db::upsert_document(&conn, &kept).unwrap();
        db::upsert_document(&conn, &gone).unwrap();

        // Only "kept.pdf" is still on disk — as if "Subfolder" was deleted.
        let still_present: HashSet<String> = ["/lib/kept.pdf".to_string()].into_iter().collect();
        let removed = prune_missing_documents(&conn, "/lib", &still_present);

        assert_eq!(removed, 1);
        assert!(db::get_document(&conn, "kept").unwrap().is_some());
        assert!(db::get_document(&conn, "gone").unwrap().is_none());
    }

    #[test]
    fn prune_missing_documents_removes_nothing_when_all_present() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        db::migrate(&conn).unwrap();
        db::add_folder(&conn, "/lib", "lib", "2026-01-01T00:00:00Z").unwrap();

        let doc = build_document(Path::new("/lib/a.pdf"), "/lib", "lib");
        db::upsert_document(&conn, &doc).unwrap();

        let still_present: HashSet<String> = ["/lib/a.pdf".to_string()].into_iter().collect();
        assert_eq!(prune_missing_documents(&conn, "/lib", &still_present), 0);
        assert!(db::get_document(&conn, &doc.id).unwrap().is_some());
    }

    #[test]
    fn reconcile_folder_core_adds_new_and_prunes_deleted_subfolder() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        db::migrate(&conn).unwrap();

        let base = std::env::temp_dir().join(format!("codex_reconcile_test_{}", std::process::id()));
        let sub = base.join("Subfolder");
        std::fs::create_dir_all(&sub).unwrap();
        let folder = base.to_string_lossy().to_string();
        db::add_folder(&conn, &folder, "lib", "2026-01-01T00:00:00Z").unwrap();

        let kept = base.join("kept.pdf");
        let doomed = sub.join("doomed.pdf");
        crate::pdf::build_test_pdf(&kept, "Kept", "", "", "", "D:20200101000000Z");
        crate::pdf::build_test_pdf(&doomed, "Doomed", "", "", "", "D:20200101000000Z");

        // First pass indexes both.
        let (added, removed) = reconcile_folder_core(&conn, &folder, "lib");
        assert_eq!((added, removed), (2, 0));

        // Delete the whole subfolder, as the user did; a reconcile pass
        // (triggered by whatever event fired, regardless of exactly which
        // paths it named) must prune the vanished document.
        std::fs::remove_dir_all(&sub).unwrap();
        let (added, removed) = reconcile_folder_core(&conn, &folder, "lib");
        assert_eq!((added, removed), (0, 1));

        let remaining = db::list_documents(&conn).unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].title, "Kept");

        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn reconcile_folder_core_never_prunes_when_root_is_unreachable() {
        // Simulates an external drive being unplugged: the whole root
        // vanishes. This must be treated as "temporarily unavailable", never
        // as license to wipe out every document that was tracked under it.
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        db::migrate(&conn).unwrap();

        let base = std::env::temp_dir().join(format!("codex_reconcile_unreachable_test_{}", std::process::id()));
        std::fs::create_dir_all(&base).unwrap();
        let folder = base.to_string_lossy().to_string();
        db::add_folder(&conn, &folder, "lib", "2026-01-01T00:00:00Z").unwrap();

        let pdf = base.join("doc.pdf");
        crate::pdf::build_test_pdf(&pdf, "Doc", "", "", "", "D:20200101000000Z");
        reconcile_folder_core(&conn, &folder, "lib");
        assert_eq!(db::list_documents(&conn).unwrap().len(), 1);

        // The root itself disappears (not just a file inside it).
        std::fs::remove_dir_all(&base).unwrap();
        let (added, removed) = reconcile_folder_core(&conn, &folder, "lib");

        assert_eq!((added, removed), (0, 0), "an unreachable root must be a no-op, not a mass deletion");
        assert_eq!(db::list_documents(&conn).unwrap().len(), 1, "the tracked document must survive");
    }

    #[test]
    fn relative_dir_reflects_subfolder_nesting() {
        let root = Path::new("/lib/Thesis");
        assert_eq!(relative_dir_of(root, Path::new("/lib/Thesis/paper.pdf")), "");
        assert_eq!(
            relative_dir_of(root, Path::new("/lib/Thesis/Chapter 1/intro.pdf")),
            "Chapter 1"
        );
        assert_eq!(
            relative_dir_of(root, Path::new("/lib/Thesis/Sources/Primary/a.pdf")),
            "Sources/Primary"
        );
    }

    #[test]
    fn build_document_records_relative_dir() {
        let base = std::env::temp_dir().join(format!("codex_reldir_test_{}", std::process::id()));
        let nested = base.join("Sources").join("Primary");
        std::fs::create_dir_all(&nested).unwrap();
        let pdf = nested.join("source.pdf");
        crate::pdf::build_test_pdf(&pdf, "A Source", "", "", "", "D:20200101000000Z");

        let doc = build_document(&pdf, &base.to_string_lossy(), &folder_leaf(&base));
        assert_eq!(doc.relative_dir, "Sources/Primary");

        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn progress_state_derives_status_and_timestamps() {
        // Partway through a 100-page doc → in progress, stamps a start.
        let (status, page, started, finished) = progress_state(100, &None, 40, "NOW");
        assert_eq!(status, "in_progress");
        assert_eq!(page, 40);
        assert_eq!(started.as_deref(), Some("NOW"));
        assert_eq!(finished, None);

        // Reaching (or passing) the last page → completed, page clamped to total.
        let (status, page, _started, finished) = progress_state(100, &Some("EARLIER".into()), 250, "NOW");
        assert_eq!(status, "completed");
        assert_eq!(page, 100);
        assert_eq!(finished.as_deref(), Some("NOW"));

        // An existing start is preserved, not overwritten.
        let (_s, _p, started, _f) = progress_state(100, &Some("EARLIER".into()), 40, "NOW");
        assert_eq!(started.as_deref(), Some("EARLIER"));

        // Back to page 0 → unread, timestamps cleared.
        let (status, page, started, finished) = progress_state(100, &Some("EARLIER".into()), 0, "NOW");
        assert_eq!(status, "unread");
        assert_eq!(page, 0);
        assert_eq!((started, finished), (None, None));

        // Unknown page count: any positive page is just "in progress".
        let (status, page, _s, _f) = progress_state(0, &None, 5, "NOW");
        assert_eq!((status.as_str(), page), ("in_progress", 5));
    }

    #[test]
    fn status_state_transitions() {
        // Finished jumps to the last page and stamps a finish time.
        let (status, page, started, finished) = status_state("completed", 200, 30, &None, "NOW").unwrap();
        assert_eq!((status.as_str(), page), ("completed", 200));
        assert_eq!(started.as_deref(), Some("NOW"));
        assert_eq!(finished.as_deref(), Some("NOW"));

        // Marking "reading" keeps the current page and clears any finish time.
        let (status, page, _s, finished) = status_state("in_progress", 200, 30, &Some("EARLIER".into()), "NOW").unwrap();
        assert_eq!((status.as_str(), page), ("in_progress", 30));
        assert_eq!(finished, None);

        // Unread clears everything.
        let (status, page, started, finished) = status_state("unread", 200, 30, &Some("EARLIER".into()), "NOW").unwrap();
        assert_eq!((status.as_str(), page), ("unread", 0));
        assert_eq!((started, finished), (None, None));

        assert!(status_state("bogus", 1, 0, &None, "NOW").is_err());
    }

    fn edit_input(id: &str, title: &str) -> UpdateDocumentInput {
        UpdateDocumentInput {
            id: id.to_string(),
            title: title.to_string(),
            authors: vec!["Ada Lovelace".to_string(), "Charles Babbage".to_string()],
            year: Some(2024),
            venue: Some("Journal of Testing".to_string()),
            doi: Some("10.1/test".to_string()),
            keywords: vec!["edited".to_string(), "kw".to_string()],
            subject: Some("An edited subject".to_string()),
        }
    }

    #[test]
    fn perform_document_edit_writes_pdf_renames_and_updates_fields() {
        let base = std::env::temp_dir().join(format!("codex_edit_test_{}", std::process::id()));
        let cache = base.join("cache");
        std::fs::create_dir_all(&base).unwrap();

        let original_path = base.join("Old Name.pdf");
        crate::pdf::build_test_pdf(&original_path, "Old Title", "Old Author", "Old", "old", "D:20200101000000Z");

        let existing = build_document(&original_path, &base.to_string_lossy(), "Papers");
        let input = edit_input(&existing.id, "Brand New Title");

        let (updated, pdf_warning) = perform_document_edit(&cache, &existing, input).unwrap();

        assert_eq!(pdf_warning, None, "a normal PDF should round-trip cleanly with no warning");
        assert_eq!(updated.title, "Brand New Title");
        assert_eq!(updated.authors, vec!["Ada Lovelace", "Charles Babbage"]);
        assert_eq!(updated.venue.as_deref(), Some("Journal of Testing"));
        assert_eq!(updated.doi.as_deref(), Some("10.1/test"));
        assert_eq!(updated.metadata_source, "manual");
        assert_eq!(updated.file_name, "Brand New Title.pdf");
        assert!(!original_path.exists(), "old file should be gone after rename");
        let new_path = base.join("Brand New Title.pdf");
        assert!(new_path.exists(), "renamed file should exist");
        assert_ne!(updated.id, existing.id, "id is path-derived, so it changes with the rename");

        // The PDF itself must reflect the edit, not just the DB-shaped struct.
        let reread = crate::pdf::extract_meta(&new_path);
        assert_eq!(reread.title.as_deref(), Some("Brand New Title"));
        assert_eq!(reread.authors, vec!["Ada Lovelace", "Charles Babbage"]);
        assert_eq!(reread.page_count, 1, "editing metadata must not touch page content");

        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn perform_document_edit_carries_thumbnail_to_new_id() {
        let base = std::env::temp_dir().join(format!("codex_edit_thumb_test_{}", std::process::id()));
        let cache = base.join("cache");
        std::fs::create_dir_all(&base).unwrap();

        let original_path = base.join("Doc.pdf");
        crate::pdf::build_test_pdf(&original_path, "Doc", "", "", "", "D:20200101000000Z");

        let mut existing = build_document(&original_path, &base.to_string_lossy(), "Papers");
        let thumb = crate::thumb::generate(&cache, &existing.id, &original_path, 128)
            .rendered_path()
            .expect("test fixture thumbnail should render")
            .to_path_buf();
        existing.thumbnail_path = Some(thumb.to_string_lossy().to_string());

        let input = edit_input(&existing.id, "Renamed Doc");
        let (updated, _pdf_warning) = perform_document_edit(&cache, &existing, input).unwrap();

        let new_thumb = updated.thumbnail_path.expect("thumbnail should survive the edit");
        assert!(new_thumb.contains(&updated.id), "thumbnail should be re-keyed to the new id");
        assert!(std::path::Path::new(&new_thumb).exists(), "carried-over thumbnail file must exist");
        assert!(!thumb.exists(), "old thumbnail file should have been moved, not duplicated");

        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn perform_document_edit_avoids_filename_collision() {
        let base = std::env::temp_dir().join(format!("codex_edit_collide_test_{}", std::process::id()));
        let cache = base.join("cache");
        std::fs::create_dir_all(&base).unwrap();

        let original_path = base.join("Doc A.pdf");
        crate::pdf::build_test_pdf(&original_path, "Doc A", "", "", "", "D:20200101000000Z");
        // A different file already sitting at the name we're about to rename into.
        std::fs::write(base.join("Taken Title.pdf"), b"unrelated").unwrap();

        let existing = build_document(&original_path, &base.to_string_lossy(), "Papers");
        let input = edit_input(&existing.id, "Taken Title");

        let (updated, _pdf_warning) = perform_document_edit(&cache, &existing, input).unwrap();

        assert_eq!(updated.file_name, "Taken Title (2).pdf");
        assert!(base.join("Taken Title.pdf").exists(), "the unrelated file must be untouched");

        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn perform_document_edit_degrades_gracefully_when_pdf_cannot_be_rewritten() {
        // Some real-world PDFs (encrypted, or with an exotic/malformed
        // structure) fail lopdf's write-then-reload verification. That must
        // not block the rest of the edit — rename and Codex's own catalog
        // should still go through, with a warning surfaced instead of a hard
        // failure. A file that isn't valid PDF at all exercises the same
        // "write_info_fields returned Err" path without needing a specific
        // exotic real-world PDF to reproduce.
        let base = std::env::temp_dir().join(format!("codex_edit_pdfwrite_fail_test_{}", std::process::id()));
        let cache = base.join("cache");
        std::fs::create_dir_all(&base).unwrap();

        let original_path = base.join("Not Actually A Pdf.pdf");
        std::fs::write(&original_path, b"not a pdf").unwrap();
        let existing = build_document(&original_path, &base.to_string_lossy(), "Papers");
        let input = edit_input(&existing.id, "New Title For Broken File");

        let (updated, pdf_warning) = perform_document_edit(&cache, &existing, input).unwrap();

        assert!(pdf_warning.is_some(), "an unrewritable PDF should surface a warning, not fail the whole edit");
        assert_eq!(updated.title, "New Title For Broken File", "Codex's own catalog must still reflect the edit");
        assert_eq!(updated.metadata_source, "manual");
        assert!(!original_path.exists(), "file should still be renamed even though its bytes weren't touched");
        assert!(base.join("New Title For Broken File.pdf").exists());

        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn perform_document_edit_rejects_blank_title_without_touching_file() {
        let base = std::env::temp_dir().join(format!("codex_edit_blank_test_{}", std::process::id()));
        let cache = base.join("cache");
        std::fs::create_dir_all(&base).unwrap();

        let original_path = base.join("Doc.pdf");
        crate::pdf::build_test_pdf(&original_path, "Doc", "", "", "", "D:20200101000000Z");
        let existing = build_document(&original_path, &base.to_string_lossy(), "Papers");

        let input = edit_input(&existing.id, "   ");
        let result = perform_document_edit(&cache, &existing, input);

        assert!(result.is_err());
        assert!(original_path.exists(), "original file must be untouched on validation failure");
        let meta = crate::pdf::extract_meta(&original_path);
        assert_eq!(meta.title.as_deref(), Some("Doc"), "PDF content must be unchanged");

        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn needs_rescan_is_true_for_never_seen_files() {
        assert!(needs_rescan(None, Path::new("/tmp/never_scanned.pdf")));
    }

    #[test]
    fn needs_rescan_is_false_for_manually_edited_docs() {
        let doc = {
            let mut d = build_document(Path::new("/tmp/never_scanned.pdf"), "/tmp", "Papers");
            d.metadata_source = "manual".to_string();
            d
        };
        // Even a real, freshly-modified file must never be silently overwritten
        // once the user has hand-edited it — so this stays false regardless of
        // what modified_at says.
        assert!(!needs_rescan(Some(&doc), Path::new("/tmp/never_scanned.pdf")));
    }

    #[test]
    fn needs_rescan_skips_unchanged_files_but_catches_edited_ones() {
        let base = std::env::temp_dir().join(format!("codex_needs_rescan_test_{}", std::process::id()));
        std::fs::create_dir_all(&base).unwrap();
        let pdf = base.join("Doc.pdf");
        crate::pdf::build_test_pdf(&pdf, "Doc", "", "", "", "D:20200101000000Z");

        let real_mtime = std::fs::metadata(&pdf).unwrap().modified().unwrap();
        let mut doc = build_document(&pdf, &base.to_string_lossy(), "Papers");
        doc.modified_at = Some(iso_from_systemtime(real_mtime));

        // Same on-disk mtime as what's stored -> nothing to do.
        assert!(!needs_rescan(Some(&doc), &pdf));

        // A stale/incorrect stored mtime (e.g. the file changed since) -> rescan.
        doc.modified_at = Some("2000-01-01T00:00:00+00:00".to_string());
        assert!(needs_rescan(Some(&doc), &pdf));

        std::fs::remove_dir_all(&base).ok();
    }

    fn overlap_test_folders() -> Vec<Folder> {
        vec![Folder {
            path: "/lib/Thesis".to_string(),
            name: "Thesis".to_string(),
            document_count: 5,
            added_at: "2026-01-01T00:00:00Z".to_string(),
        }]
    }

    #[test]
    fn detect_folder_overlap_flags_nested_and_containing_folders() {
        let existing = overlap_test_folders();

        let nested = detect_folder_overlap(&existing, Path::new("/lib/Thesis/Chapter1"));
        assert_eq!(nested.unwrap().kind, "nested_under");

        let containing = detect_folder_overlap(&existing, Path::new("/lib"));
        assert_eq!(containing.unwrap().kind, "contains");
    }

    #[test]
    fn detect_folder_overlap_allows_exact_match_and_unrelated_paths() {
        let existing = overlap_test_folders();

        // Same path as an existing collection: not an overlap, just a rescan.
        assert!(detect_folder_overlap(&existing, Path::new("/lib/Thesis")).is_none());
        // Unrelated sibling folder: no overlap.
        assert!(detect_folder_overlap(&existing, Path::new("/lib/OtherStuff")).is_none());
    }

    fn bib_doc(title: &str, authors: &[&str], year: Option<i32>) -> Document {
        let mut doc = build_document(Path::new("/tmp/never-scanned-bib-test.pdf"), "/tmp", "Papers");
        doc.title = title.to_string();
        doc.authors = authors.iter().map(|s| s.to_string()).collect();
        doc.year = year;
        doc
    }

    #[test]
    fn dedupe_citation_key_appends_letter_suffixes_on_collision() {
        let mut used = std::collections::HashSet::new();
        assert_eq!(dedupe_citation_key("smith2020", &mut used), "smith2020");
        assert_eq!(dedupe_citation_key("smith2020", &mut used), "smith2020a");
        assert_eq!(dedupe_citation_key("smith2020", &mut used), "smith2020b");
        // An unrelated key is untouched.
        assert_eq!(dedupe_citation_key("jones2021", &mut used), "jones2021");
    }

    #[test]
    fn bibtex_base_key_prefers_stored_citation_key_then_falls_back() {
        let mut doc = bib_doc("Some Title", &["Alice Smith", "Bob Jones"], Some(2020));
        assert_eq!(bibtex_base_key(&doc), "smith2020");

        doc.citation_key = Some("smith2020unique".to_string());
        assert_eq!(bibtex_base_key(&doc), "smith2020unique");
    }

    #[test]
    fn document_to_bibtex_includes_fields_and_escapes_braces() {
        let mut doc = bib_doc("A Title With {Braces}", &["Ada Lovelace"], Some(1843));
        doc.venue = Some("Journal of Testing".to_string());
        doc.doi = Some("10.1/test".to_string());
        doc.keywords = vec!["alpha".to_string(), "beta".to_string()];
        doc.abstract_text = Some("An abstract.".to_string());

        let bib = document_to_bibtex(&doc, "lovelace1843");
        assert!(bib.starts_with("@article{lovelace1843,\n"));
        assert!(bib.contains("title = {A Title With \\{Braces\\}},"));
        assert!(bib.contains("author = {Ada Lovelace},"));
        assert!(bib.contains("year = {1843},"));
        assert!(bib.contains("journal = {Journal of Testing},"));
        assert!(bib.contains("doi = {10.1/test},"));
        assert!(bib.contains("keywords = {alpha, beta},"));
        assert!(bib.contains("abstract = {An abstract.},"));

        // No venue -> @misc, not @article.
        let mut no_venue = doc.clone();
        no_venue.venue = None;
        assert!(document_to_bibtex(&no_venue, "k").starts_with("@misc{k,"));
    }

    #[test]
    fn batch_export_dedupes_across_colliding_documents() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        db::migrate(&conn).unwrap();
        db::add_folder(&conn, "/tmp", "tmp", "2026-01-01T00:00:00Z").unwrap();

        let mut a = bib_doc("First Paper", &["Alice Smith"], Some(2020));
        a.id = "a".to_string();
        a.path = "/tmp/a.pdf".to_string();
        let mut b = bib_doc("Second Paper", &["Ann Smith"], Some(2020)); // same derived key: smith2020
        b.id = "b".to_string();
        b.path = "/tmp/b.pdf".to_string();

        db::upsert_document(&conn, &a).unwrap();
        db::upsert_document(&conn, &b).unwrap();

        let mut used = std::collections::HashSet::new();
        let mut out = String::new();
        for doc in [&a, &b] {
            let key = dedupe_citation_key(&bibtex_base_key(doc), &mut used);
            out.push_str(&document_to_bibtex(doc, &key));
        }

        assert!(out.contains("@misc{smith2020,"));
        assert!(out.contains("@misc{smith2020a,"));
        assert!(out.contains("First Paper"));
        assert!(out.contains("Second Paper"));
    }

    #[test]
    fn exported_batch_round_trips_through_our_own_bibtex_importer() {
        // Three docs: two with colliding derived keys, one with a real DOI and
        // keywords, so the exported file exercises escaping, dedup, and the
        // fields the importer actually reads back.
        let mut a = bib_doc("Attention Is All You Need", &["Ashish Vaswani", "Noam Shazeer"], Some(2017));
        a.id = "a".to_string();
        a.venue = Some("NeurIPS".to_string());
        a.doi = Some("10.5555/attn".to_string());
        a.keywords = vec!["transformers".to_string(), "attention".to_string()];

        let mut b = bib_doc("A Different Paper With {Special} Chars", &["Ann Smith"], Some(2020));
        b.id = "b".to_string();
        let mut c = bib_doc("Yet Another Paper", &["Alice Smith"], Some(2020)); // collides with b's derived key
        c.id = "c".to_string();

        let mut used = std::collections::HashSet::new();
        let mut exported = String::new();
        let mut assigned_keys = Vec::new();
        for doc in [&a, &b, &c] {
            let key = dedupe_citation_key(&bibtex_base_key(doc), &mut used);
            exported.push_str(&document_to_bibtex(doc, &key));
            exported.push('\n');
            assigned_keys.push(key);
        }

        // The three keys must be distinct...
        let unique: std::collections::HashSet<_> = assigned_keys.iter().collect();
        assert_eq!(unique.len(), 3, "dedup must keep every key unique: {assigned_keys:?}");

        // ...and the file we just wrote must be valid BibTeX that our own
        // importer reads back with the right fields and no cross-talk.
        let reimported = BibIndex::parse(&exported).expect("exported .bib must re-parse");
        assert_eq!(reimported.entries.len(), 3);

        let vaswani = reimported
            .entries
            .iter()
            .find(|e| e.doi.as_deref() == Some("10.5555/attn"))
            .expect("DOI-bearing entry should round-trip");
        assert_eq!(vaswani.title.as_deref(), Some("Attention Is All You Need"));
        assert_eq!(vaswani.authors, vec!["Ashish Vaswani", "Noam Shazeer"]);
        assert_eq!(vaswani.year, Some(2017));
        assert_eq!(vaswani.venue.as_deref(), Some("NeurIPS"));
        assert!(vaswani.keywords.contains(&"attention".to_string()));

        let special = reimported
            .entries
            .iter()
            .find(|e| e.title.as_deref() == Some("A Different Paper With {Special} Chars"))
            .expect("escaped braces must round-trip to the original literal title");
        assert_eq!(special.authors, vec!["Ann Smith"]);
    }
}
