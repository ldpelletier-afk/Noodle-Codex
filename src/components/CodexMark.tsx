import { BookMarked } from 'lucide-react'
import { cn } from '@/lib/utils'

export function CodexMark({ className, iconOnly = false }: { className?: string; iconOnly?: boolean }) {
  return (
    <span className={cn('group flex items-center gap-2 select-none', className)}>
      <span className="logo-bob flex items-center justify-center w-7 h-7 rounded-lg bg-primary/10 text-primary shrink-0">
        <BookMarked className="w-4 h-4" />
      </span>
      {!iconOnly && (
        <span className="font-display text-[1.05rem] leading-none tracking-tight text-foreground">
          Codex
        </span>
      )}
    </span>
  )
}
