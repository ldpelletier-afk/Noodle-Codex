use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// Monotonic counter so each render gets its own scratch directory, even when
/// two threads render the same document id at the same time.
static WORK_SEQ: AtomicU64 = AtomicU64::new(0);

/// How many times to ask qlmanage for a thumbnail (per call) before giving up.
/// Quick Look is flaky when invoked in tight batches, so a couple of retries
/// meaningfully cut down on blank covers. Only applies to fast failures — a
/// hang (timeout) is never retried, since it won't fix itself.
const MAX_ATTEMPTS: u32 = 3;

/// Hard ceiling on a single qlmanage invocation. Quick Look hangs indefinitely
/// on malformed / encrypted / truncated PDFs, which would otherwise freeze the
/// whole render job on one bad file. A real first page renders in well under a
/// second, so this is generous.
const QL_TIMEOUT: Duration = Duration::from_secs(8);

/// Outcome of a thumbnail render.
#[derive(Debug, PartialEq)]
pub enum ThumbOutcome {
    /// Rendered successfully; the cached PNG is at this path.
    Rendered(PathBuf),
    /// qlmanage ran but produced no usable thumbnail (often transient).
    Failed,
    /// qlmanage hung and was killed — the PDF is almost certainly unsupported
    /// (encrypted, corrupt, password-protected), so retrying is pointless.
    TimedOut,
}

impl ThumbOutcome {
    pub fn rendered_path(&self) -> Option<&Path> {
        match self {
            ThumbOutcome::Rendered(p) => Some(p.as_path()),
            _ => None,
        }
    }
}

/// Directory where first-page thumbnails are cached, one PNG per document id.
pub fn thumbnails_dir(cache_root: &Path) -> PathBuf {
    cache_root.join("thumbnails")
}

fn thumb_path(cache_root: &Path, id: &str) -> PathBuf {
    thumbnails_dir(cache_root).join(format!("{id}.png"))
}

/// Returns true when a usable cached thumbnail already exists and is at least
/// as new as the source PDF (so edited PDFs get re-rendered).
fn is_cache_fresh(thumb: &Path, pdf: &Path) -> bool {
    let (Ok(tmeta), Ok(pmeta)) = (std::fs::metadata(thumb), std::fs::metadata(pdf)) else {
        return false;
    };
    if tmeta.len() == 0 {
        return false;
    }
    match (tmeta.modified(), pmeta.modified()) {
        (Ok(tm), Ok(pm)) => tm >= pm,
        _ => true, // if times are unavailable, trust the existing file
    }
}

/// Renders the first page of `pdf` to a cached PNG using macOS Quick Look
/// (`qlmanage`), which needs no extra dependencies. Retries a couple of times
/// on transient failure, but bails immediately if qlmanage hangs (a hang means
/// the PDF is unsupported and won't render on a retry either).
pub fn generate(cache_root: &Path, id: &str, pdf: &Path, size: u32) -> ThumbOutcome {
    let final_path = thumb_path(cache_root, id);
    if is_cache_fresh(&final_path, pdf) {
        return ThumbOutcome::Rendered(final_path);
    }

    let dir = thumbnails_dir(cache_root);
    if std::fs::create_dir_all(&dir).is_err() {
        return ThumbOutcome::Failed;
    }

    for attempt in 0..MAX_ATTEMPTS {
        match try_generate_once(&dir, &final_path, pdf, size) {
            ThumbOutcome::Rendered(path) => return ThumbOutcome::Rendered(path),
            ThumbOutcome::TimedOut => return ThumbOutcome::TimedOut,
            ThumbOutcome::Failed => {}
        }
        // Brief backoff before retrying — gives the Quick Look daemon room to
        // recover when a batch overwhelms it.
        if attempt + 1 < MAX_ATTEMPTS {
            std::thread::sleep(Duration::from_millis(150));
        }
    }
    ThumbOutcome::Failed
}

/// Runs qlmanage with a hard timeout. Returns whether it exited successfully;
/// `None` means it hung and was killed.
fn run_qlmanage_with_timeout(work: &Path, pdf: &Path, size: u32) -> Option<bool> {
    let mut child = Command::new("qlmanage")
        .arg("-t")
        .arg("-s")
        .arg(size.to_string())
        .arg("-o")
        .arg(work)
        .arg(pdf)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .ok()?;

    let deadline = Instant::now() + QL_TIMEOUT;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Some(status.success()),
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return None; // hung
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(_) => return Some(false),
        }
    }
}

/// A single qlmanage render attempt into a private scratch directory. The
/// scratch dir is unique per call (process id + a monotonic counter), so
/// concurrent renders — even of the same document — never clobber each other.
fn try_generate_once(dir: &Path, final_path: &Path, pdf: &Path, size: u32) -> ThumbOutcome {
    let seq = WORK_SEQ.fetch_add(1, Ordering::Relaxed);
    let work = dir.join(format!(".work-{}-{}", std::process::id(), seq));
    let _ = std::fs::remove_dir_all(&work);
    if std::fs::create_dir_all(&work).is_err() {
        return ThumbOutcome::Failed;
    }

    let succeeded = match run_qlmanage_with_timeout(&work, pdf, size) {
        Some(ok) => ok,
        None => {
            let _ = std::fs::remove_dir_all(&work);
            return ThumbOutcome::TimedOut;
        }
    };

    // qlmanage writes "<input-filename>.png" into the output dir; grab whatever
    // single PNG it produced and move it to the stable <id>.png name.
    let mut result = ThumbOutcome::Failed;
    if succeeded {
        if let Ok(entries) = std::fs::read_dir(&work) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.extension().and_then(|e| e.to_str()) == Some("png")
                    && std::fs::metadata(&p).map(|m| m.len() > 0).unwrap_or(false)
                {
                    if std::fs::rename(&p, final_path).is_ok()
                        || copy_then_remove(&p, final_path)
                    {
                        result = ThumbOutcome::Rendered(final_path.to_path_buf());
                    }
                    break;
                }
            }
        }
    }

    let _ = std::fs::remove_dir_all(&work);
    result
}

/// Fallback for rename across filesystems.
fn copy_then_remove(from: &Path, to: &Path) -> bool {
    if std::fs::copy(from, to).is_ok() {
        let _ = std::fs::remove_file(from);
        true
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_first_page_thumbnail() {
        let base = std::env::temp_dir().join(format!("codex_thumb_test_{}", std::process::id()));
        let cache = base.join("cache");
        std::fs::create_dir_all(&base).unwrap();

        let pdf = base.join("Doc.pdf");
        crate::pdf::build_test_pdf(&pdf, "Doc", "Author", "Subj", "kw", "D:20200101000000Z");

        let out = generate(&cache, "abc123", &pdf, 256);
        let path = out.rendered_path().expect("qlmanage should produce a thumbnail").to_path_buf();
        assert!(path.exists());
        assert!(std::fs::metadata(&path).unwrap().len() > 0, "thumbnail not empty");

        // Second call should hit the fresh cache and return the same path.
        let again = generate(&cache, "abc123", &pdf, 256);
        assert_eq!(again.rendered_path(), Some(path.as_path()));

        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn malformed_pdf_times_out_instead_of_hanging() {
        // qlmanage hangs indefinitely on garbage PDFs; the timeout must turn
        // that into a bounded TimedOut rather than freezing the whole job.
        let base = std::env::temp_dir().join(format!("codex_thumb_timeout_{}", std::process::id()));
        let cache = base.join("cache");
        std::fs::create_dir_all(&base).unwrap();

        let bad = base.join("garbage.pdf");
        std::fs::write(&bad, b"%PDF-1.7 this is not really a pdf").unwrap();

        let started = std::time::Instant::now();
        let outcome = generate(&cache, "bad", &bad, 128);
        let elapsed = started.elapsed();

        assert_eq!(outcome, ThumbOutcome::TimedOut, "a hanging render should report TimedOut");
        // One timeout (~8s), not three stacked retries — a hang isn't retried.
        assert!(elapsed < QL_TIMEOUT * 2, "timed-out render must not be retried: took {elapsed:?}");
        assert!(!cache.join("thumbnails").join("bad.png").exists());

        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn concurrent_generate_of_same_id_is_safe() {
        // Reproduces the re-add-folder scenario: two threads render the SAME
        // document id at the same time. With per-call scratch dirs neither
        // corrupts the other, and the cached file ends up valid.
        let base = std::env::temp_dir().join(format!("codex_thumb_concurrent_{}", std::process::id()));
        let cache = base.join("cache");
        std::fs::create_dir_all(&base).unwrap();

        let pdf = base.join("Doc.pdf");
        crate::pdf::build_test_pdf(&pdf, "Doc", "", "", "", "D:20200101000000Z");

        let handles: Vec<_> = (0..4)
            .map(|_| {
                let cache = cache.clone();
                let pdf = pdf.clone();
                std::thread::spawn(move || generate(&cache, "shared-id", &pdf, 128).rendered_path().is_some())
            })
            .collect();

        // Every concurrent render should succeed (none destroyed by a sibling).
        for h in handles {
            assert!(h.join().unwrap(), "concurrent render should still produce a thumbnail");
        }

        let final_path = cache.join("thumbnails").join("shared-id.png");
        assert!(final_path.exists());
        assert!(std::fs::metadata(&final_path).unwrap().len() > 0, "cached thumbnail must be non-empty");

        // No scratch dirs left behind.
        let leftover = std::fs::read_dir(cache.join("thumbnails"))
            .unwrap()
            .filter_map(|e| e.ok())
            .any(|e| e.file_name().to_string_lossy().starts_with(".work-"));
        assert!(!leftover, "scratch dirs should be cleaned up");

        std::fs::remove_dir_all(&base).ok();
    }
}
