export type ReadingStatus = 'unread' | 'in_progress' | 'completed'

/** Failed-render ceiling after which a document is considered un-renderable.
 * Mirrors `db::MAX_THUMB_ATTEMPTS` on the Rust side — keep the two in sync. */
export const MAX_THUMB_ATTEMPTS = 3

/** A single PDF in the library. Matches the Rust `Document` (camelCase). */
export interface Document {
  id: string
  path: string
  folderPath: string
  fileName: string
  title: string
  authors: string[]
  subject: string | null
  keywords: string[]
  pageCount: number
  sizeBytes: number
  year: number | null
  doi: string | null
  citationKey: string | null
  venue: string | null
  abstractText: string | null
  /** "pdf" | "bibtex" — where the displayed metadata came from. */
  metadataSource: string
  thumbnailPath: string | null
  /** Failed thumbnail-render attempts; at MAX_THUMB_ATTEMPTS the file is
   * treated as un-renderable and no longer offered for retry. */
  thumbAttempts: number
  category: string
  /** Subfolder path relative to the collection root, POSIX-style ("" = root). */
  relativeDir: string
  tags: string[]
  modifiedAt: string | null
  addedAt: string
  status: ReadingStatus
  currentPage: number
  /** When the document was first opened / marked started (ISO 8601). */
  startedAt: string | null
  /** When the document was marked finished (ISO 8601). */
  finishedAt: string | null
  /** Most recent open or progress update (ISO 8601). */
  lastReadAt: string | null
}

/** A reading-activity record: an "open" or "progress" event. */
export interface ReadingEvent {
  docId: string
  kind: 'open' | 'progress'
  page: number | null
  at: string
}

/** A scanned root folder — a top-level collection. */
export interface Folder {
  path: string
  name: string
  documentCount: number
  addedAt: string
}

export interface ScanResult {
  folder: Folder
  /** Total PDFs found under the folder. */
  indexed: number
  /** Of those, how many were new or changed since the last scan. */
  changed: number
  /** Previously-tracked documents whose file is no longer there. */
  removed: number
}

/** Emitted when the live folder watcher detects and indexes new PDFs while
 * Codex is running (no manual rescan needed). */
export interface LibraryChanged {
  folderPath: string
  folderName: string
  added: number
  /** Previously-tracked documents whose file is no longer there (deleted,
   * trashed, or moved out of the folder). */
  removed: number
}

export interface ScanProgress {
  folderPath: string
  processed: number
  total: number
  currentFile: string
  phase: 'indexing' | 'thumbnails' | 'done'
  done: boolean
}

export interface ThumbnailReady {
  id: string
  thumbnailPath: string
}

/** Fields a user can hand-edit. Title/authors/subject/keywords are written
 * into the actual PDF; year/venue/doi are Codex-only catalog fields. */
export interface UpdateDocumentInput {
  id: string
  title: string
  authors: string[]
  year: number | null
  venue: string | null
  doi: string | null
  keywords: string[]
  subject: string | null
}

export interface UpdateDocumentResult {
  document: Document
  /** Set when the edit was saved to Codex (and the file renamed) but the
   * PDF's own embedded metadata couldn't be updated — e.g. an encrypted or
   * unusually-structured file. `null` means the PDF was updated too. */
  pdfWarning: string | null
}

export interface BibtexStatus {
  /** Path of the attached central .bib file, if any. */
  path: string | null
  /** Documents whose displayed metadata currently comes from BibTeX. */
  matched: number
  total: number
}
