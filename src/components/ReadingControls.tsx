import { useEffect, useState } from 'react'
import { BookOpen, Check, Circle, Minus, Plus } from 'lucide-react'
import { setReadingProgress, setReadingStatus } from '@/lib/api'
import { useLibrary } from '@/contexts/LibraryContext'
import { useToast } from '@/contexts/ToastContext'
import { formatDate, cn } from '@/lib/utils'
import type { Document, ReadingStatus } from '@/lib/types'

const STATUS_OPTIONS: { id: ReadingStatus; label: string; icon: typeof Circle }[] = [
  { id: 'unread', label: 'Unread', icon: Circle },
  { id: 'in_progress', label: 'Reading', icon: BookOpen },
  { id: 'completed', label: 'Finished', icon: Check },
]

export function ReadingControls({
  doc,
  onUpdated,
}: {
  doc: Document
  onUpdated: (doc: Document) => void
}) {
  const { patchDocument, reloadReadingEvents } = useLibrary()
  const { toast } = useToast()
  const [pageInput, setPageInput] = useState(String(doc.currentPage))
  const [busy, setBusy] = useState(false)

  // Keep the editable field in sync when the document changes underneath us.
  useEffect(() => {
    setPageInput(String(doc.currentPage))
  }, [doc.id, doc.currentPage])

  const hasPages = doc.pageCount > 0
  const pct = hasPages ? Math.round((doc.currentPage / doc.pageCount) * 100) : 0

  async function apply(promise: Promise<Document>) {
    setBusy(true)
    try {
      const updated = await promise
      onUpdated(updated)
      patchDocument(updated)
      await reloadReadingEvents()
    } catch (e) {
      toast.error(`Couldn't update reading state: ${e}`)
    } finally {
      setBusy(false)
    }
  }

  function commitPage(next: number) {
    const clamped = hasPages ? Math.max(0, Math.min(next, doc.pageCount)) : Math.max(0, next)
    setPageInput(String(clamped))
    if (clamped !== doc.currentPage) apply(setReadingProgress(doc.id, clamped))
  }

  return (
    <div className="rounded-xl border border-border bg-card p-4">
      <div className="flex items-center justify-between mb-3">
        <h2 className="text-sm font-medium">Reading</h2>
        {doc.lastReadAt && (
          <span className="text-xs text-muted-foreground">Last read {formatDate(doc.lastReadAt)}</span>
        )}
      </div>

      {/* Status segmented control */}
      <div className="flex items-center gap-1 p-1 rounded-lg bg-muted mb-4">
        {STATUS_OPTIONS.map(opt => {
          const Icon = opt.icon
          const active = doc.status === opt.id
          return (
            <button
              key={opt.id}
              disabled={busy}
              onClick={() => apply(setReadingStatus(doc.id, opt.id))}
              className={cn(
                'flex-1 flex items-center justify-center gap-1.5 px-2 py-1.5 rounded-md text-xs font-medium transition-colors disabled:opacity-60',
                active ? 'bg-card text-foreground shadow-sm' : 'text-muted-foreground hover:text-foreground'
              )}
            >
              <Icon className="w-3.5 h-3.5" />
              {opt.label}
            </button>
          )
        })}
      </div>

      {/* Page progress */}
      {hasPages ? (
        <>
          <div className="flex items-center justify-between text-xs text-muted-foreground mb-1.5">
            <span>Progress</span>
            <span>{pct}%</span>
          </div>
          <div className="h-1.5 rounded-full bg-muted overflow-hidden mb-3">
            <div className="h-full bg-primary rounded-full transition-all" style={{ width: `${pct}%` }} />
          </div>
          <div className="flex items-center gap-2">
            <button
              disabled={busy || doc.currentPage <= 0}
              onClick={() => commitPage(doc.currentPage - 1)}
              className="w-8 h-8 flex items-center justify-center rounded-lg border border-border hover:bg-muted disabled:opacity-40 transition-colors"
              title="Previous page"
            >
              <Minus className="w-3.5 h-3.5" />
            </button>
            <div className="flex items-center gap-1.5 text-sm">
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
                className="w-16 px-2 py-1 rounded-md border border-border bg-background text-center focus:outline-none focus:ring-1 focus:ring-ring"
              />
              <span className="text-muted-foreground">of {doc.pageCount}</span>
            </div>
            <button
              disabled={busy || doc.currentPage >= doc.pageCount}
              onClick={() => commitPage(doc.currentPage + 1)}
              className="w-8 h-8 flex items-center justify-center rounded-lg border border-border hover:bg-muted disabled:opacity-40 transition-colors"
              title="Next page"
            >
              <Plus className="w-3.5 h-3.5" />
            </button>
          </div>
        </>
      ) : (
        <p className="text-xs text-muted-foreground">
          Page count unknown for this PDF, so progress is tracked by status only.
        </p>
      )}
    </div>
  )
}
