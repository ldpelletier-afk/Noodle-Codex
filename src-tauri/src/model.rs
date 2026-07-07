use serde::{Deserialize, Serialize};

/// A single PDF in the user's library. Field names are camelCase on the wire
/// so the React side can use them directly.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Document {
    /// Stable id derived from the absolute path (hex of a hash).
    pub id: String,
    pub path: String,
    /// Absolute path of the scanned root this document belongs to.
    pub folder_path: String,
    pub file_name: String,
    /// PDF Title metadata, falling back to a cleaned-up file name.
    pub title: String,
    pub authors: Vec<String>,
    pub subject: Option<String>,
    pub keywords: Vec<String>,
    pub page_count: u32,
    pub size_bytes: u64,
    pub year: Option<i32>,
    /// Digital Object Identifier, from BibTeX enrichment.
    pub doi: Option<String>,
    /// BibTeX citation key of the matched entry, if any.
    pub citation_key: Option<String>,
    /// Journal / book title / publisher, from BibTeX.
    pub venue: Option<String>,
    /// Abstract, from BibTeX.
    pub abstract_text: Option<String>,
    /// Where the displayed metadata came from: "pdf" or "bibtex".
    pub metadata_source: String,
    /// Absolute path to the cached first-page thumbnail PNG, if generated.
    pub thumbnail_path: Option<String>,
    /// How many times thumbnail generation has failed for this document. Once
    /// it reaches the ceiling the file is treated as un-renderable (encrypted,
    /// corrupt, unsupported) and no longer offered for retry.
    pub thumb_attempts: u32,
    /// The collection this document is filed under (derived from the source
    /// folder's leaf name unless overridden).
    pub category: String,
    /// Subfolder path relative to the scanned root, POSIX-style ("" for files
    /// directly in the root, "Chapter 1", "Sources/Primary"). Preserves the
    /// folder's own organizational structure.
    pub relative_dir: String,
    pub tags: Vec<String>,
    /// File modification time, ISO 8601.
    pub modified_at: Option<String>,
    /// When Codex first indexed this file, ISO 8601.
    pub added_at: String,
    /// "unread" | "in_progress" | "completed"
    pub status: String,
    pub current_page: u32,
    /// When the document was first opened/marked as started (ISO 8601).
    pub started_at: Option<String>,
    /// When the document was marked finished (ISO 8601).
    pub finished_at: Option<String>,
    /// Most recent time the document was opened or its progress updated.
    pub last_read_at: Option<String>,
}

/// A single reading-activity record. `open` events are logged automatically
/// when a document is opened; `progress` events when the page is updated.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ReadingEvent {
    pub doc_id: String,
    /// "open" | "progress"
    pub kind: String,
    pub page: Option<u32>,
    /// ISO 8601 timestamp.
    pub at: String,
}

/// A scanned root folder — becomes a top-level collection in the UI.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Folder {
    pub path: String,
    pub name: String,
    pub document_count: u32,
    pub added_at: String,
}

/// Progress payload emitted during a scan so the UI can show a live bar.
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ScanProgress {
    pub folder_path: String,
    pub processed: u32,
    pub total: u32,
    /// Name of the file just handled, for a "now scanning…" line.
    pub current_file: String,
    /// Phase: "indexing" (metadata) or "thumbnails" or "done".
    pub phase: String,
    pub done: bool,
}
