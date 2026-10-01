import { invoke } from '@tauri-apps/api/core'
import { convertFileSrc } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { open, save } from '@tauri-apps/plugin-dialog'
import type {
  BibtexStatus,
  Document,
  Folder,
  LibraryChanged,
  ReadingEvent,
  ReadingStatus,
  ScanProgress,
  ScanResult,
  ThumbnailReady,
  UpdateDocumentInput,
  UpdateDocumentResult,
} from './types'

export async function listDocuments(): Promise<Document[]> {
  return invoke<Document[]>('list_documents')
}

export async function getDocument(id: string): Promise<Document | null> {
  return invoke<Document | null>('get_document', { id })
}

/** Writes the edit into the PDF file itself (verified before replacing it),
 * renames the file to match the new title, and updates the catalog. If the
 * PDF's own metadata can't be safely rewritten (e.g. encrypted or unusually
 * structured), the rename + catalog update still go through and
 * `pdfWarning` explains what wasn't touched. */
export async function updateDocument(input: UpdateDocumentInput): Promise<UpdateDocumentResult> {
  return invoke<UpdateDocumentResult>('update_document', { input })
}

export async function listFolders(): Promise<Folder[]> {
  return invoke<Folder[]>('list_folders')
}

export async function removeFolder(path: string): Promise<void> {
  await invoke('remove_folder', { path })
}

export async function scanFolder(path: string): Promise<ScanResult> {
  return invoke<ScanResult>('scan_folder', { path })
}

/** Re-renders any missing thumbnails for a folder. Returns how many documents
 * are being retried. With `force`, previously-retired (un-renderable) files are
 * given another chance. Streams the same scan-progress / thumbnail-ready events. */
export async function retryThumbnails(folderPath: string, force = false): Promise<number> {
  return invoke<number>('retry_thumbnails', { folderPath, force })
}

/** Which app "Open PDF" uses: Skim when it's installed (falling back to the
 * system default otherwise), or always the user's default PDF app. */
export type PdfViewer = 'skim' | 'system'

const PDF_VIEWER_KEY = 'codex_pdf_viewer'

export function getPdfViewer(): PdfViewer {
  try {
    return localStorage.getItem(PDF_VIEWER_KEY) === 'system' ? 'system' : 'skim'
  } catch {
    return 'skim'
  }
}

export function setPdfViewer(viewer: PdfViewer): void {
  localStorage.setItem(PDF_VIEWER_KEY, viewer)
}

export async function isSkimInstalled(): Promise<boolean> {
  return invoke<boolean>('is_skim_installed')
}

/** Opens the PDF in the preferred viewer (see `getPdfViewer`), logs the open
 * as reading activity, and returns the document with its updated reading state. */
export async function openDocument(id: string): Promise<Document> {
  return invoke<Document>('open_document', { id, viewer: getPdfViewer() })
}

export async function revealInFinder(path: string): Promise<void> {
  await invoke('reveal_in_finder', { path })
}

export async function setReadingProgress(id: string, page: number): Promise<Document> {
  return invoke<Document>('set_reading_progress', { id, page })
}

export async function setReadingStatus(id: string, status: ReadingStatus): Promise<Document> {
  return invoke<Document>('set_reading_status', { id, status })
}

export async function listReadingEvents(): Promise<ReadingEvent[]> {
  return invoke<ReadingEvent[]>('list_reading_events')
}

/** Opens the native folder picker; returns the chosen absolute path or null. */
export async function pickFolder(): Promise<string | null> {
  const selected = await open({ directory: true, multiple: false, title: 'Choose a folder of PDFs' })
  if (typeof selected === 'string') return selected
  return null
}

/** Opens a native picker for a .bib file; returns the chosen path or null. */
export async function pickBibtexFile(): Promise<string | null> {
  const selected = await open({
    directory: false,
    multiple: false,
    title: 'Choose a BibTeX file',
    filters: [{ name: 'BibTeX', extensions: ['bib'] }],
  })
  if (typeof selected === 'string') return selected
  return null
}

export async function attachBibtex(path: string): Promise<BibtexStatus> {
  return invoke<BibtexStatus>('attach_bibtex', { path })
}

export async function detachBibtex(): Promise<BibtexStatus> {
  return invoke<BibtexStatus>('detach_bibtex')
}

export async function getBibtexStatus(): Promise<BibtexStatus> {
  return invoke<BibtexStatus>('get_bibtex_status')
}

export async function rematchBibtex(): Promise<BibtexStatus> {
  return invoke<BibtexStatus>('rematch_bibtex')
}

export async function exportBibtex(id: string): Promise<string> {
  return invoke<string>('export_bibtex', { id })
}

export async function exportBibtexBatch(ids: string[]): Promise<string> {
  return invoke<string>('export_bibtex_batch', { ids })
}

export async function writeTextFile(path: string, contents: string): Promise<void> {
  await invoke('write_text_file', { path, contents })
}

/** Opens a native "Save As" dialog for a .bib file; returns the chosen path,
 * or null if cancelled. */
export async function pickBibtexSavePath(defaultName: string): Promise<string | null> {
  const path = await save({
    title: 'Export BibTeX',
    defaultPath: defaultName.endsWith('.bib') ? defaultName : `${defaultName}.bib`,
    filters: [{ name: 'BibTeX', extensions: ['bib'] }],
  })
  return path ?? null
}

/** Converts an absolute file path into a webview-loadable asset URL. */
export function thumbnailUrl(path: string | null): string | null {
  if (!path) return null
  return convertFileSrc(path)
}

export function onScanProgress(handler: (p: ScanProgress) => void): Promise<UnlistenFn> {
  return listen<ScanProgress>('scan-progress', e => handler(e.payload))
}

export function onThumbnailReady(handler: (t: ThumbnailReady) => void): Promise<UnlistenFn> {
  return listen<ThumbnailReady>('thumbnail-ready', e => handler(e.payload))
}

/** Fired when Codex saves the page you're on while you read in Skim. */
export function onReadingProgress(handler: (doc: Document) => void): Promise<UnlistenFn> {
  return listen<Document>('reading-progress', e => handler(e.payload))
}

/** Fired when the live folder watcher indexes new PDFs dropped into a tracked
 * folder while Codex is running. */
export function onLibraryChanged(handler: (c: LibraryChanged) => void): Promise<UnlistenFn> {
  return listen<LibraryChanged>('library-changed', e => handler(e.payload))
}
