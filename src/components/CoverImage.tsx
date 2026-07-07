import { useState } from 'react'
import { FileText } from 'lucide-react'
import { MAX_THUMB_ATTEMPTS, type Document } from '@/lib/types'
import { thumbnailUrl } from '@/lib/api'
import { cn } from '@/lib/utils'

/** First-page thumbnail for a document. Falls back to a titled placeholder
 * while the backend is still rendering the page, or a document icon if the
 * PDF is un-renderable (encrypted / corrupt / unsupported). */
export function CoverImage({ doc, className }: { doc: Document; className?: string }) {
  const [errored, setErrored] = useState(false)
  const url = thumbnailUrl(doc.thumbnailPath)
  const showImage = url && !errored
  // Still-in-progress: no thumbnail yet but retries remain. Once retries are
  // exhausted we show the icon instead of pulsing forever.
  const stillRendering = !doc.thumbnailPath && doc.thumbAttempts < MAX_THUMB_ATTEMPTS

  return (
    <div
      className={cn(
        'relative aspect-[3/4] rounded-md overflow-hidden bg-muted ring-1 ring-black/5 shadow-sm',
        className
      )}
    >
      {showImage ? (
        <img
          src={url}
          alt={doc.title}
          loading="lazy"
          onError={() => setErrored(true)}
          className="absolute inset-0 w-full h-full object-cover object-top"
        />
      ) : (
        <div className="absolute inset-0 flex flex-col items-center justify-center gap-2 p-3 text-center bg-gradient-to-br from-muted to-card">
          {stillRendering ? (
            // Still rendering — a subtle pulse rather than an icon.
            <div className="w-full h-full animate-pulse" />
          ) : (
            <>
              <FileText className="w-6 h-6 text-muted-foreground" strokeWidth={1.5} />
              <span className="font-display text-[0.7rem] leading-tight line-clamp-4 text-muted-foreground">
                {doc.title}
              </span>
            </>
          )}
        </div>
      )}
    </div>
  )
}
