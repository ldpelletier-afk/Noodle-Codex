use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::Path;

/// Bibliographic fields pulled from a PDF's Info dictionary.
#[derive(Default, Debug)]
pub struct PdfMeta {
    pub title: Option<String>,
    pub authors: Vec<String>,
    pub subject: Option<String>,
    pub keywords: Vec<String>,
    pub page_count: u32,
    pub year: Option<i32>,
}

/// Stable per-file id: hex of a hash of the absolute path. Path-based (not
/// content-based) so moving/renaming is treated as a new document, which
/// matches how the folder-scan model works.
pub fn doc_id(path: &Path) -> String {
    let mut hasher = DefaultHasher::new();
    path.to_string_lossy().hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

/// Turns "attention_is-all  you.need" into "Attention Is All You Need" for a
/// readable fallback when the PDF has no Title metadata.
fn humanize_file_stem(stem: &str) -> String {
    let cleaned = stem.replace(['_', '-'], " ");
    let mut out = String::new();
    for (i, word) in cleaned.split_whitespace().enumerate() {
        if i > 0 {
            out.push(' ');
        }
        let mut chars = word.chars();
        match chars.next() {
            Some(first) => {
                out.extend(first.to_uppercase());
                out.push_str(chars.as_str());
            }
            None => {}
        }
    }
    if out.is_empty() {
        stem.to_string()
    } else {
        out
    }
}

/// Best-effort readable title: PDF Title metadata if present and non-empty,
/// otherwise a humanized file name.
pub fn readable_title(meta: &PdfMeta, path: &Path) -> String {
    if let Some(t) = &meta.title {
        let t = t.trim();
        if !t.is_empty() {
            return t.to_string();
        }
    }
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "Untitled".to_string());
    humanize_file_stem(&stem)
}

/// PDF date strings look like "D:20230115120000Z" — pull out the 4-digit year.
fn year_from_pdf_date(raw: &str) -> Option<i32> {
    let digits: String = raw
        .trim_start_matches("D:")
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    if digits.len() >= 4 {
        digits[0..4].parse::<i32>().ok().filter(|y| *y >= 1000 && *y <= 3000)
    } else {
        None
    }
}

/// Authors may be a single string with several names; split on common
/// separators so the UI can show them individually.
fn split_authors(raw: &str) -> Vec<String> {
    raw.split([';', ',', '&'])
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect()
}

fn split_keywords(raw: &str) -> Vec<String> {
    raw.split([';', ',', '\n'])
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect()
}

/// Reads the Info dictionary and page count. Returns a best-effort result even
/// for partially malformed PDFs (page_count may be 0, fields may be empty).
pub fn extract_meta(path: &Path) -> PdfMeta {
    let mut meta = PdfMeta::default();

    let doc = match lopdf::Document::load(path) {
        Ok(d) => d,
        Err(_) => return meta,
    };

    meta.page_count = doc.get_pages().len() as u32;

    // The trailer's /Info points at the metadata dictionary.
    if let Ok(info_ref) = doc.trailer.get(b"Info") {
        if let Ok(id) = info_ref.as_reference() {
            if let Ok(dict) = doc.get_dictionary(id) {
                let get_str = |key: &[u8]| -> Option<String> {
                    dict.get(key)
                        .ok()
                        .and_then(|o| o.as_str().ok())
                        .map(|b| decode_pdf_string(b))
                        .filter(|s| !s.trim().is_empty())
                };

                meta.title = get_str(b"Title");
                if let Some(a) = get_str(b"Author") {
                    meta.authors = split_authors(&a);
                }
                meta.subject = get_str(b"Subject");
                if let Some(k) = get_str(b"Keywords") {
                    meta.keywords = split_keywords(&k);
                }
                // Prefer creation date, fall back to modification date.
                let date = get_str(b"CreationDate").or_else(|| get_str(b"ModDate"));
                if let Some(d) = date {
                    meta.year = year_from_pdf_date(&d);
                }
            }
        }
    }

    meta
}

/// Sanitizes a title into a filesystem-safe file stem: strips characters that
/// are illegal or awkward on macOS/Windows paths, collapses whitespace, and
/// caps length so the final `<stem>.pdf` stays well under path limits.
pub fn sanitize_filename(title: &str) -> String {
    let mut out = String::new();
    for c in title.chars() {
        match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' | '\0' => out.push(' '),
            c if c.is_control() => out.push(' '),
            c => out.push(c),
        }
    }
    let collapsed = out.split_whitespace().collect::<Vec<_>>().join(" ");
    let trimmed = collapsed.trim_matches('.').trim();
    let capped: String = trimmed.chars().take(150).collect();
    if capped.is_empty() {
        "Untitled".to_string()
    } else {
        capped
    }
}

/// Picks a file path in `dir` named `<stem>.pdf`, or `<stem> (2).pdf`, etc. if
/// that name is already taken by a *different* file than `except`.
pub fn unique_pdf_path(dir: &Path, stem: &str, except: &Path) -> std::path::PathBuf {
    let mut candidate = dir.join(format!("{stem}.pdf"));
    let mut n = 2;
    while candidate.exists() && candidate != except {
        candidate = dir.join(format!("{stem} ({n}).pdf"));
        n += 1;
    }
    candidate
}

/// Rewrites a PDF's Info dictionary (Title/Author/Subject/Keywords — the only
/// fields Codex also reads back) and atomically replaces the original file.
///
/// Safety: the rewritten document is saved to a sibling temp file first and
/// re-parsed to confirm it's still a valid PDF with the same page count
/// before anything on disk is replaced. If that check fails, the original
/// file is left completely untouched and an error is returned.
pub fn write_info_fields(
    path: &Path,
    title: &str,
    author: &str,
    subject: &str,
    keywords: &str,
) -> Result<(), String> {
    use lopdf::dictionary;

    let mut doc = lopdf::Document::load(path).map_err(|e| format!("couldn't open PDF: {e}"))?;
    // lopdf leaves encrypted objects as-is, so a plain-text Info dictionary
    // would be "decrypted" into garbage by real readers. Don't touch these.
    if doc.is_encrypted() {
        return Err("the PDF is encrypted, so its built-in metadata can't be edited; original left untouched".to_string());
    }
    let original_pages = doc.get_pages().len();

    let info_dict = dictionary! {
        "Title" => lopdf::Object::string_literal(title),
        "Author" => lopdf::Object::string_literal(author),
        "Subject" => lopdf::Object::string_literal(subject),
        "Keywords" => lopdf::Object::string_literal(keywords),
    };

    match doc.trailer.get(b"Info").ok().and_then(|o| o.as_reference().ok()) {
        Some(info_id) => {
            doc.objects.insert(info_id, lopdf::Object::Dictionary(info_dict));
        }
        None => {
            let info_id = doc.add_object(info_dict);
            doc.trailer.set("Info", info_id);
        }
    }

    // lopdf writes every object into one fresh classic xref table, but keeps
    // the original trailer. Entries describing the old file's layout — /Prev
    // (earlier incremental-update sections), /XRefStm, and an xref stream's
    // own /Type /W /Index /Filter… — then point at offsets that no longer
    // exist, and the output fails to parse ("Invalid file trailer"). Keep
    // only the entries that describe the document itself.
    let mut trailer = lopdf::Dictionary::new();
    for key in [&b"Root"[..], b"Info", b"ID"] {
        if let Ok(value) = doc.trailer.get(key) {
            trailer.set(key.to_vec(), value.clone());
        }
    }
    doc.trailer = trailer;
    doc.reference_table.cross_reference_type = lopdf::xref::XrefType::CrossReferenceTable;

    let dir = path.parent().ok_or("PDF has no parent directory")?;
    let tmp_path = dir.join(format!(
        ".codex-tmp-{}-{}.pdf",
        std::process::id(),
        path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default()
    ));

    doc.save(&tmp_path).map_err(|e| format!("couldn't save rewritten PDF: {e}"))?;

    // Verify before touching the original: must still parse and have the
    // same page count.
    let verify = lopdf::Document::load(&tmp_path)
        .map_err(|e| format!("rewritten PDF failed to reload ({e}); original left untouched"));
    let verify_result = verify.and_then(|v| {
        if v.get_pages().len() == original_pages {
            Ok(())
        } else {
            Err("rewritten PDF has a different page count; original left untouched".to_string())
        }
    });

    match verify_result {
        Ok(()) => {
            std::fs::rename(&tmp_path, path)
                .map_err(|e| format!("verified, but couldn't replace original file: {e}"))?;
            Ok(())
        }
        Err(e) => {
            let _ = std::fs::remove_file(&tmp_path);
            Err(e)
        }
    }
}

/// Builds a valid single-page PDF with the given Info fields, for tests and
/// as a shared fixture builder. Uses lopdf so the output is guaranteed
/// round-trippable by `extract_meta`.
#[cfg(test)]
pub fn build_test_pdf(
    path: &Path,
    title: &str,
    author: &str,
    subject: &str,
    keywords: &str,
    creation_date: &str,
) {
    use lopdf::dictionary;
    use lopdf::{Document, Object, Stream};

    let mut doc = Document::with_version("1.5");
    let pages_id = doc.new_object_id();
    let content = b"BT /F1 24 Tf 72 700 Td (Test Page) Tj ET".to_vec();
    let content_id = doc.add_object(Stream::new(dictionary! {}, content));
    let font_id = doc.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica",
    });
    let resources_id = doc.add_object(dictionary! {
        "Font" => dictionary! { "F1" => font_id },
    });
    let page_id = doc.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        "Contents" => content_id,
        "Resources" => resources_id,
    });
    let pages = dictionary! {
        "Type" => "Pages",
        "Kids" => vec![page_id.into()],
        "Count" => 1,
    };
    doc.objects.insert(pages_id, Object::Dictionary(pages));
    let catalog_id = doc.add_object(dictionary! {
        "Type" => "Catalog", "Pages" => pages_id,
    });
    let info_id = doc.add_object(dictionary! {
        "Title" => Object::string_literal(title),
        "Author" => Object::string_literal(author),
        "Subject" => Object::string_literal(subject),
        "Keywords" => Object::string_literal(keywords),
        "CreationDate" => Object::string_literal(creation_date),
    });
    doc.trailer.set("Root", catalog_id);
    doc.trailer.set("Info", info_id);
    doc.save(path).expect("save test pdf");
}

/// PDF strings are either PDFDocEncoding (Latin-1-ish) or UTF-16BE with a BOM.
fn decode_pdf_string(bytes: &[u8]) -> String {
    if bytes.len() >= 2 && bytes[0] == 0xFE && bytes[1] == 0xFF {
        // UTF-16BE
        let u16s: Vec<u16> = bytes[2..]
            .chunks_exact(2)
            .map(|c| u16::from_be_bytes([c[0], c[1]]))
            .collect();
        String::from_utf16_lossy(&u16s)
    } else {
        // Treat as Latin-1 / PDFDocEncoding; lossy UTF-8 fallback.
        match std::str::from_utf8(bytes) {
            Ok(s) => s.to_string(),
            Err(_) => bytes.iter().map(|&b| b as char).collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn humanize_cleans_file_stems() {
        assert_eq!(humanize_file_stem("attention_is-all  you"), "Attention Is All You");
        assert_eq!(humanize_file_stem("single"), "Single");
    }

    #[test]
    fn year_parses_from_pdf_date() {
        assert_eq!(year_from_pdf_date("D:20230115120000Z"), Some(2023));
        assert_eq!(year_from_pdf_date("20191231"), Some(2019));
        assert_eq!(year_from_pdf_date("garbage"), None);
    }

    #[test]
    fn extract_meta_reads_info_dict_and_pages() {
        let dir = std::env::temp_dir().join(format!("codex_pdf_test_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let pdf = dir.join("Sample.pdf");
        build_test_pdf(
            &pdf,
            "A Sample Research Paper",
            "Jane Q. Researcher; John Doe",
            "Testing metadata",
            "pdf, test, codex",
            "D:20220615000000Z",
        );

        let meta = extract_meta(&pdf);
        assert_eq!(meta.title.as_deref(), Some("A Sample Research Paper"));
        assert_eq!(meta.authors, vec!["Jane Q. Researcher", "John Doe"]);
        assert_eq!(meta.subject.as_deref(), Some("Testing metadata"));
        assert_eq!(meta.keywords, vec!["pdf", "test", "codex"]);
        assert_eq!(meta.page_count, 1);
        assert_eq!(meta.year, Some(2022));

        // Title fallback when metadata is absent uses a humanized file name.
        let meta_none = PdfMeta::default();
        let fallback_path = dir.join("some_paper-title.pdf");
        assert_eq!(readable_title(&meta_none, &fallback_path), "Some Paper Title");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn sanitize_filename_strips_illegal_chars_and_collapses_space() {
        assert_eq!(sanitize_filename("Report: Q3 / Q4 (final)?"), "Report Q3 Q4 (final)");
        assert_eq!(sanitize_filename("  spaced   out  "), "spaced out");
        assert_eq!(sanitize_filename(""), "Untitled");
        assert_eq!(sanitize_filename("..."), "Untitled");
    }

    #[test]
    fn unique_pdf_path_avoids_collisions_but_allows_self() {
        let dir = std::env::temp_dir().join(format!("codex_unique_test_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let existing = dir.join("Doc.pdf");
        std::fs::write(&existing, b"x").unwrap();

        // A different file wanting the same stem gets a suffixed name.
        let other = dir.join("other.pdf");
        let picked = unique_pdf_path(&dir, "Doc", &other);
        assert_eq!(picked, dir.join("Doc (2).pdf"));

        // The file itself renaming to its own current name is not a collision.
        let picked_self = unique_pdf_path(&dir, "Doc", &existing);
        assert_eq!(picked_self, existing);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn write_info_fields_updates_and_is_readable() {
        let dir = std::env::temp_dir().join(format!("codex_write_test_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let pdf = dir.join("Doc.pdf");
        build_test_pdf(&pdf, "Old Title", "Old Author", "Old Subject", "old,kw", "D:20200101000000Z");

        write_info_fields(&pdf, "New Title", "New Author", "New Subject", "new,kw").unwrap();

        let meta = extract_meta(&pdf);
        assert_eq!(meta.title.as_deref(), Some("New Title"));
        assert_eq!(meta.authors, vec!["New Author"]);
        assert_eq!(meta.subject.as_deref(), Some("New Subject"));
        assert_eq!(meta.keywords, vec!["new", "kw"]);
        assert_eq!(meta.page_count, 1, "page count must be unchanged");

        // No leftover temp files.
        let leftover = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .any(|e| e.file_name().to_string_lossy().starts_with(".codex-tmp-"));
        assert!(!leftover, "temp file should not survive a successful write");

        std::fs::remove_dir_all(&dir).ok();
    }

    /// Appends an incremental-update section (as annotators and many readers
    /// save edits), so the trailer gains a /Prev pointing at the first xref.
    fn append_incremental_update(path: &Path) {
        let mut bytes = std::fs::read(path).unwrap();
        let root = lopdf::Document::load(path).unwrap().trailer.get(b"Root").unwrap().as_reference().unwrap();
        let text = String::from_utf8_lossy(&bytes).to_string();
        let start = text.rfind("startxref").unwrap();
        let prev: usize = text[start + 9..].split_whitespace().next().unwrap().parse().unwrap();

        bytes.extend_from_slice(b"\n");
        let obj_offset = bytes.len();
        bytes.extend_from_slice(b"99 0 obj\n(added later)\nendobj\n");
        let xref_offset = bytes.len();
        bytes.extend_from_slice(
            format!(
                "xref\n0 1\n0000000000 65535 f \n99 1\n{obj_offset:010} 00000 n \n\
                 trailer\n<< /Size 100 /Root {} {} R /Prev {prev} >>\nstartxref\n{xref_offset}\n%%EOF\n",
                root.0, root.1
            )
            .as_bytes(),
        );
        std::fs::write(path, bytes).unwrap();
    }

    #[test]
    fn write_info_fields_handles_incrementally_updated_pdfs() {
        let dir = std::env::temp_dir().join(format!("codex_incr_test_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let pdf = dir.join("Doc.pdf");
        build_test_pdf(&pdf, "Old Title", "Old Author", "", "", "D:20200101000000Z");
        append_incremental_update(&pdf);
        assert!(lopdf::Document::load(&pdf).unwrap().trailer.has(b"Prev"));

        write_info_fields(&pdf, "New Title", "New Author", "", "").unwrap();

        let meta = extract_meta(&pdf);
        assert_eq!(meta.title.as_deref(), Some("New Title"));
        assert_eq!(meta.page_count, 1);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn write_info_fields_rejects_missing_file_without_side_effects() {
        let dir = std::env::temp_dir().join(format!("codex_write_missing_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let missing = dir.join("nope.pdf");

        let result = write_info_fields(&missing, "T", "A", "S", "K");
        assert!(result.is_err());
        assert!(!missing.exists());

        std::fs::remove_dir_all(&dir).ok();
    }
}

