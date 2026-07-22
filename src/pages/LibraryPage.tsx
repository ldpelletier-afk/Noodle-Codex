import { useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react'
import { FolderPlus, FolderOpen, Folder, Library as LibraryIcon, ChevronRight, Quote, RefreshCw } from 'lucide-react'
import { AppShell } from '@/components/AppShell'
import { DocumentCard } from '@/components/DocumentCard'
import { ScanProgressBar } from '@/components/ScanProgressBar'
import { useLibrary } from '@/contexts/LibraryContext'
import { useLibrarySearch } from '@/contexts/LibrarySearchContext'
import { useToast } from '@/contexts/ToastContext'
import { breadcrumbs, documentsInDir, documentsUnderDir, immediateSubfolders } from '@/lib/folders'
import { exportBibtexBatch, pickBibtexSavePath, writeTextFile } from '@/lib/api'
import { MAX_THUMB_ATTEMPTS } from '@/lib/types'
import type { Document } from '@/lib/types'
import { cn } from '@/lib/utils'

// Survives this page unmounting (opening a document) so we can drop the
// reader back where they were on return. Module-scoped, so it persists
// across route changes within the session but resets on a full reload.
let savedLibraryScroll = 0

export function LibraryPage() {
  const { documents, folders, loading, scanning, progress, addFolder, retryThumbnails } = useLibrary()
  const { query } = useLibrarySearch()
  const { toast } = useToast()
  const [activeCollection, setActiveCollection] = useState<string>('all')
  const [currentDir, setCurrentDir] = useState('')
  const [exporting, setExporting] = useState(false)

  // Leaving a collection resets the folder cursor.
  useEffect(() => {
    setCurrentDir('')
  }, [activeCollection])

  // Remember where we were scrolled to when leaving (opening a document,
  // etc.) so coming back to the library drops the reader back in place
  // instead of resetting to the top.
  useEffect(() => {
    const el = document.getElementById('app-main')
    if (!el) return
    const onScroll = () => {
      savedLibraryScroll = el.scrollTop
    }
    el.addEventListener('scroll', onScroll, { passive: true })
    return () => el.removeEventListener('scroll', onScroll)
  }, [])

  // Restore that position once the grid is actually populated — restoring
  // any earlier would land in the wrong place while it's still short.
  const restoredScrollRef = useRef(false)
  useLayoutEffect(() => {
    if (!loading && !restoredScrollRef.current) {
      restoredScrollRef.current = true
      if (savedLibraryScroll > 0) {
        document.getElementById('app-main')?.scrollTo(0, savedLibraryScroll)
      }
    }
  }, [loading])

  async function handleAddFolder() {
    try {
      const result = await addFolder()
      if (!result) return
      const removedNote = result.removed > 0 ? ` (${result.removed} no longer on disk were removed)` : ''
      if (result.indexed === 0 && result.removed === 0) {
        toast.warning('No PDFs found. If this folder is under Desktop/Documents, grant Codex file access in System Settings → Privacy.')
      } else if (result.changed === 0 && result.removed === 0) {
        toast.info(`Already up to date — all ${result.indexed} PDFs were indexed already.`)
      } else if (result.changed === 0) {
        toast.info(`Nothing new${removedNote}.`)
      } else if (result.changed === result.indexed) {
        toast.success(`Indexing ${result.indexed} PDF${result.indexed === 1 ? '' : 's'}…${removedNote}`)
      } else {
        toast.success(
          `Found ${result.changed} new or updated PDF${result.changed === 1 ? '' : 's'} (${result.indexed} total)…${removedNote}`
        )
      }
    } catch (e) {
      toast.error(`Couldn't scan folder: ${e}`)
    }
  }

  const q = query.trim().toLowerCase()
  const searching = q !== ''
  // Folder navigation only applies inside one collection with no active search;
  // "All" and search results stay flat.
  const browsingFolders = activeCollection !== 'all' && !searching

  const collectionDocs = useMemo(
    () => (activeCollection === 'all' ? documents : documents.filter(d => d.folderPath === activeCollection)),
    [documents, activeCollection]
  )

  const flatResults = useMemo(() => {
    return collectionDocs.filter(d => {
      if (!q) return true
      const haystack = [
        d.title,
        d.category,
        d.fileName,
        d.relativeDir,
        d.venue ?? '',
        d.doi ?? '',
        d.citationKey ?? '',
        ...d.authors,
        ...d.keywords,
      ]
        .join(' ')
        .toLowerCase()
      return haystack.includes(q)
    })
  }, [collectionDocs, q])

  const subfolders = useMemo(
    () => (browsingFolders ? immediateSubfolders(collectionDocs, currentDir) : []),
    [browsingFolders, collectionDocs, currentDir]
  )
  const docsHere = useMemo(
    () => (browsingFolders ? documentsInDir(collectionDocs, currentDir) : []),
    [browsingFolders, collectionDocs, currentDir]
  )
  const activeFolderName = folders.find(f => f.path === activeCollection)?.name ?? 'Collection'
  const crumbs = browsingFolders ? breadcrumbs(activeFolderName, currentDir) : []
  const hasNesting = browsingFolders && collectionDocs.some(d => d.relativeDir !== '')

  // Whatever's currently in view: the subfolder you've drilled into (and
  // everything beneath it), or the flat listing (a collection, "All", or
  // search results) shown in the other branch.
  const exportScopeDocs = browsingFolders ? documentsUnderDir(collectionDocs, currentDir) : flatResults
  const exportScopeLabel = browsingFolders
    ? (crumbs[crumbs.length - 1]?.name ?? activeFolderName)
    : searching
      ? 'Search results'
      : activeCollection === 'all'
        ? 'Library'
        : activeFolderName

  // Documents in view whose first-page thumbnail never rendered. Split into
  // still-retryable (Quick Look dropped them transiently) vs. retired ones
  // (un-renderable: encrypted / corrupt / unsupported PDFs). Only the retryable
  // set drives the button, so it can't loop forever on files that never render.
  const retryableThumbDocs = exportScopeDocs.filter(
    d => !d.thumbnailPath && d.thumbAttempts < MAX_THUMB_ATTEMPTS
  )

  async function handleRetryThumbnails() {
    const folderPaths = [...new Set(retryableThumbDocs.map(d => d.folderPath))]
    try {
      let total = 0
      for (const fp of folderPaths) total += await retryThumbnails(fp)
      if (total > 0) toast.success(`Re-rendering ${total} thumbnail${total === 1 ? '' : 's'}…`)
    } catch (e) {
      toast.error(`Couldn't retry thumbnails: ${e}`)
    }
  }

  async function handleExportBib() {
    if (exportScopeDocs.length === 0) {
      toast.warning('No documents in view to export.')
      return
    }
    const savePath = await pickBibtexSavePath(exportScopeLabel)
    if (!savePath) return
    setExporting(true)
    try {
      const bib = await exportBibtexBatch(exportScopeDocs.map(d => d.id))
      await writeTextFile(savePath, bib)
      const count = exportScopeDocs.length
      toast.success(`Exported ${count} entr${count === 1 ? 'y' : 'ies'} to ${savePath.split('/').pop()}`)
    } catch (e) {
      toast.error(`Couldn't export BibTeX: ${e}`)
    } finally {
      setExporting(false)
    }
  }

  const addFolderButton = (
    <button
      onClick={handleAddFolder}
      disabled={scanning}
      className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg bg-primary text-primary-foreground text-sm font-medium hover:opacity-90 transition-opacity disabled:opacity-50"
    >
      <FolderPlus className="w-4 h-4" />
      Add folder
    </button>
  )

  const headerActions = (
    <>
      {retryableThumbDocs.length > 0 && !scanning && (
        <button
          onClick={handleRetryThumbnails}
          title={`Re-render ${retryableThumbDocs.length} missing thumbnail${retryableThumbDocs.length === 1 ? '' : 's'}`}
          className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg border border-border text-sm font-medium hover:bg-muted transition-colors"
        >
          <RefreshCw className="w-4 h-4" />
          Retry {retryableThumbDocs.length} thumbnail{retryableThumbDocs.length === 1 ? '' : 's'}
        </button>
      )}
      {folders.length > 0 && (
        <button
          onClick={handleExportBib}
          disabled={exporting}
          title={`Export "${exportScopeLabel}" as BibTeX`}
          className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg border border-border text-sm font-medium hover:bg-muted transition-colors disabled:opacity-50"
        >
          <Quote className="w-4 h-4" />
          Export .bib
        </button>
      )}
      {addFolderButton}
    </>
  )

  return (
    <AppShell actions={headerActions}>
      <div className="max-w-6xl mx-auto px-5 md:px-8 py-6">
        {scanning && progress && <ScanProgressBar progress={progress} />}

        {loading ? (
          <div className="grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5 gap-5">
            {Array.from({ length: 10 }).map((_, i) => (
              <div key={i} className="aspect-[3/4] rounded-md bg-muted animate-pulse" />
            ))}
          </div>
        ) : folders.length === 0 ? (
          <EmptyState onAdd={handleAddFolder} scanning={scanning} />
        ) : (
          <>
            <div className="flex items-center justify-between mb-4 gap-4">
              <div>
                <h1 className="font-display text-2xl">Library</h1>
                <p className="text-sm text-muted-foreground mt-0.5">
                  {documents.length} document{documents.length === 1 ? '' : 's'} in {folders.length}{' '}
                  collection{folders.length === 1 ? '' : 's'}
                </p>
              </div>
            </div>

            {/* Collection filter */}
            <div className="flex items-center gap-1.5 mb-4 overflow-x-auto pb-1">
              <CollectionChip
                label="All"
                count={documents.length}
                active={activeCollection === 'all'}
                onClick={() => setActiveCollection('all')}
              />
              {folders.map(f => (
                <CollectionChip
                  key={f.path}
                  label={f.name}
                  count={f.documentCount}
                  active={activeCollection === f.path}
                  onClick={() => setActiveCollection(f.path)}
                />
              ))}
            </div>

            {/* Breadcrumb — only while browsing a collection's folder tree */}
            {hasNesting && (
              <nav className="flex items-center flex-wrap gap-0.5 mb-5 text-sm">
                {crumbs.map((c, i) => (
                  <span key={c.path} className="flex items-center gap-0.5">
                    {i > 0 && <ChevronRight className="w-3.5 h-3.5 text-muted-foreground" />}
                    <button
                      onClick={() => setCurrentDir(c.path)}
                      className={cn(
                        'px-1.5 py-0.5 rounded hover:bg-muted transition-colors',
                        i === crumbs.length - 1 ? 'font-medium text-foreground' : 'text-muted-foreground'
                      )}
                    >
                      {c.name}
                    </button>
                  </span>
                ))}
              </nav>
            )}

            {searching ? (
              <FlatGrid docs={flatResults} emptyLabel="No documents match your search." />
            ) : browsingFolders ? (
              <FolderView
                subfolders={subfolders}
                docs={docsHere}
                onOpenFolder={setCurrentDir}
              />
            ) : (
              <FlatGrid docs={flatResults} emptyLabel="No documents here yet." />
            )}
          </>
        )}
      </div>
    </AppShell>
  )
}

function FolderView({
  subfolders,
  docs,
  onOpenFolder,
}: {
  subfolders: { name: string; path: string; count: number }[]
  docs: Document[]
  onOpenFolder: (path: string) => void
}) {
  if (subfolders.length === 0 && docs.length === 0) {
    return (
      <div className="flex flex-col items-center justify-center text-center py-24 text-muted-foreground">
        <p className="text-sm">This folder is empty.</p>
      </div>
    )
  }

  return (
    <div className="flex flex-col gap-6">
      {subfolders.length > 0 && (
        <div>
          <h2 className="text-xs font-medium text-muted-foreground uppercase tracking-wide mb-2">Folders</h2>
          <div className="grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 gap-3">
            {subfolders.map(f => (
              <button
                key={f.path}
                onClick={() => onOpenFolder(f.path)}
                className="flex items-center gap-2.5 p-3 rounded-xl border border-border bg-card hover:bg-muted transition-colors text-left animate-card-appear touch-feedback"
              >
                <Folder className="w-5 h-5 text-primary shrink-0" />
                <div className="min-w-0">
                  <p className="text-sm font-medium truncate">{f.name}</p>
                  <p className="text-xs text-muted-foreground">
                    {f.count} item{f.count === 1 ? '' : 's'}
                  </p>
                </div>
              </button>
            ))}
          </div>
        </div>
      )}

      {docs.length > 0 && (
        <div>
          {subfolders.length > 0 && (
            <h2 className="text-xs font-medium text-muted-foreground uppercase tracking-wide mb-2">Documents</h2>
          )}
          <div className="grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5 gap-5">
            {docs.map((d, i) => (
              <DocumentCard key={d.id} doc={d} style={{ animationDelay: `${Math.min(i, 20) * 20}ms` }} />
            ))}
          </div>
        </div>
      )}
    </div>
  )
}

function FlatGrid({ docs, emptyLabel }: { docs: Document[]; emptyLabel: string }) {
  if (docs.length === 0) {
    return (
      <div className="flex flex-col items-center justify-center text-center py-24 text-muted-foreground">
        <p className="text-sm">{emptyLabel}</p>
      </div>
    )
  }
  return (
    <div className="grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5 gap-5">
      {docs.map((d, i) => (
        <DocumentCard key={d.id} doc={d} style={{ animationDelay: `${Math.min(i, 20) * 20}ms` }} />
      ))}
    </div>
  )
}

function CollectionChip({
  label,
  count,
  active,
  onClick,
}: {
  label: string
  count: number
  active: boolean
  onClick: () => void
}) {
  return (
    <button
      onClick={onClick}
      className={cn(
        'flex items-center gap-1.5 px-3 py-1.5 rounded-full text-xs font-medium whitespace-nowrap transition-colors shrink-0',
        active ? 'bg-primary text-primary-foreground' : 'bg-muted text-muted-foreground hover:text-foreground'
      )}
    >
      <FolderOpen className="w-3 h-3" />
      {label}
      <span className={cn('tabular-nums', active ? 'opacity-80' : 'opacity-60')}>{count}</span>
    </button>
  )
}

function EmptyState({ onAdd, scanning }: { onAdd: () => void; scanning: boolean }) {
  return (
    <div className="flex flex-col items-center justify-center text-center py-28">
      <div className="w-16 h-16 rounded-2xl bg-primary/10 text-primary flex items-center justify-center mb-5">
        <LibraryIcon className="w-8 h-8" />
      </div>
      <h1 className="font-display text-2xl mb-1.5">Your library is empty</h1>
      <p className="text-sm text-muted-foreground max-w-sm mb-6">
        Point Codex at a folder of PDFs on your desktop. It reads each file's metadata and renders
        the first page as a cover, and mirrors the folder's own subfolder structure.
      </p>
      <button
        onClick={onAdd}
        disabled={scanning}
        className="flex items-center gap-2 px-4 py-2.5 rounded-xl bg-primary text-primary-foreground font-medium hover:opacity-90 transition-opacity disabled:opacity-50"
      >
        <FolderPlus className="w-4 h-4" />
        Add a folder
      </button>
    </div>
  )
}
