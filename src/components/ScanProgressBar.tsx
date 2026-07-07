import { Loader2 } from 'lucide-react'
import type { ScanProgress } from '@/lib/types'

const PHASE_LABEL: Record<ScanProgress['phase'], string> = {
  indexing: 'Reading metadata',
  thumbnails: 'Rendering first pages',
  done: 'Done',
}

export function ScanProgressBar({ progress }: { progress: ScanProgress }) {
  const pct = progress.total > 0 ? Math.round((progress.processed / progress.total) * 100) : 0

  return (
    <div className="rounded-xl border border-border bg-card p-3.5 mb-5">
      <div className="flex items-center gap-2 text-sm">
        <Loader2 className="w-4 h-4 animate-spin text-primary shrink-0" />
        <span className="font-medium">{PHASE_LABEL[progress.phase]}</span>
        <span className="text-muted-foreground">
          {progress.total > 0 ? `${progress.processed} / ${progress.total}` : 'scanning…'}
        </span>
        <span className="ml-auto text-muted-foreground truncate max-w-[40%] text-xs">
          {progress.currentFile}
        </span>
      </div>
      <div className="h-1.5 rounded-full bg-muted overflow-hidden mt-2.5">
        <div
          className="h-full bg-primary rounded-full transition-all duration-200"
          style={{ width: `${pct}%` }}
        />
      </div>
    </div>
  )
}
