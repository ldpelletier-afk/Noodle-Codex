import { useEffect, useMemo, useState } from 'react'
import { Link } from 'react-router-dom'
import { BookOpen, Check, ExternalLink, Minus, Plus } from 'lucide-react'
import { AppShell } from '@/components/AppShell'
import { CoverImage } from '@/components/CoverImage'
import { useLibrary } from '@/contexts/LibraryContext'
import { useToast } from '@/contexts/ToastContext'
import { openDocument, setReadingProgress, setReadingStatus } from '@/lib/api'
import { formatDate } from '@/lib/utils'
import type { Document } from '@/lib/types'

/** Most recent reading touch, falling back to when it was started so a
 * document opened once but never progressed still sorts sensibly. */
function lastTouched(d: Document): string {
  return d.lastReadAt ?? d.startedAt ?? d.addedAt
}

export function CurrentlyReadingPage() {
  const { documents, loading } = useLibrary()

  const reading = useMemo(
    () =>
      documents
        .filter(d => d.status === 'in_progress')
        .sort((a, b) => (lastTouched(a) < lastTouched(b) ? 1 : -1)),
    [documents]
  )

  return (
    <AppShell>
      <div className="max-w-4xl mx-auto px-5 md:px-8 py-6">
        <div className="mb-5">
          <h1 className="font-display text-2xl">Currently reading</h1>
          <p className="text-sm text-muted-foreground mt-0.5">
            {loading
              ? 'Loading…'
              : reading.length === 0
                ? 'Nothing in progress right now.'
                : `${reading.length} document${reading.length === 1 ? '' : 's'} in progress, most recently read first.`}
          </p>
        </div>

        {loading ? (
          <div className="flex flex-col gap-3">
            {Array.from({ length: 4 }).map((_, i) => (
              <div key={i} className="h-28 rounded-xl bg-muted animate-pulse" />
            ))}
          </div>
        ) : reading.length === 0 ? (
          <EmptyState />
        ) : (
          <div className="flex flex-col gap-3">
            {reading.map((d, i) => (
              <ReadingRow key={d.id} doc={d} style={{ animationDelay: `${Math.min(i, 20) * 25}ms` }} />
            ))}
          </div>
        )}
      </div>
    </AppShell>
  )
}

function ReadingRow({ doc, style }: { doc: Document; style?: React.CSSProperties }) {
  const { patchDocument, reloadReadingEvents } = useLibrary()
  const { toast } = useToast()
  const [pageInput, setPageInput] = useState(String(doc.currentPage))
  const [busy, setBusy] = useState(false)

  // Keep the editable field in sync when the document changes underneath us
  // (opening the PDF advances it, and so does an edit on the detail page).
  useEffect(() => {
    setPageInput(String(doc.currentPage))
  }, [doc.id, doc.currentPage])

  const hasPages = doc.pageCount > 0
  const pct = hasPages ? Math.round((doc.currentPage / doc.pageCount) * 100) : 0
  const authorLine =
    doc.authors.length > 0
      ? doc.authors.slice(0, 2).join(', ') + (doc.authors.length > 2 ? ' et al.' : '')
      : doc.category

  async function apply(promise: Promise<Document>, failure: string) {
    setBusy(true)
    try {
      const updated = await promise
      patchDocument(updated)
      await reloadReadingEvents()
    } catch (e) {
      toast.error(`${failure}: ${e}`)
    } finally {
      setBusy(false)
    }
  }

  function commitPage(next: number) {
    const clamped = hasPages ? Math.max(0, Math.min(next, doc.pageCount)) : Math.max(0, next)
    setPageInput(String(clamped))
    if (clamped !== doc.currentPage) {
      apply(setReadingProgress(doc.id, clamped), "Couldn't update progress")
    }
  }

  return (
    <div
      style={style}
      className="flex gap-4 p-3.5 rounded-xl border border-border bg-card animate-card-appear"
    >
      <Link to={`/documents/${doc.id}`} className="w-16 shrink-0 group">
        <CoverImage doc={doc} className="transition-shadow group-hover:shadow-md" />
      </Link>

      <div className="flex-1 min-w-0 flex flex-col">
        <Link to={`/documents/${doc.id}`} className="group">
          <h2 className="text-sm font-medium leading-snug line-clamp-2 group-hover:text-primary transition-colors">
            {doc.title}
          </h2>
        </Link>
        <p className="text-xs text-muted-foreground line-clamp-1 mt-0.5">{authorLine}</p>

        {/* Progress */}
        <div className="mt-2.5">
          {hasPages ? (
            <>
              <div className="flex items-center justify-between text-xs text-muted-foreground mb-1">
                <span className="tabular-nums">
                  Page {doc.currentPage} of {doc.pageCount}
                </span>
                <span className="tabular-nums">{pct}%</span>
              </div>
              <div className="h-1.5 rounded-full bg-muted overflow-hidden">
                <div className="h-full bg-primary rounded-full transition-all" style={{ width: `${pct}%` }} />
              </div>
            </>
          ) : (
            <p className="text-xs text-muted-foreground">
              Page count unknown for this PDF — tracked by status only.
            </p>
          )}
        </div>

        {/* Actions */}
        <div className="flex flex-wrap items-center gap-2 mt-3">
          <button
            disabled={busy}
            onClick={() => apply(openDocument(doc.id), "Couldn't open")}
            title={
              hasPages && doc.currentPage > 0
                ? `Open at page ${doc.currentPage}`
                : 'Open this PDF'
            }
            className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg bg-primary text-primary-foreground text-xs font-medium hover:opacity-90 transition-opacity disabled:opacity-50"
          >
            <ExternalLink className="w-3.5 h-3.5" />
            Continue
          </button>

          {hasPages && (
            <div className="flex items-center gap-1">
              <button
                disabled={busy || doc.currentPage <= 0}
                onClick={() => commitPage(doc.currentPage - 1)}
                title="Previous page"
                className="w-7 h-7 flex items-center justify-center rounded-lg border border-border hover:bg-muted disabled:opacity-40 transition-colors"
              >
                <Minus className="w-3 h-3" />
              </button>
              <input
                type="number"
                min={0}
                max={doc.pageCount}
                value={pageInput}
                disabled={busy}
                onChange={e => setPageInput(e.target.value)}
                onBlur={() => commitPage(Number(pageInput) || 0)}
                onKeyDown={e => {
                  if (e.key === 'Enter') (e.target as HTMLInputElement).blur()
                }}
                title="Set current page"
                className="w-14 px-1.5 py-1 rounded-md border border-border bg-background text-center text-xs focus:outline-none focus:ring-1 focus:ring-ring"
              />
              <button
                disabled={busy || doc.currentPage >= doc.pageCount}
                onClick={() => commitPage(doc.currentPage + 1)}
                title="Next page"
                className="w-7 h-7 flex items-center justify-center rounded-lg border border-border hover:bg-muted disabled:opacity-40 transition-colors"
              >
                <Plus className="w-3 h-3" />
              </button>
            </div>
          )}

          <button
            disabled={busy}
            onClick={() => apply(setReadingStatus(doc.id, 'completed'), "Couldn't mark finished")}
            title="Mark as finished"
            className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg border border-border text-xs font-medium hover:bg-muted transition-colors disabled:opacity-50"
          >
            <Check className="w-3.5 h-3.5" />
            Finished
          </button>

          {doc.lastReadAt && (
            <span className="text-xs text-muted-foreground ml-auto">
              Last read {formatDate(doc.lastReadAt)}
            </span>
          )}
        </div>
      </div>
    </div>
  )
}

function EmptyState() {
  return (
    <div className="flex flex-col items-center justify-center text-center py-24">
      <div className="w-14 h-14 rounded-2xl bg-primary/10 text-primary flex items-center justify-center mb-4">
        <BookOpen className="w-7 h-7" />
      </div>
      <h2 className="font-display text-lg mb-1.5">Nothing in progress</h2>
      <p className="text-sm text-muted-foreground max-w-sm mb-5">
        Open a PDF from your library, or set one to "Reading" on its page, and it will show up
        here with your place in it.
      </p>
      <Link
        to="/"
        className="px-4 py-2 rounded-xl bg-primary text-primary-foreground text-sm font-medium hover:opacity-90 transition-opacity"
      >
        Browse library
      </Link>
    </div>
  )
}
