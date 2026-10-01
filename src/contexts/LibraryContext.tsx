import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useRef,
  useState,
  type ReactNode,
} from 'react'
import {
  listDocuments,
  listFolders,
  listReadingEvents,
  scanFolder,
  removeFolder as apiRemoveFolder,
  retryThumbnails as apiRetryThumbnails,
  pickFolder,
  onScanProgress,
  onThumbnailReady,
  onLibraryChanged,
  onReadingProgress,
} from '@/lib/api'
import { useToast } from '@/contexts/ToastContext'
import type { Document, Folder, ReadingEvent, ScanProgress } from '@/lib/types'

interface LibraryValue {
  documents: Document[]
  folders: Folder[]
  readingEvents: ReadingEvent[]
  loading: boolean
  scanning: boolean
  progress: ScanProgress | null
  /** Opens the folder picker and scans the chosen folder. Returns the path and
   * indexed count, or null if cancelled. */
  addFolder: () => Promise<{ path: string; indexed: number; changed: number; removed: number } | null>
  removeFolder: (path: string) => Promise<void>
  /** Re-renders missing thumbnails for a folder. Returns how many are retried. */
  retryThumbnails: (folderPath: string, force?: boolean) => Promise<number>
  refresh: () => Promise<void>
  /** Replaces one document in place (e.g. after a reading-state change) and
   * reloads reading activity, without a full library refetch. */
  patchDocument: (doc: Document) => void
  reloadReadingEvents: () => Promise<void>
}

const LibraryContext = createContext<LibraryValue | null>(null)

export function LibraryProvider({ children }: { children: ReactNode }) {
  const [documents, setDocuments] = useState<Document[]>([])
  const [folders, setFolders] = useState<Folder[]>([])
  const [readingEvents, setReadingEvents] = useState<ReadingEvent[]>([])
  const [loading, setLoading] = useState(true)
  const [scanning, setScanning] = useState(false)
  const [progress, setProgress] = useState<ScanProgress | null>(null)
  const mounted = useRef(true)
  const { toast } = useToast()

  const refresh = useCallback(async () => {
    const [docs, fs, events] = await Promise.all([
      listDocuments(),
      listFolders(),
      listReadingEvents(),
    ])
    if (!mounted.current) return
    setDocuments(docs)
    setFolders(fs)
    setReadingEvents(events)
  }, [])

  const reloadReadingEvents = useCallback(async () => {
    const events = await listReadingEvents()
    if (mounted.current) setReadingEvents(events)
  }, [])

  const patchDocument = useCallback((doc: Document) => {
    setDocuments(prev => prev.map(d => (d.id === doc.id ? doc : d)))
  }, [])

  useEffect(() => {
    mounted.current = true
    refresh().finally(() => mounted.current && setLoading(false))

    // Patch thumbnails into place as the backend renders them.
    const unlistenThumb = onThumbnailReady(t => {
      setDocuments(prev =>
        prev.map(d => (d.id === t.id ? { ...d, thumbnailPath: t.thumbnailPath } : d))
      )
    })

    const unlistenProgress = onScanProgress(p => {
      setProgress(p)
      if (p.done) {
        setScanning(false)
        // Final reconcile so counts/metadata are authoritative.
        refresh()
      }
    })

    // The live folder watcher indexed new PDFs on its own — pull them in and
    // let the user know, since this happens with no action from them.
    const unlistenLibraryChanged = onLibraryChanged(c => {
      const parts: string[] = []
      if (c.added > 0) parts.push(`${c.added} new PDF${c.added === 1 ? '' : 's'} added`)
      if (c.removed > 0) parts.push(`${c.removed} removed`)
      if (parts.length > 0) toast.info(`${parts.join(', ')} in "${c.folderName}"`)
      refresh()
    })

    // Reading in Skim: Codex saves each page turn as progress.
    const unlistenReading = onReadingProgress(patchDocument)

    return () => {
      mounted.current = false
      unlistenReading.then(fn => fn())
      unlistenThumb.then(fn => fn())
      unlistenProgress.then(fn => fn())
      unlistenLibraryChanged.then(fn => fn())
    }
  }, [refresh, patchDocument])

  const addFolder = useCallback(async () => {
    const path = await pickFolder()
    if (!path) return null
    setScanning(true)
    setProgress({ folderPath: path, processed: 0, total: 0, currentFile: '', phase: 'indexing', done: false })
    try {
      const result = await scanFolder(path)
      // Documents are indexed by the time scanFolder resolves; thumbnails
      // continue streaming via events. Pull the indexed docs in now.
      await refresh()
      // A background thumbnail job always runs and eventually emits `done`
      // (which clears `scanning`) — except when the folder has no PDFs at
      // all, in which case nothing will ever fire that event.
      if (result.indexed === 0) setScanning(false)
      return { path, indexed: result.indexed, changed: result.changed, removed: result.removed }
    } catch (e) {
      setScanning(false)
      throw e
    }
  }, [refresh])

  const removeFolder = useCallback(
    async (path: string) => {
      await apiRemoveFolder(path)
      await refresh()
    },
    [refresh]
  )

  const retryThumbnails = useCallback(async (folderPath: string, force = false) => {
    setScanning(true)
    setProgress({ folderPath, processed: 0, total: 0, currentFile: '', phase: 'thumbnails', done: false })
    try {
      const count = await apiRetryThumbnails(folderPath, force)
      // The 'done' scan-progress event clears `scanning`; but if there was
      // nothing to retry, no job runs, so stop showing progress now.
      if (count === 0) setScanning(false)
      return count
    } catch (e) {
      setScanning(false)
      throw e
    }
  }, [])

  return (
    <LibraryContext.Provider
      value={{
        documents,
        folders,
        readingEvents,
        loading,
        scanning,
        progress,
        addFolder,
        removeFolder,
        retryThumbnails,
        refresh,
        patchDocument,
        reloadReadingEvents,
      }}
    >
      {children}
    </LibraryContext.Provider>
  )
}

export function useLibrary() {
  const ctx = useContext(LibraryContext)
  if (!ctx) throw new Error('useLibrary must be used within LibraryProvider')
  return ctx
}
