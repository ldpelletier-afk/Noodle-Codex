//! SumatraPDF integration — the Windows counterpart to Skim. Sumatra opens a
//! file at a given page from the command line, and records the page each file
//! was left on in its plain-text settings file (on open, and again when the
//! file is closed). Codex reads that back as reading progress.

// Only called on Windows; the parser is tested everywhere.
#![cfg_attr(not(windows), allow(dead_code))]

use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Locates an installed SumatraPDF: the per-user install (Sumatra's default),
/// then a machine-wide one. Returns `None` elsewhere (including on macOS,
/// where these variables don't exist).
pub fn find_exe() -> Option<PathBuf> {
    [
        ("LOCALAPPDATA", r"SumatraPDF\SumatraPDF.exe"),
        ("ProgramFiles", r"SumatraPDF\SumatraPDF.exe"),
        ("ProgramFiles(x86)", r"SumatraPDF\SumatraPDF.exe"),
    ]
    .iter()
    .filter_map(|(var, rel)| std::env::var_os(var).map(|dir| PathBuf::from(dir).join(rel)))
    .find(|p| p.is_file())
}

/// Where `exe` keeps its settings: next to the executable for portable copies,
/// otherwise `%LOCALAPPDATA%\SumatraPDF\SumatraPDF-settings.txt` (confirmed
/// for the standard installer).
pub fn settings_file(exe: &Path) -> Option<PathBuf> {
    let portable = exe.with_file_name("SumatraPDF-settings.txt");
    if portable.is_file() {
        return Some(portable);
    }
    std::env::var_os("LOCALAPPDATA")
        .map(|dir| PathBuf::from(dir).join("SumatraPDF").join("SumatraPDF-settings.txt"))
}

/// Opens `pdf` in Sumatra, at `page` when it's non-zero. `-reuse-instance`
/// sends it to an already-running Sumatra (as a tab) rather than starting a
/// second copy, and moves an already-open file to that page.
pub fn open_at_page(exe: &Path, pdf: &str, page: u32) -> bool {
    let mut cmd = std::process::Command::new(exe);
    cmd.arg("-reuse-instance");
    if page > 0 {
        cmd.arg("-page").arg(page.to_string());
    }
    cmd.arg(pdf).spawn().is_ok()
}

/// The page Sumatra recorded for each file, keyed by lowercased path (Windows
/// paths are case-insensitive). Each entry in the settings file starts with
/// `FilePath = …` and later has `PageNo = …`; nested lists such as
/// `Favorites [ ]` may sit in between. A file listed under both `FileStates`
/// and `SessionData` keeps its first (`FileStates`) value.
pub fn parse_saved_pages(settings: &str) -> HashMap<String, u32> {
    let mut pages = HashMap::new();
    let mut current: Option<String> = None;
    for line in settings.lines() {
        let line = line.trim();
        if let Some(path) = line.strip_prefix("FilePath = ") {
            current = Some(path.trim().to_lowercase());
        } else if let Some(page) = line.strip_prefix("PageNo = ") {
            if let (Some(path), Ok(page)) = (current.take(), page.trim().parse::<u32>()) {
                pages.entry(path).or_insert(page);
            }
        }
    }
    pages
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Shape of a real settings file, as written by SumatraPDF on Windows
    /// (captured in CI), with a `Favorites` list between path and page.
    const SAMPLE: &str = "\
RememberStatePerDocument = true
ReuseInstance = true
FileStates [
\t[
\t\tFilePath = C:\\probe dir\\Three Pages.pdf
\t\tFavorites [
\t\t]
\t\tIsPinned = false
\t\tPageNo = 3
\t\tZoom = fit page
\t]
\t[
\t\tFilePath = D:\\Papers\\Other.pdf
\t\tPageNo = 12
\t]
]
SessionData [
\t[
\t\tTabStates [
\t\t\t[
\t\t\t\tFilePath = C:\\probe dir\\Three Pages.pdf
\t\t\t\tPageNo = 7
\t\t\t]
\t\t]
\t]
]
";

    #[test]
    fn parse_saved_pages_reads_each_file() {
        let pages = parse_saved_pages(SAMPLE);
        assert_eq!(pages.get(r"c:\probe dir\three pages.pdf"), Some(&3), "FileStates wins over SessionData");
        assert_eq!(pages.get(r"d:\papers\other.pdf"), Some(&12));
        assert_eq!(pages.len(), 2);
    }

    /// End-to-end against a real SumatraPDF install, through Codex's own
    /// functions: open a file, move it to page 3 the way Codex does, close
    /// Sumatra, and read the page back. Needs Sumatra installed, so it's
    /// ignored by default; CI installs it and runs this explicitly.
    #[cfg(windows)]
    #[test]
    #[ignore]
    fn sumatra_end_to_end() {
        use std::time::Duration;
        let exe = find_exe().expect("SumatraPDF should be installed");
        let settings = settings_file(&exe).expect("settings file location");
        let dir = std::env::temp_dir().join("codex sumatra e2e");
        std::fs::create_dir_all(&dir).unwrap();
        let pdf = dir.join("Three Pages.pdf");
        crate::pdf::build_test_pdf_pages(&pdf, 3);
        let pdf_str = pdf.to_string_lossy().to_string();

        let close_sumatra = || {
            for _ in 0..3 {
                let _ = std::process::Command::new("taskkill").args(["/IM", "SumatraPDF.exe"]).output();
                std::thread::sleep(Duration::from_secs(3));
            }
        };

        assert!(open_at_page(&exe, &pdf_str, 0));
        std::thread::sleep(Duration::from_secs(10));
        assert!(open_at_page(&exe, &pdf_str, 3), "jump the open file to page 3");
        std::thread::sleep(Duration::from_secs(8));
        close_sumatra();

        let text = std::fs::read_to_string(&settings).expect("Sumatra should have written its settings");
        let pages = parse_saved_pages(&text);
        let _ = std::process::Command::new("taskkill").args(["/F", "/IM", "SumatraPDF.exe"]).output();
        std::fs::remove_dir_all(&dir).ok();
        assert_eq!(pages.get(&pdf_str.to_lowercase()), Some(&3), "settings: {text}");
    }

    #[test]
    fn parse_saved_pages_ignores_malformed_lines() {
        assert!(parse_saved_pages("PageNo = 4\nFilePath = C:\\a.pdf\nPageNo = x\n").is_empty());
    }
}
