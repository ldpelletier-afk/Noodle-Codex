use crate::commands;
use crate::db;
use notify::event::{AccessKind, AccessMode};
use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, Once, OnceLock};
use std::time::{Duration, Instant};
use tauri::AppHandle;

/// How long a path must go quiet before we treat a write as finished. A file
/// copy/download typically fires several events in quick succession (create,
/// then one or more modifies); this collapses them into a single ingest.
const QUIET_PERIOD: Duration = Duration::from_millis(800);
/// How often the debounce thread checks for paths whose quiet period elapsed.
const POLL_INTERVAL: Duration = Duration::from_millis(400);

fn watchers() -> &'static Mutex<HashMap<String, RecommendedWatcher>> {
    static WATCHERS: OnceLock<Mutex<HashMap<String, RecommendedWatcher>>> = OnceLock::new();
    WATCHERS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn pending() -> &'static Mutex<HashMap<PathBuf, Instant>> {
    static PENDING: OnceLock<Mutex<HashMap<PathBuf, Instant>>> = OnceLock::new();
    PENDING.get_or_init(|| Mutex::new(HashMap::new()))
}

/// True for paths worth waking a reconcile pass over — everything except
/// hidden files/dirs (dotfiles, `.DS_Store`, `.git`, etc). Deliberately not
/// restricted to `.pdf` paths or to files (as opposed to directories):
/// correctness comes from the reconcile's own directory walk, not from
/// interpreting exactly which path an event names — a bulk directory delete
/// may report a `Remove` for the directory itself rather than (or in addition
/// to) each file inside it, so this only needs to decide "something worth
/// caring about changed near here", not "what changed".
pub fn is_relevant_path(path: &Path) -> bool {
    !path
        .components()
        .filter_map(|c| c.as_os_str().to_str())
        .any(|s| s.starts_with('.'))
}

/// Event kinds worth debouncing: file/dir creation, content changes, removal,
/// and a file handle closing after a write (how many apps/downloads finalize
/// a copy).
pub fn is_relevant_event_kind(kind: &EventKind) -> bool {
    matches!(
        kind,
        EventKind::Create(_)
            | EventKind::Modify(_)
            | EventKind::Remove(_)
            | EventKind::Access(AccessKind::Close(AccessMode::Write))
    )
}

/// Resolves symlinks so comparisons are on the real path, falling back to the
/// input unchanged if it no longer exists (e.g. a file deleted mid-debounce).
/// Needed because FSEvents reports canonicalized paths — e.g. macOS's own
/// `/var/folders/...` temp dirs are actually a symlink to `/private/var/...`,
/// and notify reports the resolved form.
fn canonical_or_self(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

/// Finds which tracked folder (if any) contains `path`. The longest matching
/// root wins in case of nesting — overlapping folders are rejected elsewhere,
/// but this stays correct regardless.
pub fn resolve_tracked_folder<'a>(
    folder_paths: impl Iterator<Item = &'a str>,
    path: &Path,
) -> Option<String> {
    let canonical_path = canonical_or_self(path);
    folder_paths
        .filter(|f| canonical_path.starts_with(canonical_or_self(Path::new(f))))
        .max_by_key(|f| f.len())
        .map(|f| f.to_string())
}

/// Drains entries whose quiet period has elapsed as of `now`, removing them
/// from `map`.
fn drain_ready(map: &mut HashMap<PathBuf, Instant>, quiet: Duration, now: Instant) -> Vec<PathBuf> {
    let ready: Vec<PathBuf> = map
        .iter()
        .filter(|(_, &seen)| now.duration_since(seen) >= quiet)
        .map(|(p, _)| p.clone())
        .collect();
    for p in &ready {
        map.remove(p);
    }
    ready
}

/// Starts watching every currently-tracked folder — call once at app startup.
pub fn start_all(app: AppHandle, conn: &rusqlite::Connection) {
    if let Ok(folders) = db::list_folders(conn) {
        for f in folders {
            start_folder(app.clone(), f.path);
        }
    }
}

/// Starts watching one folder for new/changed PDFs, unless already watching
/// it. Safe to call repeatedly (e.g. on every "Add folder", including a
/// rescan of an already-tracked path).
pub fn start_folder(app: AppHandle, folder_path: String) {
    ensure_drain_thread(app);

    let mut guard = watchers().lock().unwrap_or_else(|p| p.into_inner());
    if guard.contains_key(&folder_path) {
        return;
    }

    let watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        let Ok(event) = res else { return };
        if !is_relevant_event_kind(&event.kind) {
            return;
        }
        let mut buf = pending().lock().unwrap_or_else(|p| p.into_inner());
        let now = Instant::now();
        for path in event.paths {
            if is_relevant_path(&path) {
                buf.insert(path, now);
            }
        }
    });

    match watcher {
        Ok(mut w) => {
            if w.watch(Path::new(&folder_path), RecursiveMode::Recursive).is_ok() {
                guard.insert(folder_path, w);
            }
        }
        Err(_) => { /* best-effort: no live updates for this folder, manual rescans still work */ }
    }
}

/// Stops watching a folder (e.g. after `remove_folder`). Dropping the watcher
/// tears down its underlying OS resources.
pub fn stop_folder(folder_path: &str) {
    watchers().lock().unwrap_or_else(|p| p.into_inner()).remove(folder_path);
}

/// Starts the single background thread that periodically drains debounced
/// paths and ingests them. Idempotent — only the first call actually spawns it.
fn ensure_drain_thread(app: AppHandle) {
    static STARTED: Once = Once::new();
    STARTED.call_once(move || {
        std::thread::spawn(move || loop {
            std::thread::sleep(POLL_INTERVAL);
            let ready = {
                let mut buf = pending().lock().unwrap_or_else(|p| p.into_inner());
                drain_ready(&mut buf, QUIET_PERIOD, Instant::now())
            };
            if ready.is_empty() {
                continue;
            }
            process_ready_paths(&app, ready);
        });
    });
}

/// Finds which tracked folders had activity near the drained paths, then
/// reconciles each of them once (add/change + prune) rather than trying to
/// interpret the individual paths — see `is_relevant_path` for why.
fn process_ready_paths(app: &AppHandle, ready: Vec<PathBuf>) {
    let Ok(paths) = commands::app_paths(app) else { return };
    let Ok(conn) = rusqlite::Connection::open(&paths.db_file) else { return };
    let _ = db::configure(&conn);
    let Ok(folders) = db::list_folders(&conn) else { return };

    let mut affected: std::collections::HashSet<String> = std::collections::HashSet::new();
    for p in ready {
        if let Some(folder) = resolve_tracked_folder(folders.iter().map(|f| f.path.as_str()), &p) {
            affected.insert(folder);
        }
    }

    for folder_path in affected {
        let Some(folder) = folders.iter().find(|f| f.path == folder_path) else { continue };
        commands::reconcile_folder(&conn, app, &folder_path, &folder.name);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use notify::event::{DataChange, ModifyKind};

    #[test]
    fn is_relevant_path_excludes_hidden_segments_but_allows_any_extension() {
        assert!(is_relevant_path(Path::new("/lib/Thesis/paper.pdf")));
        // Not restricted to .pdf or to files — a plain subfolder name (as
        // reported by a directory Remove/Create event) must pass too, since
        // correctness comes from the reconcile's own walk, not this filter.
        assert!(is_relevant_path(Path::new("/lib/Thesis/Subfolder")));
        assert!(is_relevant_path(Path::new("/lib/Thesis/notes.txt")));
        assert!(!is_relevant_path(Path::new("/lib/Thesis/.DS_Store")));
        assert!(!is_relevant_path(Path::new("/lib/.git/objects/paper.pdf")), "hidden ancestor dir must be excluded");
    }

    #[test]
    fn is_relevant_event_kind_matches_create_modify_remove_and_write_close() {
        assert!(is_relevant_event_kind(&EventKind::Create(notify::event::CreateKind::File)));
        assert!(is_relevant_event_kind(&EventKind::Modify(ModifyKind::Data(DataChange::Any))));
        assert!(is_relevant_event_kind(&EventKind::Access(AccessKind::Close(AccessMode::Write))));
        // Deletions must wake a reconcile pass too, so removed files/folders
        // get pruned from Codex without waiting for a manual rescan.
        assert!(is_relevant_event_kind(&EventKind::Remove(notify::event::RemoveKind::File)));
        assert!(is_relevant_event_kind(&EventKind::Remove(notify::event::RemoveKind::Folder)));
        assert!(!is_relevant_event_kind(&EventKind::Access(AccessKind::Open(AccessMode::Read))));
    }

    #[test]
    fn resolve_tracked_folder_picks_the_containing_and_longest_root() {
        let folders = vec!["/lib/Thesis".to_string(), "/lib/Thesis/Sources".to_string(), "/lib/Other".to_string()];

        // Deepest matching root wins when folders happen to nest.
        assert_eq!(
            resolve_tracked_folder(folders.iter().map(|s| s.as_str()), Path::new("/lib/Thesis/Sources/a.pdf")),
            Some("/lib/Thesis/Sources".to_string())
        );
        assert_eq!(
            resolve_tracked_folder(folders.iter().map(|s| s.as_str()), Path::new("/lib/Thesis/intro.pdf")),
            Some("/lib/Thesis".to_string())
        );
        assert_eq!(
            resolve_tracked_folder(folders.iter().map(|s| s.as_str()), Path::new("/lib/Unrelated/x.pdf")),
            None
        );
    }

    #[test]
    fn drain_ready_only_returns_entries_past_the_quiet_period() {
        let now = Instant::now();
        let mut map = HashMap::new();
        map.insert(PathBuf::from("/a.pdf"), now - Duration::from_millis(900));
        map.insert(PathBuf::from("/b.pdf"), now - Duration::from_millis(100));

        let ready = drain_ready(&mut map, Duration::from_millis(800), now);

        assert_eq!(ready, vec![PathBuf::from("/a.pdf")]);
        assert!(map.contains_key(Path::new("/b.pdf")), "still-quiet entry should remain pending");
        assert!(!map.contains_key(Path::new("/a.pdf")), "drained entry should be removed");
    }

    #[test]
    fn real_filesystem_events_are_detected_by_notify() {
        // End-to-end proof that the notify crate + our filtering actually see
        // real OS-level file creation, not just that our pure helpers agree
        // with each other in isolation.
        let dir = std::env::temp_dir().join(format!("codex_watch_test_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();

        let (tx, rx) = std::sync::mpsc::channel::<PathBuf>();
        let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
            let Ok(event) = res else { return };
            if !is_relevant_event_kind(&event.kind) {
                return;
            }
            for path in event.paths {
                if is_relevant_path(&path) {
                    let _ = tx.send(path);
                }
            }
        })
        .unwrap();
        watcher.watch(&dir, RecursiveMode::Recursive).unwrap();

        // Give the watcher a moment to fully register before writing.
        std::thread::sleep(Duration::from_millis(200));
        let new_pdf = dir.join("dropped.pdf");
        std::fs::write(&new_pdf, b"%PDF-1.4\n%%EOF").unwrap();

        let seen = rx.recv_timeout(Duration::from_secs(5));
        assert!(seen.is_ok(), "expected a filesystem event for the new PDF within 5s");
        // FSEvents reports the canonicalized path (e.g. macOS's temp dir is a
        // `/var/...` symlink to `/private/var/...`), which is exactly why
        // `resolve_tracked_folder` canonicalizes before comparing.
        assert_eq!(seen.unwrap(), canonical_or_self(&new_pdf));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn resolve_tracked_folder_matches_through_a_symlinked_ancestor() {
        // Regression test for the real bug the integration test above caught:
        // a tracked folder recorded via its symlinked path (as the OS may
        // report it) must still match events reported via the resolved path.
        let base = std::env::temp_dir().join(format!("codex_watch_symlink_test_{}", std::process::id()));
        let real_dir = base.join("real");
        let link = base.join("link");
        std::fs::create_dir_all(&real_dir).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&real_dir, &link).unwrap();

        let folders = vec![link.to_string_lossy().to_string()];
        // The event only fires once the file exists, so — same as in
        // production — its canonicalization succeeds through the same
        // symlink chain as the tracked folder's.
        let event_path = real_dir.join("paper.pdf");
        std::fs::write(&event_path, b"%PDF-1.4\n%%EOF").unwrap();

        assert_eq!(
            resolve_tracked_folder(folders.iter().map(|s| s.as_str()), &event_path),
            Some(link.to_string_lossy().to_string())
        );

        std::fs::remove_dir_all(&base).ok();
    }
}
