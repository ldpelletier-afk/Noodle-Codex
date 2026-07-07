import type { LucideIcon } from 'lucide-react'

export function StatCard({
  label,
  value,
  sublabel,
  icon: Icon,
}: {
  label: string
  value: string
  sublabel?: string
  icon?: LucideIcon
}) {
  return (
    <div className="h-full rounded-xl border border-border bg-card p-4 flex flex-col justify-between">
      <div className="flex items-center justify-between">
        <p className="text-xs text-muted-foreground">{label}</p>
        {Icon && <Icon className="w-3.5 h-3.5 text-muted-foreground" />}
      </div>
      <div>
        <p className="font-display text-2xl leading-tight mt-1">{value}</p>
        {sublabel && <p className="text-[0.7rem] text-muted-foreground mt-0.5">{sublabel}</p>}
      </div>
    </div>
  )
}
