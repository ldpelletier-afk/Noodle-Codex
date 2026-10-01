import { Link } from 'react-router-dom'
import { Check } from 'lucide-react'
import { CoverImage } from '@/components/CoverImage'
import type { Document } from '@/lib/types'

export function DocumentCard({ doc, style }: { doc: Document; style?: React.CSSProperties }) {
  const authorLine =
    doc.authors.length > 0
      ? doc.authors.slice(0, 2).join(', ') + (doc.authors.length > 2 ? ' et al.' : '')
      : doc.category

  const reading = doc.status === 'in_progress'
  const finished = doc.status === 'completed'
  const pct = doc.pageCount > 0 ? Math.round((doc.currentPage / doc.pageCount) * 100) : 0

  return (
    <Link
      to={`/documents/${doc.id}`}
      style={style}
      data-tour="document-card"
      className="group flex flex-col gap-2 animate-card-appear touch-feedback"
    >
      <div className="relative">
        <CoverImage doc={doc} />
        {finished && (
          <span
            className="absolute top-1.5 right-1.5 w-5 h-5 rounded-full bg-success text-white flex items-center justify-center shadow"
            title="Finished"
          >
            <Check className="w-3 h-3" strokeWidth={3} />
          </span>
        )}
        {reading && (
          <div className="absolute inset-x-1.5 bottom-1.5">
            <div className="h-1 rounded-full bg-black/25 overflow-hidden backdrop-blur-sm">
              <div className="h-full bg-primary rounded-full" style={{ width: `${Math.max(pct, 4)}%` }} />
            </div>
          </div>
        )}
      </div>
      <div>
        <h3 className="text-sm font-medium leading-snug line-clamp-2 group-hover:text-primary transition-colors">
          {doc.title}
        </h3>
        <p className="text-xs text-muted-foreground line-clamp-1 mt-0.5">{authorLine}</p>
        <p className="text-[0.7rem] text-muted-foreground mt-0.5">
          {reading && doc.pageCount > 0 ? (
            <span className="text-primary">Reading · {pct}%</span>
          ) : (
            <>
              {doc.pageCount > 0 ? `${doc.pageCount} pp` : '—'}
              {doc.year ? ` · ${doc.year}` : ''}
            </>
          )}
        </p>
      </div>
    </Link>
  )
}
